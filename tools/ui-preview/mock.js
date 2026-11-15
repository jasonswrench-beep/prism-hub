// A stand-in for the hub so the window can be looked at in a browser. Add ?state=busy,
// ?state=problem, ?state=offline or ?state=hub to the address to see those states.
(function () {
  const state = new URLSearchParams(location.search).get("state") || "";
  const now = Math.floor(Date.now() / 1000);
  const app = (o) => Object.assign({ description: "", installed_version: null, offer: null, blocked: null, unavailable: null, only_preview: false, job: null, problem: null, running: false, can_open: false, can_reinstall: false, can_uninstall: false, can_folder: false, whats_new: true, extras: [] }, o);
  const view = {
    brand: { product_name: "Prism Hub", suite_name: "PrismOpenResearch", accent: "#151a3d", accent_dark: "#aab4ff", sibling_label: "Also from Jason Wrench: OpenStage Hub", repository_url: "https://github.com/jasonswrench-beep/prism-hub", version: "1.0.0" },
    apps: [
      app({ id: "prismqda", name: "PrismQDA", letters: "qd", color: "#5B45E0", state: "update", installed_version: "0.1.0-beta.1", offer: { version: "0.2.0-beta.1", preview: true, size: 10610168 }, can_open: true, can_uninstall: true, can_folder: true, can_reinstall: true, extras: [{ label: "portable copy 0.2.0-beta.1", size: 11576446 }] }),
      app({ id: "prismstatistics", name: "PrismStatistics", letters: "st", color: "#2F64D6", state: "current", installed_version: "0.1.0", offer: { version: "0.1.0", preview: false, size: 20386856 }, can_open: true, can_uninstall: true, can_folder: true }),
      app({ id: "prism-reference-manager", name: "Prism Reference Manager", letters: "rm", color: "#4B5FA8", state: "not_installed", offer: { version: "0.1.2", preview: true, size: 8713432 }, only_preview: true, extras: [{ label: "portable copy 0.1.2", size: 11042988 }, { label: "browser add-on 0.1.2", size: 24275 }] }),
    ],
    coming: [{ id: "prism-survey", name: "Prism Survey", description: "Design and run surveys", letters: "sv", color: "#0F7B6C" }],
    web: [{ id: "docs", name: "Documentation", description: "Guides for every Prism app", letters: "dc", color: "#151a3d" }],
    status: { checking: false, offline: false, last_check: now - 600, updates_ready: 1, checks_disabled: false, check_error: null, catalog_date: "2026-11-15T16:43:27Z", no_catalog: false, hub_update: null, hub_required: false, hub_job: null, hub_problem: null },
    settings: { offer_previews: true, start_with_windows: false, keep_in_tray: false, daily_check: true, previews_locked: false, start_locked: false, checks_locked: false, policy_sources: [] },
    first_run: state === "first",
    tray: false,
  };
  if (state === "busy") {
    view.apps[0].state = "busy";
    view.apps[0].job = { kind: "update", stage: "downloading", received: 4200000, total: 10610168, cancellable: true };
    view.apps[2].state = "busy";
    view.apps[2].job = { kind: "install", stage: "waiting", received: 0, total: 0, cancellable: true };
  }
  if (state === "problem") {
    view.apps[2].state = "problem";
    view.apps[2].problem = "The download did not match its checksum, so nothing was installed.";
  }
  if (state === "offline") {
    view.status.offline = true;
    view.apps.forEach((a) => (a.blocked = "Offline"));
  }
  if (state === "hub") view.status.hub_update = "1.1.0";
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        console.log("invoke", cmd, args);
        if (cmd === "get_view" || cmd === "first_run_answer") return JSON.parse(JSON.stringify(view));
        if (cmd === "set_setting") {
          view.settings[args.key] = args.value;
          return JSON.parse(JSON.stringify(view));
        }
        if (cmd === "open_app") throw "The program file is missing from its install folder. Reinstall the app to fix this.";
      },
    },
    event: { listen: async () => () => {} },
  };
})();
