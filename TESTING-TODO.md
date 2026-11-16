# Testing to do

Checks only a person can do, on a real Windows account. Tick each one off with the date and what you saw.

This is OpenStage Hub's code with Prism's brand file, so the hub's behaviour is tested there too; these are the Prism-specific checks plus the ones every release needs.

Already checked by Claude on 2026-11-15 (no need to repeat unless something changes):

- `npm test` (7 tests), `npm run typecheck` and `cargo test` (21 tests) pass.
- The catalog script (dry run) found PrismQDA 0.1.0 beta 1, PrismStatistics 0.1.0 and Prism Reference Manager 0.1.2 and their extras, and checked every setup file's SHA-256 and signature (Jason Wrench, timestamped).
- A development copy, given that catalog signed with a test key, lists the three apps; PrismQDA (Windows lists 0.1.0) and PrismStatistics (0.1.1) show as installed, Prism Reference Manager offers Install.
- The icons were drawn from `brand/PrismOpenResearch.svg` by Edge, with and without the tray dot.
- The unsigned installer is 3.08 MB (target 8 MB).

Checked by Claude on 2026-11-16:

- `npm test` (8 tests), `npm run typecheck` and `cargo test` (21 tests) pass.
- The catalog script (dry run) found three new releases and checked each setup file's SHA-256 and signature: PrismQDA 0.1.0 beta 2 (preview), PrismStatistics 0.2.1 and Prism Reference Manager 0.1.3. Each still installs per user, and its program file and the name Windows lists are the ones in `catalog/apps.json`.
- PrismQDA takes its statistics from the shared core at the tag `prismqda-stats-v1.0.0` (its commit 372fc00) and has no copy of its own.

## First release (list item 1)

Done by 2026-11-15: the hub key made (its public half is in `brand/brand.json`), the repository public, the catalog published, Prism Hub 1.0.0 and 1.0.1 released and signed, and you confirmed the hub works and updated an app with it. The OpenStage Hub link in the footer now opens (that repository is public). Still to do:

- [x] 2026-11-16: **Update Prism Catalog.bat** run. Claude downloaded the published catalog (generated 2026-11-16 21:44 UTC): its signature checks against the key in `brand/brand.json`, and it offers PrismQDA 0.1.0 beta 2 (preview), PrismStatistics 0.2.2, Prism Reference Manager 0.1.3 and the hub 1.0.1, each with its icon.
- [ ] In the installed hub, Check for updates offers those three versions for the apps you have installed.
- [ ] Each app's quiet install switch, below under "Installing, updating, removing".
- [ ] Web links: none yet. Add the Prism website, documentation or roadmap addresses to `web` in `catalog/apps.json` when there are any.
- [ ] PrismStatistics's release titles say "teaching preview", but GitHub lists them as full releases and their versions (0.2.1) have no `-beta` part, so the hub shows them with no Preview tag. Say if they should be shown as previews (the app would then need versions such as `0.2.1-preview.1`).

## Preview or full release (list item 2)

Settled: Prism Reference Manager is a full release from 0.1.2 on. Its release notes say so (its commit ab293c3), GitHub no longer marks 0.1.2 or 0.1.3 as pre-release, and the hub already showed it with no Preview tag. Nothing in the hub changed.

## One codebase with OpenStage Hub (list item 3)

Asked of the shared core in `CORE-REQUESTS.md` (H1), not built here. One ruling is yours first:

- [ ] **Each app's icon.** The two hubs' 1.0.1 releases show the apps' icons in two different ways. Prism Hub carries each icon inside the signed catalog; OpenStage Hub downloads each one separately from its catalog release. The shared code can keep only one. Prism Hub's way keeps the rule that the network is used only for the catalog, its signature and the files the person asks for, and OpenStage Hub's PNGs are small enough to go into its catalog as they are. Say which to keep.

## The window (installed hub, not the development copy)

- [ ] It opens about 470 by 720 with the PrismOpenResearch icon and "Prism Hub" in the header, and lists the three apps.
- [ ] First run asks about Start with Windows once, with "Not now" first. It does not ask again.
- [ ] Apps installed before the hub show as installed. Open opens each one.
- [ ] Ctrl and plus zooms to 200 percent; everything still fits and works.
- [ ] Narrator or NVDA: every button names its app ("Install Prism Reference Manager"); progress stages and failures are read.
- [ ] Windows' contrast themes and dark mode: the text and the navy buttons can still be seen.

## Installing, updating, removing (on a clean Windows account)

Each app's quiet switch has not yet been tried. For each, try Install, then Uninstall, then Install again:

- [ ] PrismQDA (`/P`): its progress bar shows, no pages; Windows lists 0.1.0.
- [ ] PrismStatistics (`/P`)
- [ ] Prism Reference Manager (`/P`)
- [ ] Other downloads: the portable copies and the browser add-on arrive whole.
- [ ] Uninstall says the app's documents and projects stay; afterwards PrismQDA projects and the reference library are still there. (Prism Reference Manager's own uninstaller removes its settings and its `prism-rm://` link, by its design; the libraries stay.)
- [ ] Prism Reference Manager 0.1.3 and PrismQDA now have their own Check for updates. After an app updates itself, the hub shows "0.1.3 installed" (Windows' three numbers) and offers nothing until a newer release is in the catalog.
- [ ] Update with the app open: the row says "Close ... to continue"; the hub never closes it.
- [ ] Cancel while downloading: nothing is installed and `%LOCALAPPDATA%\org.prismopenresearch.hub\downloads` is empty.
- [ ] No step asks for administrator rights.

## Tray, offline, self-update, policy, uninstalling

- [ ] Start with Windows: the hub is in the tray at sign-in with no window; Task Manager shows under 30 MB with the window closed.
- [ ] The tray icon shows its dot and one notification when an update is published, not repeated.
- [ ] With the network unplugged: every app listed, Open works, Install and Update show "Offline".
- [ ] Publish a 1.0.1 and update the catalog: "A new version of the hub is ready", Update works, the new hub says it was updated.
- [ ] Each setting in `docs/policy.md`, with `policy.json` beside the installed `prism-hub.exe`.
- [ ] Uninstalling the hub removes it, its start-up entry and `%LOCALAPPDATA%\org.prismopenresearch.hub`. Every Prism app is still installed and opens.
