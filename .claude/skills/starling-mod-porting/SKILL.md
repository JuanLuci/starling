---
name: starling-mod-porting
description: Port a Java (Fabric, Forge, NeoForge) client mod to Starling as a Starling mod (JavaScript or Rust, the starling:mods WIT API), or write a new Starling mod, test it without the game with starling-mod run (JSON lines and PNGs), and add what the API lacks. Use when asked to port, recreate or "make a Starling version of" a Minecraft Java mod (AppleSkin, Jade, JEI, minimaps, HUD and inventory mods...), to write or debug a Starling mod, or to extend the mod API.
---

# Porting a mod to Starling

Starling mods are sandboxed WebAssembly components (Rust, `crates/starling-mod-sdk`) or JavaScript
files run by the game's shared engine. They see the game only through
`crates/starling-mod-sdk/wit/starling.wit` and the capabilities their `horizon-mod.toml` declares.
A port reproduces the original's behaviour; it never runs or copies Java.

Read `docs/mod-porting.md` first: the workflow, the Fabric → Starling mapping tables, the
pitfalls and the port log. Then:

1. **Features.** List what the original does for the player, one line each, must-have or later.
   Check its licence before reusing any of its code, textures or text; never use Mojang's.
2. **Language.** JavaScript for HUD/screen mods and fast iteration (copy
   `crates/starling-mod-sdk/examples/peek` or `keystrokes`); Rust for heavy logic (copy
   `food-info` or `toggle-sprint`; build with `cargo run -p cargo-horizon-mod -- horizon-mod build`
   in the mod's folder; add new Rust examples to the workspace members).
3. **Map** each feature with the tables in `docs/mod-porting.md`. Any feature with no call is a gap.
4. **Gaps.** Either add the call (rules in "Adding to the API": new functions and interfaces only,
   never change a record or add an `events` hook; wire runtime.rs, game/mods.rs, the CLI's
   pretend world, js-engine/engine.js + `npm run build`, the Rust SDK; test in
   `crates/horizon-mods/tests/sandbox.rs`) or record it as "missing" in the port log.
5. **Test without the game**, looping until it is right:
   ```
   cargo run -p starling-mod-cli -- run <mod folder or .hmod> --resources .local/horizon-client/26.3 \
       --png <scratch dir> --scenario <file.json>
   ```
   Read every JSON line (a `stopped` line has the reason) and look at every PNG. Use a phone-sized
   GUI (`"gui": {"width": 533, "height": 240}`) and the scenario fields `player`, `target`,
   `mining`, `blocks` and the `controls` step to set the scene. `--watch` reruns on every save.
6. **Test in the game** on the PC: `target/quick/horizon-client.exe --world <name> --resources
   .local/horizon-client/26.3 --dev-mod <mod> --control-socket <short path>`, driven with
   `target/quick/horizon-ctl.exe --socket <path>` (`state`, `screen`, `touch tap X Y`,
   `key e tap`, `screenshot <png>`). Do not use the user's phone without asking.
7. **Record the port** in the port log of `docs/mod-porting.md`: what it does, the API it added,
   what is missing.

Keep manifests honest: the fewest capabilities, a description saying what the mod does and, for a
port, which mod it follows.
