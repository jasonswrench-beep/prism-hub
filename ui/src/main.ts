// Prism Hub window. All state lives in the hub (Rust); this file draws the view it sends and
// passes clicks back. Re-drawing keeps keyboard focus on the same control.

import { invoke, listen, type AppRow, type JobView, type SimpleRow, type View } from "./types.js";
import { $, ago, announce, ask, closeMenu, el, menuButton, menuIsOpen, size, type MenuItem } from "./dom.js";

let view: View | null = null;
let pending = false;
let page: "home" | "settings" = location.hash === "#settings" ? "settings" : "home";
/** The last action per app, so Retry repeats it. */
const lastAction = new Map<string, "install" | "uninstall" | "reinstall" | "extra">();
/** The stage last read out per app, so each change is announced once. */
const lastStage = new Map<string, string>();

async function refresh(): Promise<void> {
  view = await invoke<View>("get_view");
  // Never redraw under an open menu: it would close in the person's hands.
  if (menuIsOpen()) {
    pending = true;
    return;
  }
  render();
}

document.addEventListener("menuclosed", () => {
  if (pending) {
    pending = false;
    render();
  }
});

function fail(e: unknown): void {
  const msg = typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong.";
  void ask("That did not work", msg, [{ label: "OK", primary: true, value: "ok" }]);
}

function act(cmd: string, args: Record<string, unknown> = {}): void {
  invoke(cmd, args).catch(fail);
}

// ------------------------------------------------------------------------------------------
// Rows

function stageText(row: AppRow | null, job: JobView): string {
  const name = row?.name ?? "the hub";
  switch (job.stage) {
    case "downloading":
      return job.total ? `Downloading ${Math.floor((job.received / job.total) * 100)}%` : "Downloading";
    case "checking":
      return "Checking";
    case "waiting":
      return `Close ${name} to continue. The ${job.kind === "uninstall" ? "uninstall" : "update"} starts when it closes.`;
    case "installing":
      return "Installing";
    case "uninstalling":
      return "Uninstalling";
  }
}

function markEl(letters: string, color: string): HTMLElement {
  const m = el("span", { class: "mark", "aria-hidden": "true", text: letters });
  m.style.setProperty("background", color);
  return m;
}

function versionLine(r: AppRow): string {
  if (r.state === "unavailable") return r.unavailable ?? "";
  const parts: string[] = [];
  if (r.installed_version !== null) {
    if (r.state === "update" && r.offer) parts.push(`${r.installed_version || "Installed"} · ${r.offer.version} available`);
    else parts.push(`Installed ${r.installed_version || ""}`.trim());
  } else if (r.offer) {
    parts.push(`${r.offer.version} · ${size(r.offer.size)} download`);
  }
  if (r.blocked && (r.state === "not_installed" || r.state === "update")) parts.push(r.blocked);
  return parts.join(" · ");
}

function mainButton(r: AppRow): HTMLButtonElement | null {
  const b = (label: string, aria: string, run: () => void, disabled = false) => {
    const btn = el("button", { type: "button", class: "btn action", "aria-label": aria, "data-key": `main:${r.id}`, text: label, disabled });
    btn.addEventListener("click", run);
    return btn;
  };
  switch (r.state) {
    case "not_installed":
      return b("Install", `Install ${r.name}`, () => install(r), !!r.blocked);
    case "update":
      return b("Update", `Update ${r.name}`, () => install(r), !!r.blocked);
    case "current":
      return r.can_open ? b("Open", `Open ${r.name}`, () => act("open_app", { id: r.id })) : null;
    case "busy": {
      const can = !!r.job?.cancellable;
      const doing = { install: "installing", update: "updating", reinstall: "reinstalling", uninstall: "uninstalling", extra: "the download of", hub: "updating" }[r.job?.kind ?? "install"];
      return b("Cancel", can ? `Cancel ${doing} ${r.name}` : `${r.name} is being changed and cannot be stopped now`, () => act("cancel", { id: r.id }), !can);
    }
    case "problem":
      return b("Retry", `Retry ${r.name}`, () => retry(r), !!r.blocked && lastAction.get(r.id) !== "uninstall");
    case "unavailable":
      return null;
  }
}

function install(r: AppRow): void {
  lastAction.set(r.id, "install");
  act("install", { id: r.id });
}

function retry(r: AppRow): void {
  const a = lastAction.get(r.id) ?? "install";
  if (a === "uninstall") act("uninstall", { id: r.id });
  else if (a === "reinstall") act("reinstall", { id: r.id });
  else if (a === "extra") act("dismiss_problem", { id: r.id });
  else act("install", { id: r.id });
}

async function confirmUninstall(r: AppRow): Promise<void> {
  const answer = await ask(`Uninstall ${r.name}?`, `This runs ${r.name}'s own uninstaller. Its projects, libraries and data stay where they are, and you can install it again at any time.`, [
    { label: "Cancel", value: "no" },
    { label: "Uninstall", primary: true, value: "yes" },
  ]);
  if (answer === "yes") {
    lastAction.set(r.id, "uninstall");
    act("uninstall", { id: r.id });
  }
}

function rowMenu(r: AppRow): MenuItem[] {
  const items: MenuItem[] = [];
  const busy = r.state === "busy";
  if (r.can_open && r.state !== "current") items.push({ label: `Open ${r.name}`, run: () => act("open_app", { id: r.id }) });
  if (r.whats_new) items.push({ label: "What's new", run: () => act("whats_new", { id: r.id }) });
  if (r.can_folder) items.push({ label: "Open install folder", run: () => act("open_folder", { id: r.id }) });
  if (r.can_reinstall && !busy && !r.blocked)
    items.push({
      label: "Reinstall",
      run: () => {
        lastAction.set(r.id, "reinstall");
        act("reinstall", { id: r.id });
      },
    });
  if (r.can_uninstall && !busy) items.push({ label: "Uninstall…", run: () => void confirmUninstall(r) });
  r.extras.forEach((x, i) => {
    if (!busy && !r.blocked)
      items.push({
        label: `Download ${x.label} (${size(x.size)})`,
        separatorBefore: i === 0,
        run: () => {
          lastAction.set(r.id, "extra");
          act("download_extra", { id: r.id, index: i });
        },
      });
  });
  if (r.state === "problem") items.push({ label: "Dismiss message", separatorBefore: true, run: () => act("dismiss_problem", { id: r.id }) });
  return items;
}

function appRow(r: AppRow): HTMLLIElement {
  const nameLine = el("div", { class: "name" }, r.name);
  if (r.offer?.preview && (r.state === "not_installed" || r.state === "update")) nameLine.append(el("span", { class: "tag", text: "Preview" }));
  const info = el("div", { class: "info" }, nameLine);
  if (r.state === "busy" && r.job) {
    const text = stageText(r, r.job);
    info.append(el("div", { class: "meta", text }));
    const bar = el("progress", { max: "100", "aria-label": `${text}: ${r.name}`, "data-progress": r.id });
    if (r.job.stage === "downloading" && r.job.total) bar.value = Math.floor((r.job.received / r.job.total) * 100);
    else bar.removeAttribute("value"); // indeterminate while checking, waiting or installing
    if (r.job.stage !== "waiting") info.append(bar);
  } else if (r.state === "problem") {
    info.append(el("div", { class: "meta", text: versionLine({ ...r, state: r.installed_version !== null ? "current" : "not_installed" }) }));
    info.append(el("div", { class: "problem-text", text: r.problem ?? "" }));
  } else {
    info.append(el("div", { class: "meta", text: versionLine(r) }));
    if (r.only_preview && r.state === "not_installed") info.append(el("div", { class: "meta", text: "Only a preview is available. Previews are early and may change." }));
  }
  const li = el("li", { class: `row${r.state === "problem" ? " problem" : ""}` }, markEl(r.letters, r.color), info);
  li.append(mainButton(r) ?? el("span", { class: "action" }));
  const items = rowMenu(r);
  if (items.length) {
    const wrap = el("div", { class: "menu-wrap more-slot" });
    const more = el("button", { type: "button", class: "icon-btn more", "aria-haspopup": "menu", "aria-expanded": "false", "aria-label": `More for ${r.name}`, "data-key": `more:${r.id}` });
    more.innerHTML = '<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false"><circle cx="5" cy="12" r="2" fill="currentColor"/><circle cx="12" cy="12" r="2" fill="currentColor"/><circle cx="19" cy="12" r="2" fill="currentColor"/></svg>';
    const menu = el("div", { class: "menu", role: "menu", "aria-label": `${r.name} actions`, hidden: true });
    menuButton(more, menu, () => rowMenu(r));
    wrap.append(more, menu);
    li.append(wrap);
  } else {
    li.append(el("span", { class: "more-slot" }));
  }
  return li;
}

function webRow(w: SimpleRow): HTMLLIElement {
  const btn = el("button", { type: "button", class: "btn secondary action", "aria-label": `Launch ${w.name} in your browser`, "data-key": `web:${w.id}`, text: "Launch" });
  btn.addEventListener("click", () => act("open_web", { id: w.id }));
  return el("li", { class: "row simple" }, markEl(w.letters, w.color), el("div", { class: "info" }, el("div", { class: "name", text: w.name }), w.description ? el("div", { class: "meta", text: w.description }) : null), btn);
}

function comingRow(c: SimpleRow): HTMLLIElement {
  return el("li", { class: "row coming" }, markEl(c.letters, c.color), el("div", { class: "info" }, el("div", { class: "name", text: c.name }), el("div", { class: "meta", text: c.description })));
}

// ------------------------------------------------------------------------------------------
// Status line, banner, settings

function hubUpdateButton(label = "Update"): HTMLButtonElement {
  const b = el("button", { type: "button", class: "btn", "data-key": "hub-update", "aria-label": `Update ${view?.brand.product_name ?? "the hub"}`, text: label });
  b.addEventListener("click", () => act("update_hub"));
  return b;
}

function renderStatus(v: View): void {
  const s = v.status;
  const box = $("status");
  box.replaceChildren();
  if (s.hub_job) {
    box.append(`${stageText(null, s.hub_job)}: ${v.brand.product_name}`);
    if (s.hub_job.cancellable) {
      const c = el("button", { type: "button", class: "btn secondary", "data-key": "hub-cancel", text: "Cancel" });
      c.addEventListener("click", () => act("cancel", { id: "hub" }));
      box.append(c);
    }
    return;
  }
  if (s.hub_problem) {
    box.append(s.hub_problem);
    if (s.hub_update) box.append(hubUpdateButton("Retry"));
    return;
  }
  if (s.hub_update) {
    box.append(s.hub_required ? `This catalog needs a newer ${v.brand.product_name}.` : "A new version of the hub is ready.", hubUpdateButton());
    return;
  }
  let text: string;
  if (s.checking) text = "Checking for updates…";
  else if (s.checks_disabled) text = "Update checks are turned off by your organization.";
  else if (s.offline) text = "Offline, showing what is installed.";
  else if (s.check_error) text = s.check_error;
  else if (s.updates_ready > 0) text = s.updates_ready === 1 ? "1 update ready." : `${s.updates_ready} updates ready.`;
  else if (s.no_catalog) text = "The app list has not been downloaded yet.";
  else if (s.last_check) text = `Checked for updates ${ago(s.last_check)}.`;
  else text = "Not checked for updates yet.";
  box.append(text);
}

function renderBanner(v: View): void {
  const b = $("banner");
  b.replaceChildren();
  if (v.status.hub_required && !v.status.hub_job) {
    b.append(el("div", { class: "banner", role: "region", "aria-label": "Hub update needed" }, el("p", { text: `The app list was made for a newer ${v.brand.product_name}. Update the hub to install or update apps. Installed apps still open.` }), hubUpdateButton()));
  }
}

function toggle(key: keyof View["settings"], label: string, help: string, locked: boolean, lockText = "Turned off by your organization."): HTMLElement {
  const s = view!.settings;
  const id = `set-${key}`;
  const input = el("input", { type: "checkbox", class: "switch", role: "switch", id, "data-key": id, "aria-describedby": `${id}-help`, disabled: locked });
  input.checked = !!s[key];
  input.addEventListener("change", () => {
    invoke<View>("set_setting", { key, value: input.checked })
      .then((v) => {
        view = v;
        render();
      })
      .catch((e) => {
        input.checked = !input.checked;
        fail(e);
      });
  });
  const box = el("div", { class: "setting" }, el("label", { for: id, text: label }), input, el("div", { class: "help", id: `${id}-help`, text: help }));
  if (locked) box.append(el("div", { class: "locked", text: lockText }));
  return box;
}

function renderSettings(v: View): void {
  const s = v.settings;
  const body = $("settings-body");
  body.replaceChildren(
    toggle("offer_previews", "Offer preview versions", "Previews are early versions and may change. Most of the apps are in preview today, so with this off very little would be offered.", s.previews_locked),
    toggle("start_with_windows", "Start with Windows", `Starts ${v.brand.product_name} in the tray when you sign in. It then checks for updates once a day.`, s.start_locked),
    toggle("keep_in_tray", "Keep running in the tray when I close the window", "The window closes fully; a small icon stays in the tray.", false),
    toggle("daily_check", "Check once a day while in the tray", "Only the app list is fetched. Nothing is downloaded or installed without a click.", s.checks_locked),
  );
  const repo = el("button", { type: "button", class: "link", "data-key": "repo", text: v.brand.repository_url.replace("https://", "") });
  repo.addEventListener("click", () => act("open_link", { which: "repository" }));
  const dl = el(
    "dl",
    {},
    el("dt", { text: "Version" }),
    el("dd", { text: v.brand.version }),
    el("dt", { text: "Licence" }),
    el("dd", { text: "Apache License 2.0" }),
    el("dt", { text: "App list" }),
    el("dd", { text: v.status.catalog_date ? new Date(v.status.catalog_date).toLocaleString() : "Not downloaded yet" }),
    el("dt", { text: "Source" }),
    el("dd", {}, repo),
  );
  const about = el("section", { class: "about", "aria-labelledby": "about-heading" }, el("h2", { id: "about-heading", text: `About ${v.brand.product_name}` }), dl);
  if (s.policy_sources.length) about.append(el("p", { class: "meta", text: `Some settings are set by your organization (${s.policy_sources.join("; ")}).` }));
  body.append(about);
}

function topMenu(): MenuItem[] {
  const v = view;
  const items: MenuItem[] = [];
  if (v && !v.status.checks_disabled) items.push({ label: "Check for updates", run: () => act("check_now") });
  items.push({ label: "Settings", run: () => go("settings") });
  if (v?.tray) items.push({ label: `Quit ${v.brand.product_name}`, separatorBefore: true, run: () => act("quit") });
  return items;
}

function go(p: "home" | "settings"): void {
  page = p;
  render();
  ($(p === "settings" ? "settings-heading" : "desktop-heading") as HTMLElement).focus();
}

// ------------------------------------------------------------------------------------------

function announceChanges(v: View): void {
  for (const r of v.apps) {
    const now = r.job ? `${r.job.kind}:${r.job.stage}` : r.state === "problem" ? `problem:${r.problem}` : r.state;
    const before = lastStage.get(r.id);
    lastStage.set(r.id, now);
    if (before === undefined || before === now) continue;
    if (r.job) announce(`${r.name}: ${stageText(r, r.job)}`);
    else if (r.state === "problem") announce(`${r.name}: ${r.problem}`);
    else if (before.startsWith("install:") || before.startsWith("update:") || before.startsWith("reinstall:")) announce(`${r.name} ${r.installed_version ?? ""} is installed.`);
    else if (before.startsWith("uninstall:")) announce(`${r.name} was uninstalled.`);
    else if (before.startsWith("extra:")) announce(`${r.name}: the download is in your Downloads folder.`);
  }
}

function render(): void {
  const v = view;
  if (!v) return;
  const focusKey = (document.activeElement as HTMLElement | null)?.dataset?.key;
  document.documentElement.style.setProperty("--brand-light", v.brand.accent);
  document.documentElement.style.setProperty("--brand-dark", v.brand.accent_dark);
  document.title = v.brand.product_name;
  $("suite-name").textContent = v.brand.suite_name;
  $("sibling").textContent = v.brand.sibling_label;

  $("home").hidden = page !== "home";
  $("settings").hidden = page !== "settings";

  const list = $("desktop-list");
  list.replaceChildren(...v.apps.map(appRow));
  const empty = $("desktop-empty");
  empty.hidden = v.apps.length > 0;
  empty.textContent = v.status.no_catalog ? "The list of apps comes from the internet the first time. Connect, then choose Check for updates in the settings menu." : "No apps are listed.";
  $("web-section").hidden = v.web.length === 0;
  $("web-list").replaceChildren(...v.web.map(webRow));
  $("coming-section").hidden = v.coming.length === 0;
  $("coming-list").replaceChildren(...v.coming.map(comingRow));
  renderBanner(v);
  renderStatus(v);
  if (page === "settings") renderSettings(v);
  announceChanges(v);

  if (focusKey) {
    const target = document.querySelector<HTMLElement>(`[data-key="${CSS.escape(focusKey)}"]`);
    if (target && !(target as HTMLButtonElement).disabled) target.focus();
    else if (page === "home") ($("desktop-heading") as HTMLElement).focus();
  }
}

async function firstRun(): Promise<void> {
  const v = view!;
  const answer = await ask(
    `Start ${v.brand.product_name} with Windows?`,
    `It would start in the tray when you sign in and check for updates once a day. Most people do not need this: open ${v.brand.product_name} when you want to install or update an app. You can change this later in Settings.`,
    [
      { label: "Not now", primary: true, value: "no" },
      { label: "Start with Windows", value: "yes" },
    ],
  );
  view = await invoke<View>("first_run_answer", { startWithWindows: answer === "yes" }).catch((e) => {
    fail(e);
    return view!;
  });
  render();
}

function start(): void {
  menuButton($("top-menu-btn"), $("top-menu"), topMenu);
  $("settings-back").addEventListener("click", () => go("home"));
  $("sibling").addEventListener("click", () => act("open_link", { which: "sibling" }));
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && page === "settings" && !menuIsOpen() && !($("dialog") as HTMLDialogElement).open) go("home");
  });
  // Relative times ("10 minutes ago") are brought up to date when the window comes back into
  // view, not on a timer.
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) void refresh();
  });
  window.addEventListener("focus", () => void refresh());

  listen<null>("hub://changed", () => void refresh());
  listen<string>("hub://page", (p) => {
    closeMenu(false);
    go(p === "settings" ? "settings" : "home");
  });
  listen<{ id: string; received: number; total: number }>("hub://progress", (p) => {
    // Progress moves the bar and the percentage without redrawing the row.
    if (p.id === "hub" && view?.status.hub_job) {
      view.status.hub_job.received = p.received;
      view.status.hub_job.total = p.total;
      renderStatus(view);
      return;
    }
    const bar = document.querySelector<HTMLProgressElement>(`progress[data-progress="${CSS.escape(p.id)}"]`);
    if (!bar || !p.total) return;
    const pct = Math.floor((p.received / p.total) * 100);
    bar.value = pct;
    const meta = bar.previousElementSibling;
    if (meta) meta.textContent = `Downloading ${pct}%`;
  });

  invoke<View>("get_view").then((v) => {
    view = v;
    render();
    if (v.first_run) void firstRun();
  }, fail);
}

start();
