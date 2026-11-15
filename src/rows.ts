// SPDX-License-Identifier: Apache-2.0
// One row per app, web link and coming app. Each desktop row has one main button whose label depends on the app's state, and a three-dot
// menu for everything else. Every button names its app for screen readers ("Update OpenStage Author"); the round mark is decorative.
import type { AppView, JobEvent, View } from "./types";
import { bindMenuButton } from "./menus";

export interface RowState {
  job: JobEvent | null;
  problem: string | null;
  offline: boolean;
}

export interface RowActions {
  install(a: AppView): void;
  cancel(a: AppView): void;
  open(a: AppView): void;
  whatsNew(a: AppView): void;
  folder(a: AppView): void;
  uninstall(a: AppView): void;
  download(a: AppView, index: number): void;
}

/** 0.2.0 alpha 4 for 0.2.0-alpha.4. */
export function spoken(version: string): string {
  const [main, pre] = version.replace(/^v/, "").split(/-(.*)/s);
  return pre ? `${main} ${pre.replace(/\./g, " ")}` : main;
}

export function size(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(bytes >= 100 * 1024 * 1024 ? 0 : 1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function ago(seconds: number, now: number): string {
  const minutes = Math.floor((now / 1000 - seconds) / 60);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.floor(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}

/** The thin line at the bottom of the window. */
export function statusLine(v: View, now: number): string {
  if (v.keyMissing) return "This copy has no catalog key, so it shows only what is installed";
  switch (v.fetch.kind) {
    case "notYet":
      return "Checking for updates";
    case "off":
      return "Update checks are turned off on this computer";
    case "offline":
      return "Offline, showing what is installed";
    case "refused":
      return v.fetch.message;
  }
  const ready = v.updatesReady + (v.hubUpdate ? 1 : 0);
  if (ready > 0) return `${ready} update${ready === 1 ? "" : "s"} ready`;
  return v.lastCheck ? `Checked for updates ${ago(v.lastCheck, now)}` : "Checked for updates";
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

/** The app's own icon when the catalog carries one, otherwise the round mark: letters on the app's colour. Decorative either way: the name is
 *  always text beside it. The icon is an image from the signed catalog (a data: address), so it runs nothing and loads nothing. */
function mark(letters: string, color?: string, icon?: string | null): HTMLElement {
  if (icon && /^data:image\/(svg\+xml|png);base64,/.test(icon)) {
    const img = el("img", "mark icon");
    img.src = icon;
    img.alt = "";
    img.setAttribute("aria-hidden", "true");
    return img;
  }
  const m = el("span", color ? "mark" : "mark web", letters);
  m.setAttribute("aria-hidden", "true");
  if (color) m.style.setProperty("--mark", color);
  return m;
}

function previewTag(): HTMLElement {
  return el("span", "tag", "Preview");
}

const BUSY_WORDS: Record<string, string> = {
  downloading: "Downloading",
  checking: "Checking",
  waiting: "Waiting for you to close it",
  installing: "Installing",
  removing: "Removing",
};

/** The words under the name while a job runs. */
function progressWords(job: JobEvent): string {
  if (job.stage === "downloading" && job.total > 0) return `Downloading ${Math.floor((100 * job.received) / job.total)}%`;
  if (job.stage === "waiting") return job.message;
  return BUSY_WORDS[job.stage] ?? job.message;
}

/** A row's identity for redrawing: when it is unchanged apart from download progress, the row is kept and only its bar moves. */
function signature(a: AppView, s: RowState): string {
  return JSON.stringify([a, s.problem, s.offline, s.job ? s.job.stage : null]);
}

/** Moves a kept row's progress bar and words without redrawing it (so an open menu stays open). */
export function patchProgress(row: HTMLElement, job: JobEvent | null) {
  if (!job) return;
  const bar = row.querySelector("progress");
  const words = row.querySelector(".detail");
  if (bar && job.total > 0) bar.value = job.received / job.total;
  if (words) words.textContent = progressWords(job);
}

export function appRow(a: AppView, s: RowState, act: RowActions, keep?: HTMLElement | null): HTMLElement {
  const sig = signature(a, s);
  if (keep && keep.dataset.sig === sig) {
    patchProgress(keep, s.job);
    return keep;
  }
  const row = el("li", "row");
  row.dataset.sig = sig;
  row.dataset.id = a.id;
  const info = el("div", "info");
  const name = el("span", "name", a.name);
  const detail = el("span", "detail");
  info.append(name, detail);

  const main = el("button", "main-button");
  main.type = "button";
  main.id = `main-${a.id}`;
  let mainAction: (() => void) | null = null;
  const installed = a.installed;
  const installedText = installed ? spoken(installed.version) : "";

  if (s.job) {
    const cancellable = s.job.stage !== "installing" && s.job.stage !== "removing";
    main.textContent = "Cancel";
    main.setAttribute("aria-label", cancellable ? `Cancel for ${a.name}` : `Cancel is not possible while the installer runs (${a.name})`);
    if (cancellable) mainAction = () => act.cancel(a);
    else main.setAttribute("aria-disabled", "true");
    detail.textContent = progressWords(s.job);
    const bar = el("progress");
    bar.setAttribute("aria-label", `${a.name}: ${BUSY_WORDS[s.job.stage] ?? "working"}`);
    if (s.job.stage === "downloading" && s.job.total > 0) bar.value = s.job.received / s.job.total;
    else bar.removeAttribute("value");
    info.append(bar);
  } else if (s.problem) {
    main.textContent = "Retry";
    main.setAttribute("aria-label", `Retry ${a.name}`);
    mainAction = () => act.install(a);
    detail.textContent = s.problem;
    detail.classList.add("problem");
  } else if (a.needsNewerHub) {
    detail.textContent = installed ? `${installedText}. Needs a newer hub to update` : "Needs a newer hub";
  } else if (installed && a.update && a.offer) {
    main.textContent = s.offline ? "Offline" : "Update";
    detail.textContent = `${installedText}, ${spoken(a.offer.version)} available${a.running ? " (open now)" : ""}`;
    if (a.offer.preview) detail.append(previewTag());
    if (s.offline) {
      main.setAttribute("aria-disabled", "true");
      main.setAttribute("aria-label", `Update ${a.name}: offline`);
    } else {
      main.setAttribute("aria-label", `Update ${a.name}`);
      mainAction = () => act.install(a);
    }
  } else if (installed) {
    main.textContent = "Open";
    main.classList.add("quiet");
    main.setAttribute("aria-label", `Open ${a.name}`);
    mainAction = () => act.open(a);
    detail.textContent = installed.exact ? installedText : `${installedText} installed`;
    if (installed.exact && /-/.test(installed.version)) detail.append(previewTag());
  } else if (a.offer) {
    main.textContent = s.offline ? "Offline" : "Install";
    detail.textContent = `${spoken(a.offer.version)}, ${size(a.offer.size)}`;
    if (a.offer.preview) detail.append(previewTag());
    if (s.offline) {
      main.setAttribute("aria-disabled", "true");
      main.setAttribute("aria-label", `Install ${a.name}: offline`);
    } else {
      main.setAttribute("aria-label", `Install ${a.name}`);
      mainAction = () => act.install(a);
    }
  } else if (a.previewBlocked) {
    detail.textContent = "Only a preview exists, and previews are turned off on this computer";
  } else {
    detail.textContent = "No release yet";
  }
  main.addEventListener("click", () => mainAction?.());

  const actions = el("div", "row-actions");
  if (main.textContent) actions.append(main);
  actions.append(...moreMenu(a, s, act));
  row.append(mark(a.letters, a.color, a.icon), info, actions);
  return row;
}

function moreMenu(a: AppView, s: RowState, act: RowActions): HTMLElement[] {
  const button = el("button", "more-button", "⋯");
  button.type = "button";
  button.id = `more-${a.id}`;
  button.setAttribute("aria-label", `More for ${a.name}`);
  button.setAttribute("aria-haspopup", "menu");
  button.setAttribute("aria-expanded", "false");
  const menu = el("ul", "menu");
  menu.id = `menu-${a.id}`;
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", `More for ${a.name}`);
  menu.hidden = true;
  button.setAttribute("aria-controls", menu.id);

  const busy = !!s.job;
  const entries: [string, string, boolean][] = [];
  const installed = !!a.installed;
  if (installed) entries.push(["open", `Open`, a.installed!.folderKnown]);
  entries.push(["whatsNew", "What's new", true]);
  if (installed) entries.push(["folder", "Open install folder", a.installed!.folderKnown]);
  if (installed && a.offer && !a.needsNewerHub) {
    const exactSame = a.installed!.exact && a.installed!.version === a.offer.version;
    const label = a.update || exactSame ? "Reinstall" : `Reinstall as ${spoken(a.offer.version)}`;
    entries.push(["reinstall", label, !busy && !s.offline]);
  }
  if (installed) entries.push(["uninstall", "Uninstall", !busy]);
  for (const x of a.extras) entries.push([`extra:${x.index}`, `Download ${x.name} (${size(x.size)})`, !busy && !s.offline]);

  for (const [action, label, enabled] of entries) {
    const li = el("li");
    li.setAttribute("role", "none");
    const item = el("button", "", label);
    item.type = "button";
    item.setAttribute("role", "menuitem");
    item.dataset.action = action;
    item.disabled = !enabled;
    li.append(item);
    menu.append(li);
  }
  bindMenuButton(button, menu, (action) => {
    if (action === "open") act.open(a);
    else if (action === "whatsNew") act.whatsNew(a);
    else if (action === "folder") act.folder(a);
    else if (action === "reinstall") act.install(a);
    else if (action === "uninstall") act.uninstall(a);
    else if (action.startsWith("extra:")) act.download(a, Number(action.slice(6)));
  });
  return [button, menu];
}

export function webRow(w: { id: string; name: string; description: string }, launch: () => void): HTMLElement {
  const row = el("li", "row");
  const info = el("div", "info");
  info.append(el("span", "name", w.name));
  if (w.description) info.append(el("span", "detail", w.description));
  const button = el("button", "main-button quiet", "Launch");
  button.type = "button";
  button.id = `web-${w.id}`;
  button.setAttribute("aria-label", `Launch ${w.name} in your browser`);
  button.addEventListener("click", launch);
  const actions = el("div", "row-actions");
  actions.append(button);
  row.append(mark(w.name.replace(/[^A-Za-z]/g, "").slice(0, 2).toLowerCase()), info, actions);
  return row;
}

export function comingRow(c: { id: string; name: string; description: string; mark?: { letters: string; color: string; icon?: string | null } | null }): HTMLElement {
  const row = el("li", "row");
  const info = el("div", "info");
  info.append(el("span", "name", c.name));
  if (c.description) info.append(el("span", "detail", c.description));
  const m = c.mark ? mark(c.mark.letters, c.mark.color, c.mark.icon) : mark(c.name.replace(/[^A-Za-z]/g, "").slice(0, 2).toLowerCase());
  row.append(m, info);
  return row;
}
