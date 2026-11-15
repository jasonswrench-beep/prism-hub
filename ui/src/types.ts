// The view the hub sends (src-tauri/src/view.rs). Keep the two in step.

export type Stage = "downloading" | "checking" | "waiting" | "installing" | "uninstalling";
export type Kind = "install" | "update" | "reinstall" | "uninstall" | "extra" | "hub";

export interface JobView {
  kind: Kind;
  stage: Stage;
  received: number;
  total: number;
  cancellable: boolean;
}

export interface AppRow {
  id: string;
  name: string;
  letters: string;
  color: string;
  description: string;
  state: "not_installed" | "current" | "update" | "busy" | "problem" | "unavailable";
  installed_version: string | null;
  offer: { version: string; preview: boolean; size: number } | null;
  blocked: string | null;
  unavailable: string | null;
  only_preview: boolean;
  job: JobView | null;
  problem: string | null;
  running: boolean;
  can_open: boolean;
  can_reinstall: boolean;
  can_uninstall: boolean;
  can_folder: boolean;
  whats_new: boolean;
  extras: { label: string; size: number }[];
}

export interface SimpleRow {
  id: string;
  name: string;
  description: string;
  letters: string;
  color: string;
}

export interface Status {
  checking: boolean;
  offline: boolean;
  last_check: number | null;
  updates_ready: number;
  checks_disabled: boolean;
  check_error: string | null;
  catalog_date: string | null;
  no_catalog: boolean;
  hub_update: string | null;
  hub_required: boolean;
  hub_job: JobView | null;
  hub_problem: string | null;
}

export interface SettingsView {
  offer_previews: boolean;
  start_with_windows: boolean;
  keep_in_tray: boolean;
  daily_check: boolean;
  previews_locked: boolean;
  start_locked: boolean;
  checks_locked: boolean;
  policy_sources: string[];
}

export interface View {
  brand: {
    product_name: string;
    suite_name: string;
    accent: string;
    accent_dark: string;
    sibling_label: string;
    repository_url: string;
    version: string;
  };
  apps: AppRow[];
  coming: SimpleRow[];
  web: SimpleRow[];
  status: Status;
  settings: SettingsView;
  first_run: boolean;
  tray: boolean;
}

interface TauriGlobal {
  core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> };
  event: { listen<T>(name: string, cb: (e: { payload: T }) => void): Promise<() => void> };
}

declare global {
  interface Window {
    __TAURI__: TauriGlobal;
  }
}

export function invoke<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return window.__TAURI__.core.invoke<T>(cmd, args);
}

export function listen<T>(name: string, cb: (payload: T) => void): void {
  void window.__TAURI__.event.listen<T>(name, (e) => cb(e.payload));
}
