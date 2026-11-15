// SPDX-License-Identifier: Apache-2.0
// Makes the hub's installer, on this PC (or on GitHub's build machine, without signing).
//
//   node tools/build-installer.mjs            build the setup file (not signed)
//   node tools/build-installer.mjs --sign     build it, and sign every program in it and the setup itself
//
// What it does, in order (the same steps as OpenExplainer's tools/build-installer.mjs):
//   1. copies the brand file's values into tauri.conf.json, and checks that the version in Cargo.toml, tauri.conf.json and package.json is
//      RELEASE (below), and that the hub key's public half is in the brand file (a hub without it can trust no catalog);
//   2. builds the program and its installer with Tauri, with --remap-path-prefix for the user folder, Rust's folders and this repository,
//      so no folder name of the building computer is written into the program;
//   3. checks Tauri's installer script carries the web view's setup inside (never downloaded while installing);
//   4. unpacks the setup with 7-Zip and checks every program in it: none may name this computer's user folder, and with --sign every one, and
//      the setup, must be signed and timestamped by the publisher (OPENSTAGE_SIGN_PUBLISHER when it is set). Signing is done by Tauri's sign
//      command with tools/sign_windows.py, the suite's shared signing script (its top says which settings it needs);
//   5. copies the setup to src-tauri/target/release-files/<Product-Name>-<RELEASE>-Setup.exe, with its SHA-256 in a .sha256.txt beside it,
//      and says whether it is under the 8 MB target.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { applyBrand } from "./apply-brand.mjs";

/** The release this build is. Raise it for every release; the hub compares it with the catalog's hub entry. Tags are v<RELEASE>. */
export const RELEASE = "1.0.0";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = join(root, "src-tauri");
const brand = JSON.parse(readFileSync(join(root, "brand", "brand.json"), "utf8"));
const appExe = join(tauriDir, "target", "release", `${brand.programName}.exe`);
const nsisScript = join(tauriDir, "target", "release", "nsis", "x64", "installer.nsi");
const bundleDir = join(tauriDir, "target", "release", "bundle", "nsis");
const outDir = join(tauriDir, "target", "release-files");
const signDir = join(tauriDir, "target", "signing");
const signScript = join(root, "tools", "sign_windows.py");
const sevenZip = process.env.OPENSTAGE_7ZIP ?? "C:\\Program Files\\7-Zip\\7z.exe";
const MAKERS_FILES = new Set(["microsoftedgewebview2setup.exe"]);
const PROGRAM = /\.(exe|dll)$/i;
const TARGET_BYTES = 8 * 1024 * 1024;

const say = (words) => console.log(words);
function fail(words) {
  console.error(`STOPPED: ${words}`);
  process.exit(1);
}

export function folderFlags(home = homedir(), env = process.env, repo = root) {
  const folders = [
    [home, "/home"],
    [env.CARGO_HOME || join(home, ".cargo"), "/cargo"],
    [env.RUSTUP_HOME || join(home, ".rustup"), "/rustup"],
    [repo, "/hub"],
  ];
  return [...folders.map(([folder, name]) => `--remap-path-prefix=${folder}=${name}`), "-Clink-arg=/PDBALTPATH:%_PDB%"];
}

function rustEnv(more = {}) {
  const env = { ...process.env, ...more };
  delete env.RUSTFLAGS;
  env.CARGO_ENCODED_RUSTFLAGS = folderFlags().join("\x1f");
  return env;
}

export function namingUserFolder(files, home = homedir()) {
  const needles = [...new Set([home.toLowerCase(), home.replace(/\\/g, "/").toLowerCase()])];
  return files.filter((file) => {
    const bytes = readFileSync(file);
    const text = bytes.toString("latin1").toLowerCase();
    const wide = bytes.toString("utf16le").toLowerCase();
    return needles.some((n) => text.includes(n) || wide.includes(n));
  });
}

export function webviewMode(script) {
  const found = /^\s*!define\s+INSTALLWEBVIEW2MODE\s+"?([A-Za-z]+)"?\s*$/m.exec(script);
  return found ? found[1] : null;
}

function run(program, args, options = {}) {
  say(`  > ${program} ${args.join(" ")}`);
  const done = spawnSync(program, args, { stdio: "inherit", ...options });
  return done.status ?? 1;
}

function programsIn(folder) {
  const out = [];
  for (const entry of readdirSync(folder, { withFileTypes: true, recursive: true })) {
    if (entry.isFile() && PROGRAM.test(entry.name)) out.push(join(entry.parentPath ?? entry.path, entry.name));
  }
  return out.sort();
}

function checkSignatures(files, publisher) {
  if (files.length === 0) return { ok: true, words: "" };
  const env = { ...process.env };
  if (publisher) env.OPENSTAGE_SIGN_PUBLISHER = publisher;
  else delete env.OPENSTAGE_SIGN_PUBLISHER;
  const done = spawnSync("python", ["-I", signScript, "--check", ...files], { env, encoding: "utf8" });
  return { ok: done.status === 0, words: `${done.stdout ?? ""}${done.stderr ?? ""}`.trim() };
}

function checkSetup(setup, sign) {
  if (!existsSync(sevenZip)) fail(`7-Zip is not at ${sevenZip} (set OPENSTAGE_7ZIP), so the programs inside the setup could not be checked.`);
  const work = mkdtempSync(join(tmpdir(), "hub-setup-check-"));
  try {
    const unpacked = spawnSync(sevenZip, ["x", "-y", `-o${work}`, setup], { encoding: "utf8" });
    if (unpacked.status !== 0) fail(`7-Zip could not unpack the setup: ${(unpacked.stdout + unpacked.stderr).slice(-400)}`);
    const inside = programsIn(work);
    const named = namingUserFolder(inside);
    if (named.length > 0) fail(`these programs in the setup name this computer's user folder: ${named.map((f) => relative(work, f)).join(", ")}`);
    say(`  ${inside.length} programs in the setup; none names this computer's user folder`);
    if (!sign) return;
    const theirs = inside.filter((f) => MAKERS_FILES.has(f.split(/[\\/]/).pop().toLowerCase()));
    const ours = [setup, ...inside.filter((f) => !theirs.includes(f))];
    const publisher = process.env.OPENSTAGE_SIGN_PUBLISHER || undefined;
    const mine = checkSignatures(ours, publisher);
    if (!mine.ok) fail(`the signed setup did not pass:\n${mine.words}`);
    const makers = checkSignatures(theirs, undefined);
    if (!makers.ok) fail(`a program its maker signs does not carry a valid signature:\n${makers.words}`);
    say(`  the setup and ${ours.length - 1} programs in it are signed and timestamped${publisher ? ` by ${publisher}` : ""}; ${theirs.length} kept their makers' signatures`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function versionsMatch() {
  const conf = JSON.parse(readFileSync(join(tauriDir, "tauri.conf.json"), "utf8")).version;
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
  const cargo = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(tauriDir, "Cargo.toml"), "utf8"))?.[1];
  return { conf, pkg, cargo, ok: conf === RELEASE && pkg === RELEASE && cargo === RELEASE };
}

function main(argv) {
  const sign = argv.includes("--sign");
  say(`${brand.productName} ${RELEASE}: building the installer${sign ? " and signing it" : ""}.`);

  say("1. The brand and the version.");
  applyBrand();
  const v = versionsMatch();
  if (!v.ok) fail(`the version must be ${RELEASE} everywhere: tauri.conf.json says ${v.conf}, package.json ${v.pkg}, Cargo.toml ${v.cargo}.`);
  if (!brand.catalogPublicKey) {
    if (sign) fail("brand/brand.json has no hub key yet (Make Hub Key.bat makes it). A released hub without it could trust no catalog.");
    say("  NOTE: no hub key in brand/brand.json yet, so this copy will trust no catalog. Fine for trying the window; not for a release.");
  }
  say(`  ${brand.productName}, ${brand.identifier}, version ${RELEASE}`);

  let signConfig = null;
  if (sign) {
    mkdirSync(signDir, { recursive: true });
    const ready = spawnSync("python", ["-I", signScript, "--ready"], { stdio: "inherit", env: { ...process.env, OPENSTAGE_SIGN_METADATA: join(signDir, "metadata.json") } });
    if (ready.status !== 0) fail("signing is not ready (what is missing is listed above).");
    signConfig = join(signDir, "tauri-sign.json");
    writeFileSync(signConfig, JSON.stringify({ bundle: { windows: { signCommand: { cmd: "python", args: ["-I", signScript, "--description", brand.productName, "%1"] } } } }, null, 1) + "\n");
  }

  say("2. The program and the installer.");
  if (existsSync(bundleDir)) for (const name of readdirSync(bundleDir)) if (name.endsWith("-setup.exe")) rmSync(join(bundleDir, name));
  const tauri = [join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"), "build", ...(signConfig ? ["--config", signConfig] : [])];
  if (run(process.execPath, tauri, { cwd: root, env: rustEnv({ OPENSTAGE_SIGN_METADATA: join(signDir, "metadata.json") }) }) !== 0) {
    fail("the program or the installer did not build. What Tauri said is above.");
  }
  const setups = existsSync(bundleDir) ? readdirSync(bundleDir).filter((n) => n.endsWith("-setup.exe")) : [];
  if (setups.length !== 1) fail(`expected one setup file in ${bundleDir}, found ${setups.length}.`);
  const setup = join(bundleDir, setups[0]);

  say("3. Checks.");
  if (namingUserFolder([appExe]).length > 0) fail(`the program names this computer's user folder: ${appExe}`);
  say("  the program does not name this computer's user folder");
  const mode = existsSync(nsisScript) ? webviewMode(readFileSync(nsisScript, "utf8")) : null;
  if (mode !== "embedBootstrapper" && mode !== "offlineInstaller") {
    fail(`the installer would get the web view's setup by ${mode ?? "an unknown way"}, which downloads it while installing. tauri.conf.json must embed it.`);
  }
  say(`  the installer carries the web view's setup (${mode}); it downloads nothing`);
  checkSetup(setup, sign);

  mkdirSync(outDir, { recursive: true });
  const name = `${brand.productName.replace(/\s+/g, "-")}-${RELEASE}-Setup.exe`;
  const final = join(outDir, name);
  copyFileSync(setup, final);
  const digest = createHash("sha256").update(readFileSync(final)).digest("hex");
  writeFileSync(`${final}.sha256.txt`, `${digest}  ${name}\n`);
  const bytes = statSync(final).size;
  say(`Done: ${final} (${(bytes / 1048576).toFixed(2)} MB, ${bytes <= TARGET_BYTES ? "under" : "OVER"} the 8 MB target)\n  SHA-256 ${digest}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url).toLowerCase() === resolve(process.argv[1]).toLowerCase()) main(process.argv.slice(2));
