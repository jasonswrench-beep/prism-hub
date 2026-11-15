// SPDX-License-Identifier: Apache-2.0
// Copies the brand file's values (brand/brand.json) into the places Tauri reads them from: src-tauri/tauri.conf.json (product name,
// program name, identifier, publisher, Start menu folder). The Rust code and the window read brand.json directly.
//
//   node tools/apply-brand.mjs           write them
//   node tools/apply-brand.mjs --check   only say whether they already match (the build runs this)
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const confFile = join(root, "src-tauri", "tauri.conf.json");

export function branded(conf, brand) {
  const next = structuredClone(conf);
  next.productName = brand.productName;
  next.mainBinaryName = brand.programName;
  next.identifier = brand.identifier;
  next.bundle.publisher = brand.publisher;
  next.bundle.windows.nsis.startMenuFolder = brand.startMenuFolder;
  return next;
}

export function applyBrand({ check = false } = {}) {
  const brand = JSON.parse(readFileSync(join(root, "brand", "brand.json"), "utf8"));
  const conf = JSON.parse(readFileSync(confFile, "utf8"));
  const next = branded(conf, brand);
  const same = JSON.stringify(next) === JSON.stringify(conf);
  if (!check && !same) writeFileSync(confFile, JSON.stringify(next, null, 2) + "\n");
  return same;
}

if (process.argv[1] && fileURLToPath(import.meta.url).toLowerCase() === resolve(process.argv[1]).toLowerCase()) {
  const check = process.argv.includes("--check");
  const same = applyBrand({ check });
  console.log(same ? "tauri.conf.json matches brand/brand.json." : check ? "tauri.conf.json does not match brand/brand.json." : "tauri.conf.json now matches brand/brand.json.");
  if (check && !same) process.exitCode = 1;
}
