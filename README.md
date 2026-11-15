# Prism Hub

Prism Hub is a small Windows program that lists every app in the PrismOpenResearch suite and
installs, updates and opens each one from its signed GitHub release. It needs no account, stays
out of the way by default, and never installs a file that fails its checksum and signature check.

- **Desktop**: one row per app, with one button that fits the app's state (Install, Open,
  Update, Cancel or Retry) and a menu for everything else (What's new, Open install folder,
  Reinstall, Uninstall, Other downloads).
- **Web**: links that open in your own browser.
- **Coming soon**: apps that are on the way.

It is a per-user program: installing it, and every app it installs, needs no administrator.

## How it decides what to offer

The hub reads one signed file, the Prism catalog (`prism-catalog.json` on the `catalog`
release of this repository), instead of asking GitHub about each app. Every installer it
downloads must match the SHA-256 in the catalog and carry a valid, timestamped Windows code
signature from the expected signer. A file that fails either check is deleted and nothing is
installed. See [docs/catalog.md](docs/catalog.md).

Installed apps are found the way Windows lists them (Settings › Apps), so apps installed before
the hub existed show as installed. The apps keep their own update buttons; both routes use the
same signed installers.

## Privacy

A check for updates downloads the catalog and nothing else. No usage data, no list of installed
apps and no identifier leaves the computer. The hub never opens an app's projects or library and
never changes an app's settings.

## Start with Windows and the tray

Off by default; the hub asks once on first run, with "Not now" first. When it is on, the hub
starts in the tray at sign-in with no window, checks the catalog once a day, and shows one
Windows notification per new version. Settings › "Keep running in the tray when I close the
window" can be changed on its own.

## For IT staff

A `policy.json` beside `prism-hub.exe`, in `%ProgramData%\Prism Hub\` (administrator-owned), or
Group Policy values under `Software\Policies\Prism Hub` can turn off update checks,
start-with-Windows and previews, or hide apps. See [docs/policy.md](docs/policy.md).

## Building

Needs Rust, Node.js, Python 3, the GitHub command line (`gh`), and for signed builds the
signing setup the other apps use (the `OPENSTAGE_SIGN_*` variables and `az login`).

| Double-click | What it does |
| --- | --- |
| `Make Hub Key.bat` | Once: makes the catalog key (you choose the password) and puts its public half in `brand/brand.json`. |
| `Build Prism Hub.bat` | Tests, notices, and the signed installer in `dist\` with its SHA-256 file. Publishes nothing. |
| `Update Prism Catalog.bat` | After any app release: reads the releases, checks every setup file, writes, signs and (after asking) uploads the catalog. |

For development: `npm install`, then `npm run dev`. `npm test` runs the Rust tests.
`python tools/ui-preview/serve.py` shows the window in a browser with made-up data.

Every brand value (name, identifier, Start menu folder, icon, accent color, catalog address,
catalog key) lives in [brand/brand.json](brand/brand.json); `npm run brand` writes it into the
Tauri configuration, the installer hooks and the icons.

## Licence

Apache License 2.0. See [LICENSE](LICENSE), [NOTICE](NOTICE) and [THIRD-PARTY.md](THIRD-PARTY.md).
