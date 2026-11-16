<p align="center"><img src="https://github.com/jasonswrench-beep/openstage-hub/raw/main/brands/prism/PrismOpenResearch.svg" width="96" alt="The PrismOpenResearch icon: a rainbow prism with an open centre and a separated gold corner"></p>

# Prism Hub

One small Windows program that lists every app in the PrismOpenResearch suite and installs, updates and opens each one from its signed GitHub release.

- **No account.** Nothing to sign in to, and nothing about you or your computer is ever sent anywhere.
- **Stays out of the way.** It does not start with Windows unless you ask it to, and it never installs anything without your click.
- **Never installs anything unchecked.** Every installer must match its SHA-256 in the signed catalog and carry a valid, timestamped code signature from Jason Wrench, checked by Windows itself. Anything else is deleted before it runs.
- **Per user.** The hub and every Prism app install for one person, with no administrator rights.

Apache 2.0.

## Using it

Download `Prism-Hub-<version>-Setup.exe` from [Releases](https://github.com/jasonswrench-beep/prism-hub/releases) and run it. The first copy is installed by hand; after that the hub updates itself.

Each app has one button: **Install**, **Open**, **Update**, **Cancel** while it works, or **Retry** with a plain sentence about what went wrong. The three-dot menu beside it holds Open, What's new, Open install folder, Reinstall, Uninstall, and Other downloads when an app has them. Apps you installed before the hub show as installed straight away.

Offline, the hub still lists every app from the last catalog it fetched, and Open still works.

Settings: Offer preview versions, Start with Windows, Keep running in the tray, Check once a day while in the tray, and About.

## Where the code is

Prism Hub is built from the same code as OpenStage Hub, in [jasonswrench-beep/openstage-hub](https://github.com/jasonswrench-beep/openstage-hub). Everything that makes it Prism Hub (its name, icon, key, list of apps and their icons) is in that repository's `brands/prism` folder, and `Build Prism Hub.bat`, `Run Prism Hub.bat` and `Update Prism Catalog.bat` are there too. The documents (how it works, the catalog, the IT policy file, releasing) and the list of checks only a person can do are there as well.

This repository holds Prism Hub's releases: each version's signed setup, and the release named `catalog` with the signed list of apps that installed hubs read. Both have to stay here, at the addresses built into every installed Prism Hub. The code up to 1.0.1 is in this repository's history.
