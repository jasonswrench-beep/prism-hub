# How Prism Hub works

## The window

A native window (Tauri: a Rust shell with a TypeScript page) of about 470 by 720 pixels. The page cannot reach the network or the disk; its content security policy allows only the app's own channel, and everything it does goes through the commands in `src-tauri/src/commands.rs`, each of which acts only on what the signed catalog names.

## The catalog

The hub reads one small signed file instead of asking GitHub about each app: the repositories name their installers in different ways, and GitHub's unsigned lookup allows only 60 requests an hour per network, which a shared campus network can use up. Release downloads have no such limit.

- It lives at `https://github.com/jasonswrench-beep/prism-hub/releases/download/catalog/prism-catalog.json`, with `prism-catalog.json.sig` beside it.
- The signature is Ed25519 over the file's exact bytes. The public half of the hub key is built into the program (`brand/brand.json`, `catalogPublicKey`). A catalog that fails the check is ignored.
- Every address in it must be `https://github.com/<that app's repository>/releases/download/...`; every checksum, version, switch and file name is checked before it is used (`src-tauri/src/catalog.rs`).
- A catalog older than the one already on disk is refused, so an old signed catalog cannot be replayed to offer older installers.
- A catalog that needs a newer hub (`minimumHubVersion`, or a higher `format`) shows only the hub's update banner.

[catalog.md](catalog.md) has the format.

## Installing and updating: six steps

`src-tauri/src/jobs.rs`:

1. Download the installer from the catalog's address into `%LOCALAPPDATA%\org.prismopenresearch.hub\downloads`. Only github.com and GitHub's release download hosts are accepted; each redirect is followed by hand and its host checked.
2. Check its SHA-256 against the catalog. A mismatch deletes the file and stops.
3. Check its code signature with Windows' own check (WinVerifyTrust, with revocation): valid, timestamped and signed by the catalog's expected signer (Jason Wrench). Anything else deletes the file and stops.
4. If the app is running, ask the person to close it and wait. The hub never closes an app itself.
5. Run the installer with its quiet switch (from the catalog), which shows the installer's own progress bar rather than its pages. Installs are per user; an installer that asks for administrator rights is not run.
6. Read the installed version back from Windows' per-user installed apps list and confirm it, then delete the download.

Cancel works while downloading, checking or waiting; not while the installer itself runs.

### Which version is installed

Windows' installed apps list shows only the three numbers of a Prism app (0.1.0, not 0.1.0 beta 1). The hub records the exact release it installs, and trusts that record only while Windows still lists the same three numbers. For an app installed before the hub, or updated by the app's own Check for updates, the hub knows only the three numbers: it shows "0.2.0 installed" and offers Update only when a release with higher numbers appears (Reinstall in the menu installs the newest one now).

## Opening

Open runs the program file (from the catalog) in the install folder Windows recorded. It never waits on the network.

## Uninstalling

Runs the uninstaller Windows lists, without its questions (its `QuietUninstallString`, or its `UninstallString` with the catalog's quiet uninstall switch), after a confirmation saying the app's documents and projects stay where they are. The hub then watches Windows' list until the app is gone.

Uninstalling the hub removes the hub, its start-up entry, its cached catalog and its download folder (`src-tauri/installer-hooks.nsh`). It never removes the apps.

## The hub's own updates

The catalog's `hub` entry names the hub's newest release. When it is newer, the window shows "A new version of the hub is ready" with an Update button. The hub runs steps 1 to 3 on its own installer, closes, the installer runs (showing its progress bar), and the new hub starts. After the restart it says "The hub was updated to ..." once.

## The tray and Start with Windows

Off by default; the hub asks once, on first run, with "Not now" first. When Start with Windows is on, the hub starts with `--tray` from the per-user start-up entry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), with no window. Turning it on also turns on Keep running in the tray, which can then be changed on its own.

In the tray: left click opens the window; right click lists the installed apps (each opens directly), Check for updates, Settings and Quit. A dot on the icon means an update is ready, and each new version gets one Windows notification, never repeated.

Closing the window quits the hub unless the tray is on; then the window is closed fully (not hidden), so the hub uses little memory.

## When the network is used

- When the window opens, and when the person presses Check for updates: the catalog and its signature (two small files).
- While in the tray, once a day if that setting is on: the same two files. The thread wakes at most every six hours to look at the clock, because a computer that sleeps overnight does not count that time.
- When the person presses Install, Update, Reinstall or Other downloads: that one file.

Nothing else. No usage data, no list of installed apps and no identifier ever leaves the computer. Requests are made by Windows' own HTTPS code (WinHTTP) with no cookies.

## What is kept on disk

Under `%LOCALAPPDATA%\org.prismopenresearch.hub`: `settings.json` (settings, the exact release the hub installed for each app, which versions were already announced, when the catalog was last fetched), `catalog\` (the last good catalog and its signature, checked again every time they are read), and `downloads\` (emptied after every install and when the hub starts).

## Development switches

A development build (not a release build) also reads:

- `HUB_CATALOG_DIR`: a folder holding a catalog and its signature, used instead of GitHub.
- `HUB_TEST_KEY`: a test key's public half, used instead of the built-in one.
- `--print-view`: prints what the window would show, after fetching the catalog, and stops.

`tests/ui-preview.html` (with `npm run dev`, at `http://localhost:1430/tests/ui-preview.html`) runs the real window code against a stand-in for the Rust side, for looking at every row state and testing by keyboard and screen reader.
