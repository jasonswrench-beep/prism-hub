// Writes every file that carries a brand value from brand/brand.json, so the brand lives in one
// place: src-tauri/tauri.conf.json, src-tauri/installer-hooks.nsh, ui/brand/mark.svg and the
// program icons (made from brand/<icon> by `tauri icon` when the picture changed).
//
//   node tools/apply-brand.mjs          write them
//   node tools/apply-brand.mjs --check  only say whether they are up to date (exit 1 if not)

import { readFileSync, writeFileSync, existsSync, mkdirSync, statSync, readdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brand = JSON.parse(readFileSync(join(root, "brand", "brand.json"), "utf8"));
const check = process.argv.includes("--check");
let stale = [];

const csp = [
  "default-src 'self' ipc: http://ipc.localhost",
  "base-uri 'none'",
  "object-src 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
  "img-src 'self' data:",
  "style-src 'self'",
  "script-src 'self'",
  "connect-src ipc: http://ipc.localhost",
  "font-src 'self'",
].join("; ");

const conf = {
  $schema: "https://schema.tauri.app/config/2",
  productName: brand.product_name,
  mainBinaryName: brand.binary_name,
  identifier: brand.identifier,
  build: {
    frontendDist: "../ui",
    beforeDevCommand: "npm run ui",
    beforeBuildCommand: "npm run ui",
  },
  app: {
    withGlobalTauri: true,
    windows: [],
    security: { csp },
  },
  bundle: {
    active: true,
    targets: ["nsis"],
    icon: ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"],
    publisher: brand.publisher,
    copyright: brand.copyright,
    category: "Utility",
    shortDescription: brand.short_description,
    longDescription: brand.long_description,
    resources: {
      "../LICENSE": "LICENSE.txt",
      "../NOTICE": "NOTICE.txt",
      "../THIRD-PARTY.md": "THIRD-PARTY.md",
    },
    windows: {
      webviewInstallMode: { type: "downloadBootstrapper", silent: true },
      nsis: {
        installMode: "currentUser",
        languages: ["English"],
        displayLanguageSelector: false,
        installerIcon: "icons/icon.ico",
        startMenuFolder: brand.start_menu_folder,
        installerHooks: "./installer-hooks.nsh",
      },
    },
  },
};

const hooks = `; Generated from brand/brand.json by tools/apply-brand.mjs. Edit brand.json, not this file.
;
; Uninstalling the hub removes the hub, its start-up entry, its cached catalog, its download
; folder and its settings. It never removes the apps it installed: they stay in Windows'
; installed apps list and keep working.

!macro NSIS_HOOK_POSTUNINSTALL
  \${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Run" "${brand.product_name}"
    SetShellVarContext current
    RmDir /r "$LOCALAPPDATA\\${brand.identifier}"
  \${EndIf}
!macroend
`;

function put(rel, text) {
  const p = join(root, rel);
  const old = existsSync(p) ? readFileSync(p, "utf8") : null;
  if (old === text) return;
  if (check) {
    stale.push(rel);
    return;
  }
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, text);
  console.log(`wrote ${rel}`);
}

put("src-tauri/tauri.conf.json", JSON.stringify(conf, null, 2) + "\n");
put("src-tauri/installer-hooks.nsh", hooks);

const svg = join(root, "brand", brand.icon);
put("ui/brand/mark.svg", readFileSync(svg, "utf8"));

// Program icons: remade only when the picture is newer than the icons.
const ico = join(root, "src-tauri", "icons", "icon.ico");
if (!existsSync(ico) || statSync(svg).mtimeMs > statSync(ico).mtimeMs) {
  if (check) {
    stale.push("src-tauri/icons");
  } else {
    const icons = join(root, "src-tauri", "icons");
    const tauri = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
    execFileSync(process.execPath, [tauri, "icon", svg, "-o", icons], { stdio: "inherit" });
    // Keep only the Windows icons this program uses.
    const keep = new Set(["32x32.png", "128x128.png", "128x128@2x.png", "icon.ico", "icon.png"]);
    for (const name of readdirSync(icons)) {
      if (!keep.has(name)) rmSync(join(icons, name), { recursive: true, force: true });
    }
  }
}

if (check) {
  if (stale.length) {
    console.log("Out of date with brand/brand.json: " + stale.join(", ") + ". Run: npm run brand");
    process.exit(1);
  }
  console.log("Brand files are up to date.");
}
