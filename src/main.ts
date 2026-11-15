// SPDX-License-Identifier: Apache-2.0
// The hub's window. It shows what the Rust side reports (the view) and asks it to act; it never reaches the network or the disk itself.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import brand from "../brand/brand.json";
import type { AppView, JobEvent, View } from "./types";
import { appRow, comingRow, statusLine, webRow, type RowState } from "./rows";
import { bindMenuButton, closeAllMenus } from "./menus";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

let view: View | null = null;
/** The last progress event of each running job, by app id. */
const jobs = new Map<string, JobEvent>();
/** The problem sentence of the last failed job, by app id, until the next try. */
const problems = new Map<string, string>();
/** A sentence that replaces the status line once, after a job ends ("OpenExplainer 0.2.0 alpha 5 is installed."). */
let notice: string | null = null;

// ---- Brand ------------------------------------------------------------------------------------------------------------------------------

document.title = brand.productName;
$("product-name").textContent = brand.productName;
$("sibling").textContent = brand.sibling.label;
document.documentElement.style.setProperty("--accent-light", brand.colors.accentLight);
document.documentElement.style.setProperty("--accent-dark", brand.colors.accentDark);

// ---- Speaking to screen readers ---------------------------------------------------------------------------------------------------------

function announce(words: string, urgent = false) {
  const region = $(urgent ? "alert" : "announce");
  region.textContent = "";
  // A new text node after a moment, so the same words twice are still read.
  window.setTimeout(() => (region.textContent = words), 50);
}

// ---- Drawing ---------------------------------------------------------------------------------------------------------------------------

function stateOf(app: AppView): RowState {
  return { job: jobs.get(app.id) ?? null, problem: problems.get(app.id) ?? null, offline: isOffline() };
}

function isOffline(): boolean {
  if (!view) return false;
  return view.fetch.kind === "offline" || view.fetch.kind === "off" || view.keyMissing;
}

function draw() {
  if (!view) return;
  const v = view;
  const focusedId = document.activeElement instanceof HTMLElement ? document.activeElement.id : "";

  drawBanner(v);
  const hideApps = v.needsNewerHub;
  const desktop = $("desktop");
  const kept = new Map([...desktop.children].map((li) => [(li as HTMLElement).dataset.id, li as HTMLElement]));
  syncRows(desktop, hideApps ? [] : v.apps.map((a) => appRow(a, stateOf(a), actions, kept.get(a.id))));
  const empty = $("desktop-empty");
  empty.hidden = hideApps || v.apps.length > 0;
  empty.textContent = emptyText(v);

  $("web").replaceChildren(...(hideApps ? [] : v.web.map((w) => webRow(w, () => act("open_web", { id: w.id })))));
  $("web-section").hidden = hideApps || v.web.length === 0;
  $("soon").replaceChildren(...(hideApps ? [] : v.comingSoon.map(comingRow)));
  $("soon-section").hidden = hideApps || v.comingSoon.length === 0;

  $("status").textContent = notice ?? statusLine(v, Date.now());
  // Drawing replaces the rows; put the focus back on the same control (ids are stable: main-<app>, more-<app>).
  if (focusedId) document.getElementById(focusedId)?.focus();
}

/** Puts `rows` in `list`, leaving each row that is already there untouched (so its focus and an open menu survive). */
function syncRows(list: HTMLElement, rows: HTMLElement[]) {
  const current = [...list.children] as HTMLElement[];
  if (current.length === rows.length && current.every((c, i) => c.dataset.id === rows[i].dataset.id)) {
    rows.forEach((row, i) => current[i] !== row && current[i].replaceWith(row));
  } else {
    list.replaceChildren(...rows);
  }
}

function emptyText(v: View): string {
  if (v.keyMissing) return "This copy of the hub was built without its catalog key, so it cannot list the apps yet.";
  if (v.fetch.kind === "notYet") return "Fetching the list of apps.";
  if (v.fetch.kind === "off") return "Update checks are turned off on this computer, and no list of apps has been fetched yet.";
  if (v.fetch.kind === "offline") return `Offline. Connect to the internet once to see the ${brand.shortName} apps.`;
  if (v.fetch.kind === "refused") return v.fetch.message;
  return "No apps are listed.";
}

function drawBanner(v: View) {
  const banner = $("banner");
  banner.replaceChildren();
  const text = document.createElement("p");
  if (v.hubUpdate || v.needsNewerHub) {
    const job = jobs.get("hub");
    text.textContent = job
      ? `${job.message}${job.total ? ` ${Math.floor((100 * job.received) / job.total)}%` : ""}`
      : v.needsNewerHub && !v.hubUpdate
        ? "The list of apps needs a newer hub. Download it from the hub's releases page."
        : v.needsNewerHub
          ? "The list of apps needs a newer hub. Update the hub to see the apps."
          : "A new version of the hub is ready.";
    banner.append(text);
    const button = document.createElement("button");
    button.type = "button";
    button.className = "primary";
    if (v.hubUpdate) {
      button.textContent = job ? "Cancel" : "Update";
      button.setAttribute("aria-label", job ? "Cancel the hub's update" : `Update ${brand.productName}`);
      button.addEventListener("click", () => (job ? act("cancel_job", { id: "hub" }) : act("update_hub")));
    } else {
      button.textContent = "Releases page";
      button.addEventListener("click", () => act("open_link", { which: "releases" }));
    }
    banner.append(button);
    banner.hidden = false;
  } else if (v.justUpdated) {
    text.textContent = `The hub was updated to ${v.justUpdated}.`;
    banner.append(text);
    banner.hidden = false;
  } else {
    banner.hidden = true;
  }
}

// ---- Acting ----------------------------------------------------------------------------------------------------------------------------

async function act(command: string, args: Record<string, unknown> = {}) {
  try {
    const result = await invoke(command, args);
    if (result && typeof result === "object" && "apps" in (result as object)) {
      view = result as View;
      draw();
    }
  } catch (e) {
    const words = String(e);
    announce(words, true);
    notice = words;
    draw();
  }
}

function confirmUninstall(app: AppView) {
  const dialog = $<HTMLDialogElement>("confirm-dialog");
  $("confirm-title").textContent = `Uninstall ${app.name}?`;
  $("confirm-text").textContent = `This runs ${app.name}'s own uninstaller. Your documents and projects stay where they are.`;
  $("confirm-yes").textContent = "Uninstall";
  $("confirm-yes").setAttribute("aria-label", `Uninstall ${app.name}`);
  const opener = document.activeElement as HTMLElement | null;
  dialog.returnValue = "";
  dialog.onclose = () => {
    opener?.focus();
    if (dialog.returnValue === "yes") start(app.id, "uninstall");
  };
  dialog.showModal();
  $("confirm-no").focus();
}

function start(id: string, command: "start_job" | "uninstall", extra: Record<string, unknown> = {}) {
  problems.delete(id);
  notice = null;
  void act(command, { id, ...extra });
}

const actions = {
  install: (a: AppView) => start(a.id, "start_job"),
  cancel: (a: AppView) => act("cancel_job", { id: a.id }),
  open: (a: AppView) => act("open_app", { id: a.id }),
  whatsNew: (a: AppView) => act("open_release_page", { id: a.id }),
  folder: (a: AppView) => act("open_folder", { id: a.id }),
  uninstall: confirmUninstall,
  download: (a: AppView, index: number) => {
    problems.delete(a.id);
    notice = null;
    void act("download_extra", { id: a.id, index });
  },
};

async function refresh() {
  notice = null;
  if (view) {
    view = { ...view, fetch: { kind: "notYet" } };
    $("status").textContent = "Checking for updates";
  }
  try {
    view = await invoke<View>("refresh");
  } catch (e) {
    announce(String(e), true);
  }
  draw();
  if (view) announce(statusLine(view, Date.now()));
}

// ---- Dialogs ---------------------------------------------------------------------------------------------------------------------------

function openSettings() {
  if (!view) return;
  const dialog = $<HTMLDialogElement>("settings-dialog");
  const v = view;
  const values: Record<string, boolean> = {
    offerPreviews: v.settings.offerPreviews,
    startWithWindows: v.startWithWindows,
    keepInTray: v.settings.keepInTray,
    dailyCheck: v.settings.dailyCheck,
  };
  const locked: Record<string, boolean> = { offerPreviews: !v.policy.previews, startWithWindows: !v.policy.startWithWindows, dailyCheck: !v.policy.checks };
  for (const box of dialog.querySelectorAll<HTMLInputElement>("input[data-setting]")) {
    const name = box.dataset.setting!;
    box.checked = values[name] && !locked[name];
    box.disabled = !!locked[name];
  }
  const notes = [
    !v.policy.previews && "Preview versions are turned off on this computer.",
    !v.policy.startWithWindows && "Start with Windows is turned off on this computer.",
    !v.policy.checks && "Update checks are turned off on this computer.",
  ].filter(Boolean);
  const note = $("policy-note");
  note.hidden = notes.length === 0;
  note.textContent = notes.length ? `${notes.join(" ")} Whoever looks after it set this.` : "";
  const opener = document.activeElement as HTMLElement | null;
  dialog.onclose = () => opener?.focus();
  if (!dialog.open) dialog.showModal();
}

for (const box of document.querySelectorAll<HTMLInputElement>("#settings-dialog input[data-setting]")) {
  box.addEventListener("change", async () => {
    try {
      view = await invoke<View>("set_setting", { name: box.dataset.setting, value: box.checked });
      draw();
      // Start with Windows also turns on the tray; show that at once.
      const tray = document.getElementById("set-keepInTray") as HTMLInputElement;
      tray.checked = view.settings.keepInTray;
    } catch (e) {
      box.checked = !box.checked;
      announce(String(e), true);
    }
  });
}

function openAbout() {
  if (!view) return;
  $("about-version").textContent = view.hubVersion;
  $("about-catalog").textContent = view.catalogDate ? new Date(view.catalogDate).toLocaleString() : "Not fetched yet";
  const dialog = $<HTMLDialogElement>("about-dialog");
  const opener = document.activeElement as HTMLElement | null;
  dialog.onclose = () => opener?.focus();
  dialog.showModal();
}

$("settings-about").addEventListener("click", () => {
  $<HTMLDialogElement>("settings-dialog").close();
  openAbout();
});
$("about-repository").addEventListener("click", () => act("open_link", { which: "repository" }));
$("sibling").addEventListener("click", () => act("open_link", { which: "sibling" }));

function askFirstRun() {
  if (!view || view.settings.askedStartWithWindows || !view.policy.startWithWindows) return;
  const dialog = $<HTMLDialogElement>("first-run-dialog");
  dialog.returnValue = "";
  dialog.onclose = async () => {
    try {
      view = await invoke<View>("answer_first_run", { start: dialog.returnValue === "yes" });
      draw();
    } catch (e) {
      announce(String(e), true);
    }
  };
  dialog.showModal();
}

// ---- The settings menu -----------------------------------------------------------------------------------------------------------------

bindMenuButton($("menu-button"), $("main-menu"), (action) => {
  if (action === "check") void refresh();
  if (action === "settings") openSettings();
  if (action === "about") openAbout();
  if (action === "quit") void invoke("quit_hub");
});

document.addEventListener("click", (e) => {
  if (!(e.target as HTMLElement).closest(".menu-wrap, .row-actions")) closeAllMenus();
});

// ---- Events from the Rust side ---------------------------------------------------------------------------------------------------------

const SPOKEN: Record<string, string> = {
  downloading: "Downloading",
  checking: "Checking",
  waiting: "Waiting",
  installing: "Installing",
  removing: "Removing",
};

void listen<JobEvent>("job", (e) => {
  const job = e.payload;
  const name = job.id === "hub" ? brand.productName : (view?.apps.find((a) => a.id === job.id)?.name ?? job.id);
  const before = jobs.get(job.id);
  if (job.stage === "done" || job.stage === "failed" || job.stage === "cancelled") {
    jobs.delete(job.id);
    if (job.stage === "failed") problems.set(job.id, job.message);
    notice = job.stage === "failed" ? null : job.message;
    announce(job.stage === "failed" ? `${name}: ${job.message}` : job.message, job.stage === "failed");
  } else {
    jobs.set(job.id, job);
    // Each new stage is announced once (not every percent).
    if (before?.stage !== job.stage) announce(job.stage === "waiting" ? job.message : `${SPOKEN[job.stage] ?? job.stage} ${name}`);
  }
  draw();
});

void listen<View>("view", (e) => {
  view = e.payload;
  draw();
});

void listen("show-settings", () => openSettings());
void listen("check-now", () => void refresh());

// The status line says "10 minutes ago": it is brought up to date when the window comes back into view, not by a timer.
document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && draw());
window.addEventListener("focus", () => draw());

// ---- Start: show what is known at once (from the last catalog on disk), then fetch the catalog -----------------------------------------

async function main() {
  try {
    view = await invoke<View>("get_view");
    draw();
  } catch (e) {
    announce(String(e), true);
  }
  if (await invoke<boolean>("take_pending_settings").catch(() => false)) openSettings();
  await refresh();
  askFirstRun();
}

void main();
