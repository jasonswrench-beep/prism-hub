# Requests to the shared core

Things Prism Hub needs from `openstage-core` that the core does not offer yet. Prism Hub does not build its own version of a
shared piece when the core could offer it. Newest last. Core as read: `CORE-NOTES.md` of 2026-11-16 (commit c7e3ae1), which
has no section for the hubs.

**Status, 2026-11-16:** Prism Hub 1.0.1 and OpenStage Hub 1.0.1 are two copies of one program, each with its own brand file.
Until H1 is done, a change to how the hub behaves is still made in both copies.

## H1. One hub codebase with two brand files

**For:** development list item 3 ("One shared codebase with OpenStage Hub"). OpenStage Hub's list has the same item (its 3), so
this one request answers both.

### What the two copies are today

Compared file by file on 2026-11-16 (Prism Hub at 9be7b63, OpenStage Hub at 877ee0c), ignoring line endings:

- **The same in both:** `index.html`, `vite.config.ts`, `tsconfig.json`, `src/main.ts`, `src/menus.ts`,
  `src-tauri/src/` apart from the three files below, `src-tauri/installer-hooks.nsh`, `src-tauri/build.rs`,
  `tools/apply-brand.mjs`, `tools/build-installer.mjs`, `tools/make-icons.mjs`, `tools/prompt.mjs`, `tools/sign_windows.py`,
  `tests/ui-preview.html`.
- **Brand files, different on purpose:** `brand/` (the brand file and the suite icon), `catalog/apps.json`, `catalog/icons/`,
  the four `.bat` files (named after the hub), `README.md`, `CLAUDE.md`, `NOTICE`, `TESTING-TODO.md`, the product name in
  `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` (written by `tools/apply-brand.mjs`), the icons in
  `src-tauri/icons/` (drawn from the brand icon by `tools/make-icons.mjs`), the app rows in `tests/ui-preview-stub.ts`, and
  the names and addresses in `docs/`.
- **One real drift: each app's own icon (both 1.0.1, made the same day in two sessions).**
  - OpenStage Hub publishes each icon as a PNG beside its catalog on the `catalog` release. The catalog names it
    (`apps[].icon`: `url`, `sha256`, `size`, at most 256 KB). The hub downloads each one once into an `icons` folder, keeps it
    only if its SHA-256 matches, and checks it again when it shows it (`fetch_icons` and `icon_data` in `hub.rs`).
  - Prism Hub carries each icon inside the signed catalog as a `data:` address of an SVG or PNG (`apps[].mark.icon`, at most
    32 KB decoded, checked by `icon_ok` in `catalog.rs`). Nothing is downloaded and the icon is there offline.
  - Both are `format` 1, and a hub of either kind ignores the other's field (the row then shows the round mark), so neither
    catalog breaks the other hub.
  - This touches `src/rows.ts`, `src/style.css`, `src/types.ts`, `src-tauri/src/catalog.rs`, `hub.rs`, `state.rs`,
    `tools/catalog-lib.mjs`, `tools/update-catalog.mjs` and the tests. Everything else that differs is a name.

### Asked

1. **A home for the hub's code in the core, with a tag** (for example `hub/` and `hub-v1.1.0`), under the core's usual rule: a
   change is made in the core first and gets a new tag, and each hub moves its pin only with its own tests passing.
2. **Each hub repository keeps only its brand and its releases**: `brand/`, `catalog/apps.json` and `catalog/icons/`, its
   `.bat` files, README, docs, `TESTING-TODO.md`, `NOTICE`, and its GitHub releases (the hub's setup and the `catalog`
   release, which have to stay at the address in its brand file). The layout is the core's to choose. Prism Hub's preference,
   because it matches how the core already shares code:
   - **Rust:** the shell as a library crate taken by git tag. Each hub keeps a small binary crate in `src-tauri` with its
     `tauri.conf.json`, `build.rs`, icons and a `main.rs` that hands the library its brand. Three things in today's code
     assume the brand file sits two folders up, which a crate fetched by tag cannot reach, so the library needs them given:
     `brand.rs` (`include_str!("../../brand/brand.json")`), `HUB_VERSION` (`CARGO_PKG_VERSION`, which would become the
     library's version, not the hub's), and `tray.rs` (`include_bytes!` of `tray.png` and `tray-dot.png`). `lib.rs` calls
     `tauri::generate_context!()`, which must stay in the binary crate.
   - **The page and the tools:** an `npm pack` file on the tag's release, as `@openstage/pstat` is published, holding `src/`,
     `index.html`, `tools/` and the generic tests. `src/main.ts` imports `../brand/brand.json`, so the page needs the brand
     by an alias each hub's `vite.config.ts` sets. The tools already take every name from the brand file.
   - The other layout, one folder in the core holding the code and both brand folders and building either hub, would also
     do. It would move the brand files out of the hub repositories, which the CLAUDE.md of each hub would then say.
3. **One icon design, settled before the code is merged.** Prism Hub asks for its own (icons inside the signed catalog). The
   reason is a rule both hubs' CLAUDE.md files state: "The network is used only for the catalog, its signature, and the
   files the person asks for." OpenStage Hub's icons are downloaded without being asked for. They also add a second path from
   the network to the window with its own checks. OpenStage Hub's eight PNGs are 4 to 14 KB each, under the 32 KB limit, so
   they can go into its catalog as they are. This is Jason's ruling (see `TESTING-TODO.md`).
4. **Two small things for the shared code, found in this round:**
   - The catalog script decides a release is a preview by its version alone (`0.1.0-beta.2` is a preview, `0.2.1` is not)
     and does not read GitHub's pre-release flag. A dry run could print a note when the two disagree, so a release marked
     pre-release on GitHub with a plain version is noticed (this is how Prism Reference Manager 0.1.2 was found). Prism Hub
     has not made this change, so the two copies stay the same.
   - Prism Hub's `docs/how-it-works.md` did not list the `--install <app id>` development switch that both copies have; it
     does now. The shared docs should keep it.

### What Prism Hub does until then

It keeps its copy, takes nothing from OpenStage Hub's repository, and makes every behaviour change in both copies (the rule in
its CLAUDE.md). It will move to the core's tag, check its 8 JavaScript tests, the type check and its 21 Rust tests, and then
remove its copy of the shared files.
