# Wrench

A native mod (horizon-mixins bundle) after the Wrench of [Create](https://github.com/Creators-of-Create/Create) (by
simibubi and the Create team). Create's items and blocks are not here: the wrench is a vanilla item and it works on
vanilla blocks. The code is this mod's own.

- **Right click** with the wrench (a stick; Options > Mods > Wrench cycles stick, blaze rod, breeze rod, bone) turns
  the block you aim at a quarter turn, clockwise as you look at the face you clicked: furnaces, chests, pistons,
  observers, hoppers, logs and pillars, stairs, slabs, trapdoors, signs, heads... Stairs and trapdoors clicked on a
  side flip upside down; slabs flip from any face. A turn the block could not stay in (a torch off its wall) is
  skipped. A click it can do nothing with is used as usual.
- **Sneaking right click** takes a machine apart: pistons, observers, dispensers, droppers, hoppers, the crafter,
  redstone parts, buttons, pressure plates, rails, lightning rods. It breaks as if mined with the right tool: it drops
  itself and what it held (nothing in creative, as when mining). Its settings page turns this off.
- Never turned: halves of bigger things (beds, doors, tall plants, double chests, an extended piston), portals, game
  master blocks.

Single player and LAN (the host's world): every player with the wrench. Joined to a server: nothing (the server's
world is not the game's own).

`mixins/wrench/runtime.rs` holds the rules (with tests), `native.rs` applies them, `use.rs` hooks
`Game::use_item` (not a hook point: the mod builds on the commits whose internals it calls), and `list.rs`,
`settings.rs`, `click.rs` hook Options > Mods' hook points.

Missing next to Create's: its own item and model, the sound, turning Create's kinetic blocks (there are none),
rotating a whole contraption.
