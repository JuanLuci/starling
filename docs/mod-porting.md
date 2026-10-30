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
   `inventory-click`, `creative`, `chat`, `chat-send`, `hud`, `ui`, `keys`, `input`, `sound`,
   `net`, `storage`). A JavaScript mod
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
| Light Overlay's / Lighty's spawn checks over an area (`isValidSpawn`, `isCollisionShapeFullBlock`, `SpawnPlacements.isSpawnPositionOk`) | `level.get-spawn-spots(from, to)`: the places a zombie has room to spawn (vanilla's own check, `noCollision` included), with their block and sky light and ground height, as typed arrays (thousands cost a millisecond); the light is the mod's to judge |
| `world.getEntities()` near the player | `level.get-entities` |
| an entity's display name, max health, armor, `isBaby` | `level.get-entity-info` |
| `player.getInventory()`, main/off hand, cursor stack, hovered slot | `inventory.get-slot`, `get-held`, `get-cursor`, `get-hovered` |
| the open container's slots | `inventory.get-container` |
| `ScreenHandler.slots` by id, the cursor stack | `slots.get-menu` (capability `inventory-click`): the open menu by vanilla's slot ids, the player's inventory menu with no screen open |
| `interactionManager.clickSlot(syncId, slot, button, SlotActionType, player)` | `slots.click(slot, button, kind)`: `pickup`, `quick-move`, `swap`, `clone`, `throw`, `pickup-all`; done in order after the hook, predicted and sent as vanilla's on a server, with no screen open too |
| Buttons on a container's window placed from its slots (Quark's chest buttons, Inventory Profiles' sort buttons: `leftPos`/`topPos` and a slot's `x`/`y`) | `widgets.add-window-button` (capability `ui`): a small button placed from the box around the container's slots or the player's inventory, as the game lays them out on the screen open (any chest size, a shulker box, the inventory, the creative inventory tab), shown on the menus it names; its icon the whole button (`<icon>_highlighted` under the mouse). JavaScript: `widgets.addWindowButton({...}, onPress)` |
| An item's place in the creative inventory (Inventory Profiles' default sort, `CreativeModeTabs` search tab order) | `inventory.get-item-order(stack)` |
| `AbstractContainerScreen.leftPos`, `topPos`, `imageWidth`, `imageHeight` (EMI's and JEI's panels beside the window) | `screens.get-window()`: the game screen's window in GUI pixels, the open recipe book moving it |
| A modded container screen's `Slot`s (what Quark and Inventory Profiles read to place their buttons on it) | `screens.set-slots([{slot, x, y}, ...])` from the mod's own screen: other mods' window buttons are placed and pressed on it as on the game's screens |
| A mod's own button icons | `assets/<id>/textures/gui/sprites/*.png` in the `.hmod`, as `<id>:<name>`; no capability needed (they load as the game starts) |
| `SlotActionType.QUICK_CRAFT` (the mouse dragging a stack across slots) | `slots.drag(slots, button)`: 0 shares the cursor's stack out evenly, 1 one each, 2 a full stack each (creative); the start, slot and end clicks in one call |
| `Item` facts (name, max stack, durability, `FoodComponent`) | `inventory.get-item-info` |
| JEI's / EMI's recipe lookups (an item's recipes and uses, by station) | `recipes.count-recipes(stack, lookup)` and `get-recipes(stack, lookup, kind, start, count)` (capability `recipes`): the crafting, furnace, campfire, stonecutting, smithing and brewing recipes the game knows, each input's choices and the outputs as stacks; a stack with components finds only its own (a Potion of Healing) |
| EMI's +, JEI's recipe transfer (the recipe book's `ServerboundPlaceRecipePacket`) | `recipes.fill(id, all)`: the recipe book's button for the recipe in the open crafting grid or furnace, as vanilla's client asks a server; false when the open screen takes no such recipe |
| `ItemGroups` and `ItemGroup.getDisplayStacks()`, the creative search (`SearchManager`) | `creative.get-tabs`, `creative.get-items(tab, search, start, count)` (capability `creative`): the tabs and items the game's creative screen shows, searched as its box searches |
| `CreativeInventoryScreen.onMouseClick` on a tab's slot | `creative.click(tab, search, index, button, kind)`: `pickup`, `quick-move`, `swap`, `clone`, `throw` with the creative screen's semantics (creative mode only) |
| `ClientPlayerInteractionManager.clickCreativeStack` / `dropCreativeStack`, the creative cursor | `creative.set-slot(slot, stack)` (the player's inventory menu's ids, 1 to 45), `creative.set-carried(stack)` (none: Destroy Item); a server gets vanilla's `SetCreativeModeSlot` |
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
| `InventoryScreen.drawEntity` (the player in the inventory) | `draw.player(x, y, width, height, size, turn)`: the head follows the mouse (ahead on a phone), `turn` degrees more |
| `Screen.getTooltipFromItem` + `drawTooltip` (an item's full tooltip) | `draw.item-tooltip(x, y, stack)`: name, lore, enchantments, attributes, the mods' lines |
| `WorldRenderEvents.AFTER_TRANSLUCENT` / a `DebugRenderer` with `RenderType.lines()` (Light Overlay's crosses, chunk borders, waypoint beams) | `world-draw.set-layer(id, {lines: [...]})` (capability `world-draw`): coloured lines with a width in pixels, from an origin, depth tested or `through-walls`; the game keeps drawing the layer until the mod sets it again or `remove-layer`s it, so no frame hook |
| Xaero's / JourneyMap's waypoint marks (a textured quad facing the camera, its name and distance) | `world-draw.set-icons(id, [...])`: a GUI sprite (the mod's own from its `assets/`) at a point, tinted, with a text over it and one under it, the same size on screen at any distance (`fixed-size`) and seen through walls if asked; the nearer drawn over the farther |
| a translucent quad layer (Lighty's carpet, a marker, a schematic's ghost) | the layer's `boxes`: see-through filled boxes, unlit |
| `Font.drawInBatch` in the world (name tags, Lighty's numbers, a waypoint's name) | the layer's `texts` (many at once: places and colours as numbers, the texts the lines of one string, `facing` `up` to lie on the ground) and `labels` (a few, rich text, `fixed-size` for a waypoint's) |

### Screens and input

| Fabric | Starling |
|---|---|
| a custom `Screen` | `screens.open("name")` opens `<mod id>:name`; `registration.screens`; `events.screen-render`, `screen-mouse`, `screen-key`, `screen-opened`, `screen-closed` |
| `ScreenEvents.AFTER_INIT` / `afterRender` on the game's screens | the same hooks receive the game's screens (`pause`, `inventory`, `container`, `creative`, `title`, `worlds`, `multiplayer`, `death`, `options`...): draw over them, take their mouse and keys first |
| Mod Menu's `getModConfigScreenFactory` | `screens.set-config("settings")` in `init`: a Config button on Options > Mods opens that screen |
| `MinecraftClient.setScreen` mixin swapping `CreativeInventoryScreen` for the mod's | `screens.replace("creative", "name")`: the mod's screen opens wherever the game's would (the inventory key in creative), `""` gives it back (the screen open changes over at once); there `slots` clicks the player's inventory as the creative screen does and the inventory key closes it |
| `Screens.getButtons(screen).add(ButtonWidget.builder(text, onPress)...)` in `ScreenEvents.AFTER_INIT`, NeoForge's `ScreenEvent.Init.Post.addListener` | `widgets.add-screen-button` in `init` (capability `ui`): a button on a game screen (`pause`, `title`, `inventory`...) placed from an `anchor` (Bedrock's `anchor_from`/`anchor_to`), drawn as that screen's buttons, with an icon and a tooltip; a press is `events.key(id, true)` then `(id, false)`. JavaScript: `widgets.addScreenButton({...}, onPress)` |
| Bedrock's touch buttons | `widgets.add-touch-button` (round, square or wide; an icon or a label; where it starts): the player moves, resizes and hides it in the touch layout editor; its id is a keybind's or its own. JavaScript: `widgets.addTouchButton({...}, onPress)` |
| Bedrock's resource packs retexturing the touch controls | `widgets.style-control("jump", label, icon, shape)`, or a pack's `assets/starling/textures/gui/sprites/touch/<name>.png` (and `<name>_pressed.png`) |
| a text field on a phone | `mobile.keyboard(true)` over the mod's screen |
| a scrollbar or slots a finger drags across, on a phone (fingers moving up or down scroll otherwise) | `mobile.drag-areas([[x, y, w, h], ...])`: there a finger presses and drags whichever way it moves |
| `KeyBindingHelper.registerKeyBinding` + `wasPressed` | `registration.keybinds`, `events.key(id, pressed)`, `keys.is-down(id)`; each keybind also gets a touch button on phones (wide, its name on it; `widgets.add-touch-button` with its id gives it a look) |
| `KeyBinding.setPressed` for sprint/sneak (ToggleSprint) | `input.hold("key.sprint", true)` (capability `input`; only sprint and sneak) |
| `ItemTooltipCallback` | `registration.tooltips`, `events.tooltip(stack)` returning extra lines |
| `TooltipComponentCallback` / `ClientTooltipComponent` (ShulkerBoxTooltip's preview, a bundle's) | `tooltips.add-grid(grid, instead)` from `events.tooltip` (capability `hud`): stacks as slots under the lines, tinted (a shulker box's colour), and with `instead` without the game's list of a container's contents; the stack's `components` (SNBT) hold what it carries |
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
  `touch` (a touch button pressed). For screens with slots: `{"click": [x, y], "press": false}`
  (a press held), `{"release": [x, y]}`, `{"drag": [x, y]}`, `{"scroll": [x, y], "lines": -1}`,
  `{"key-press": "key.keyboard.left.shift", "pressed": true}`.
- Slots: the scenario's `menu` (`{"slots": {"36": {"id": "minecraft:stone", "count": 32}},
  "carried": ...}`) is what `slots.get-menu` reads. Pickup clicks (buttons 0 and 1, -999 outside),
  `creative`'s clicks and sets change it before the step draws, as the game's simplest clicks do;
  the other clicks are lines only (`"pretended": false`).
- Creative: `"player": {"game_mode": "creative"}`; the tabs are the resources'
  `creative-inventory.json`, else a few made up; `"player": {"touch": true}` for a phone.
- Buttons (`widgets`): a `widgets` line lists them whenever they change; `{"screen": "pause"}`
  draws the mod's buttons on that game screen and `click` presses them; `{"hud": {"touch":
  true}}` draws its touch buttons where they start.
- Recipes (`recipes`): the resources' data pack recipes (shaped, shapeless, the furnaces', the
  campfire's, stonecutting, smithing upgrades, item tags expanded), else two made up; a
  `{"screen": "inventory"}` or `container` step gives `screens.get-window` its window (and the
  inventory a 2 by 2 grid for `recipes.fill`, each fill a `fill` line).
- Tooltips: `{"tooltip": "minecraft:red_shulker_box", "components": "{\"minecraft:container\":
  [...]}"}` gives the stack those components (SNBT, as the game does); the step's result lists
  the mod's lines and its grids (`tooltips.add-grid`), each stack as `[id, count]`.
- The world (`world-draw`): a `world` line whenever the layers change, each layer's lines and
  boxes counted by colour, its texts by text, the box they span and its labels; the icons
  (`set-icons`) under `icons`. The scenario's
  `blocks` are the level (`level.get-spawn-spots` finds air over air on any of them but liquids)
  and `"light": [block, sky]` its light everywhere.

A phone's GUI is about 533 by 240 GUI pixels: try `"gui": {"width": 533, "height": 240}` before
calling a layout done.

### A content mod's blocks: `starling-mod blocks`

```
starling-mod blocks <content mod .hmod or folder> --resources <dir> --out <dir> \
    [--only <text>] [--tile <pixels>]
```

Draws every block the mod declares on contact sheets (`<dir>/blocks-NN.png`, 48 tiles a page),
its models baked as the game bakes them (parents, composite and OBJ models, the blockstate's
turns; a pane or a wall alone), and a kinetic part a second time turned an eighth of a turn with
its extra model and shaft ends. Its JSON lines say what would draw wrong: `missing-model`,
`unresolved-slot`, `missing-texture`, `mesh` (an OBJ that does not read), `nothing-drawn` (a
model only the mod's code builds: Create's copycats) and `fighting-faces` (two textures in one
place: they flicker). Look at the pages before taking a block to the phone. Light and the blocks
around are the world's: the PC client (`horizon-ctl`) shows those.

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
  hookName: TypeError ...`). `init` and `command` throw to fail on purpose. A call returning
  `result` (`slots.click`, `creative.set-slot`, `widgets.add-screen-button`...) throws when it
  errs: wrap the ones that can (outside creative mode, a full click queue) in `try`.
- **JavaScript speed**: each call from JavaScript into the game costs a few microseconds (the
  engine's interpreter crossing into native code), and so does each builtin (`push`, `Map.get`).
  The engine queues a render hook's `fill`, `gradient`, `text`, `sprite`, `item`, `item-id`,
  `push` and `pop` and sends them in one call when the hook returns, with text widths cached (40
  texts a frame: 0.09 ms on the PC, were 0.4); `tooltip`, `item-tooltip`, `button`, `panel` and
  `player` send the queue first, so a frame with fewer of them is cheaper. Reads of `game`, `level` and `inventory` are
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
| Bedrock Edition's creative inventory | [bedrock-inventory](../crates/starling-mod-sdk/examples/bedrock-inventory) | JavaScript | in place of the creative screen, mouse and touch |
| Quark's chest buttons / Inventory Profiles' sorting | [inventory-buttons](../crates/starling-mod-sdk/examples/inventory-buttons) | JavaScript | sort, take all, put all, on chests and the inventory |
| EMI | [emi](../crates/starling-mod-sdk/examples/emi) | JavaScript | index, favourites, recipes and uses by station, filling the grid |
| Light Overlay / Lighty | [light-overlay](../crates/starling-mod-sdk/examples/light-overlay) | JavaScript | cross, number, tile and marker modes |
| ShulkerBoxTooltip | [shulker-box-tooltip](../crates/starling-mod-sdk/examples/shulker-box-tooltip) | JavaScript | compact and full previews, mouse and touch |

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

### Bedrock's creative inventory → bedrock-inventory

- Does: in creative mode, in place of the creative screen: the creative screen as the game draws
  it (its tabs above and below, the tab's items, the search, the scrollbar; its hotbar row a
  sixth row of items) with its Survival Inventory tab's window beside it, as Bedrock puts the
  inventory beside the items (the armour around the player, the off hand, inventory, hotbar,
  Destroy Item, and a 2x2 crafting grid in the room at the player's right), both at once; the
  Survival Inventory tab shows only that window, the tabs around it. The windows are the game's
  textures (a resource pack's too). The mouse as on the game's screens (Shift, double click, a
  drag sharing a stack out, middle click, 1-9/F/Q, a click outside throws); fingers as Bedrock's
  (tap to pick and to put, hold to choose how much (on the crafting result, how many crafts),
  double tap across or to craft all, a finger drawn across slots shares the pick out, drag and
  drop, tap outside to throw); stacks slide to where they went; the tabs' items take away what
  is put on them. Java UI goes back to the game's screen, Bedrock UI (a button there) comes back;
  the choice and the inventory-only view are kept.
- API added: the `creative` interface and capability (`get-tabs`, `get-items`, `click`,
  `set-slot`, `set-carried`), `screens.replace`, `screens.set-slots` (its slots, so other mods'
  window buttons sit on it), `draw.player`, `draw.item-tooltip`, `slots.drag`
  (vanilla's quick craft), `mobile.drag-areas` (where a finger drags instead of scrolling: the
  scrollbar, the inventory); `slots.click` on the stand-in clicks as the creative screen (each
  changed slot sent as `SetCreativeModeSlot`, the crafting grid, armour and off hand too);
  `get-menu` shows clicks from the next hook.
- Missing: saved hotbars (C and X); the survival inventory in its place (`replace` takes only
  `creative`).

### Quark's chest buttons and sorting → inventory-buttons

- Does: on chests, double chests, barrels, ender chests and shulker boxes (the containers of 27
  slots or more, as Quark's), Take all and Sort in the container's title row, Deposit all and
  Sort in the inventory's; on the inventory and the creative screen's inventory tab, Sort. Take
  all and Deposit all Shift-click every stack of their part (the hotbar stays, as Quark's Insert
  keeps it); Sort merges stacks and orders them as the creative inventory lists items (Inventory
  Profiles Next's default), the hotbar kept, in clicks a tick (a long sort goes on over the next
  ticks, planned again from the menu as it then is), so a vanilla server takes it, as Inventory
  Profiles does (Quark sorts on its server). Its own 12-pixel icons, drawn as the game's buttons
  are (black outline, light and dark bevel, white outline when highlighted).
- API added: `widgets.add-window-button`, placed by the game from a part's slots (so no screen
  of the mod's own layout: a modded container screen whose slots the game knows gets them too);
  `inventory.get-item-order`; a mod's own GUI sprites without `registries`.
- Missing: Quark's Shift variants (only the items already on the other side), sorting locked
  slots, Inventory Profiles' column and row sorts. A mod's own screen shows the buttons when it
  says where it draws its slots (`screens.set-slots`), as Bedrock Inventory does: neither mod
  knows the other.

### EMI → emi

- Does: every item beside the inventory, chests, crafting tables, furnaces and the creative
  screen, in the creative inventory's order, a page at a time, searched as the creative search
  searches by a box under the window (the phone's keyboard comes up); favourites (A) left of the
  window, kept. R or a click shows an item's recipes, U or a right click its uses, from the index,
  a recipe or the game's slots; the recipes screen has a tab per station with how many (crafting
  table, furnace, blast furnace, smoker, campfire, stonecutter, smithing table, brewing stand),
  recipes laid out as their station's window, a slot's choices in turn each second, cooking time
  and experience; Recipes and Uses switch, Backspace goes back. + puts a crafting or furnace
  recipe's ingredients in the open grid (Shift: as many as can be) and goes back to it. Every key
  has a button for a phone: ++ for Shift +, Back for Backspace, a star (the favourite, framed in
  gold) for A. The recipes screen is drawn as EMI's: vanilla's light panel 176 wide, the stations'
  tabs over it, a bar for the station and one for the page with their arrows, the station in a
  strip at its left, recipes in EMI's layouts (crafting's large result, a cooking arrow filling
  over its time), + and ++ right of each, the index and favourites beside it. On a touch screen
  (no hover) a tap picks a stack, framed, with its tooltip, and the buttons under the recipes act
  on it; a second tap shows its recipes. The index starts under the game's close button there.
- Game fix found with it: a mod's screen over the inventory now takes the fingers; the touch
  inventory under it took them (a tap on EMI's recipes picked the hidden inventory's stacks).
- API added: the `recipes` interface and capability (`count-recipes`, `get-recipes`, `fill`),
  `screens.get-window`. Item stacks' `name` is the name the player sees, as the interface says
  (it was the chat's bracketed one: `[Oak Planks]`). A page of recipes can be hundreds of stacks:
  fetched in the click's or key's hook (an event's time), not each frame.
- Missing: EMI's world interactions (stripping, waxing), tags shown as tags, the recipe tree,
  cheat mode, its own search syntax (`@mod`, `#tag`), a crafting grid's "craftable" filter.

### Light Overlay and Lighty → light-overlay

- Does: where monsters can spawn within 8, 12 or 16 blocks, by the game's own check (a block to
  spawn on, room for a zombie's box): green where block light keeps them off (hidden on
  request), orange where they spawn at night (sky light over 7), red where they spawn at any
  time, as Lighty colours them; not in mushroom fields. Four modes: Cross (Light Overlay's lines
  corner to corner), Number (Lighty's block light beyond the sky light, lying on the ground and
  turned to be read from where the player looks), Tile (Lighty's see-through carpet) and Marker
  (a see-through block). F7 or its touch button turns it on and off, F8 or Config picks the mode,
  the green places and the reach, kept. It looks again when the player gets to another block,
  each second, and two ticks after a block placed or broken.
- API added: the `world-draw` interface and capability (layers of lines, boxes, texts and
  labels, depth tested or through walls), `level.get-spawn-spots`. A keybind's release now
  reaches the mod wherever it lands (F8 opened its screen, whose key took the release, and the
  second F8 was lost).
- Missing: Lighty's farmland overlay, Light Overlay's colour settings; nothing is drawn on a
  block's face other than the ground's.

### ShulkerBoxTooltip → shulker-box-tooltip

- Does: a shulker box's tooltip shows what it holds as slots in the box's colour, in place of the
  game's list of its first five stacks; so does any stack carrying a container's items (a chest
  or barrel picked with its contents). Each kind of item once with how many there are
  (ShulkerBoxTooltip's compact preview), or each slot where it is (its full one): its Config
  (on the Mods page) picks which, kept, and Shift shows the other. A phone has no Shift: there
  the Config is the way, the tap's tooltip showing the one picked.
- API added: `tooltips.add-grid` (with the `hud` capability). The game's tooltips (inventory,
  creative, stations, the recipe book, a finger's long press) draw a mod's grids under the
  lines, the box placed as vanilla places it. A grid's colour crosses from JavaScript as its
  bits (the engine's `js-tooltips`): componentize-js passes a u32 of 2^31 or more wrong when it
  is a function's argument rather than inside a list.
- Missing: ShulkerBoxTooltip's ender chest preview (the server keeps that inventory), its lock
  key, the preview of a box placed in the world.

### Waypoints (Xaero's / JourneyMap's waypoints)

- Does: a screen of saved places (add here, pick, forget, rename with the phone's keyboard), the
  picked one's direction and distance at the top of the HUD; opened by J, by its touch button (a
  compass, at the left) or by its button on the pause menu. In the world, each place of the
  dimension has a beam of its colour up from it (hidden by what is in front, as JourneyMap's
  beacons) and on the beam a diamond of its colour, its name over it and its distance under it,
  seen through hills and the same size at any distance (as Xaero's waypoints); the picked one
  ringed in white. The diamonds are the mod's own sprites.
- API added: the `widgets` interface (screen buttons, touch buttons, restyled controls); GUI
  sprites of other namespaces in the atlas; `world-draw` (the beams), `world-draw.set-icons`
  (the diamonds and their texts); `cargo horizon-mod build` packs the crate's `assets/` and
  `data/`, so a Rust mod has sprites of its own.
- Missing: a minimap, death points, waypoint colours and icons of the player's choosing.

### Candidates

| Original | What it needs |
|---|---|
| JEI / REI | as emi: `recipes`, `screens.get-window`, `creative.get-items` |
| Xaero's / JourneyMap world map | world-read over a wider area, map storage, a full-screen map screen |
| Inventory Profiles (auto refill) | `slots` (done) and a tick hook: refill the held stack when it runs out |
| Mouse Tweaks / sorting | built into Horizon's extras already (tweaks.rs) |
| Litematica | world-read over the schematic, ghost blocks (`world-draw` boxes are a start: textured block models are not), storage |
| Simple Voice Chat | server side, audio |
