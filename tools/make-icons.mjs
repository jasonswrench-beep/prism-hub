// SPDX-License-Identifier: Apache-2.0
// Makes every icon the hub needs from the brand's SVG (brand/brand.json names it):
//   src-tauri/icons/*            the program's and the installer's icons (Tauri's own icon command makes them from a 1024 picture)
//   src-tauri/icons/tray.png     the tray icon
//   src-tauri/icons/tray-dot.png the tray icon with the small dot that means an update is ready
//   public/mark.svg              the mark in the window's header
//
// The SVG is drawn in headless Microsoft Edge (the same way as OpenStage Author's tools/svg_to_png.mjs, credited in NOTICE), so this needs Edge
// and Node. Run it after changing the brand's icon: node tools/make-icons.mjs
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brand = JSON.parse(readFileSync(join(root, "brand", "brand.json"), "utf8"));
const iconsDir = join(root, "src-tauri", "icons");
const EDGE = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The SVG with a dot in the lower right corner: the brand's colour, ringed in white so it shows on a dark or a light taskbar. */
function withDot(svg) {
  const dot = `<circle cx="800" cy="800" r="200" fill="#FFFFFF"/><circle cx="800" cy="800" r="150" fill="${brand.colors.brand}"/>`;
  return svg.replace(/<\/svg>\s*$/, `${dot}</svg>`);
}

async function draw(pages) {
  if (!EDGE) throw new Error("Microsoft Edge was not found, so the icon could not be drawn.");
  const port = 9461;
  const profile = mkdtempSync(join(tmpdir(), "hub-icons-edge-"));
  const edge = spawn(EDGE, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--disable-gpu", "--hide-scrollbars", "about:blank"], { stdio: "ignore" });
  try {
    let targets = null;
    for (let i = 0; i < 60 && !targets; i++) {
      try {
        targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      } catch {
        await sleep(250);
      }
    }
    const page = targets.find((t) => t.type === "page");
    const ws = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((ok, bad) => { ws.onopen = ok; ws.onerror = bad; });
    let id = 0;
    const pending = new Map();
    ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
    const send = (method, params = {}) => new Promise((ok) => { const n = ++id; pending.set(n, ok); ws.send(JSON.stringify({ id: n, method, params })); });
    await send("Page.enable");
    await send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } });
    for (const { svg, size, out } of pages) {
      await send("Emulation.setDeviceMetricsOverride", { width: size, height: size, deviceScaleFactor: 1, mobile: false });
      const html = `<!doctype html><html><head><style>html,body{margin:0;padding:0;background:transparent}svg{display:block;width:${size}px;height:${size}px}</style></head><body>${svg.replace(/<\?xml[^>]*\?>/, "")}</body></html>`;
      await send("Page.navigate", { url: "data:text/html;charset=utf-8," + encodeURIComponent(html) });
      await sleep(700);
      const shot = await send("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: size, height: size, scale: 1 }, fromSurface: true });
      writeFileSync(out, Buffer.from(shot.result.data, "base64"));
      console.log(`  drew ${out} (${size} by ${size})`);
    }
    ws.close();
  } finally {
    edge.kill();
    await sleep(300);
    rmSync(profile, { recursive: true, force: true });
  }
}

const svg = readFileSync(join(root, brand.icon), "utf8");
const work = mkdtempSync(join(tmpdir(), "hub-icons-"));
mkdirSync(iconsDir, { recursive: true });
mkdirSync(join(root, "public"), { recursive: true });
const big = join(work, "icon-1024.png");
await draw([
  { svg, size: 1024, out: big },
  { svg, size: 64, out: join(iconsDir, "tray.png") },
  { svg: withDot(svg), size: 64, out: join(iconsDir, "tray-dot.png") },
]);

// Tauri's icon command makes the .ico and the square pictures; only the ones tauri.conf.json names are kept.
const made = join(work, "tauri");
const done = spawnSync(process.execPath, [join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"), "icon", big, "-o", made], { stdio: "inherit" });
if (done.status !== 0) throw new Error("Tauri's icon command failed.");
for (const name of ["32x32.png", "128x128.png", "128x128@2x.png", "icon.ico", "icon.png"]) copyFileSync(join(made, name), join(iconsDir, name));
writeFileSync(join(root, "public", "mark.svg"), svg);
rmSync(work, { recursive: true, force: true });
console.log(`Icons made from ${brand.icon}: ${readdirSync(iconsDir).join(", ")}`);
