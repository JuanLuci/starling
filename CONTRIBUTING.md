# Contributing to Starling

Thanks for helping. Starling is the Android port of [Horizon](https://github.com/ludovic111/horizon), Minecraft: Java Edition 26.3 rewritten in Rust. This page is the short version of how we work.

## Where a change goes

- **The game itself** (gameplay, world generation, rendering shared with the desktop, menus) belongs upstream, in Horizon, where it is checked against vanilla. Follow [Horizon's contributing guide](https://github.com/ludovic111/horizon/blob/main/CONTRIBUTING.md): vanilla is the specification, and gameplay changes come with a parity harness.
- **The Android app** belongs here: the packaging (`packaging/android/`), the app's entry (`crates/horizon-client/src/android_lib.rs`), touch controls (`touch.rs`, `game/touch.rs`), the Starling folder and imports (`import.rs`, `game/imports.rs`, `game/files_offer.rs`) and anything that only matters on a phone.

## Build it

You need Rust (stable) with the `aarch64-linux-android` target, [cargo-ndk](https://github.com/bbqsrc/cargo-ndk), the Android SDK (build-tools 35.0.0, platform 36, NDK r28c), a JDK and Python 3.10+.

```sh
python packaging/android/build_apk.py            # build the library, then the APK
python packaging/android/build_apk.py --install  # and install it over adb
```

The APK carries no game files. On its first start the app signs in, downloads them from Mojang for an account that owns the game and prepares them on the phone; to skip that while testing, prepare the resources from your own copy of Minecraft 26.3 as described in [Build from source](README.md#build-from-source), zip them as a `.starling` file and open it with the app. The setup's hand-drawn skin comes from `python packaging/lowbudget/make.py` (Pillow): run it after changing `hand.py`, `make.py` or `strings.json`, and commit `pack/` with them.

## Test on a phone

Say what you checked on a real device, and on which one. A `horizon-args.txt` in the app's own files adds command-line arguments and environment variables at launch (one per line, `NAME=value` for the environment); with `--control-socket` and `HORIZON_CONTROL_PORT`, `horizon-ctl` drives the game over `adb forward`. Empty the file afterwards so the app starts normally.

## Before a pull request

```sh
git config core.hooksPath .githooks
python3 tools/pr_ready.py
```

It runs rustfmt, clippy with warnings denied, the tests of the crates you touched, the Python tests and the public-tree check. Keep pull requests small, and write commit messages in English.

Never commit game files, decompiled source or prepared resources (`.local/`): `tools/check_public_tree.py` rejects them.
