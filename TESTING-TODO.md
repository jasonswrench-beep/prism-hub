# Testing to do

Checks only a person can do, on a real Windows account. Tick each one off with the date and what you saw.

This is OpenStage Hub's code with Prism's brand file, so the hub's behaviour is tested there too; these are the Prism-specific checks plus the ones every release needs.

Already checked by Claude on 2026-11-15 (no need to repeat unless something changes):

- `npm test` (7 tests), `npm run typecheck` and `cargo test` (21 tests) pass.
- The catalog script (dry run) found PrismQDA 0.1.0 beta 1, PrismStatistics 0.1.0 and Prism Reference Manager 0.1.2 and their extras, and checked every setup file's SHA-256 and signature (Jason Wrench, timestamped).
- A development copy, given that catalog signed with a test key, lists the three apps; PrismQDA (Windows lists 0.1.0) and PrismStatistics (0.1.1) show as installed, Prism Reference Manager offers Install.
- The icons were drawn from `brand/PrismOpenResearch.svg` by Edge, with and without the tray dot.
- The unsigned installer is 3.08 MB (target 8 MB).

## Before the first release

- [ ] Run **Make Hub Key.bat**, back up the `Prism Hub key` folder in your user folder, commit `brand/brand.json`.
- [ ] Make the prism-hub repository public (catalog and hub downloads need it).
- [ ] Run **Update Prism Catalog.bat** and check the release named `catalog` has the two files.
- [ ] The footer's OpenStage Hub link goes to `github.com/jasonswrench-beep/openstage-hub/releases`, which is private today.
- [ ] Web links: none yet. Add the Prism website, documentation or roadmap addresses to `web` in `catalog/apps.json` when there are any.
- [ ] Prism Reference Manager 0.1.2 is marked pre-release on GitHub, but its version has no `-beta` part, so the hub treats it as stable (no Preview tag). Say if it should be shown as a preview.

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
- [ ] Uninstall says the app's documents and projects stay; afterwards PrismQDA projects and the reference library are still there.
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
