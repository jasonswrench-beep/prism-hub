// SPDX-License-Identifier: Apache-2.0
// Makes the hub key: a new Ed25519 key pair, separate from the apps' update keys, that signs the catalog and the hub's own updates.
//   - The private half is saved with your password, OUTSIDE the repository (by default in "<product name> key" in your user folder, for example
//     "Prism Hub key"; set HUB_KEY to put it somewhere else). It never goes on GitHub. Keep a backup copy somewhere safe, with the password.
//   - The public half is written into brand/brand.json (catalogPublicKey), so it is built into every hub from now on.
// It refuses to replace a key that already exists: a new key would mean every hub already installed stops trusting the catalog.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { makeKey, publicOf } from "./catalog-lib.mjs";
import { askSecret } from "./prompt.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const readBrand = () => JSON.parse(readFileSync(join(root, "brand", "brand.json"), "utf8"));
export const keyFile = () => {
  const b = readBrand();
  return process.env.HUB_KEY || join(homedir(), `${b.productName} key`, `${b.programName}-catalog-key.pem`);
};

async function main() {
  const file = keyFile();
  const brandFile = join(root, "brand", "brand.json");
  const brand = JSON.parse(readFileSync(brandFile, "utf8"));
  if (existsSync(file)) {
    console.log(`A hub key is already at:\n  ${file}\nNothing was changed. (A new key would stop every installed hub from trusting the catalog.)`);
    return;
  }
  if (brand.catalogPublicKey) {
    console.log("brand/brand.json already names a hub key, but its private half is not at");
    console.log(`  ${file}`);
    console.log("If you have it elsewhere, set HUB_KEY to its file. Nothing was changed.");
    process.exitCode = 1;
    return;
  }
  console.log("Choose a password for the hub key. You will need it each time the catalog is updated. It is not shown as you type.");
  const one = await askSecret("Password: ");
  if (one.length < 10) {
    console.log("Please use at least 10 characters. Nothing was made.");
    process.exitCode = 1;
    return;
  }
  const two = await askSecret("The same password again: ");
  if (one !== two) {
    console.log("The two did not match. Nothing was made.");
    process.exitCode = 1;
    return;
  }
  const { privatePem, publicB64 } = makeKey(one);
  if (publicOf(privatePem, one) !== publicB64) throw new Error("The new key did not check out.");
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, privatePem, { mode: 0o600 });
  writeFileSync(
    join(dirname(file), "README.txt"),
    `This folder holds the private half of the ${brand.productName} catalog key, protected by your password.\r\n` +
      "Keep a backup copy of this folder somewhere safe (not on GitHub). Without it and its password, a new catalog cannot be published\r\n" +
      "for the hubs already installed, and they would need a new hub release with a new key.\r\n",
  );
  brand.catalogPublicKey = publicB64;
  writeFileSync(brandFile, JSON.stringify(brand, null, 2) + "\n");
  console.log(`\nDone.\n  Private half (keep it, back it up, never upload it): ${file}`);
  console.log(`  Public half, now in brand/brand.json: ${publicB64}`);
  console.log("\nCommit brand/brand.json. Hubs built from now on trust catalogs signed with this key.");
}

if (process.argv[1] && fileURLToPath(import.meta.url).toLowerCase() === resolve(process.argv[1]).toLowerCase()) {
  main().catch((e) => {
    console.error(`STOPPED: ${e.message}`);
    process.exitCode = 1;
  });
}
