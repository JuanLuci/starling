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
- CI builds and tests both targets independently and reports failures separately.

## Important scope note

Horizon is a Rust reimplementation of Minecraft, not a thin wrapper around a vanilla Java client. A 26.3-to-1.21.11 backport is therefore an engine compatibility project across many gameplay and data systems. Updating only the version constants or protocol number is not a valid port.
