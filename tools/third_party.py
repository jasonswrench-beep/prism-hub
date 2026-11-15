"""Generate THIRD-PARTY.md from the shipped dependency graph, and check licenses.

Run from the repository root:  python tools/third_party.py
Exits with status 1 if a dependency's license is not on the allow-list, so a build stops
instead of shipping an unreviewed license.
"""
import datetime
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWED = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "Unicode-DFS-2016",
           "MPL-2.0", "CC0-1.0", "0BSD", "BSL-1.0", "Unlicense", "Apache-2.0 WITH LLVM-exception", "CDLA-Permissive-2.0", "NCSA"}


def license_ok(expr):
    if not expr:
        return False
    for alt in re.split(r"\s+OR\s+|/", expr.replace("(", "").replace(")", "")):
        if all(p.strip() in ALLOWED for p in re.split(r"\s+AND\s+", alt)):
            return True
    return False


meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked", "--manifest-path", str(ROOT / "src-tauri" / "Cargo.toml")], text=True, encoding="utf-8"))
pkgs = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = next(p["id"] for p in meta["packages"] if p["name"] == "prism-hub")
seen, stack = set(), [root]
while stack:
    i = stack.pop()
    if i in seen:
        continue
    seen.add(i)
    for d in nodes[i]["deps"]:
        normal = any(k["kind"] is None for k in d["dep_kinds"])
        target_ok = any((k.get("target") or "") == "" or "windows" in (k.get("target") or "") for k in d["dep_kinds"])
        if normal and target_ok:
            stack.append(d["pkg"])

shipped = sorted((pkgs[i] for i in seen if pkgs[i]["name"] != "prism-hub"), key=lambda p: (p["name"], p["version"]))
bad = [p for p in shipped if not license_ok(p.get("license"))]

lines = ["# Third-party software", "",
         "Prism Hub includes the following third-party components. Each is used under its own license; the full license texts are available from each project's repository.", "",
         f"Generated {datetime.date.today().isoformat()} from the shipped dependency graph by tools/third_party.py.", "",
         "| Component | Version | License | Source |", "|---|---|---|---|"]
for p in shipped:
    lines.append(f"| {p['name']} | {p['version']} | {p.get('license') or 'see source'} | {p.get('repository') or ''} |")
lines += ["", "The Windows WebView2 runtime that displays the window is part of Windows and is not distributed with this program.", ""]
(ROOT / "THIRD-PARTY.md").write_text("\n".join(lines), encoding="utf-8", newline="\n")
print(f"{len(shipped)} Rust components")
if bad:
    print("LICENSES NEEDING REVIEW:")
    for p in bad:
        print("  ", p["name"], p.get("version"), p.get("license"))
    sys.exit(1)
print("All licenses are on the allow-list.")
