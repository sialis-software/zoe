# Notices

Zoe's own code is licensed under MIT or Apache-2.0 (see `LICENSE-MIT` and `LICENSE-APACHE`). It carries and uses the following works of others.

## Bundled with the app

| Work | Source | License |
|---|---|---|
| Berean Standard Bible (BSB), with Strong's numbers | [berean.bible](https://berean.bible), as a CrossWire SWORD package | Creative Commons CC0 (public domain dedication) |
| Strong's Greek Bible Dictionary | CrossWire SWORD package, from James Strong's work (1890) | Public domain |
| Strong's Hebrew Bible Dictionary | CrossWire SWORD package, from James Strong's work (1890) | Public domain |
| M'Cheyne reading plan | Robert Murray M'Cheyne's calendar of 1842 | Public domain |
| The app icon | The Greek letter zeta, from [Wikimedia Commons](https://en.wikipedia.org/wiki/Zeta#/media/File:Zeta_uc_lc.svg) (`Zeta_uc_lc.svg`) | Public domain |
| The interface icons (`crates/app/assets/icons`) | Generated for this project with an AI assistant | Part of Zoe (MIT or Apache-2.0) |

The modules are in `crates/app/assets/modules`, with their sources listed in the `README.md` there.

## Used by the tests only (not part of the program)

- **Noto Sans** (`crates/app/tests-assets/fonts`), copyright 2022 The Noto Project Authors, under the SIL Open Font License 1.1 (`OFL.txt` there). The tests lay text out with it so that the layout is the same on every computer.

## Libraries

The libraries the program is built from, and their license texts, are in [`THIRD-PARTY-LICENSES.txt`](THIRD-PARTY-LICENSES.txt). The list is made by `scripts/third-party.sh`, and also shown in the app on the About page. `vendor/cosmic-text` is a copy of cosmic-text 0.15.0 (MIT or Apache-2.0, System76) with one fix described in `Cargo.toml`.

## Reading other modules

Zoe reads SWORD modules of the [CrossWire Bible Society](https://crosswire.org) and downloads more from its library when you ask. Those modules belong to their publishers and have their own licenses, which Zoe shows with each module. The module format is the work of the SWORD Project; Zoe is an independent implementation and contains no SWORD library code.
