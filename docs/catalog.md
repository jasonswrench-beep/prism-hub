# The catalog

## Updating it

Double-click **Update Prism Catalog.bat** after any app release. It:

1. reads each repository's newest published stable and preview releases (drafts never count);
2. finds each setup file by the app's `setupPattern`, downloads any it has not checked before, computes its SHA-256 and checks its code signature (valid, timestamped, signed by the expected signer);
3. lists every setup file that is missing or not properly signed and stops if there is any, before signing anything;
4. prints a table of what changed, asks for the hub key's password, signs the catalog, checks the signature against the key built into the hub, and uploads both files to the release named `catalog`.

Until it runs, the hub keeps offering the previous version, which is safe.

Options (from a console): `--dry-run` checks everything and shows the table but signs and uploads nothing; `--local` signs but leaves the files in `catalog/out` instead of uploading.

It needs Node.js and the GitHub command line (`gh`), signed in as the owner. Checked setup files are remembered in `catalog/work/checked.json` by GitHub's own SHA-256 of each file, so each is downloaded once.

## The source file: catalog/apps.json

The facts GitHub cannot tell the script:

| Field | Meaning |
| --- | --- |
| `id` | Short name, lower case, used by the hub and the policy file |
| `name` | As shown in the hub |
| `mark` | `letters` (one or two) and `color` (`#RRGGBB`) for the round mark |
| `description` | One line |
| `repository` | `owner/name` on GitHub |
| `installedAs` | The name Windows lists in installed apps (DisplayName) |
| `programFile` | The program's file name inside its install folder |
| `quietSwitch` | The installer's switches for an install with no questions that shows its progress bar |
| `quietUninstallSwitch` | Used only when Windows lists no quiet uninstall command (default `/S`) |
| `expectedSigner` | The name on the code signature |
| `setupPattern` | A regular expression matching exactly one file of a release: its setup |
| `ignoreTags` | Releases never to list (for example, ones published before code signing began) |
| `extras` | Other downloads: `name`, `description` and a `pattern` matching the file |
| `windowsVersion` | Only if Windows lists something other than the release's three numbers |
| `minimumHubVersion` | Only if this entry needs a newer hub |
| `comingSoonUntilReleased` | For an app still being made: listed under Coming soon, with its mark, until its repository is public and has a published release; from then on it is a Desktop row like the others, with no change to this file |

`web` lists the Web section (an entry with an empty `url` is left out until it has one), and `comingSoon` the Coming soon section.

### The installers' quiet switches today

| Apps | Installer | Switch | Tried |
| --- | --- | --- | --- |
| PrismQDA, PrismStatistics, Prism Reference Manager | The suite's standard per-user installer (Tauri's NSIS) | `/P`: no questions, its progress bar shown | Not yet (TESTING-TODO.md) |

## The published file

`prism-catalog.json`, signed as `prism-catalog.json.sig` (the Ed25519 signature of the file's exact bytes, in base64):

```json
{
  "format": 1,
  "generated": "2026-11-15T18:04:00Z",
  "minimumHubVersion": "1.0.0",
  "hub": { "version": "1.0.0", "windowsVersion": "1.0.0", "url": "...", "sha256": "...", "size": 0, "releasePage": "..." },
  "apps": [
    {
      "id": "prismqda", "name": "PrismQDA", "mark": { "letters": "qd", "color": "#5B45E0" },
      "description": "...", "repository": "jasonswrench-beep/prismqda",
      "installedAs": "PrismQDA", "programFile": "prismqda-app.exe", "quietSwitch": ["/P"], "expectedSigner": "Jason Wrench",
      "stable": null,
      "preview": {
        "version": "0.1.0-beta.1", "windowsVersion": "0.1.0",
        "url": "https://github.com/jasonswrench-beep/prismqda/releases/download/v0.1.0-beta.1/PrismQDA_0.1.0-beta.1_x64-setup.exe",
        "sha256": "...", "size": 15247912,
        "releasePage": "https://github.com/jasonswrench-beep/prismqda/releases/tag/v0.1.0-beta.1"
      },
      "extras": []
    }
  ],
  "web": [{ "id": "roadmap", "name": "Roadmap and feature requests", "description": "...", "url": "https://..." }],
  "comingSoon": [{ "id": "an-app-on-the-way", "name": "An app on the way", "description": "..." }]
}
```

A new field can be added without breaking older hubs (they ignore what they do not know). A change older hubs would misread raises `format` or `minimumHubVersion`, and those hubs then show only their update banner.

## Stable and preview

With **Offer preview versions** on (the default while the suite is in preview), the hub offers the newer of an app's stable and preview releases. With it off, it offers the stable release, or the preview marked Preview when there is no stable one. Turning previews off never pushes anyone backward: they keep their version until a stable one passes it. A policy file can turn previews off entirely (see [policy.md](policy.md)).
