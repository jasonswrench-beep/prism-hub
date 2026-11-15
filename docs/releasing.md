# Making a hub release

## Once: the hub key

Double-click **Make Hub Key.bat**. You choose its password. The private half is saved in `Prism Hub key` in your user folder (outside the repository, never on GitHub); keep a backup of that folder with the password. The public half is written into `brand/brand.json`; commit it. Every hub built from then on trusts catalogs signed with that key, and only those.

The hub key is separate from the apps' own update keys and from the code-signing certificate.

## Once: the repository

The repository must be **public** before any hub can download the catalog or its own updates: release files of a private repository are not downloadable without signing in.

## Each release

1. Raise `RELEASE` in `tools/build-installer.mjs`, and the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, to the new version (such as `1.0.1`). The build stops if they differ.
2. Run the checks: `npm test`, `npm run typecheck`, and `cargo test` in `src-tauri`.
3. Commit and push.
4. On the owner's computer, signed in to the signing service with its settings in the environment (the top of `tools/sign_windows.py` names them): `node tools/build-installer.mjs --sign`. It stops on any problem: a missing hub key, a program that names this computer's user folder, an installer that would download the web view's setup, or a program in the setup that is not signed and timestamped. It writes `src-tauri/target/release-files/Prism-Hub-<version>-Setup.exe` and its `.sha256.txt`, and says whether the setup is under 8 MB.
5. Make the release with exactly those two files, tagged `v<version>` at the pushed commit, with notes in plain words: what changed, who signed it, the SHA-256, and what has not been tried.
6. Run **Update Prism Catalog.bat**, so the catalog's hub entry names the new release. Installed hubs then offer the update.

The first release (1.0.0) is installed by hand from GitHub. Updates work from then on.

## Brand values

Every brand-specific value is in `brand/brand.json`. After changing one, run `node tools/apply-brand.mjs` (the build runs it too) so `tauri.conf.json` follows, and `node tools/make-icons.mjs` if the icon changed.
