// SPDX-License-Identifier: Apache-2.0
// The shapes the Rust side sends (src-tauri/src/hub.rs and jobs.rs).

export type Fetch = { kind: "notYet" } | { kind: "fresh" } | { kind: "offline" } | { kind: "off" } | { kind: "refused"; message: string };

export interface View {
  hubVersion: string;
  catalogDate: string | null;
  fetch: Fetch;
  lastCheck: number | null;
  keyMissing: boolean;
  needsNewerHub: boolean;
  hubUpdate: { version: string; size: number } | null;
  justUpdated: string | null;
  apps: AppView[];
  web: { id: string; name: string; description: string; url: string }[];
  comingSoon: { id: string; name: string; description: string; mark?: { letters: string; color: string; icon?: string | null } | null }[];
  updatesReady: number;
  settings: { offerPreviews: boolean; keepInTray: boolean; dailyCheck: boolean; askedStartWithWindows: boolean };
  startWithWindows: boolean;
  policy: { checks: boolean; startWithWindows: boolean; previews: boolean };
  jobs: string[];
  repository: string;
}

export interface AppView {
  id: string;
  name: string;
  letters: string;
  color: string;
  /** The app's own icon (a data: address from the signed catalog), shown in place of the round mark when there is one. */
  icon?: string | null;
  description: string;
  installed: { version: string; exact: boolean; folderKnown: boolean } | null;
  offer: { version: string; preview: boolean; size: number } | null;
  update: boolean;
  running: boolean;
  needsNewerHub: boolean;
  previewBlocked: boolean;
  extras: { index: number; name: string; description: string; size: number }[];
}

export interface JobEvent {
  id: string;
  stage: "downloading" | "checking" | "waiting" | "installing" | "removing" | "done" | "failed" | "cancelled";
  received: number;
  total: number;
  message: string;
}
