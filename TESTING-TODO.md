# Checks only a person can do

The automated tests (`npm test`; add `PRISM_HUB_NET_TESTS=1` for the real-download test) cover
the catalog signature, version choice, policy, address checks, the signature check on a real
Prism installer, and download-checksum-signature on a real release. These need a person and a
Windows account.

## Before the first release

- [ ] Run `Make Hub Key.bat`, choose the password, and back up `%USERPROFILE%\.prism-hub-catalog.key`
      and the password somewhere off this computer.
- [ ] Make the `prism-hub` repository public (it is private today; release downloads from a
      private repository fail for everyone else, so the hub would always say it cannot download
      the app list).
- [ ] Push the code, then run `Update Prism Catalog.bat` and let it upload. Open
      `https://github.com/jasonswrench-beep/prism-hub/releases/tag/catalog` and confirm both files are there.
- [ ] Run `Build Prism Hub.bat`. Confirm `dist\Prism-Hub-1.0.0-setup.exe` shows Jason Wrench
      as the signer (Properties › Digital Signatures) and is under 8 MB.
- [ ] Publish release `v1.0.0` with the setup file and `SHA256SUMS.txt`, then run
      `Update Prism Catalog.bat` again so the catalog carries the hub entry.

## Day 1: the window

- [ ] Install the hub from the setup file. No administrator prompt. Start menu folder is
      PrismOpenResearch.
- [ ] The window is about 470 by 720, the PrismOpenResearch mark and name are on the left, the
      settings menu on the right.
- [ ] First run asks about Start with Windows once, "Not now" first and focused. Escape also means
      Not now. It is not asked again.
- [ ] Apps already installed show as installed with the right version, and Open starts each one.
- [ ] Open install folder opens the right folder; What's new opens the release page in your browser.

## Day 2: the catalog

- [ ] With the published catalog, the status line says "Checked for updates just now".
- [ ] Change one character in `%LOCALAPPDATA%\org.prismopenresearch.hub\catalog\prism-catalog.json`
      and restart the hub: the changed file is ignored (with the network off, the list comes from
      the copy inside the installer, or is empty).

## Day 3: install, update, uninstall (on a clean Windows account)

Each app's quiet switch must be tried once; `/P` is what Tauri installers use for a progress bar
with no pages.

- [ ] Install each of the three apps from the hub: progress shows Downloading, Checking, Installing;
      only the installer's own small progress bar appears; no administrator prompt; the row then
      says "Installed" with the right version and Open works.
- [ ] Cancel during Downloading: the row goes back to Install and nothing is left in
      `%LOCALAPPDATA%\org.prismopenresearch.hub\downloads`.
- [ ] Update with the app open: the row says "Close … to continue" and waits; it never closes the
      app. Closing it lets the update run. Cancel while waiting stops cleanly.
- [ ] A broken download (for a test, edit one SHA-256 in a test catalog signed with a test key):
      "The download did not match its checksum, so nothing was installed." and Retry.
- [ ] Uninstall from the hub: the confirmation says data stays; the app disappears from Windows'
      installed apps; its projects or library are still there; Reinstall/Install brings it back.
- [ ] Other downloads: the portable copy and the browser add-on land in Downloads and are shown in
      File Explorer. The portable copy never shows as installed.
- [ ] Turn previews off: an app on a preview keeps its version (no "update" to an older stable).
- [ ] Update an app with its own Check for updates button: the hub shows the new version as
      installed next time it opens.

## Day 3: tray and start-up

- [ ] Turn on Start with Windows; sign out and in. The hub starts in the tray with no window in
      under a second, and Task Manager shows it under 30 MB.
- [ ] Left click opens the window; right click lists the installed apps (each opens), Check for
      updates, Settings and Quit.
- [ ] With an update published and the window closed, a dot appears on the tray icon and one
      Windows notification shows. Restarting the hub does not show it again.
- [ ] With the window open, Task Manager shows the hub (with its WebView2 processes) under 120 MB.
- [ ] Unplug the network: the list still shows, installed apps open, Install and Update are greyed
      with "Offline". Plug it back in and reopen the window: it checks again.
- [ ] Closing the window with the tray off quits the hub; with the tray on it leaves only the tray
      icon (the window fully closed).

## Day 4: finish

- [ ] Publish a v1.0.1 test release and catalog: the status line says "A new version of the hub is
      ready", Update installs it and the hub starts again on the new version.
- [ ] Policy: put `policy.json` (see docs/policy.md) beside `prism-hub.exe` with each key in turn
      and confirm the effect; confirm Settings › About names the file.
- [ ] Keyboard only: Tab reaches every control in reading order; the three-dot menus open with
      Enter or the arrow keys and close with Escape; Settings closes with Escape.
- [ ] Narrator: each button names its app ("Update PrismQDA"), progress changes are read out.
- [ ] Dark mode and high contrast (Windows contrast themes) are readable.
- [ ] Windows display scaling at 200 percent: the window is sharp and nothing is cut off.
- [ ] Uninstall the hub: the apps stay installed and keep working; the start-up entry,
      `%LOCALAPPDATA%\org.prismopenresearch.hub` and the tray icon are gone.
