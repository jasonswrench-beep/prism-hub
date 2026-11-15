# Policy for IT staff

Prism Hub reads policy from any of these places. Policy can only switch things off or hide
apps; a value that would switch something on is ignored, so a planted file can at most hide or
disable things.

1. `policy.json` beside `prism-hub.exe` (the hub's install folder, by default
   `%LOCALAPPDATA%\Prism Hub\`). This is where a deployment tool that installs the hub per user
   puts it.
2. `%ProgramData%\Prism Hub\policy.json`, for the whole computer. It counts only when the folder
   and the file are owned by Administrators, SYSTEM or TrustedInstaller and no other account can
   change them (anyone may create folders in ProgramData).
3. Group Policy registry values under `HKLM\Software\Policies\Prism Hub` or
   `HKCU\Software\Policies\Prism Hub`.

## policy.json

```json
{
  "update_checks": false,
  "start_with_windows": false,
  "offer_previews": false,
  "hide_apps": ["prismstatistics"]
}
```

| Key | Effect |
| --- | --- |
| `update_checks: false` | The hub never downloads the catalog. It lists apps from the last catalog it has (or the one inside its installer) and the status line says checks are turned off. Installing from that catalog still works when a person presses Install. |
| `start_with_windows: false` | The start-with-Windows setting is locked off, its start-up entry is removed, and the first-run question is not asked. |
| `offer_previews: false` | Only stable releases are offered. Apps with only a preview show no Install button. |
| `hide_apps` | Catalog ids of apps not to show (see the catalog for ids: `prismqda`, `prismstatistics`, `prism-reference-manager`). |

## Group Policy values

| Value | Type | Effect |
| --- | --- | --- |
| `UpdateChecks` | DWORD 0 | As `update_checks: false`. |
| `StartWithWindows` | DWORD 0 | As `start_with_windows: false`. |
| `OfferPreviews` | DWORD 0 | As `offer_previews: false`. |
| `HideApps` | REG_MULTI_SZ, or REG_SZ separated by commas | As `hide_apps`. |

Settings › About lists where the active policy came from.
