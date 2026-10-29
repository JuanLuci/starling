# Native mods on Starling

Horizon's native mods ([native-mods.md](native-mods.md)) are horizon-mixins bundles: Rust that hooks the game's own
functions and is compiled into the game. On a computer the launcher downloads Rust and builds the game with them. A
phone cannot build the game (Rust and the NDK are gigabytes, and the build takes hours on a phone), so Starling builds
it in the cloud and the phone swaps in the result:

1. **The build.** GitHub builds this version's game library (`libhorizon_android.so`, about 30 MB) with a set of mods
   compiled in (`.github/workflows/native-mods.yml`, `packaging/android/native_build.sh`). It publishes it as two
   assets of the `native-builds` release: `v<version>__<mods>.so` and `v<version>__<mods>.json`, the JSON saying what
   went in and the library's SHA-256.
2. **The download.** In the game, the Native mods card (`game/native_builds.rs`) lists the builds for this version,
   downloads one into the app's files (`files/native/<name>/`), checks its SHA-256 against the JSON (both from GitHub
   over HTTPS), and chooses it for the next start (`files/native/current`).
3. **The swap.** At the next start `StarlingActivity` loads the chosen library in place of the APK's own
   (`NativeMods.java`), so Java's native methods bind to it. NativeActivity still loads the APK's library: its entry
   point is `starling_on_create` (the manifest's `android.app.func_name`, `android_lib.rs`), which finds the chosen
   build already loaded (`STARLING_GAME_LIBRARY`) and starts the game there. Activity and game share one library.

**Safety nets.** A build is only loaded by the version it was made for (after an update it waits for its rebuild). A
build that closes twice before its first frame is set aside: the next start plays the APK's own game and the card
says why. "Play without native mods" stops using one. Nothing is installed: the APK stays the release.

## What builds

Only the mods in `native-mods/` (each folder `native-mods/<id>/mixins/<id>/`, the registry's `.hnmod` layout). A native
mod runs with the game's full access, unlike Starling's WebAssembly mods, so each is read before it is added there.
`native-mods/prebuilt.txt` lists the sets built for every release (`android.yml` asks for them after publishing).
Another set is asked for with an issue (`.github/ISSUE_TEMPLATE/native-build.yml`): the card's **Ask for another
set** opens the form with the version filled in, and the player, signed in to GitHub, submits it. The workflow reads
the issue, builds, publishes and comments; the card finds the build afterwards.

| Mod | What it is |
|---|---|
| `wrench` | After Create's Wrench (Create by simibubi and the Create team). A stick (or blaze rod, breeze rod, bone: its settings page) turns the block you aim at a quarter turn; sneaking takes a machine apart (pistons, observers, hoppers, redstone parts, rails), which drops itself and what it held. |

## The build (native_build.sh)

1. horizon-mixins at Horizon's launcher's pinned commit (`MIXINS_REV`), built once into the cache.
2. The source of the commit (git archive, without links), and copies of the bundles, their pins set to that commit.
3. **One identity for the game module.** In Starling, `crates/horizon-client/src/game.rs` is compiled by two crates:
   horizon-client's binary and horizon-android (`android_lib.rs`). horizon-mixins refuses a module destination
   reached under two names (`E_TARGET_AMBIGUOUS`), so while it plans, horizon-android's library is an empty file (the
   lockfile stays as it is). The bundles' targets stay Horizon's (`horizon_client::game::Game<'_>::use_item`), so
   Horizon's own native mods apply unchanged.
4. `horizon-mixins apply` into a stage, then into a build folder kept between builds: files whose bytes changed get
   the time of now, the others keep theirs, so Cargo recompiles only what the mods touched.
5. `build_apk.py --profile fast`, as a release: the stripped library, an APK with the mods in (for testing), and
   `build.json`.

## Where it builds

- **GitHub's runners** always work: public repositories' Actions are free. Not timed there yet; `android.yml`'s
  release, the same profile with 3 jobs and swap, takes about 12 minutes.
- **The build VM until 2026-11-21**, when set up: the repository variable `STARLING_VM` = `on` and Azure's federated
  login (`AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`). A run starts the VM, builds on its
  self-hosted runner (label `starling-vm`), and deallocates it when no other build waits. On the 21st, or without the
  setup, it builds on GitHub's runners by itself.

The VM is not left on: at US$ 0.484 an hour, 24 hours a day until the 21st would be about US$ 270, more than the
credit. Started per build it costs cents a build.

### Setting up the VM (the owner, once)

1. Azure login for the workflow, no password stored: an app registration with a federated credential for the
   subject `repo:JuanLuci/starling:ref:refs/heads/android` (issues and dispatches run on the default branch), with the
   role **Virtual Machine Contributor** on the VM `starling-build` only. Put its client, tenant and subscription ids in
   the repository's variables.
2. On the VM: the GitHub runner (Settings → Actions → Runners → New self-hosted runner, label `starling-vm`) as a
   service, with `PATH` holding `~/.cargo/bin` and `ANDROID_HOME=~/android` in its `.env`.
3. Settings → Actions → General → "Require approval for all external contributors": a self-hosted runner on a
   public repository must not run a stranger's pull request.
4. The variable `STARLING_VM` = `on`.

## Security

- **No secret in the APK.** The game only reads public GitHub pages and assets.
- **An issue's text is anyone's**: the workflow reads it from the environment, never pasted into a script, and builds
  only versions with a tag and mods in `native-mods/`.
- **The mods' code is compiled where no token is**: the build job's checkouts keep no credentials, and the jobs that
  publish and comment do not build.
- **The library is checked** against the SHA-256 in its JSON, both from the `native-builds` release, which only this
  repository's workflows write.

## Status (2026-10-29)

Tried:

- **Wrench's rules**: its 8 tests pass (`rustc --edition 2024 --test runtime.rs`).
- **The build on the VM**, Starling `f2da8d3` with the Wrench: horizon-mixins applied it (4 injections, 2 modules),
  the game's `use_item` starts with its hook, its text is in the library, which exports both `starling_on_create` and
  `ANativeActivity_onCreate`, with no warning from the mod. From an empty build folder (8 vCPUs): 252 s with the quick
  profile, 225 s with fast (311 crates; horizon-mixins built once before, in about a minute). The fast library is 64
  MB stripped, the APK with it 33 MB.
- **The workflow on GitHub** (a dispatch for 0.1.3): the request, the runner, the tag's checkout, horizon-mixins and
  the script ran; the build stopped at `E_TARGET_NOT_FOUND` for `native_mods`, as it must: 0.1.3 has no hook points.
  **0.1.4 is the first version that builds**; nothing was published.
- **CI** builds the APK with the loader and the card.

Not tried yet: anything on a phone (the start through `starling_on_create`, the card, the download, the swap, the
Wrench in a world), the VM's runner and Azure login, a player's issue.
