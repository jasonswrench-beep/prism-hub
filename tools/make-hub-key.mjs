// Makes the hub's catalog key (Make Hub Key.bat runs this) and writes its public half into
// brand/brand.json, so the next build of the hub accepts catalogs signed with it.
//
// The private key goes to %USERPROFILE%\.prism-hub-catalog.key (or PRISM_HUB_KEY), outside the
// repository. The signing tool asks for the password twice; it is never stored.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brandPath = join(root, "brand", "brand.json");
const brand = JSON.parse(readFileSync(brandPath, "utf8"));
const key = process.env.PRISM_HUB_KEY || join(homedir(), ".prism-hub-catalog.key");

if (brand.catalog_public_key) {
  console.log("brand/brand.json already has a catalog key. Making a new one would make every installed hub refuse new catalogs");
  console.log("until it is updated by hand. If that is really wanted, empty catalog_public_key in brand/brand.json first.");
  process.exit(1);
}
if (existsSync(key)) {
  console.log(`A key already exists at ${key}. Its public half is ${key}.pub; copy that into catalog_public_key in brand/brand.json,`);
  console.log("or move the old key away to make a new one.");
  process.exit(1);
}

console.log(`Making the hub key at ${key}. Choose a password you will keep; it is asked for each time the catalog is signed.\n`);
const tauri = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const done = spawnSync(process.execPath, [tauri, "signer", "generate", "-w", key], { stdio: "inherit" });
if (done.status !== 0 || !existsSync(`${key}.pub`)) {
  console.log("\nThe key was not made.");
  process.exit(1);
}
brand.catalog_public_key = readFileSync(`${key}.pub`, "utf8").trim();
writeFileSync(brandPath, JSON.stringify(brand, null, 2) + "\n");
console.log(`\nDone. The public half is now in brand/brand.json. Back up ${key} and its password.`);
console.log("Next: build the hub (Build Prism Hub.bat), then publish the first catalog (Update Prism Catalog.bat).");
