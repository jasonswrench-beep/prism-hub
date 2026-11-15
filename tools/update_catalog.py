#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Writes, signs and publishes the Prism catalog (Update Prism Catalog.bat runs this).

For each app in catalog/sources.json it reads the repository's newest stable and preview
releases, finds the setup file, downloads it, computes its SHA-256 and checks its code
signature. Then it writes prism-catalog.json, asks for the hub key's password, signs the file
and uploads both files to the release named "catalog" in the hub's repository.

It stops, changing nothing online, if any setup file is missing, unsigned or signed by someone
else, or if GitHub's own SHA-256 for a file differs from the one computed here.

  python tools/update_catalog.py                 the whole job (asks before uploading)
  python tools/update_catalog.py --no-upload     write and sign, but do not upload
  python tools/update_catalog.py --unsigned      write only (no key, no upload): for a look
  python tools/update_catalog.py --key FILE      the hub key (default %USERPROFILE%\\.prism-hub-catalog.key,
                                                 or PRISM_HUB_KEY)
  python tools/update_catalog.py --yes           upload without asking (the password is still asked)
  python tools/update_catalog.py --out DIR       where to write (default dist\\catalog)

Needs: the GitHub command line (gh, signed in) and the hub's node_modules (npm install), which
holds the signing tool.
"""

import base64
import datetime
import getpass
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BRAND = json.loads((ROOT / "brand" / "brand.json").read_text(encoding="utf-8"))
SOURCES = json.loads((ROOT / "catalog" / "sources.json").read_text(encoding="utf-8"))
CATALOG_NAME = BRAND["catalog_file"]
CATALOG_TAG = "catalog"


class Stop(Exception):
    pass


# ---------------------------------------------------------------------------------------------
# Versions (semantic versioning order, enough for release tags)

VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$")


def version_key(tag):
    m = VERSION.match(tag.strip())
    if not m:
        return None
    major, minor, patch, pre = int(m[1]), int(m[2]), int(m[3]), m[4]
    if pre is None:
        pre_key = (1,)  # a release sorts after all of its pre-releases
    else:
        parts = []
        for p in pre.split("."):
            parts.append((0, int(p), "") if p.isdigit() else (1, 0, p))
        pre_key = (0, tuple(parts))
    return (major, minor, patch, pre_key)


def plain_version(tag):
    return tag.strip().lstrip("vV")


# ---------------------------------------------------------------------------------------------
# GitHub, through the gh command line

def gh(*args, text=True):
    done = subprocess.run(["gh", *args], capture_output=True, text=text, encoding="utf-8" if text else None)
    if done.returncode != 0:
        raise Stop(f"gh {' '.join(args)} failed:\n{(done.stderr or '').strip()}")
    return done.stdout


def releases(repo):
    data = json.loads(gh("api", f"repos/{repo}/releases?per_page=60"))
    out = []
    for r in data:
        if r.get("draft"):
            continue
        key = version_key(r["tag_name"])
        if key is None:
            continue  # tags such as update-feed or catalog are not app releases
        out.append((key, r))
    out.sort(key=lambda kr: kr[0], reverse=True)
    return out


def newest(repo):
    """The newest stable release, and the newest pre-release if it is newer than that stable one."""
    stable = preview = None
    for key, r in releases(repo):
        if r.get("prerelease"):
            if preview is None:
                preview = (key, r)
        elif stable is None:
            stable = (key, r)
    if stable and preview and preview[0] < stable[0]:
        preview = None
    return (stable[1] if stable else None), (preview[1] if preview else None)


def asset(release, pattern, what):
    found = [a for a in release.get("assets", []) if re.search(pattern, a["name"])]
    if len(found) != 1:
        names = ", ".join(a["name"] for a in release.get("assets", [])) or "none"
        raise Stop(f"{what}: release {release['tag_name']} has {len(found)} files matching {pattern!r} (its files: {names}).")
    return found[0]


def download(repo, tag, name, folder):
    gh("release", "download", tag, "-R", repo, "-p", name, "-D", str(folder), "--clobber")
    path = Path(folder) / name
    if not path.is_file():
        raise Stop(f"{name} did not download.")
    return path


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_signing_tools():
    spec = importlib.util.spec_from_file_location("sign_windows", ROOT / "tools" / "sign_windows.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


SIGN = load_signing_tools()


def check_signed(path, signer, what):
    info = SIGN.signatures([path])[0]
    why = SIGN.problem(info, signer)
    if why:
        raise Stop(f"{what}: {Path(path).name} is {why}. Nothing was published.")


def entry_for(repo, release, pattern, signer, folder, what):
    a = asset(release, pattern, what)
    print(f"  {what}: {release['tag_name']}  {a['name']}  ({a['size'] / 1048576:.1f} MB)", flush=True)
    path = download(repo, release["tag_name"], a["name"], folder)
    digest = sha256(path)
    github = (a.get("digest") or "").removeprefix("sha256:")
    if github and github != digest:
        raise Stop(f"{what}: GitHub says {a['name']} has SHA-256 {github}, but the downloaded file has {digest}.")
    if path.stat().st_size != a["size"]:
        raise Stop(f"{what}: {a['name']} is {path.stat().st_size} bytes, but GitHub lists {a['size']}.")
    if path.suffix.lower() == ".exe":
        check_signed(path, signer, what)
    return {
        "version": plain_version(release["tag_name"]),
        "url": a["browser_download_url"],
        "sha256": digest,
        "size": a["size"],
        "release_page": release["html_url"],
    }, path


def build(folder):
    catalog = {
        "format": 1,
        "generated": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "minimum_hub_version": SOURCES.get("minimum_hub_version", "1.0.0"),
        "hub": None,
        "apps": [],
        "web": SOURCES.get("web", []),
    }
    hub = SOURCES["hub"]
    print(f"{BRAND['product_name']} ({hub['repository']})", flush=True)
    try:
        stable, _ = newest(hub["repository"])
    except Stop:
        stable = None
    if stable:
        entry, path = entry_for(hub["repository"], stable, hub["setup"], hub["expected_signer"], folder, "hub")
        catalog["hub"] = entry
        path.unlink(missing_ok=True)
    else:
        print("  no hub release yet: the catalog carries no hub update", flush=True)

    for src in SOURCES["apps"]:
        print(f"{src['name']} ({src['repository']})", flush=True)
        app = {k: src[k] for k in ("id", "name", "mark", "section", "description", "repository", "installed_as", "program_file", "quiet_switch", "uninstall_switch", "expected_signer")}
        stable, preview = newest(src["repository"])
        if not stable and not preview:
            raise Stop(f"{src['name']}: the repository has no release with a version tag.")
        app["stable"] = app["preview"] = None
        for label, rel in (("stable", stable), ("preview", preview)):
            if rel:
                entry, path = entry_for(src["repository"], rel, src["setup"], src["expected_signer"], folder, f"{src['name']} {label}")
                app[label] = entry
                path.unlink(missing_ok=True)
        # Extras come from the newest release, and their label names its version.
        newest_release = preview if preview else stable
        app["extras"] = []
        for x in src.get("extras", []):
            entry, path = entry_for(src["repository"], newest_release, x["pattern"], src["expected_signer"], folder, f"{src['name']} {x['label']}")
            app["extras"].append({"label": f"{x['label']} {entry['version']}", "file_name": path.name, "url": entry["url"], "sha256": entry["sha256"], "size": entry["size"]})
            path.unlink(missing_ok=True)
        catalog["apps"].append(app)

    for c in SOURCES.get("coming_soon", []):
        catalog["apps"].append({"id": c["id"], "name": c["name"], "mark": c["mark"], "section": "coming-soon", "description": c.get("description", "")})
    return catalog


# ---------------------------------------------------------------------------------------------
# What changed

def published():
    repo = BRAND["repository"]
    with tempfile.TemporaryDirectory() as d:
        try:
            gh("release", "download", CATALOG_TAG, "-R", repo, "-p", CATALOG_NAME, "-D", d, "--clobber")
            return json.loads((Path(d) / CATALOG_NAME).read_text(encoding="utf-8"))
        except (Stop, OSError, ValueError):
            return None


def changes(old, new):
    def versions(c):
        out = {}
        if not c:
            return out
        if c.get("hub"):
            out[(BRAND["product_name"], "stable")] = c["hub"]["version"]
        for a in c.get("apps", []):
            for ch in ("stable", "preview"):
                if a.get(ch):
                    out[(a["name"], ch)] = a[ch]["version"]
        return out

    o, n = versions(old), versions(new)
    rows = []
    for key in sorted(set(o) | set(n), key=lambda k: (k[0].lower(), k[1])):
        before, after = o.get(key, "-"), n.get(key, "-")
        rows.append((key[0], key[1], before, after, "" if before == after else "changed"))
    width = max([len(r[0]) for r in rows] + [3])
    print()
    print(f"{'App'.ljust(width)}  Channel  Published now  This catalog")
    for name, ch, before, after, note in rows:
        print(f"{name.ljust(width)}  {ch.ljust(7)}  {before.ljust(13)}  {after}  {note}")
    print()
    return any(r[4] for r in rows)


# ---------------------------------------------------------------------------------------------
# Signing

def key_id_of_public(b64):
    text = base64.b64decode(b64).decode("utf-8")
    raw = base64.b64decode(text.strip().splitlines()[-1])
    return raw[2:10]


def key_id_of_signature(sig_file_text):
    text = base64.b64decode(sig_file_text.strip()).decode("utf-8")
    lines = text.strip().splitlines()
    raw = base64.b64decode(lines[1])
    return raw[2:10]


def sign(path, key):
    tauri = ROOT / "node_modules" / "@tauri-apps" / "cli" / "tauri.js"
    if not tauri.is_file():
        raise Stop("The signing tool is missing. Run `npm install` in the hub's folder first.")
    if not Path(key).is_file():
        raise Stop(f"The hub key is not at {key}. Make it with Make Hub Key.bat, or name it with --key.")
    env = dict(os.environ)
    if not env.get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD"):
        env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = getpass.getpass("Password for the hub key: ")
    env.pop("TAURI_SIGNING_PRIVATE_KEY", None)
    done = subprocess.run(["node", str(tauri), "signer", "sign", "-f", str(key), str(path)], capture_output=True, text=True, env=env)
    sig = Path(str(path) + ".sig")
    if done.returncode != 0 or not sig.is_file():
        raise Stop("Signing failed (is the password right?):\n" + (done.stderr or done.stdout).strip())
    public = BRAND.get("catalog_public_key", "").strip()
    if public and key_id_of_signature(sig.read_text(encoding="utf-8")) != key_id_of_public(public):
        raise Stop("The catalog was signed with a key other than the one built into the hub (brand/brand.json). Nothing was published.")
    if not public:
        print("Note: brand/brand.json has no catalog_public_key yet, so no hub will accept this catalog.")
    return sig


def upload(files, assume_yes):
    repo = BRAND["repository"]
    if not assume_yes:
        answer = input(f"Upload {', '.join(Path(f).name for f in files)} to the '{CATALOG_TAG}' release of {repo}? [y/N] ").strip().lower()
        if answer not in ("y", "yes"):
            print("Not uploaded. The files are in " + str(Path(files[0]).parent))
            return False
    try:
        gh("release", "view", CATALOG_TAG, "-R", repo, "--json", "tagName")
    except Stop:
        print(f"Making the '{CATALOG_TAG}' release.")
        gh("release", "create", CATALOG_TAG, "-R", repo, "--prerelease", "--latest=false", "--title", "App catalog (not a download)",
           "--notes", f"The signed list of apps that {BRAND['product_name']} reads. Not a download: install {BRAND['product_name']} from its own releases.")
    gh("release", "upload", CATALOG_TAG, "-R", repo, "--clobber", *[str(f) for f in files])
    print(f"Published to https://github.com/{repo}/releases/tag/{CATALOG_TAG}")
    return True


def main(argv):
    args = list(argv)

    def flag(name):
        if name in args:
            args.remove(name)
            return True
        return False

    def option(name, default=None):
        if name in args:
            i = args.index(name)
            value = args[i + 1]
            del args[i:i + 2]
            return value
        return default

    unsigned, no_upload, assume_yes = flag("--unsigned"), flag("--no-upload"), flag("--yes")
    key = option("--key", os.environ.get("PRISM_HUB_KEY") or str(Path.home() / ".prism-hub-catalog.key"))
    out = Path(option("--out", str(ROOT / "dist" / "catalog")))
    if args:
        print(__doc__)
        return 2
    try:
        out.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="prism-catalog-") as folder:
            catalog = build(folder)
        target = out / CATALOG_NAME
        target.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8", newline="\n")
        old_sig = Path(str(target) + ".sig")
        old_sig.unlink(missing_ok=True)
        any_change = changes(published(), catalog)
        print(f"Wrote {target}")
        if unsigned:
            print("Not signed (--unsigned), so it was not published.")
            return 0
        sig = sign(target, key)
        print(f"Signed: {sig}")
        if no_upload:
            return 0
        if not any_change:
            print("No version changed since the published catalog; uploading anyway refreshes its date.")
        upload([target, sig], assume_yes)
        return 0
    except Stop as e:
        print("\nStopped: " + str(e))
        return 1
    except KeyboardInterrupt:
        print("\nStopped.")
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
