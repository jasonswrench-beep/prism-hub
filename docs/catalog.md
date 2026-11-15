# The Prism catalog

The hub learns what exists from one file, `prism-catalog.json`, published with its signature
`prism-catalog.json.sig` on the release named `catalog` in this repository:

    https://github.com/jasonswrench-beep/prism-hub/releases/download/catalog/prism-catalog.json

Why one file: the repositories name their installers in different ways, so guessing file names
would break; and GitHub's unsigned lookup allows only 60 requests an hour per network, which a
shared campus network can use up. Release downloads have no such limit.

## Trust

- The hub has the public half of the hub key built in (`catalog_public_key` in
  `brand/brand.json`). A catalog whose signature does not check out is ignored, and the hub
  keeps the last good one. The cached copy on disk is checked again every time it is read.
- A catalog older than the one the hub already has is ignored, so an old file cannot be
  replayed to hold people on an old version.
- Even a signed catalog cannot point the hub at a download outside github.com and its release
  download host, name a program file with a folder in it, or pass an installer anything but
  plain `/SWITCH` arguments.
- Every installer must match the catalog's SHA-256 and carry a valid, timestamped Windows code
  signature from the entry's `expected_signer`. A file that fails is deleted before anything
  runs, and stays locked against changes from the check until the installer finishes.
- If a catalog sets `minimum_hub_version` above the running hub, the hub reads only its `hub`
  entry and asks to be updated first. Installed apps still open.

## One app's entry

| Field | Meaning |
| --- | --- |
| `id` | Short name, lowercase letters, digits and dashes. |
| `name`, `description` | Shown in the row. |
| `mark` | Two letters and a `#rrggbb` color for the round mark. |
| `section` | `desktop` or `coming-soon`. |
| `repository` | The GitHub repository the releases come from. |
| `installed_as` | The name Windows lists in installed apps (the uninstall entry's DisplayName). |
| `program_file` | The program's file name inside its install folder. |
| `quiet_switch` | The installer's switch for a no-questions install with only a progress bar (`/P` for the Tauri installers the Prism apps use). |
| `uninstall_switch` | The same for the uninstaller (`/P`; it keeps the app's data). |
| `expected_signer` | The name the installer's code signature must show. |
| `stable`, `preview` | Each: `version`, `url`, `sha256`, `size`, `release_page`, or `null`. |
| `extras` | Other downloads (portable copy, browser add-on): `label`, `file_name`, `url`, `sha256`, `size`. |
| `minimum_hub_version` | Optional, per app. |

The catalog also has `format` (1), `generated`, `minimum_hub_version`, `hub` (the hub's own
newest installer, same fields as `stable`) and `web` (links: `id`, `name`, `description`, `url`,
optional `mark`).

## Updating it

`Update Prism Catalog.bat` (tools/update_catalog.py) reads `catalog/sources.json`, then for each
repository finds the newest stable release and the newest pre-release (kept only when it is newer
than the stable one), downloads each setup file and extra, computes its SHA-256, checks it against
the digest GitHub reports, checks its code signature, writes the catalog, prints what changed,
asks for the hub key's password, signs, and asks before uploading both files. It stops without
publishing anything if a setup file is missing, unsigned or signed by someone else.

Run it after any app release. Until it runs, the hub keeps offering the previous version, which is
safe.

To add an app, add an entry to `catalog/sources.json` with the fields above plus `setup` (a
pattern matching its setup file's name) and `extras` (label and pattern). Coming-soon apps go in
`coming_soon`, web links in `web`.

## Channels

With "Offer preview versions" on (the default while the suite is in preview), the hub offers the
newer of stable and preview. With it off, it offers stable; an app with only a preview still has
it offered, marked Preview. Someone on a preview who turns previews off keeps their version until
a stable one passes it. Policy can turn previews off entirely (then preview-only apps show no
Install button).
