# Porting Java mods to Starling

How to bring a Fabric (or Forge/NeoForge) client mod to Starling as a Starling mod, for people and
for AI agents (the `starling-mod-porting` skill in `.claude/skills/` follows this page). The last
part is the port log: every port made so far, what it needed from the API and what is still
missing. Add to it with each port.

What this page is not: running Java mods (a JVM inside the game is ruled out, see
[java-mod-compat](research/java-mod-compat.md)), or carrying over the data and assets of content
mods from a modpack, which `horizon-port-pack` does (see [modpacks](modpacks.md)).

## The idea

A port reproduces what a mod **does**, not its code. A Fabric mod is Java patching the game
(mixins) and calling Fabric API; a Starling mod is a sandboxed WebAssembly component, or a
JavaScript file, that sees the game only through the `starling:mods` interfaces
([starling.wit](../crates/starling-mod-sdk/wit/starling.wit)) and the capabilities its manifest
asks for. So a port is: list the mod's features, find the Starling call for each, write the mod,
and where Starling has no call, add one to the API, for every mod after it too.

Licences: read the original's licence before reusing anything from it. Behaviour can always be
reimplemented; code, textures and translations only as their licence allows (many popular mods are
MIT or LGPL, some are All Rights Reserved). Credit the original in the manifest's description and
the source. Never take anything from Mojang's assets or code.

## Workflow

1. **List the features.** From the mod's page, wiki and in-game behaviour: each thing the player
   sees or does, one line each ("shows saturation on the hunger bar", "the tooltip of a food shows
   hunger and saturation"). Mark each must-have or later.
2. **Pick the language.**
   - **JavaScript**: `horizon-mod.toml` + `mod.js`, no build step; the game's shared engine runs
     it; best for HUD, screens and quick iteration. See
     [hello-js](../crates/starling-mod-sdk/examples/hello-js),
     [peek](../crates/starling-mod-sdk/examples/peek),
     [keystrokes](../crates/starling-mod-sdk/examples/keystrokes).
   - **Rust**: `cargo horizon-mod new`, `cargo horizon-mod build` with
     [starling-mod-sdk](../crates/starling-mod-sdk); best for heavy logic and unit tests. See
     [food-info](../crates/starling-mod-sdk/examples/food-info),
     [waypoints](../crates/starling-mod-sdk/examples/waypoints),
     [toggle-sprint](../crates/starling-mod-sdk/examples/toggle-sprint).
   - Java through TeaVM works (Java to JavaScript, then the shared engine) but has no SDK yet.
3. **Map each feature** with the tables below. Write down every feature no call covers: those are
   the port's gaps.
4. **Close the gaps in the API** (see [Adding to the API](#adding-to-the-api)), or leave them in
   the port log as "later" when they are big.
5. **Declare capabilities**: only what the mod uses (`game-read`, `world-read`, `inventory-read`,
   `chat`, `chat-send`, `hud`, `ui`, `keys`, `input`, `sound`, `net`, `storage`). A JavaScript mod
   calling an interface it did not declare stops with "uses X: capability Y is missing from its
   manifest".
6. **Test without the game** with `starling-mod run` (below), then in the game.
7. **Record the port** in the [port log](#port-log).

## Mapping: Fabric to Starling

Hooks are fields of `events` (exports); calls are `interface.function` (imports). Rust names use
`snake_case` (`game::get_player`), JavaScript `camelCase` (`getPlayer`).

### Lifecycle and events

| Fabric | Starling |
|---|---|
| `ClientModInitializer.onInitializeClient` | `events.init` returning a `registration` (which hooks the mod wants) |
| `ClientTickEvents.END_CLIENT_TICK` | `registration.tick`, `events.tick(n)` (20 a second) |
| `WorldRenderEvents.END` / per frame work | `registration.frame`, `events.frame(seconds)` |
| `ClientPlayConnectionEvents.JOIN` / `DISCONNECT` | `registration.connection`, `events.joined(server)`, `events.left` |
| `ClientPlayerEvent` dimension change | `events.dimension-changed` |
| `UseItemCallback`, `UseBlockCallback`, `AttackEntityCallback`, `AttackBlockCallback` | `registration.interaction`, `events.use-item`, `use-block`, `attack-entity`, `attack-block` (return false to cancel) |
| `ClientPlayerBlockBreakEvents.AFTER` | `events.block-broken` |
| `LivingEntity` damage (the player) | `registration.damage`, `events.player-hurt` |

### Reading the game

| Fabric | Starling |
|---|---|
| `MinecraftClient.player` (position, health, food, XP, game mode...) | `game.get-player` |
| `player.getStatusEffects()` | `game.get-effects` |
| `world.getTimeOfDay()`, weather, difficulty, server address | `game.get-clock`, `game.get-world-info` |
| `MinecraftClient.crosshairTarget` | `game.get-target` (block with state and face, or entity) |
| `ClientPlayerInteractionManager` breaking progress | `game.get-mining` |
| `MinecraftClient.currentScreen` | `game.get-screen` |
| `Window.getScaledWidth/Height`, GUI scale | `game.get-gui` |
| `MinecraftClient.getCurrentFps()` | `game.get-fps` |
| `world.getBlockState(pos)` | `level.get-block` (`minecraft:oak_log[axis=y]`) |
| block name, pick item, hardness, `isToolRequired`, mineable tags, `canHarvest` | `level.get-block-info` |
| `world.getBiome`, light levels | `level.get-biome`, `level.get-light` |
| `world.getEntities()` near the player | `level.get-entities` |
| an entity's display name, max health, armor, `isBaby` | `level.get-entity-info` |
| `player.getInventory()`, main/off hand, cursor stack, hovered slot | `inventory.get-slot`, `get-held`, `get-cursor`, `get-hovered` |
| the open container's slots | `inventory.get-container` |
| `ScreenHandler.slots` by id, the cursor stack | `slots.get-menu` (capability `inventory-click`): the open menu by vanilla's slot ids, the player's inventory menu with no screen open |
| `interactionManager.clickSlot(syncId, slot, button, SlotActionType, player)` | `slots.click(slot, button, kind)`: `pickup`, `quick-move`, `swap`, `clone`, `throw`, `pickup-all`; done in order after the hook, predicted and sent as vanilla's on a server, with no screen open too |
| `Item` facts (name, max stack, durability, `FoodComponent`) | `inventory.get-item-info` |
| `options.forwardKey.isPressed()` (the game's controls) | `keys.is-control-down` (`key.forward`, `key.jump`, `key.attack`...) |
| click counting (CPS) | `keys.presses` (counts every press, even shorter than a frame) |
| touch screen? | `mobile.has-touch` |

### Drawing

| Fabric | Starling |
|---|---|
| `HudRenderCallback` | `registration.hud`, `events.hud-render(context)` with `draw.*` |
| `HudLayerRegistrationCallback` / `HudElementRegistry.attachElementBefore/After/replaceElement` | `registration.hud-elements` (`hud-part` + `before`/`after`/`replace`), `events.render-hud-element` |
| retained text lines in a corner | `hud.set-panel(corner, lines)`, `hud.clear` (drawn every frame by the game, no hook) |
| `DrawContext.fill`, `fillGradient` | `draw.fill`, `draw.gradient` (ARGB) |
| `DrawContext.drawText`, `TextRenderer.getWidth` | `draw.text` (rich text: plain, `§` codes or JSON components), `draw.text-width` |
| `DrawContext.drawItem` | `draw.item` (a stack), `draw.item-id` |
| `DrawContext.drawGuiTexture` (sprites) | `draw.sprite("hud/heart/full")`: any sprite of the GUI atlas, nine-sliced per its `.mcmeta`; `item/compass_16` (an item texture); a content mod's `<id>:my_icon` (`assets/<id>/textures/gui/sprites/my_icon.png`) |
| `DrawContext.drawTooltip` | `draw.tooltip` |
| `ButtonWidget` look | `draw.button(x, y, w, h, label, hovered, active)` |
| a dialog background | `draw.panel` |
| `MatrixStack.push/translate/scale/pop` | `draw.push(x, y, scale)`, `draw.pop` |

### Screens and input

| Fabric | Starling |
|---|---|
| a custom `Screen` | `screens.open("name")` opens `<mod id>:name`; `registration.screens`; `events.screen-render`, `screen-mouse`, `screen-key`, `screen-opened`, `screen-closed` |
| `ScreenEvents.AFTER_INIT` / `afterRender` on the game's screens | the same hooks receive the game's screens (`pause`, `inventory`, `container`, `creative`, `title`, `worlds`, `multiplayer`, `death`, `options`...): draw over them, take their mouse and keys first |
| Mod Menu's `getModConfigScreenFactory` | `screens.set-config("settings")` in `init`: a Config button on Options > Mods opens that screen |
| `Screens.getButtons(screen).add(ButtonWidget.builder(text, onPress)...)` in `ScreenEvents.AFTER_INIT`, NeoForge's `ScreenEvent.Init.Post.addListener` | `widgets.add-screen-button` in `init` (capability `ui`): a button on a game screen (`pause`, `title`, `inventory`...) placed from an `anchor` (Bedrock's `anchor_from`/`anchor_to`), drawn as that screen's buttons, with an icon and a tooltip; a press is `events.key(id, true)` then `(id, false)`. JavaScript: `widgets.addScreenButton({...}, onPress)` |
| Bedrock's touch buttons | `widgets.add-touch-button` (round, square or wide; an icon or a label; where it starts): the player moves, resizes and hides it in the touch layout editor; its id is a keybind's or its own. JavaScript: `widgets.addTouchButton({...}, onPress)` |
| Bedrock's resource packs retexturing the touch controls | `widgets.style-control("jump", label, icon, shape)`, or a pack's `assets/starling/textures/gui/sprites/touch/<name>.png` (and `<name>_pressed.png`) |
| a text field on a phone | `mobile.keyboard(true)` over the mod's screen |
| `KeyBindingHelper.registerKeyBinding` + `wasPressed` | `registration.keybinds`, `events.key(id, pressed)`, `keys.is-down(id)`; each keybind also gets a touch button on phones (wide, its name on it; `widgets.add-touch-button` with its id gives it a look) |
| `KeyBinding.setPressed` for sprint/sneak (ToggleSprint) | `input.hold("key.sprint", true)` (capability `input`; only sprint and sneak) |
| `ItemTooltipCallback` | `registration.tooltips`, `events.tooltip(stack)` returning extra lines |
| `ClientCommandRegistrationCallback` | `registration.commands`, `events.command(name, args)` |
| `ClientReceiveMessageEvents` / `ClientSendMessageEvents` | `registration.chat` / `chat-send`, `events.chat-received`, `chat-sending` |
| `player.sendMessage` (client only) | `chat.show`, `chat.show-overlay` (action bar) |
| sending chat or a command as the player | `chat-send.send` (rate limited) |
| `ClientPlayNetworking` payloads | `registration.channels`, `net.send`, `events.payload` |
| sounds for the player only | `sound.play` |
| config files (Cloth Config, YACL, owo-config) | `storage.get/set/delete` (a quota'd key/value folder) + a settings screen |

## Testing without the game: `starling-mod run`

```
starling-mod run <mod folder or .hmod> [--scenario <file.json>] [--step <json>]... \
    [--storage <dir>] [--resources <dir>] [--png <dir>] [--watch]
```

(`cargo run -p starling-mod-cli -- run ...` from this repository.) It loads the mod as the game
would into a pretend world and prints one JSON line per thing that happens: what loaded, each
step's result (draw calls, handled or not, held controls), chat lines, holds, stops with their
reason. Made to be read by a program: an agent can change the mod, run it and read the result.

- `--resources .local/horizon-client/26.3` (a prepared resources folder): real item and block
  names, food, hardness, tools, creature health, the font's widths.
- `--png <dir>`: each step that drew writes `stepN.png` (HUD, screens, tooltips, HUD panels) with
  the real font and sprites when `--resources` is given. Look at them.
- `--watch`: runs again whenever the mod or the scenario changes.
- A scenario: `{"player": {"held": "minecraft:iron_pickaxe", "food": 6}, "target": {"block":
  "minecraft:iron_ore"}, "mining": 0.4, "gui": {"width": 533, "height": 240}, "steps": [...]}`;
  `target` can be an entity (`{"entity": "minecraft:zombie", "health": 14}`).
- Steps: `tick`, `frame`, `hud`, `screen`, `click`, `type`, `press`, `close`, `command`, `chat`,
  `send`, `tooltip`, `key`, `controls` (the game's controls held), `join`, `leave`, `hurt`,
  `touch` (a touch button pressed).
- Buttons (`widgets`): a `widgets` line lists them whenever they change; `{"screen": "pause"}`
  draws the mod's buttons on that game screen and `click` presses them; `{"hud": {"touch":
  true}}` draws its touch buttons where they start.

A phone's GUI is about 533 by 240 GUI pixels: try `"gui": {"width": 533, "height": 240}` before
calling a layout done.

In the game: `--dev-mod <folder or .hmod>` on the PC client reloads the mod when it changes;
`horizon-ctl` drives the game (`touch tap X Y` puts a finger on it, `screenshot`, `state`). On a
phone, `cargo horizon-mod push` (Rust) or an `adb push` of the `.hmod` into
`/storage/emulated/0/Starling/horizon-mods` reloads it. A JavaScript `.hmod` is a zip of
`horizon-mod.toml` and `mod.js`.

## Pitfalls

- **Every registration field**: `init` returns the whole `registration` record (JavaScript too:
  copy hello-js's). Hooks a JavaScript mod leaves out answer as unhandled.
- **Colours** are ARGB for `fill` and `gradient` (`0x80000000` is half-transparent black) and RGB
  for `text`. In JavaScript they pass right whatever the alpha (the engine works around a
  componentize-js bug with values of 2^31 and more).
- **A JavaScript hook that throws** stops the mod; the log says why and where (`[mod id]
  hookName: TypeError ...`). `init` and `command` throw to fail on purpose.
- **JavaScript speed**: each call from JavaScript into the game costs a few microseconds (the
  engine's interpreter crossing into native code), and so does each builtin (`push`, `Map.get`).
  The engine queues a render hook's `fill`, `gradient`, `text`, `sprite`, `push` and `pop` and
  sends them in one call when the hook returns, with text widths cached (40 texts a frame: 0.09
  ms on the PC, were 0.4); `item`, `item-id`, `tooltip`, `button` and `panel` send the queue
  first, so a frame with fewer of them is cheaper. Reads of `game`, `level` and `inventory` are
  kept until the game's snapshot changes (once a tick) and handed out as shallow copies (the
  player: 37 us a read, 2 us kept); `keys.is-control-down` reads every control at once a hook.
  The first start after the engine changed runs on
  quick code (Winch: two seconds on one core, its code about two and a half times slower) until
  the full compile is kept. Wasmtime 36's Winch crashed the engine on ARM; 50's is on trial
  there. A mod trapping on quick code is started again on full code (at once if it was starting,
  once the full compile is kept if it was running); `STARLING_QUICK_COMPILER=off` turns quick
  code off.
- **Out of memory**: a mod has 64 MiB. The engine is built with componentize-js 0.23 or later:
  before it every string passed to the game leaked, and a mod drawing text each frame ran out in
  minutes.
- **`hud.set-panel`** is for text that changes rarely (set it when it changes); `hud-render` runs
  every frame (keep it cheap).
- **Screens are named without a namespace** for the game's (`pause`, `inventory`) and
  `<mod id>:<name>` for a mod's.
- **Touch**: no hover on a phone; anything shown on hover needs another way (a tap, a line on the
  screen; a screen button's tooltip only shows with a mouse). Fingers are big: buttons of 20 GUI
  pixels or more. A phone's top right corner is crowded (the menus' buttons): put touch buttons
  at the left or middle.
- **Buttons are keybinds**: a button's press comes to `events.key` with the button's id, so a
  button and a keybind with one id are one control (Waypoints' J key, its compass and its pause
  button all open its screen).
- **Storage keys** are `[a-z0-9_.-]{1,64}`; values are bytes (JSON as text is fine).

## Adding to the API

The API is `starling:mods@0.1.0`. A mod built before a change must still load:

- Add **functions** to interfaces; never change a record or a function's type (a mod importing it
  would no longer link). New data goes in a new record returned by a new function
  (`level.get-block-info` beside `level.get-block`).
- A new **interface** is a new import; if it needs a capability, add it to `manifest.rs`
  (`Capability`, its interfaces and its line on the Mods page).
- Do not add **hooks** to `events` (every mod would have to export them). Deliver new events
  through existing hooks or calls (`keys.presses` instead of a click event).
- Wire it: the host side in `crates/horizon-mods/src/runtime.rs`, what the game fills in
  `crates/horizon-client/src/game/mods.rs`, the pretend world in `crates/starling-mod-cli`, the
  JavaScript engine's import list in `js-engine/engine.js` (then `npm run build` there), and the
  Rust SDK's re-exports in `crates/starling-mod-sdk/src/lib.rs`.
- Test it in `crates/horizon-mods/tests/sandbox.rs` and with `starling-mod run`.

## Port log

One entry per port: the original, the port, what it does, what the port added to the API, and
what is still missing (the next port's API work).

| Original | Port | Language | Status |
|---|---|---|---|
| AppleSkin | [food-info](../crates/starling-mod-sdk/examples/food-info) | Rust | HUD and tooltips done |
| Jade / WTHIT | [peek](../crates/starling-mod-sdk/examples/peek) | JavaScript | blocks and creatures done |
| ToggleSprint | [toggle-sprint](../crates/starling-mod-sdk/examples/toggle-sprint) | Rust | done |
| Keystrokes | [keystrokes](../crates/starling-mod-sdk/examples/keystrokes) | JavaScript | done, with settings |
| Mod Menu | built into Options > Mods | game | list, descriptions, Config button done |
| Xaero's / JourneyMap waypoints | [waypoints](../crates/starling-mod-sdk/examples/waypoints) | Rust | list screen, HUD pointer, pause and touch buttons |
| Elytra Swapper | [elytra-swap](../crates/starling-mod-sdk/examples/elytra-swap) | JavaScript | done, with a touch button |

### AppleSkin → food-info

- Does: saturation and exhaustion over the hunger bar, the held food's gain blinking on the icons
  it would fill, nutrition and saturation in food tooltips.
- API used: `game.get-player` (food, saturation, exhaustion), `inventory.get-held`,
  `inventory.get-item-info` (food), `registration.hud-elements` before and after `food`,
  `draw.sprite`, `events.tooltip`.
- Missing: exhaustion is 0 on servers (they do not send it); the "estimated health regen" overlay.

### Jade / WTHIT → peek

- Does: a tooltip at the top of the screen for what the crosshair is on: a block's icon, name, a
  crop's growth, the tool it takes (green when the held item harvests it), breaking progress; a
  creature's name, hearts and armor; whose it is (Minecraft or a content mod's namespace).
- API added: `level.get-block-info`, `level.get-entity-info`, `game.get-mining`.
- Missing: container contents, furnace progress, beehive honey and the like (block entity data);
  other mods' names (needs the namespace to mod name); settings.

### ToggleSprint → toggle-sprint

- Does: V toggles sprinting, G sneaking, held until pressed again, kept between sessions; the
  original's HUD lines (toggled, held with the key, vanilla sprint, flying).
- API added: capability `input` (`input.hold`), `keys.is-control-down`.
- Missing: fly boost and descend in creative (the original's extras).

### Keystrokes → keystrokes

- Does: W A S D, both mouse buttons with clicks per second, jump; lit while held (keyboard,
  controller or the touch stick); settings screen for the clicks per second and the corner.
- API added: `keys.is-control-down`, `keys.presses`.
- Note: Horizon's extras already have a keystroke overlay; this port exists to prove the API.

### Mod Menu → Options > Mods

- Does: every installed mod with its switch, its description under it (readable without hover),
  what it may do in its tooltip, and a Config button for mods that name a settings screen.
- API added: `screens.set-config`.
- Missing: icons, authors and links pages, search and filters.

### Elytra Swapper → elytra-swap

- Does: R (or the elytra on the touch controls) puts on the elytra from the inventory in place
  of the chestplate worn, or the best chestplate in place of the elytra, with no screen open;
  says which in the action bar.
- API added: the `slots` interface (capability `inventory-click`): `get-menu` by vanilla's slot
  ids and `click`. Tested on a vanilla server: its own `execute if items` saw the swap.
- Missing: swapping on take-off (needs a flying event or reading the player's gliding each tick:
  `game.get-player().gliding` and a tick hook would do).

### Waypoints (Xaero's / JourneyMap's waypoints)

- Does: a screen of saved places (add here, pick, forget, rename with the phone's keyboard), the
  picked one's direction and distance at the top of the HUD; opened by J, by its touch button (a
  compass, at the left) or by its button on the pause menu.
- API added: the `widgets` interface (screen buttons, touch buttons, restyled controls); GUI
  sprites of other namespaces in the atlas.
- Missing: a minimap, waypoints drawn in the world (3D), death points.

### Candidates

| Original | What it needs |
|---|---|
| JEI / EMI / REI | recipes read (`recipes` interface), an overlay beside inventories |
| Xaero's / JourneyMap world map | world-read over a wider area, map storage, a full-screen map screen |
| Inventory Profiles (auto refill) | `slots` (done) and a tick hook: refill the held stack when it runs out |
| Mouse Tweaks / sorting | built into Horizon's extras already (tweaks.rs) |
| Litematica | world-read, drawing in the world (3D), storage |
| Simple Voice Chat | server side, audio |
