# Android dual-version port: Minecraft Java 1.21.11

## Goal

Add Minecraft Java Edition 1.21.11 as a selectable Android game target while keeping the existing 26.3 target working. Work on this target belongs on `android-1.21.11`; do not change the existing `android` branch as part of the backport.

This document is a port plan, not a claim that the game engine already supports 1.21.11.

## Version facts

| Property | 1.21.11 | Existing 26.3 |
| --- | ---: | ---: |
| Java runtime level | 21 | 25 |
| Protocol | 774 | 777 |
| Data version | 4671 | 5023 |
| Resource pack format | 75.0 | 97.1 |
| Data pack format | 94.1 | 121.0 |

The 1.21.11 values must be checked against Mojang's version manifest and the target client/server `version.json` during implementation. Do not infer compatibility from the version string alone.

## Vanilla resource acquisition and preparation

- Use Mojang's official version manifest to resolve the exact 1.21.11 version JSON, client jar, server jar, asset index, and asset-object hashes. Verify every downloaded file against its published SHA-1 before using it.
- Prepare the game data from the user's own legitimate Minecraft Java installation or a local development copy. Do not commit, mirror, bundle, or publish Mojang's client jar, textures, sounds, fonts, language assets, or other proprietary game files in this repository or in the APK.
- Keep all downloaded inputs and generated outputs outside Git (for example under `.local/minecraft/1.21.11` and `.local/horizon-client/1.21.11`). The prepared manifest must identify 1.21.11 and data version 4671; it must never be accepted by the current 26.3 renderer, which checks for data version 5023.
- Run the matching vanilla data generator and Horizon exporters with Java 21. Export the registries, block and item definitions, tags, recipes, world-generation data, models, blockstates, textures, sounds, fonts, and language resources needed by Horizon. Record the exact manifest JSON hash, jar hashes, asset-index id/hash, exporter version, and output digest so the preparation is reproducible.
- The existing `tools/version_port.py prepare` currently stops when it sees `client_mappings` because its class-diff pipeline was built for unobfuscated releases. 1.21.11 needs a deliberate obfuscated-version path or a verified official unobfuscated development artifact before that pipeline can be used; do not remove the guard blindly or treat obfuscated class names as stable source symbols.
- The normal Android release workflow currently builds the native app and does not ship Minecraft game files. Preserve that model: users prepare/import their own game resources through the launcher.

## Horizon settings parity

The 1.21.11 target should use the same Horizon Android settings and UX as 26.3 wherever the setting is engine-independent. This includes graphics/render scale, FPS and performance controls, HUD, touch layout, keyboard/mouse, controller mappings, audio, accessibility, language, and mod/shader management. Keep the settings UI and saved preferences shared; do not fork a second 1.21.11-only settings screen.

Only settings whose meaning or supported range genuinely differs between Minecraft versions should be version-scoped. Store those values under the selected version profile, validate them against that version's capabilities, and leave the other version's settings untouched. Switching versions must not reset common Horizon preferences or silently copy incompatible world/resource settings.

## Architecture constraints

- Keep the current 26.3 implementation and its generated version constants intact.
- Do not replace the global `crates/horizon-core/src/version.rs` constants with 1.21.11 values: that would silently relabel the 26.3 engine and can corrupt protocol, save, and pack compatibility.
- Introduce an explicit version target/profile at the Android packaging boundary. Each profile must carry its own game metadata, required Java runtime, prepared resources, protocol/data versions, and native engine build.
- Share Android UI, storage, input plumbing, and code only where the two game versions have verified-compatible behavior. Keep version-dependent registries, packets, world generation, data fixes, rendering assets, and game rules isolated.
- Keep per-version prepared files and caches separate. Never reuse 26.3 prepared resources or worlds as 1.21.11 data.
- The launcher must identify the selected target and load only its matching native library/resources. If both native builds cannot fit the APK budget, use separately downloadable version packs without changing the app's single-launcher UX.

## Port milestones

1. **Baseline and source pinning**
   - Record the exact Horizon source revision and Mojang 1.21.11 client/server manifest entries.
   - Capture the current 26.3 Android build and test results as a regression baseline.
   - Inventory every 26.3 assumption in version constants, parity fixtures, resource preparation, protocol, registries, and packaging.

2. **Version-aware build and metadata**
   - Add generated metadata for 1.21.11 without editing the 26.3 metadata.
   - Add independent build targets and cache keys.
   - Make the Android launcher list both versions and route to the matching runtime.

3. **Engine backport**
   - Port version-dependent registries, block/item/entity definitions, components, recipes, tags, commands, world generation, chunk/data formats, resource loading, and protocol packets.
   - Keep shared engine changes only when parity tests prove they behave identically.
   - Implement required save/data conversion explicitly; reject unsupported future-format worlds rather than silently opening them.

4. **Android integration**
   - Build the 1.21.11 native library for supported Android ABIs.
   - Verify touch, keyboard, mouse, controller, audio, lifecycle, file access, and shader paths on Android.
   - Keep the 26.3 native library and its data path independent.

5. **Validation and release**
   - Run formatting, clippy, unit tests, parity harnesses, protocol tests, packaging checks, and both Android builds.
   - Test fresh install, upgrade, switching versions, world create/open/save/reopen, resource packs, single-player, and multiplayer against matching-version servers.
   - Publish only after both version targets pass; a 1.21.11 failure must not block or overwrite the known-good 26.3 artifact.

## Acceptance criteria

- The Android app offers 1.21.11 and 26.3 as separate selectable targets.
- Each target reports its own correct version and uses matching assets, registries, protocol, Java runtime, and save format.
- 1.21.11 can create, save, and reopen a world and connect to a matching 1.21.11 server.
- 26.3 continues to build, launch, create/open worlds, and connect to matching 26.3 servers.
- Switching targets never mixes world folders, caches, prepared assets, or native libraries.
- Common Horizon Android settings persist when switching versions; version-specific options remain isolated.
- CI builds and tests both targets independently and reports failures separately.

## Important scope note

Horizon is a Rust reimplementation of Minecraft, not a thin wrapper around a vanilla Java client. A 26.3-to-1.21.11 backport is therefore an engine compatibility project across many gameplay and data systems. Updating only the version constants or protocol number is not a valid port.
