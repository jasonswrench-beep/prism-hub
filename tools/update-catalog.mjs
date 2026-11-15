// SPDX-License-Identifier: Apache-2.0
// Update <short name> Catalog.bat (Update Prism Catalog.bat here) runs this. It:
//   1. reads each repository's newest stable and preview releases on GitHub (drafts never count) and finds each setup file;
//   2. downloads each setup file it has not checked before, computes its SHA-256 and checks its code signature (valid, timestamped, signed by
//      the expected signer); it STOPS if any setup file is missing or unsigned;
//   3. writes the catalog, prints a short table of what changed, asks for the hub key's password and signs it;
//   4. checks the signature with the public key built into the hub (brand/brand.json), and uploads the catalog and its signature to the
//      release named "catalog" in the hub's repository.
// Until it runs after an app release, the hub keeps offering the previous version, which is safe.
//
//   node tools/update-catalog.mjs            all of the above
//   node tools/update-catalog.mjs --local    sign, but do not upload: the files are left in catalog/out (to try with a development hub)
//   node tools/update-catalog.mjs --dry-run  find and check everything and show the table; sign and upload nothing
//
// It uses the GitHub command line (gh), signed in as the owner, for reading releases and uploading. Checked setup files are remembered in
// catalog/work/checked.json by GitHub's own SHA-256 of each file, so a setup file is downloaded once, not on every run.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import { fileURLToPath } from "node:url";
import { appEntry, changes, checkSignature, downloadUrl, findAsset, pickReleases, publicOf, releaseEntry, signBytes, stamp, table } from "./catalog-lib.mjs";
import { keyFile } from "./make-hub-key.mjs";
import { askSecret } from "./prompt.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const work = join(root, "catalog", "work");
const out = join(root, "catalog", "out");
const brand = JSON.parse(readFileSync(join(root, "brand", "brand.json"), "utf8"));
const sources = JSON.parse(readFileSync(join(root, "catalog", "apps.json"), "utf8"));
const checkedFile = join(work, "checked.json");
/** Every setup file that is missing or not properly signed. All are listed together, and then the script stops before signing anything. */
const problems = [];

const say = (w) => console.log(w);
class Stop extends Error {}
const stop = (w) => {
  throw new Stop(w);
};

function gh(args, { json = true, allowFail = false } = {}) {
  const done = spawnSync("gh", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (done.status !== 0) {
    if (allowFail) return null;
    stop(`gh ${args.join(" ")} failed: ${(done.stderr || done.stdout).trim().slice(0, 500)}`);
  }
  return json ? JSON.parse(done.stdout) : done.stdout;
}

function releasesOf(repository) {
  return gh(["api", `repos/${repository}/releases?per_page=100`]);
}

async function download(url, file) {
  const answer = await fetch(url, { redirect: "follow" });
  if (!answer.ok) stop(`downloading ${url} failed (${answer.status}).`);
  await pipeline(Readable.fromWeb(answer.body), createWriteStream(file));
}

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

/** Windows' own signature check, through PowerShell's Get-AuthenticodeSignature. */
function signatureOf(file) {
  const script =
    `$s = Get-AuthenticodeSignature -LiteralPath $env:HUB_FILE; ` +
    `$n = if ($s.SignerCertificate) { $s.SignerCertificate.GetNameInfo(0, $false) } else { '' }; ` +
    `[pscustomobject]@{ status = "$($s.Status)"; signer = $n; timestamped = [bool]$s.TimeStamperCertificate } | ConvertTo-Json -Compress`;
  const done = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], { encoding: "utf8", env: { ...process.env, HUB_FILE: file } });
  if (done.status !== 0) stop(`the signature of ${file} could not be checked: ${done.stderr.trim()}`);
  return JSON.parse(done.stdout);
}

/** Downloads (once) and checks one setup file. Returns its SHA-256. */
async function checked(asset, url, expectedSigner, what, memory) {
  const githubSum = (asset.digest ?? "").replace(/^sha256:/, "");
  const known = githubSum && memory[githubSum];
  if (known && known.size === asset.size && known.signer === expectedSigner) {
    say(`  ${what}: ${asset.name} (checked before)`);
    return githubSum;
  }
  say(`  ${what}: downloading ${asset.name} (${(asset.size / 1048576).toFixed(1)} MB) to check it`);
  const file = join(work, "download.part");
  try {
    await download(url, file);
    const sum = sha256(file);
    if (githubSum && sum !== githubSum) stop(`${what}: the downloaded ${asset.name} does not match GitHub's own SHA-256 of it.`);
    const sig = signatureOf(file);
    const problem =
      sig.status === "NotSigned" ? "is not signed"
      : sig.status !== "Valid" ? `is not validly signed (Windows says ${sig.status})`
      : !sig.timestamped ? "is signed without a timestamp"
      : sig.signer.toLowerCase() !== expectedSigner.toLowerCase() ? `is signed by ${sig.signer}, not ${expectedSigner}`
      : null;
    if (problem) {
      problems.push(`${what}: ${asset.name} ${problem}. If this release came before signing began, add its tag to "ignoreTags" in catalog/apps.json.`);
      return null;
    }
    memory[sum] = { name: asset.name, size: asset.size, signer: expectedSigner, checked: stamp() };
    writeFileSync(checkedFile, JSON.stringify(memory, null, 1) + "\n");
    return sum;
  } finally {
    rmSync(file, { force: true });
  }
}

async function buildCatalog(memory) {
  const apps = [];
  const comingSoon = [...sources.comingSoon];
  for (const source of sources.apps) {
    say(`${source.name} (${source.repository})`);
    const ignored = new Set(source.ignoreTags ?? []);
    const { stable, preview } = pickReleases(releasesOf(source.repository).filter((r) => !ignored.has(r.tag_name)));
    // An app still being made is listed under Coming soon until people can actually download it: a public repository with a release.
    if (source.comingSoonUntilReleased) {
      const visibility = gh(["repo", "view", source.repository, "--json", "visibility"]).visibility;
      if ((!stable && !preview) || visibility !== "PUBLIC") {
        say(`  Coming soon (${visibility !== "PUBLIC" ? `its repository is ${visibility.toLowerCase()}` : "no published release yet"})`);
        comingSoon.push({ id: source.id, name: source.name, description: source.description ?? "", mark: source.mark });
        continue;
      }
    }
    if (!stable && !preview) problems.push(`${source.name} has no published release.`);
    const entry = async (release) => {
      if (!release) return null;
      const found = findAsset(release, source.setupPattern, source.name);
      if (found.error) {
        problems.push(found.error);
        return null;
      }
      const url = downloadUrl(source.repository, release.tag_name, found.asset.name);
      const sum = await checked(found.asset, url, source.expectedSigner, `${source.name} ${release.tag_name}`, memory);
      return sum && releaseEntry(source.repository, release, found.asset, sum, source.windowsVersion);
    };
    const stableEntry = await entry(stable);
    const previewEntry = await entry(preview);
    // Extras (a portable copy, an add-on) come from the newest release that carries them; their SHA-256 is GitHub's own, checked by download.
    const extras = [];
    for (const x of source.extras ?? []) {
      const release = [preview, stable].find((r) => r && r.assets.some((a) => new RegExp(x.pattern).test(a.name)));
      if (!release) continue;
      const asset = release.assets.find((a) => new RegExp(x.pattern).test(a.name));
      const file = join(work, "download.part");
      say(`  ${release.tag_name}: downloading ${asset.name} to compute its SHA-256`);
      await download(downloadUrl(source.repository, release.tag_name, asset.name), file);
      extras.push({ name: x.name, description: x.description ?? "", fileName: asset.name, url: downloadUrl(source.repository, release.tag_name, asset.name), sha256: sha256(file), size: asset.size });
      rmSync(file, { force: true });
    }
    apps.push(appEntry(source, stableEntry, previewEntry, extras));
  }

  // The hub's own newest release (its catalog release is not a version, so it never counts).
  let hub = null;
  const hubReleases = pickReleases(releasesOf(sources.hub.repository));
  const hubRelease = hubReleases.stable ?? hubReleases.preview;
  if (hubRelease) {
    say(`${brand.productName} (${sources.hub.repository})`);
    const found = findAsset(hubRelease, sources.hub.setupPattern, brand.productName);
    if (found.error) problems.push(found.error);
    else {
      const url = downloadUrl(sources.hub.repository, hubRelease.tag_name, found.asset.name);
      const sum = await checked(found.asset, url, brand.expectedSigner, `${brand.productName} ${hubRelease.tag_name}`, memory);
      hub = sum && releaseEntry(sources.hub.repository, hubRelease, found.asset, sum);
    }
  }
  if (problems.length > 0) stop(`these setup files are missing or not properly signed, so no catalog was made:\n  - ${problems.join("\n  - ")}`);

  const web = sources.web.filter((w) => {
    if (w.url) return true;
    say(`  (left out until it has an address: ${w.name})`);
    return false;
  });
  return { format: 1, generated: stamp(), minimumHubVersion: sources.minimumHubVersion, hub, apps, web, comingSoon };
}

function published() {
  const dir = join(work, "published");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const got = gh(["release", "download", brand.catalog.tag, "--repo", brand.repository, "--pattern", brand.catalog.file, "--dir", dir], { json: false, allowFail: true });
  const file = join(dir, brand.catalog.file);
  return got !== null && existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
}

async function main(argv) {
  const dry = argv.includes("--dry-run");
  const local = argv.includes("--local");
  mkdirSync(work, { recursive: true });
  if (spawnSync("gh", ["auth", "status"], { encoding: "utf8" }).status !== 0) stop("the GitHub command line (gh) is not signed in. Run: gh auth login");
  if (!dry && !brand.catalogPublicKey) stop("there is no hub key yet. Run Make Hub Key.bat first.");
  const visibility = gh(["repo", "view", brand.repository, "--json", "visibility"]).visibility;
  if (visibility !== "PUBLIC") say(`NOTE: ${brand.repository} is ${visibility.toLowerCase()}. Hubs can download the catalog only once the repository is public.\n`);

  const memory = existsSync(checkedFile) ? JSON.parse(readFileSync(checkedFile, "utf8")) : {};
  const catalog = await buildCatalog(memory);
  const before = published();
  const rows = changes(before, catalog);
  say(`\nWhat changed (* marks a change):\n${table(rows)}\n`);
  const bytes = Buffer.from(JSON.stringify(catalog, null, 2) + "\n", "utf8");
  mkdirSync(out, { recursive: true });
  if (dry) {
    writeFileSync(join(out, `${brand.catalog.file}.unsigned`), bytes);
    say(`Dry run: nothing was signed or uploaded. The catalog it would publish is in catalog/out/${brand.catalog.file}.unsigned`);
    return;
  }
  if (before && !rows.some((r) => r.changed)) {
    say("Nothing changed since the published catalog, so nothing was published.");
    return;
  }

  const file = keyFile();
  if (!existsSync(file)) stop(`the hub key's private half is not at ${file}. Set HUB_KEY to where it is.`);
  const pem = readFileSync(file, "utf8");
  const password = await askSecret("The hub key's password: ");
  if (!password) stop("no password was typed. Nothing was signed.");
  let signature;
  try {
    if (publicOf(pem, password) !== brand.catalogPublicKey) stop("this key is not the one built into the hub (brand/brand.json). Nothing was signed.");
    signature = signBytes(bytes, pem, password) + "\n";
  } catch (e) {
    if (e instanceof Stop) throw e;
    stop(`the password did not open the hub key (${password.length} characters were typed). Nothing was signed. To try the password on its own: node tools\\make-hub-key.mjs --check`);
  }
  if (!checkSignature(bytes, signature, brand.catalogPublicKey)) stop("the new signature did not check out. Nothing was uploaded.");
  const catalogFile = join(out, brand.catalog.file);
  const signatureFile = join(out, brand.catalog.signature);
  writeFileSync(catalogFile, bytes);
  writeFileSync(signatureFile, signature);
  say(`Signed: catalog/out/${brand.catalog.file} and its signature.`);
  if (local) {
    say("--local: not uploaded. A development hub reads this folder when HUB_CATALOG_DIR names it.");
    return;
  }

  const exists = gh(["release", "view", brand.catalog.tag, "--repo", brand.repository, "--json", "tagName"], { allowFail: true });
  if (!exists) {
    say(`Making the release "${brand.catalog.tag}" in ${brand.repository}.`);
    gh(
      ["release", "create", brand.catalog.tag, "--repo", brand.repository, "--prerelease", "--title", `${brand.shortName} catalog`, "--notes",
        `The signed list of ${brand.shortName} apps that ${brand.productName} reads. Updated by Update ${brand.shortName} Catalog.bat; not a program release.`],
      { json: false },
    );
  }
  // Both in one upload. A hub that fetches between the two files sees a mismatch and keeps its last good catalog until its next check.
  gh(["release", "upload", brand.catalog.tag, signatureFile, catalogFile, "--repo", brand.repository, "--clobber"], { json: false });
  say(`Published to https://github.com/${brand.repository}/releases/tag/${brand.catalog.tag}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url).toLowerCase() === resolve(process.argv[1]).toLowerCase()) {
  main(process.argv.slice(2)).catch((e) => {
    console.error(`\nSTOPPED: ${e.message}`);
    process.exitCode = 1;
  });
}
