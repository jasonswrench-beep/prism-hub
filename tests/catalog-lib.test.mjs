// SPDX-License-Identifier: Apache-2.0
// node --test tests/*.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import {
  appEntry, baseVersion, changes, checkSignature, compareVersions, downloadUrl, findAsset, makeKey, pickReleases, publicOf, releaseEntry, signBytes, stamp,
} from "../tools/catalog-lib.mjs";

const rel = (tag, draft = false, assets = []) => ({ tag_name: tag, draft, assets });

test("versions sort the way the hub sorts them", () => {
  const order = ["0.1.0-alpha.1", "0.1.0-alpha.10", "0.1.0-beta.1", "0.1.0-rc.1", "0.1.0", "0.2.0-alpha.1", "1.0.0"];
  for (let i = 1; i < order.length; i++) assert.ok(compareVersions(order[i], order[i - 1]) > 0, `${order[i - 1]} < ${order[i]}`);
  assert.equal(baseVersion("v0.2.0-alpha.4"), "0.2.0");
});

test("drafts and non-version tags never count, and an older preview is left out", () => {
  // As GitHub showed OpenStage Recorder on 2026-11-15: v0.1.0 exists only as a draft.
  const recorder = [rel("v0.1.0-alpha.3", true), rel("v0.1.0", true), rel("v0.1.0-alpha.4"), rel("v0.1.0-alpha.3"), rel("catalog")];
  const r = pickReleases(recorder);
  assert.equal(r.stable, null);
  assert.equal(r.preview.tag_name, "v0.1.0-alpha.4");
  const both = pickReleases([rel("v0.1.1"), rel("v0.2.0-alpha.4"), rel("v0.1.0")]);
  assert.equal(both.stable.tag_name, "v0.1.1");
  assert.equal(both.preview.tag_name, "v0.2.0-alpha.4");
  const passed = pickReleases([rel("v0.2.0"), rel("v0.2.0-rc.1")]);
  assert.equal(passed.preview, null, "a preview older than the stable release is not listed");
});

test("exactly one setup file must match", () => {
  const r = rel("v0.1.0-alpha.5", false, [{ name: "OpenStage-Author-0.1.0-alpha.5-Setup.exe", size: 5 }, { name: "OpenStage-Author-0.1.0-alpha.5-Setup.exe.sha256.txt", size: 1 }]);
  assert.equal(findAsset(r, "^OpenStage-Author.*[Ss]etup\\.exe$", "Author").asset.name, "OpenStage-Author-0.1.0-alpha.5-Setup.exe");
  assert.match(findAsset(r, "^Nothing$", "Author").error, /no setup file matches/);
  assert.match(findAsset(r, "^OpenStage", "Author").error, /more than one/);
});

test("every app's setup pattern matches the file names its releases have used", () => {
  const sources = JSON.parse(readFileSync(new URL("../catalog/apps.json", import.meta.url), "utf8"));
  // As the releases on GitHub named them on 2026-11-15.
  const seen = {
    prismqda: ["PrismQDA_0.1.0-beta.1_x64-setup.exe"],
    prismstatistics: ["PrismStatistics_0.1.0_x64-setup.exe"],
    "prism-reference-manager": ["Prism-Reference-Manager-0.1.2-setup.exe", "Prism-Reference-Manager-0.1.0-setup.exe"],
  };
  const notSetups = ["PrismQDA_0.1.0-beta.1_portable.zip", "Prism-Reference-Manager-0.1.2-portable.zip", "Prism-Reference-Manager-0.1.2-browser-capture.zip"];
  for (const app of sources.apps) for (const other of notSetups) assert.doesNotMatch(other, new RegExp(app.setupPattern), `${app.id}: ${other} is not a setup`);
  for (const app of sources.apps) {
    for (const name of seen[app.id]) assert.match(name, new RegExp(app.setupPattern), app.id);
    assert.doesNotMatch(`${seen[app.id][0]}.sha256.txt`, new RegExp(app.setupPattern), `${app.id}: the checksum file must not match`);
  }
});

test("a release entry points at github.com and keeps Windows' version", () => {
  const r = rel("v0.2.0-alpha.4");
  const e = releaseEntry("jasonswrench-beep/OpenExplainer", r, { name: "OpenExplainer-0.2.0-alpha.4-Setup.exe", size: 15247912 }, "ab".repeat(32));
  assert.equal(e.url, "https://github.com/jasonswrench-beep/OpenExplainer/releases/download/v0.2.0-alpha.4/OpenExplainer-0.2.0-alpha.4-Setup.exe");
  assert.equal(e.windowsVersion, "0.2.0");
  assert.equal(e.version, "0.2.0-alpha.4");
  assert.equal(downloadUrl("o/r", "v1", "A B.exe"), "https://github.com/o/r/releases/download/v1/A%20B.exe");
  assert.equal(stamp(new Date("2026-11-15T18:04:00.123Z")), "2026-11-15T18:04:00Z");
});

test("the change table marks what changed", () => {
  const app = (v) => appEntry({ id: "x", name: "X", mark: {}, repository: "o/x", installedAs: "X", programFile: "x.exe", quietSwitch: ["/P"], expectedSigner: "J" }, null, { version: v, sha256: v }, []);
  const before = { apps: [app("0.1.0-alpha.1")], hub: null, web: [], comingSoon: [] };
  const after = { apps: [app("0.1.0-alpha.2")], hub: null, web: [], comingSoon: [] };
  const rows = changes(before, after);
  assert.ok(rows.find((r) => r.name === "X (preview)").changed);
  assert.ok(!rows.find((r) => r.name === "X (stable)").changed);
  assert.ok(!changes(after, after).some((r) => r.changed));
});

test("a catalog signed with the hub key checks out, and one changed byte does not", () => {
  const { privatePem, publicB64 } = makeKey("a long test password");
  assert.equal(Buffer.from(publicB64, "base64").length, 32);
  assert.equal(publicOf(privatePem, "a long test password"), publicB64);
  assert.throws(() => publicOf(privatePem, "wrong password"));
  assert.match(privatePem, /ENCRYPTED PRIVATE KEY/, "the private half is kept with its password");
  const bytes = Buffer.from('{"format":1}\n');
  const sig = signBytes(bytes, privatePem, "a long test password");
  assert.ok(checkSignature(bytes, sig, publicB64));
  const changed = Buffer.from(bytes);
  changed[2] ^= 1;
  assert.ok(!checkSignature(changed, sig, publicB64));
});
