//! Loading and calling mods with wasmtime's component model. A Starling mod links
//! `starling:mods@0.1.0` (starling-mod-sdk/wit); one built for Horizon links its `horizon:mods`
//! 0.1 interfaces (horizon-mod-sdk/wit, Horizon's own), which this host offers too, and keeps 0.1's
//! hooks, with Horizon's `command`, `ui`, `render` and `input`. A server mod links Horizon's
//! `horizon-server-mod` world (API 0.3).
use crate::manifest::{Capability, Manifest, Package, STORAGE_DIR, Side};
use server_bindings::{
    HorizonServerMod,
    exports::horizon::mods::server_events::Registration as ServerRegistration,
    horizon::mods::{players, world as mod_world},
};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap,
    component::{Component, HasSelf, Linker},
};

mod starling {
    wasmtime::component::bindgen!({
        path: "../starling-mod-sdk/wit",
        world: "starling-mod",
        // `creative`'s outputs carry stacks, and outputs compare.
        additional_derives: [PartialEq],
    });
}
mod v1 {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "horizon-mod",
    });
}
/// The server mods' world: `log` and `storage` are the client world's.
mod server_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "horizon-server-mod",
        additional_derives: [PartialEq],
        with: {
            "horizon:mods/log": super::v1::horizon::mods::log,
            "horizon:mods/storage": super::v1::horizon::mods::storage,
        },
    });
}
/// Horizon's capability `ui` beyond `horizon-mod` (docs/mods.md): a second world, so Horizon
/// mods built before it (no `ui-events` export) still instantiate as `horizon-mod` and simply
/// hear nothing of their widgets.
mod ui_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "ui-host",
    });
}
/// Horizon's capabilities `render` and `input` (docs/mods.md): the game draws the shapes.
mod render_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "render-host",
    });
}
use render_bindings::RenderHost;
/// Horizon's capability `input`'s records (WIT `interface input`).
pub use render_bindings::horizon::mods::input::{Event as InputEvent, HeldItem, ItemUse};
/// What Horizon's capability `render` asks the game to draw (WIT `interface render`).
pub use render_bindings::horizon::mods::render::{
    Corner as RenderCorner, FarTerrain as ModFarTerrain, FillShape, FrameRate, ItemShape,
    Minimap as ModMinimap, Point as RenderPoint, QuadShape, Rect as RenderRect, Shape, SpriteShape,
    TextShape, Zoom as ModZoom,
};
/// Horizon's capability `recipes`'s imports (docs/mods.md): the game's recipes and items.
mod recipes_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "recipes-host",
    });
}
/// Capability `ui`'s recipe viewer panel (WIT `interface browser`).
pub use recipes_bindings::horizon::mods::browser::{
    Click as BrowserClick, Event as BrowserEvent, ItemKey as BrowserItemKey,
    Layout as BrowserLayout, RecipeView as BrowserRecipe, SlotClick as BrowserSlotClick,
    Stack as BrowserStack,
};
/// What Horizon's capability `recipes` reads (WIT `interface recipes`); Starling's are
/// [`Recipe`] and [`RecipeKind`].
pub use recipes_bindings::horizon::mods::recipes::{
    Item as RecipeItem, Kind as HorizonRecipeKind, Recipe as HorizonRecipe, Stack as RecipeStack,
};
use recipes_bindings::{
    RecipesHost,
    horizon::mods::{browser, recipes as horizon_recipes},
};
/// Horizon's `game.raycast`'s hit.
pub use v1::horizon::mods::game::Hit;
/// Horizon's server mods' types (`horizon-server-mod`); Starling's [`BlockPos`] and [`GameMode`]
/// keep the plain names.
pub use server_bindings::{
    exports::horizon::mods::server_events::{
        Argument, ArgumentKind, ArgumentValue, BlockPos as ServerBlockPos, ServerCommand,
    },
    horizon::mods::players::{GameMode as ServerGameMode, Player as ServerPlayer},
    horizon::mods::world::BlockChange,
};
type ServerEvents = server_bindings::exports::horizon::mods::server_events::Guest;
use ui_bindings::UiHost;
/// The widget model a Horizon `ui` screen or settings page shows (WIT `interface ui`), for the
/// game to draw with its menus' own widgets.
pub use ui_bindings::horizon::mods::ui::{
    ButtonWidget, ItemWidget, ListWidget, Screen as UiScreen, TextWidget, Widget as UiWidget,
};
// bindgen's type: no derive.
#[allow(clippy::derivable_impls)]
impl Default for SpawnSpots {
    fn default() -> Self {
        SpawnSpots {
            positions: Vec::new(),
            block_light: Vec::new(),
            sky_light: Vec::new(),
            tops: Vec::new(),
        }
    }
}
impl SpawnSpots {
    /// Adds a place.
    pub fn push(&mut self, pos: [i32; 3], block_light: u8, sky_light: u8, top: f32) {
        self.positions.extend(pos);
        self.block_light.push(block_light);
        self.sky_light.push(sky_light);
        self.tops.push(top);
    }
}
pub use starling::exports::starling::mods::events::{
    ChatAction, HudElement, HudPart, KeyEvent, Keybind, MouseAction, MouseEvent, Placement,
    RenderContext, SendAction,
};
pub use starling::starling::mods::{
    creative::{ClickKind as CreativeClick, ItemPage, Tab, TabKind},
    game::{BlockHit, Clock, Effect, EntityHit, Gui, Mining, Player, Target, WorldInfo},
    hud::{Corner, Line},
    inventory::{Container, Food, ItemInfo},
    level::{BlockInfo, Entity, EntityInfo, SpawnSpots},
    recipes::{Lookup, Recipe, RecipeKind},
    slots::{ClickKind, Menu},
    tooltips::ItemGrid,
    types::{BlockPos, Direction, GameMode, Hand, ItemStack},
    // `Shape` is Horizon's `render` shape: a button's is `ButtonShape`.
    widgets::{Anchor, ScreenButton, Shape as ButtonShape, TouchButton, WindowButton, WindowPart},
    world_draw::{
        Boxes as WorldBoxes, Facing, Icon as WorldIcon, Label as WorldLabel, Layer as WorldLayer,
        Lines as WorldLines, Point, Texts as WorldTexts,
    },
};
use starling::{
    exports::starling::mods::events::Registration,
    starling::mods::{
        chat, chat_send, creative, draw, game, hud, input, inventory, keys, level, log as mod_log,
        mobile, net, recipes, screens, slots, sound, storage, tooltips, types, widgets, world_draw,
    },
};

/// A mod's linear memory cap.
const MEMORY: usize = 64 << 20;
/// Fuel (about one unit per WebAssembly instruction) per call: a bound on
/// runaway loops, far above the hooks' time budgets.
const FUEL_INIT: u64 = 2_000_000_000;
const FUEL_TICK: u64 = 50_000_000;
const FUEL_FRAME: u64 = 10_000_000;
const FUEL_RENDER: u64 = 20_000_000;
const FUEL_EVENT: u64 = 100_000_000;
/// A JavaScript mod's first call of a render hook compiles the functions it runs (the engine
/// compiles a function the first time it is called: a screen's first frame can cost eight of
/// its next), so each render hook's first call gets an event's fuel.
const FUEL_FIRST_RENDER: u64 = FUEL_EVENT;
/// Blocks a mod may set per `world.set-blocks` call.
pub const BLOCKS_PER_CALL: usize = 4096;
/// Chat lines a mod may show per call, HUD lines and their length.
const CHAT_PER_CALL: usize = 8;
/// `command.run-quiet` lines one call of a mod may queue (a quiet payload's
/// cap, `horizon_protocol::quiet::MAX_LINES`).
const QUIET_PER_CALL: usize = 256;
/// How far `game.raycast` sees, in blocks.
pub const RAYCAST_MAX: f32 = 1024.0;
const HUD_LINES: usize = 16;
const LINE_CHARS: usize = 256;
/// Longer texts: chat components, tooltips, drawn text.
const TEXT_CHARS: usize = 4096;
/// Draw calls one render hook may make.
const DRAW_OPS: usize = 4096;
/// `chat-send.send`: messages per second, and their length (the server's limit).
const SENDS_PER_SECOND: usize = 4;
const SEND_CHARS: usize = 256;
/// `net.send`'s payload limit (ServerboundCustomPayloadPacket's).
const PAYLOAD_BYTES: usize = 32 << 10;
/// `level.get-entities`' cap.
const ENTITIES: usize = 128;
/// `slots.click`s (and `creative`'s clicks and sets) one call may queue.
const CLICKS_PER_CALL: usize = 64;
/// `mobile.drag-areas` kept.
const DRAG_AREAS: usize = 64;
/// `world-draw`'s caps: a mod's layers, and the lines and labels among them.
const WORLD_LAYERS: usize = 16;
const WORLD_LINES: usize = 65536;
const WORLD_BOXES: usize = 16384;
const WORLD_TEXTS: usize = 16384;
const WORLD_LABELS: usize = 256;
const WORLD_ICONS: usize = 256;
/// `level.get-spawn-spots`' longest side.
const SPAWN_SPOTS_SIDE: i32 = 64;
/// The items one `creative.get-items` may list.
const CREATIVE_PAGE: u32 = 256;
/// The game's screens a mod's may stand in for (`screens.replace`).
pub const REPLACEABLE_SCREENS: [&str; 1] = ["creative"];
/// Buttons one mod may put on the game's screens, and on the touch controls.
const SCREEN_BUTTONS: usize = 32;
const TOUCH_BUTTONS: usize = 16;
const WINDOW_BUTTONS: usize = 16;
/// The game's own touch buttons a mod may restyle (`widgets.style-control`).
pub const TOUCH_CONTROLS: [&str; 11] = [
    "jump",
    "sneak",
    "attack",
    "use",
    "inventory",
    "chat",
    "command",
    "pause",
    "camera",
    "drop",
    "offhand",
];
/// Total bytes a mod's `storage` may hold on disk, across all its keys.
const STORAGE_QUOTA: u64 = 4 << 20;
/// Distinct keys a mod's `storage` may hold: `STORAGE_QUOTA` alone doesn't
/// bound a flood of zero- or near-zero-byte files (each still a file name,
/// a directory entry and an inode).
const STORAGE_MAX_KEYS: usize = 64;
/// Bytes `storage::set` may write across one hook call: fuel bounds a
/// mod's CPU time per call, not the time a synchronous disk write takes, so
/// a cheap-in-fuel loop replacing one key with large payloads needs its own
/// cap, separate from the on-disk quota it would otherwise stay under.
const STORAGE_WRITE_BUDGET: u64 = 1 << 20;

/// What `world-read` mods see of the level: the loaded chunks around the player, taken once a
/// tick by the game.
pub trait LevelView: Send + Sync {
    /// `minecraft:oak_log[axis=y]`.
    fn block(&self, pos: [i32; 3]) -> Option<String>;
    fn biome(&self, pos: [i32; 3]) -> Option<String>;
    /// Block light, sky light.
    fn light(&self, pos: [i32; 3]) -> Option<(u8, u8)>;
    /// What the game shows of the block at `pos` (`level.get-block-info`).
    fn block_info(&self, _pos: [i32; 3]) -> Option<BlockInfo> {
        None
    }
    /// `level.get-spawn-spots` from `from` to `to` (the least corner, then the greatest).
    fn spawn_spots(&self, _from: [i32; 3], _to: [i32; 3]) -> SpawnSpots {
        SpawnSpots::default()
    }
}
/// The game's items by id, for `inventory.get-item-info`.
pub trait ItemCatalog: Send + Sync {
    fn info(&self, id: &str) -> Option<ItemInfo>;
}
/// The creative screen's tabs and their items, for `creative`.
pub trait CreativeCatalog: Send + Sync {
    fn tabs(&self) -> Vec<Tab>;
    /// Up to `count` of tab `tab`'s items from `start` (the search tab's that match `search`).
    fn items(&self, tab: &str, search: &str, start: usize, count: usize) -> ItemPage;
    /// `inventory.get-item-order`: the stack's place in the search tab with nothing searched
    /// (its components' own place, else its item's first); None for one the tab lacks.
    fn order(&self, stack: &ItemStack) -> Option<u32>;
}
/// The game's recipes, for `recipes`.
pub trait RecipeCatalog: Send + Sync {
    /// How many recipes `lookup` finds for `stack` (by its item, and its components when it has
    /// any) of each kind there are any of, in order.
    fn count(&self, stack: &ItemStack, lookup: Lookup) -> Vec<(RecipeKind, u32)>;
    /// Up to `count` of the recipes of `kind` `lookup` finds for `stack`, from `start`.
    fn recipes(
        &self,
        stack: &ItemStack,
        lookup: Lookup,
        kind: RecipeKind,
        start: usize,
        count: usize,
    ) -> Vec<Recipe>;
    /// A recipe's kind and the grid it needs: its width and height, a shapeless one's inputs
    /// and 0.
    fn shape(&self, id: &str) -> Option<(RecipeKind, u32, u32)>;
}
/// What the open screen's recipe book places (`recipes.fill`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Crafting {
    /// A crafting grid this many cells a side (the inventory's 2, a crafting table's 3).
    Grid(u32),
    /// A furnace of this kind: smelting, blasting or smoking.
    Furnace(RecipeKind),
}
/// A text's width in GUI pixels.
pub type Measure = Arc<dyn Fn(&str) -> f32 + Send + Sync>;

/// What mods may read of the game, copied once per tick by the game (the parts a running mod's
/// capabilities reach).
#[derive(Clone, Default)]
pub struct Snapshot {
    pub player: Option<Player>,
    pub effects: Vec<Effect>,
    pub clock: Option<Clock>,
    pub world: Option<WorldInfo>,
    /// None: the crosshair is on nothing.
    pub target: Option<Target>,
    pub mining: Option<Mining>,
    pub screen: Option<String>,
    pub gui: Option<Gui>,
    pub fps: u32,
    /// 41 slots: hotbar, main, armour (feet to head), offhand.
    pub inventory: Vec<Option<ItemStack>>,
    pub cursor: Option<ItemStack>,
    pub hovered: Option<ItemStack>,
    pub container: Option<Container>,
    /// The open menu by vanilla's slot ids (`slots.get-menu`).
    pub menu: Option<Menu>,
    /// Nearest first.
    pub entities: Vec<Entity>,
    /// What the game shows of `entities` (`level.get-entity-info`).
    pub entity_info: Vec<EntityInfo>,
    pub level: Option<Arc<dyn LevelView>>,
    pub items: Option<Arc<dyn ItemCatalog>>,
    /// The creative tabs (`creative`).
    pub creative: Option<Arc<dyn CreativeCatalog>>,
    /// The recipes (`recipes`), and what the open screen's recipe book places.
    pub recipes: Option<Arc<dyn RecipeCatalog>>,
    pub crafting: Option<Crafting>,
    /// The open game screen's window (`screens.get-window`): x, y, width, height.
    pub window: Option<[f32; 4]>,
    pub measure: Option<Measure>,
    /// The mods' keybinds held now, as `<mod id>:<keybind id>`.
    pub keys_down: Vec<String>,
    /// The player plays with a touch screen (`mobile.has-touch`).
    pub touch: bool,
    /// The screen in GUI pixels (Horizon's capability `render`'s `gui-size`).
    pub gui_size: (f32, f32),
    /// The main hand's stack (Horizon's capability `input`'s `held`).
    pub held: Option<HeldItem>,
    /// The game's frame counter: fps, min, 1% low (`render.frame-stats`).
    pub frame: (u32, u32, u32),
    /// The movement and click mappings Horizon's `input.mapping` answers for:
    /// (mapping, the bound key's label, held).
    pub mappings: Vec<(&'static str, String, bool)>,
    /// Attack and use clicks in the last second.
    pub cps: (u32, u32),
    /// The HUD font's advance per character from U+0000
    /// (`render.text-width`); empty: 6 each.
    pub font_widths: Arc<[f32]>,
    /// The players on the server, for server mods.
    pub players: Vec<ServerPlayer>,
    /// The first block along the player's view within [`RAYCAST_MAX`] and
    /// its distance from the eye, for `game.raycast`; the game works it out
    /// only while a mod calls it ([`Mods::wants_raycast`]).
    pub view_hit: Option<(f32, Hit)>,
    /// Every item and recipe, for Horizon's capability `recipes`; the game makes them
    /// once, and only while a mod reads them ([`Mods::wants_recipes`]).
    pub horizon_recipes: Option<Arc<Recipes>>,
    /// The `book-id`s of the recipes the player's book holds.
    pub known_recipes: Arc<[i32]>,
    /// The open screen takes recipes (`recipes.place`).
    pub placing: bool,
    /// The player is in creative mode (`recipes.give`).
    pub creative_mode: bool,
}

/// Horizon's capability `recipes`'s tables: `items` in the registry's order, which
/// each [`RecipeStack::item`] indexes.
#[derive(Clone, Debug, Default)]
pub struct Recipes {
    pub items: Vec<RecipeItem>,
    pub recipes: Vec<HorizonRecipe>,
}

/// Which mods a game starts: the window's game the client mods, the game
/// simulating the world the server mods, single player in the frame both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Runs {
    Client,
    Server,
    Both,
}
impl Runs {
    fn starts(self, side: Side) -> bool {
        match self {
            Runs::Both => true,
            Runs::Client => side != Side::Server,
            Runs::Server => side == Side::Server,
        }
    }
}

/// The game's controls mods read and hold (`keys.is-control-down`, `keys.presses`,
/// `input.hold`), in the order of [`Controls`].
pub const CONTROLS: [&str; 9] = [
    "key.forward",
    "key.back",
    "key.left",
    "key.right",
    "key.jump",
    "key.sneak",
    "key.sprint",
    "key.attack",
    "key.use",
];
/// Each of [`CONTROLS`] now: held, and how many times it has been pressed. The game sets them
/// every frame ([`Mods::set_controls`]).
pub type Controls = [(bool, u64); 9];

/// What mods asked the game to do since the last [`Mods::drain`].
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// `chat.show`: a line after the mod's name (rich text).
    Chat { name: String, text: String },
    /// `chat.show-overlay`: the action bar.
    Overlay { text: String },
    /// A mod command's feedback, red when `error`.
    Feedback { text: String, error: bool },
    /// Horizon's `command.run`: a vanilla command line, without its slash, to run as the player.
    Command { line: String },
    /// `command.run-quiet`: vanilla command lines to run as the player with
    /// nothing in the chat (`horizon_protocol::quiet` on a server).
    QuietCommands { mod_id: String, lines: Vec<String> },
    /// The mod was turned off: it trapped, overran or failed to start.
    Stopped { name: String, reason: String },
    /// `players.tell`: a line after the mod's name, to the player with this id.
    Tell {
        name: String,
        player: String,
        text: String,
    },
    /// `players.broadcast`: a line after the mod's name, to every player.
    Broadcast { name: String, text: String },
    /// A server mod command's feedback, to the player who typed it.
    Reply {
        player: String,
        text: String,
        error: bool,
    },
    /// `world.set-blocks`: changes to make in `dimension`, in order, each
    /// state checked by the [`WorldView`] lent for the call.
    SetBlocks {
        name: String,
        dimension: String,
        blocks: Vec<BlockChange>,
    },
    /// Horizon's `ui.open-screen`: the mod's entry in `entries()`; what it shows is
    /// [`Mods::screen`].
    OpenUiScreen { owner: usize },
    /// Horizon's `ui.close-screen`: the mod closed its own screen (no `closed` back).
    CloseUiScreen { owner: usize },
    /// Horizon's `render.shake`: strength 0 to 1, fading out over `seconds` (≤ 10).
    Shake { strength: f32, seconds: f32 },
    /// Horizon's `recipes.place`: the recipe book's click on this display id, all of
    /// it when `all`.
    PlaceRecipe { book_id: i32, all: bool },
    /// Horizon's `recipes.give`: a stack for a creative player.
    Give {
        item: String,
        count: u32,
        components: String,
    },
    /// `chat-send.send`: a chat line or `/command` as the player.
    Send { message: String },
    /// `sound.play`.
    Sound { id: String, volume: f32, pitch: f32 },
    /// `net.send`.
    Payload { channel: String, data: Vec<u8> },
    /// `screens.open`: `<mod id>:<name>`.
    OpenScreen { screen: String },
    /// `screens.close`: the mod's screen, if one is open.
    CloseScreen { id: String },
    /// `screens.keyboard`: the on-screen keyboard over the mod's screen.
    Keyboard { id: String, shown: bool },
    /// `slots.click`: a click on the open menu's slot (vanilla's id, -999 outside).
    Click {
        id: String,
        slot: i32,
        button: u8,
        kind: ClickKind,
    },
    /// `slots.drag`: the cursor's stack dragged across the open menu's slots `slots` with
    /// button `button` held (QUICK_CRAFT).
    Drag {
        id: String,
        slots: Vec<i32>,
        button: u8,
    },
    /// `mobile.drag-areas`: where on mod `id`'s open screen a finger drags instead of scrolling
    /// (x, y, width, height in GUI pixels).
    DragAreas { id: String, areas: Vec<[f32; 4]> },
    /// `screens.set-slots`: where mod `id`'s open screen draws the open menu's slots (menu slot,
    /// x, y of its 16-pixel square in GUI pixels).
    ScreenSlots {
        id: String,
        slots: Vec<(u32, f32, f32)>,
    },
    /// `recipes.fill`: the recipe book's placing of recipe `recipe` in the open screen.
    FillRecipe {
        id: String,
        recipe: String,
        all: bool,
    },
    /// `creative.click`: a click on item `index` of tab `tab`'s list for `search`.
    CreativeClick {
        id: String,
        tab: String,
        search: String,
        index: u32,
        button: u8,
        kind: CreativeClick,
    },
    /// `creative.set-slot`: the player's inventory menu's slot `slot` set.
    CreativeSlot {
        id: String,
        slot: u32,
        stack: Option<ItemStack>,
    },
    /// `creative.set-carried`: the cursor's stack set.
    CreativeCarried {
        id: String,
        stack: Option<ItemStack>,
    },
    /// `input.hold`: mod `id` holds `control` (`key.sprint` or `key.sneak`), or lets it go.
    Hold {
        id: String,
        control: String,
        held: bool,
    },
}

/// The game's world as capability `world` reads it during a call: the
/// game lends one with [`Mods::lend_world`] and takes it back after.
pub trait WorldView: std::any::Any + Send {
    /// The block state at a position, `minecraft:oak_log[axis=y]`; `None`
    /// where no chunk of that dimension is loaded.
    fn block(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String>;
    /// The biome at a position, `minecraft:plains`; `None` as for blocks.
    fn biome(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String>;
    /// Whether `dimension` is one the server has.
    fn has_dimension(&self, dimension: &str) -> bool;
    /// `Err` (why) for a state string that names no block state.
    fn check_state(&self, state: &str) -> Result<(), String>;
}

/// The view lent for the current calls, shared by every mod's store.
type WorldSlot = Arc<Mutex<Option<Box<dyn WorldView>>>>;

/// One drawing call of a mod's render hook, in GUI pixels.
#[derive(Clone, Debug)]
pub enum DrawOp {
    Fill {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: u32,
    },
    Gradient {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        top: u32,
        bottom: u32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        color: u32,
        shadow: bool,
    },
    Item {
        x: f32,
        y: f32,
        stack: ItemStack,
    },
    ItemId {
        x: f32,
        y: f32,
        id: String,
    },
    Sprite {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        sprite: String,
    },
    Tooltip {
        x: f32,
        y: f32,
        lines: Vec<String>,
    },
    Button {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: String,
        hovered: bool,
        active: bool,
    },
    Panel {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Push {
        x: f32,
        y: f32,
        scale: f32,
    },
    Pop,
    /// `draw.player`.
    Player {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        turn: f32,
    },
    /// `draw.item-tooltip`.
    ItemTooltip {
        x: f32,
        y: f32,
        stack: ItemStack,
    },
}

/// A mod's text panel, drawn every frame until it changes.
#[derive(Clone, Debug)]
pub struct Panel {
    pub corner: Corner,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Installed and turned off by the player.
    Off,
    Running,
    /// Turned on, and run by another game: a server mod in the window while
    /// the world runs on its server thread.
    Elsewhere,
    /// Could not load or start, or stopped while running.
    Failed(String),
}

/// One installed mod, for the Mods page.
pub struct Entry {
    pub path: PathBuf,
    /// None when the manifest could not be read (`status` says why).
    pub manifest: Option<Manifest>,
    pub status: Status,
    /// The mod's average time per call of its busiest hook.
    pub cost: Duration,
    run: Option<Running>,
    modified: Option<SystemTime>,
    /// It stopped on the quick compiler's code: it starts again once its full code is kept.
    retry_full: bool,
}
impl Entry {
    /// The running mod has a settings page (Horizon's capability `ui` set widgets with
    /// `ui.set-settings`, or `input` registered keys the player may bind there): its row in
    /// Options > Mods gains a button.
    pub fn has_ui_settings(&self) -> bool {
        self.run.as_ref().is_some_and(|r| {
            let d = r.store.data();
            !d.ui.settings.is_empty() || !d.input.keys.is_empty()
        })
    }
}

/// A mod's buttons (`widgets`): on the game's screens and on the touch controls, and the looks it
/// gave the game's own touch buttons.
#[derive(Clone, Debug, Default)]
pub struct Widgets {
    pub screen: Vec<ScreenButton>,
    pub touch: Vec<TouchButton>,
    pub styles: Vec<ControlStyle>,
    pub window: Vec<WindowButton>,
}

/// `widgets.style-control`: one of [`TOUCH_CONTROLS`] with the mod's label, icon and shape ("" for
/// the game's).
#[derive(Clone, Debug, PartialEq)]
pub struct ControlStyle {
    pub control: String,
    pub label: String,
    pub icon: String,
    pub shape: ButtonShape,
}

/// The hooks a mod asked for, whichever API it was built for.
#[derive(Clone, Default)]
pub struct Hooks {
    pub commands: Vec<String>,
    pub tick: bool,
    pub frame: bool,
    pub chat: bool,
    pub chat_send: bool,
    pub hud: bool,
    pub hud_elements: Vec<HudElement>,
    pub screens: bool,
    pub tooltips: bool,
    pub keybinds: Vec<Keybind>,
    pub channels: Vec<String>,
    pub connection: bool,
    pub interaction: bool,
    pub damage: bool,
}
impl From<Registration> for Hooks {
    fn from(r: Registration) -> Self {
        Hooks {
            commands: r.commands,
            tick: r.tick,
            frame: r.frame,
            chat: r.chat,
            chat_send: r.chat_send,
            hud: r.hud,
            hud_elements: r.hud_elements,
            screens: r.screens,
            tooltips: r.tooltips,
            keybinds: r.keybinds,
            channels: r.channels,
            connection: r.connection,
            interaction: r.interaction,
            damage: r.damage,
        }
    }
}
impl From<v1::exports::horizon::mods::events::Registration> for Hooks {
    fn from(r: v1::exports::horizon::mods::events::Registration) -> Self {
        Hooks {
            commands: r.commands,
            tick: r.tick,
            frame: r.frame,
            chat: r.chat,
            ..Hooks::default()
        }
    }
}

/// The component's exports, by the API it was built for.
enum Guest {
    /// Built for Horizon's API 0.1.
    Horizon(v1::HorizonMod),
    Starling(starling::StarlingMod),
    /// A server mod (Horizon's API 0.3), and the hooks its `init` asked for (its [`Hooks`] are
    /// only its commands' names).
    Server(HorizonServerMod, ServerRegistration),
}

struct Running {
    store: Store<State>,
    guest: Guest,
    /// A Horizon mod's `ui-events` export; none for a Starling mod, a server mod or one built
    /// before `ui`.
    ui: Option<UiHost>,
    hooks: Hooks,
    capabilities: Vec<Capability>,
    /// The render hooks called already (`hud`, `element`, `screen`).
    rendered: Vec<&'static str>,
    /// It imports Horizon's `game.raycast`.
    raycasts: bool,
    /// It imports Horizon's capability `recipes`.
    reads_recipes: bool,
}
impl Running {
    /// A server mod's hooks; None for a client mod.
    fn server(&self) -> Option<&ServerRegistration> {
        match &self.guest {
            Guest::Server(_, r) => Some(r),
            Guest::Horizon(_) | Guest::Starling(_) => None,
        }
    }
}

/// What Horizon's capability `ui` pushed since the hooks last ran.
#[derive(Default)]
struct UiState {
    /// The screen `ui.open-screen` last pushed, until `ui.close-screen`.
    screen: Option<UiScreen>,
    /// The widgets `ui.set-settings` last pushed (the mod's page in Options > Mods).
    settings: Vec<UiWidget>,
    /// The calls since the last hook ended, for [`Output`].
    open: bool,
    close: bool,
}

/// Horizon's capability `ui`'s panel beside container screens (`browser`).
#[derive(Default)]
struct BrowserState {
    items: Vec<BrowserStack>,
    /// Which `set-items` or `set-recipes` call the panel is from, unique
    /// across mods and reloads, so the game converts them once.
    version: u64,
    /// `set-recipes`' title and recipes, shown instead of the items.
    recipes: Option<RecipePage>,
    events: VecDeque<BrowserEvent>,
}
/// What `browser.set-recipes` shows: a title, tabs (`tab` the selected
/// one) and the recipes.
#[derive(Clone, Debug)]
pub struct RecipePage {
    pub title: String,
    pub tabs: Vec<BrowserStack>,
    pub tab: u32,
    pub recipes: Vec<BrowserRecipe>,
}
static BROWSER_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// Items a panel may hold, and events kept for `browser.poll`.
const BROWSER_ITEMS: usize = 8192;
/// `recipes.place` and `give` calls a hook call may make.
const RECIPE_ACTIONS: usize = 36;
/// Recipes `set-recipes` may show, slots in one, and stacks a slot cycles.
const BROWSER_RECIPES: usize = 256;
const RECIPE_SLOTS: usize = 9;
const RECIPE_TABS: usize = 16;
const SLOT_STACKS: usize = 64;
const BROWSER_EVENTS: usize = 256;
impl BrowserState {
    fn push(&mut self, event: BrowserEvent) {
        if self.events.len() == BROWSER_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

struct State {
    name: String,
    id: String,
    /// A JavaScript mod's source, which the shared engine asks for (`js-source.get`).
    js_source: Option<String>,
    snapshot: Arc<Snapshot>,
    /// Which snapshot it is (`js-source.snapshot`): changed with it.
    snapshot_id: u32,
    controls: Controls,
    out: Vec<Output>,
    panel: Option<Panel>,
    /// Some while a render hook runs: what it drew.
    draw: Option<Vec<DrawOp>>,
    /// Some while the tooltip hook runs: the grids it added, and whether instead of the game's
    /// lines of a container's contents.
    grids: Option<Vec<(ItemGrid, bool)>>,
    /// Its settings screen (`screens.set-config`), `<mod id>:<name>`.
    config: Option<String>,
    /// The game's screens its own stand in for (`screens.replace`): the game's name, then
    /// `<mod id>:<name>`.
    replaces: Vec<(String, String)>,
    widgets: Widgets,
    /// Its `world-draw` layers by id, in the order first set.
    world: Vec<(String, WorldLayer)>,
    /// Its layers' icons (`world-draw.set-icons`) by id, in the order first set.
    icons: Vec<(String, Vec<WorldIcon>)>,
    /// The mod's payload channels.
    channels: Vec<String>,
    /// When `chat-send.send` last sent, within the last second.
    sent: VecDeque<Instant>,
    /// Horizon's capability `ui`: its screen and settings page.
    ui: UiState,
    /// Horizon's capability `render`'s HUD shapes, drawn until replaced.
    shapes: Vec<Shape>,
    /// Horizon's capability `render`'s corner panel.
    corner: Option<CornerPanel>,
    /// Horizon's capability `render`'s zoom, when the mod zooms.
    zoom: Option<ModZoom>,
    /// Horizon's capability `render`'s minimap, when the mod shows one.
    minimap: Option<ModMinimap>,
    /// Horizon's capability `render`'s far terrain, when the mod draws one.
    far_terrain: Option<ModFarTerrain>,
    /// Horizon's capability `input`: claimed items, keys and their events.
    input: InputState,
    /// Horizon's capability `ui`'s panel beside container screens.
    browser: BrowserState,
    /// Capability `storage`'s folder; made on first `set`.
    data_dir: PathBuf,
    /// Bytes `storage::set` may still write before this hook call ends
    /// (reset per call in [`Mods::call`]): fuel bounds a mod's CPU time but
    /// not disk I/O time, so a loop of small-fuel, large-payload `set`s
    /// needs its own budget.
    storage_budget: u64,
    limits: Limits,
    /// Capability `world`'s view of the game's world, lent for the calls.
    world_view: WorldSlot,
}

/// A mod's memory and instance limits, noting a memory growth they refused: a mod that runs out
/// of memory usually traps right after, and says so instead of "a panic".
struct Limits {
    inner: StoreLimits,
    /// The bytes a refused growth asked for.
    refused: Option<usize>,
}

impl wasmtime::ResourceLimiter for Limits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let grows = self.inner.memory_growing(current, desired, maximum)?;
        if !grows {
            self.refused = Some(desired);
        }
        Ok(grows)
    }
    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.inner.table_growing(current, desired, maximum)
    }
    fn instances(&self) -> usize {
        self.inner.instances()
    }
    fn tables(&self) -> usize {
        self.inner.tables()
    }
    fn memories(&self) -> usize {
        self.inner.memories()
    }
}

/// The installed mods and the engine running them. `Mods::default()` is
/// the state with none: no engine, and every hook a no-op.
#[derive(Default)]
pub struct Mods {
    engine: Option<(Engine, Linker<State>)>,
    entries: Vec<Entry>,
    snapshot: Arc<Snapshot>,
    controls: Controls,
    out: Vec<Output>,
    /// Where compiled mods are kept ([`Mods::load_cached`]).
    cache: Option<PathBuf>,
    /// The shared JavaScript engine, compiled once for every JavaScript mod.
    js: Option<Component>,
    /// Counts the snapshots ([`Mods::set_snapshot`]).
    snapshot_id: u32,
    world: WorldSlot,
    /// Moves on whenever a mod starts or stops: the game resends the server
    /// mods' commands to players when it changes.
    generation: u64,
}

// The game loads mods on a thread of their own: compiling one can take seconds.
const _: fn() = || {
    fn send<T: Send>() {}
    send::<Mods>();
};

/// What the chat hooks made of a received line.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatVerdict {
    Keep,
    Hide,
    /// Shown instead (rich text).
    Replace(String),
}

impl Mods {
    /// Changes whenever the running mods (so their commands) change.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Whether a running mod may use capability `world`: only then is a
    /// view worth lending.
    pub fn wants_world(&self) -> bool {
        self.entries.iter().any(|e| {
            e.status == Status::Running
                && e.manifest
                    .as_ref()
                    .is_some_and(|m| m.capabilities.contains(&Capability::World))
        })
    }
    /// Lends the game's world to capability `world` for the calls until
    /// [`Mods::take_world`]; with none lent, `get-block` finds nothing and
    /// `set-blocks` checks only its count.
    pub fn lend_world(&mut self, view: Box<dyn WorldView>) {
        if let Ok(mut slot) = self.world.lock() {
            *slot = Some(view);
        }
    }
    /// The view [`Mods::lend_world`] lent, back to the game.
    pub fn take_world(&mut self) -> Option<Box<dyn WorldView>> {
        self.world.lock().ok()?.take()
    }

    /// Reads every mod at `paths` and starts those `enabled` says are on,
    /// client and server mods alike. A command that `reserved` (vanilla's
    /// commands) or an earlier mod already uses is refused.
    pub fn load(
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
    ) -> Mods {
        Mods::load_for(Runs::Both, paths, enabled, reserved)
    }
    /// [`Mods::load`], starting only the mods `runs` says this game runs; the
    /// others that are on are [`Status::Elsewhere`].
    pub fn load_for(
        runs: Runs,
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
    ) -> Mods {
        Self::load_cached_for(runs, paths, enabled, reserved, None)
    }

    /// [`Mods::load`], keeping each mod's compiled code in `cache` (a folder only the game
    /// writes) for the next start: compiling a large mod, such as one carrying a JavaScript
    /// engine, takes seconds on a computer and more on a phone. Compiled code no installed mod
    /// is made of any more is removed.
    pub fn load_cached(
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
        cache: Option<&Path>,
    ) -> Mods {
        Self::load_cached_for(Runs::Both, paths, enabled, reserved, cache)
    }
    /// [`Mods::load_cached`], starting only the mods `runs` says this game runs, as
    /// [`Mods::load_for`].
    pub fn load_cached_for(
        runs: Runs,
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
        cache: Option<&Path>,
    ) -> Mods {
        let mut mods = Mods {
            cache: cache.map(Path::to_path_buf),
            ..Mods::default()
        };
        // The digests of every installed mod's code, on or off, kept in the cache.
        let mut digests = Vec::new();
        // Of the copies of one mod (a download's `name (2).hmod` beside `name.hmod`), the newest
        // file runs, whatever the names' order; the older ones stay off and say so.
        let packages: Vec<_> = paths
            .iter()
            .map(|p| (modified(p), Package::read(p)))
            .collect();
        let mut newest: HashMap<&str, usize> = HashMap::new();
        for (i, (modified, package)) in packages.iter().enumerate() {
            if let Ok(package) = package {
                newest
                    .entry(package.manifest.id.as_str())
                    .and_modify(|best| {
                        if *modified > packages[*best].0 {
                            *best = i;
                        }
                    })
                    .or_insert(i);
            }
        }
        for (i, (path, (modified, package))) in paths.iter().zip(&packages).enumerate() {
            let modified = *modified;
            let mut entry = Entry {
                path: path.clone(),
                manifest: None,
                status: Status::Off,
                cost: Duration::ZERO,
                run: None,
                modified,
                retry_full: false,
            };
            match package {
                Err(e) => entry.status = Status::Failed(e.clone()),
                Ok(package) => {
                    let copy = newest[package.manifest.id.as_str()];
                    let on = enabled(&package.manifest.id) && copy == i;
                    entry.manifest = Some(package.manifest.clone());
                    if let (Some(_), Some(wasm)) = (cache, &package.wasm) {
                        digests.push(digest(wasm));
                    }
                    if let (Some(_), Some(_), Some(engine)) =
                        (cache, &package.js, js_engine::DIGEST)
                    {
                        digests.push(engine.to_owned());
                    }
                    if on && !runs.starts(package.manifest.side) {
                        entry.status = Status::Elsewhere;
                    } else if on {
                        // Content is read before the world loads; a mod whose
                        // content is refused stays off, with the reason.
                        let content = crate::Content::read(path, &package.manifest);
                        entry.status =
                            match content.and_then(|_| mods.start(path, package, &reserved)) {
                                Ok(run) => {
                                    entry.run = run;
                                    Status::Running
                                }
                                Err(e) => Status::Failed(e),
                            };
                    }
                    if copy != i {
                        let newer = paths[copy]
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        entry.status = Status::Failed(format!("an older copy: {newer} runs"));
                    }
                }
            }
            if let (Status::Failed(reason), Some(m)) = (&entry.status, &entry.manifest) {
                log::warn!("Horizon: mod {} did not start: {reason}", m.id);
            }
            mods.entries.push(entry);
        }
        if let (Some(dir), Some((engine, _))) = (cache, &mods.engine) {
            prune(dir, engine, &digests);
        }
        mods
    }

    fn engine(&mut self) -> Result<&(Engine, Linker<State>), String> {
        if self.engine.is_none() {
            let engine = make_engine(wasmtime::Strategy::Cranelift, true)?;
            let mut linker = Linker::new(&engine);
            starling::StarlingMod::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            v1::HorizonMod::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            players::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            mod_world::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            UiHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            RenderHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            RecipesHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            self.engine = Some((engine, linker));
        }
        Ok(self.engine.as_ref().expect("made above"))
    }

    /// The shared JavaScript engine's component, compiled (or read from the cache) once. With
    /// nothing kept, it is compiled by the quick compiler for now (seconds, where the full one
    /// takes a phone tens of them) while the full one compiles it in the background for the next
    /// start; the component then belongs to that quick engine ([`Component::engine`]).
    fn js_component(&mut self) -> Result<Component, String> {
        if let Some(component) = &self.js {
            return Ok(component.clone());
        }
        let cache = self.cache.clone();
        let (engine, _) = self.engine()?;
        let engine = engine.clone();
        let component = js_engine::component(&engine, cache.as_deref())?;
        self.js = Some(component.clone());
        Ok(component)
    }

    /// Starts a mod's code; `Ok(None)` for a content-only mod, which has
    /// nothing to run (the game reads its content before the world loads).
    /// `path` is the mod's own `.hmod` or folder, for capability `storage`'s
    /// per-mod directory. A mod that fails on the quick compiler's code is started again on the
    /// full compiler's, which every mod gets from then on.
    fn start(
        &mut self,
        path: &Path,
        package: &Package,
        reserved: &dyn Fn(&str) -> bool,
    ) -> Result<Option<Running>, String> {
        STARTED_QUICK.set(false);
        QUICK_TRAPPED.set(false);
        match self.start_once(path, package, reserved) {
            Err(e) if STARTED_QUICK.get() && QUICK_TRAPPED.get() => {
                log::warn!(
                    "Horizon: mod {} failed on quick code ({e}): compiling it fully",
                    package.manifest.id
                );
                QUICK_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
                self.js = None;
                self.start_once(path, package, reserved)
            }
            other => other,
        }
    }

    fn start_once(
        &mut self,
        path: &Path,
        package: &Package,
        reserved: &dyn Fn(&str) -> bool,
    ) -> Result<Option<Running>, String> {
        let m = &package.manifest;
        if package.wasm.is_none() && package.js.is_none() {
            return Ok(None);
        }
        // Two running mods with the same id would share one `data_dir`
        // (capability `storage`): the first loaded keeps the id.
        if self
            .entries
            .iter()
            .any(|e| e.run.is_some() && e.manifest.as_ref().is_some_and(|other| other.id == m.id))
        {
            return Err(format!("another running mod already uses id {}", m.id));
        }
        let server = m.side == Side::Server;
        let taken: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .filter(|r| r.server().is_some() == server)
            .flat_map(|r| r.hooks.commands.iter().cloned())
            .collect();
        let snapshot = self.snapshot.clone();
        let snapshot_id = self.snapshot_id;
        let world_view = self.world.clone();
        let cache = self.cache.clone();
        let js_component = match &package.js {
            Some(_) => Some(self.js_component()?),
            None => None,
        };
        let (engine, linker) = self.engine()?;
        let (component, js_linker) = match (&package.wasm, js_component) {
            (Some(wasm), _) => (compile(engine, wasm, cache.as_deref())?, None),
            (None, Some(component)) => {
                // The engine imports every interface: the mod gets the ones its manifest
                // declares, and a call into another stops it.
                let linker = js_linker(component.engine(), &m.capabilities, &component)
                    .map_err(|e| e.to_string())?;
                (component, Some(linker))
            }
            (None, None) => return Ok(None),
        };
        // The quick compiler's engine for a mod started before its full compile.
        STARTED_QUICK.set(!Engine::same(component.engine(), engine));
        let engine = &component.engine().clone();
        let for_starling = js_linker.is_some() || {
            let ty = component.component_type();
            let imports: Vec<String> = ty
                .imports(engine)
                .map(|(name, _)| name.to_string())
                .collect();
            m.check_imports(imports.iter().map(String::as_str))?;
            ty.exports(engine)
                .any(|(name, _)| name.starts_with("starling:mods/events@0.1."))
        };
        // A function this Horizon doesn't have: say so, not wasmtime's linking error.
        let functions = match &package.wasm {
            // A Starling mod may be built by newer tools than this check reads.
            Some(wasm) if for_starling => crate::api_level::imports(wasm).unwrap_or_default(),
            Some(wasm) => crate::api_level::imports(wasm)?,
            None => Vec::new(),
        };
        let (level, function) = crate::api_level::needed(functions.iter().map(String::as_str));
        if let Some(why) = crate::api_level::too_new(level, function) {
            return Err(why);
        }
        let linker = js_linker.as_ref().unwrap_or(linker);
        let data_dir = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(STORAGE_DIR)
            .join(&m.id);
        let state = State {
            name: m.name.clone(),
            id: m.id.clone(),
            js_source: package.js.clone(),
            snapshot,
            snapshot_id,
            controls: Controls::default(),
            out: Vec::new(),
            panel: None,
            draw: None,
            grids: None,
            config: None,
            replaces: Vec::new(),
            widgets: Widgets::default(),
            world: Vec::new(),
            icons: Vec::new(),
            channels: Vec::new(),
            sent: VecDeque::new(),
            ui: UiState::default(),
            shapes: Vec::new(),
            corner: None,
            zoom: None,
            minimap: None,
            far_terrain: None,
            input: InputState::default(),
            browser: BrowserState::default(),
            data_dir,
            storage_budget: STORAGE_WRITE_BUDGET,
            limits: Limits {
                inner: StoreLimitsBuilder::new()
                    .memory_size(MEMORY)
                    .instances(16)
                    .build(),
                refused: None,
            },
            world_view,
        };
        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(FUEL_INIT).map_err(|e| e.to_string())?;
        let (mut guest, ui, mut hooks): (Guest, Option<UiHost>, Hooks) = if for_starling {
            let exports = starling::StarlingMod::instantiate(&mut store, &component, linker)
                .map_err(|e| quick_detail(&m.id, &e))?;
            let hooks = exports
                .starling_mods_events()
                .call_init(&mut store)
                .map_err(|e| quick_detail(&m.id, &e))??;
            (Guest::Starling(exports), None, hooks.into())
        } else if server {
            let instance = linker
                .instantiate(&mut store, &component)
                .map_err(|e| quick_detail(&m.id, &e))?;
            let exports =
                HorizonServerMod::new(&mut store, &instance).map_err(|e| quick_detail(&m.id, &e))?;
            let r = exports
                .horizon_mods_server_events()
                .call_init(&mut store)
                .map_err(|e| quick_detail(&m.id, &e))??;
            (Guest::Server(exports, r), None, Hooks::default())
        } else {
            let instance = linker
                .instantiate(&mut store, &component)
                .map_err(|e| quick_detail(&m.id, &e))?;
            let exports =
                v1::HorizonMod::new(&mut store, &instance).map_err(|e| quick_detail(&m.id, &e))?;
            // `ui-events` is a Horizon mod built since `ui`'s; older ones have no export and stay
            // callable through `events` alone.
            let ui = UiHost::new(&mut store, &instance).ok();
            let hooks = exports
                .horizon_mods_events()
                .call_init(&mut store)
                .map_err(|e| quick_detail(&m.id, &e))??;
            (Guest::Horizon(exports), ui, hooks.into())
        };
        let free = |c: &str| {
            let free = !reserved(c) && !taken.iter().any(|t| t == c);
            if !free {
                log::warn!("Horizon: mod {} cannot take /{c}: already a command", m.id);
            }
            free
        };
        hooks.commands.retain(|c| free(c));
        if let Guest::Server(_, r) = &mut guest {
            r.commands.retain(|c| free(&c.name));
            for c in &mut r.commands {
                if let Err(e) = check_arguments(&c.arguments) {
                    log::warn!(
                        "Horizon: mod {}'s /{}: {e}; it takes free text",
                        m.id,
                        c.name
                    );
                    c.arguments.clear();
                }
            }
            hooks.commands = r.commands.iter().map(|c| c.name.clone()).collect();
        }
        hooks.channels.retain(|c| {
            let ok = channel_ok(c);
            if !ok {
                log::warn!("Horizon: mod {} cannot use channel {c:?}", m.id);
            }
            ok
        });
        hooks.hud_elements.truncate(64);
        hooks.keybinds.truncate(64);
        store.data_mut().channels = hooks.channels.clone();
        self.generation += 1;
        self.out.append(&mut store.data_mut().out);
        Ok(Some(Running {
            store,
            guest,
            ui,
            hooks,
            capabilities: m.capabilities.clone(),
            rendered: Vec::new(),
            raycasts: functions.iter().any(|f| f == "game.raycast"),
            reads_recipes: functions.iter().any(|f| f.starts_with("recipes.")),
        }))
    }

    /// True when at least one mod runs: the game skips its mod work otherwise.
    pub fn is_running(&self) -> bool {
        self.entries.iter().any(|e| e.run.is_some())
    }
    /// True when a server mod runs: the server skips its mod work otherwise.
    pub fn runs_server(&self) -> bool {
        self.running().any(|r| r.server().is_some())
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    fn running(&self) -> impl Iterator<Item = &Running> {
        self.entries.iter().filter_map(|e| e.run.as_ref())
    }
    /// The settings screen running mod `id` offers (`screens.set-config`), `<mod id>:<name>`.
    pub fn config_screen(&self, id: &str) -> Option<String> {
        let state = self
            .running()
            .map(|r| r.store.data())
            .find(|s| s.id == id)?;
        state.config.clone()
    }
    /// Whether a running mod runs on the quick compiler's code (its full code not kept yet).
    pub fn on_quick_code(&self) -> bool {
        let Some((main, _)) = &self.engine else {
            return false;
        };
        self.running()
            .any(|r| !Engine::same(r.store.engine(), main))
    }
    /// The mod screen (`<mod id>:<name>`) standing in for the game's `screen`
    /// (`screens.replace`): the first running mod's that asked.
    pub fn replacement(&self, screen: &str) -> Option<String> {
        self.running().find_map(|r| {
            let state = r.store.data();
            state
                .replaces
                .iter()
                .find(|(game, _)| game == screen)
                .map(|(_, own)| own.clone())
        })
    }
    /// Every running mod's buttons, with its id.
    pub fn widgets(&self) -> impl Iterator<Item = (&str, &Widgets)> {
        self.running()
            .map(|r| r.store.data())
            .map(|s| (s.id.as_str(), &s.widgets))
    }
    /// Every running mod's world layers (`world-draw`), with its id.
    pub fn world_layers(&self) -> impl Iterator<Item = (&str, &[(String, WorldLayer)])> {
        self.running()
            .map(|r| r.store.data())
            .map(|s| (s.id.as_str(), s.world.as_slice()))
    }
    /// The running mods' `world-draw` icons: mod id, then each layer's.
    pub fn world_icons(&self) -> impl Iterator<Item = (&str, &[(String, Vec<WorldIcon>)])> {
        self.running()
            .map(|r| r.store.data())
            .map(|s| (s.id.as_str(), s.icons.as_slice()))
    }
    /// Whether a running mod asked for `c` (the game fills that part of the snapshot only then).
    pub fn uses(&self, c: Capability) -> bool {
        self.running().any(|r| r.capabilities.contains(&c))
    }
    /// Whether a running mod's hooks say `wants`.
    pub fn wants(&self, wants: impl Fn(&Hooks) -> bool) -> bool {
        self.running().any(|r| wants(&r.hooks))
    }
    /// What mods read until the next snapshot.
    pub fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = Arc::new(snapshot);
        self.snapshot_id = self.snapshot_id.wrapping_add(1);
    }
    /// The game's controls now: every frame, so a mod drawing them keeps up with the keys.
    pub fn set_controls(&mut self, controls: Controls) {
        self.controls = controls;
    }
    /// The snapshot mods read now.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Calls `f` on every running mod whose hooks `wants`, with `fuel`; the results in order.
    fn each<R>(
        &mut self,
        fuel: u64,
        wants: impl Fn(&Hooks) -> bool,
        mut f: impl FnMut(&Guest, &mut Store<State>) -> wasmtime::Result<R>,
    ) -> Vec<R> {
        let mut out = Vec::new();
        for i in 0..self.entries.len() {
            if self.entries[i]
                .run
                .as_ref()
                .is_some_and(|r| wants(&r.hooks))
                && let Some(r) = self.call(i, fuel, &mut f)
            {
                out.push(r);
            }
        }
        out
    }
    /// Calls `f` on every running server mod whose hooks `wants`, with `fuel`.
    fn each_server(
        &mut self,
        fuel: u64,
        wants: impl Fn(&ServerRegistration) -> bool,
        mut f: impl FnMut(&ServerEvents, &mut Store<State>) -> wasmtime::Result<()>,
    ) {
        for i in 0..self.entries.len() {
            if self.entries[i]
                .run
                .as_ref()
                .and_then(Running::server)
                .is_some_and(&wants)
            {
                self.call(i, fuel, |g, s| match g {
                    Guest::Server(m, _) => f(m.horizon_mods_server_events(), s),
                    Guest::Horizon(_) | Guest::Starling(_) => Ok(()),
                });
            }
        }
    }

    /// One call into mod `i`; a trap or an overrun turns the mod off.
    fn call<R>(
        &mut self,
        i: usize,
        fuel: u64,
        f: impl FnOnce(&Guest, &mut Store<State>) -> wasmtime::Result<R>,
    ) -> Option<R> {
        let entry = &mut self.entries[i];
        let run = entry.run.as_mut()?;
        run.store.data_mut().snapshot = self.snapshot.clone();
        run.store.data_mut().snapshot_id = self.snapshot_id;
        run.store.data_mut().controls = self.controls;
        run.store.data_mut().storage_budget = STORAGE_WRITE_BUDGET;
        let started = Instant::now();
        let result = run
            .store
            .set_fuel(fuel)
            .and_then(|()| f(&run.guest, &mut run.store));
        // An average over about the last 64 calls.
        entry.cost = (entry.cost * 63 + started.elapsed()) / 64;
        let data = run.store.data_mut();
        let (open, close) = (
            std::mem::take(&mut data.ui.open),
            std::mem::take(&mut data.ui.close),
        );
        self.out.append(&mut data.out);
        if open {
            self.out.push(Output::OpenUiScreen { owner: i });
        }
        if close {
            self.out.push(Output::CloseUiScreen { owner: i });
        }
        match result {
            Ok(r) => Some(r),
            Err(e) => {
                // On the quick compiler's code the fault may be the compiler's: the mod starts
                // again on full code once the background compile has kept it.
                let quick = self
                    .engine
                    .as_ref()
                    .is_some_and(|(main, _)| !Engine::same(run.store.engine(), main))
                    && compiler_fault(&e);
                if quick {
                    log::warn!(
                        "Horizon: mod {} on quick code ({}): {e:?}",
                        run.store.data().id,
                        std::env::consts::ARCH
                    );
                    QUICK_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
                    entry.retry_full = true;
                }
                let reason = match run.store.data().limits.refused {
                    Some(bytes) => format!(
                        "it ran out of memory (it asked for {} MiB; a mod has {} MiB)",
                        bytes >> 20,
                        MEMORY >> 20
                    ),
                    None if quick => format!(
                        "{} on quick code; it starts again on full code shortly",
                        reason(&e)
                    ),
                    None => reason(&e),
                };
                log::warn!("Horizon: mod {} stopped: {reason}", run.store.data().id);
                let name = run.store.data().name.clone();
                self.out.push(Output::Stopped {
                    name,
                    reason: reason.clone(),
                });
                entry.status = Status::Failed(reason);
                entry.run = None;
                self.generation += 1;
                None
            }
        }
    }

    /// A call of render hook `hook` into mod `i`: what it drew.
    fn render(
        &mut self,
        i: usize,
        hook: &'static str,
        f: impl FnOnce(&starling::StarlingMod, &mut Store<State>) -> wasmtime::Result<()>,
    ) -> Vec<DrawOp> {
        let mut fuel = FUEL_RENDER;
        if let Some(run) = self.entries[i].run.as_mut() {
            run.store.data_mut().draw = Some(Vec::new());
            if !run.rendered.contains(&hook) {
                run.rendered.push(hook);
                fuel = FUEL_FIRST_RENDER;
            }
        }
        self.call(i, fuel, |g, s| match g {
            Guest::Starling(m) => f(m, s),
            Guest::Horizon(_) | Guest::Server(..) => Ok(()),
        });
        self.entries[i]
            .run
            .as_mut()
            .and_then(|r| r.store.data_mut().draw.take())
            .unwrap_or_default()
    }

    /// The running mods' indices whose hooks say `wants`.
    fn indices(&self, wants: impl Fn(&Hooks) -> bool) -> Vec<usize> {
        (0..self.entries.len())
            .filter(|&i| {
                self.entries[i]
                    .run
                    .as_ref()
                    .is_some_and(|r| wants(&r.hooks))
            })
            .collect()
    }
    /// The index of the running mod with id `id`.
    fn by_id(&self, id: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.run.is_some() && e.manifest.as_ref().is_some_and(|m| m.id == id))
    }

    /// A game tick.
    pub fn tick(&mut self, tick: u64) {
        self.each(
            FUEL_TICK,
            |h| h.tick,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_tick(s, tick),
                Guest::Horizon(m) => m.horizon_mods_events().call_tick(s, tick),
                Guest::Server(..) => Ok(()),
            },
        );
    }
    /// A rendered frame, `seconds` after the last.
    pub fn frame(&mut self, seconds: f32) {
        self.each(
            FUEL_FRAME,
            |h| h.frame,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_frame(s, seconds),
                Guest::Horizon(m) => m.horizon_mods_events().call_frame(s, seconds),
                Guest::Server(..) => Ok(()),
            },
        );
    }
    /// A chat line the player receives: its plain text and its JSON component. Each mod sees
    /// what the ones before it made of it; a hidden line goes no further.
    pub fn chat(&mut self, plain: &str, component: &str) -> ChatVerdict {
        let mut verdict = ChatVerdict::Keep;
        let (mut plain, mut component) = (plain.to_owned(), component.to_owned());
        for i in self.indices(|h| h.chat) {
            let result = self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m
                    .starling_mods_events()
                    .call_chat_received(s, &plain, &component),
                Guest::Horizon(m) => m
                    .horizon_mods_events()
                    .call_chat(s, &plain)
                    .map(|()| ChatAction::Keep),
                Guest::Server(..) => Ok(ChatAction::Keep),
            });
            match result {
                Some(ChatAction::Hide) => return ChatVerdict::Hide,
                Some(ChatAction::Replace(text)) => {
                    let text = clip_to(text, TEXT_CHARS);
                    plain = text.clone();
                    component = text.clone();
                    verdict = ChatVerdict::Replace(text);
                }
                Some(ChatAction::Keep) | None => {}
            }
        }
        verdict
    }
    /// A line the player is about to send: None when a mod cancelled it.
    pub fn chat_sending(&mut self, message: &str) -> Option<String> {
        let mut message = message.to_owned();
        for i in self.indices(|h| h.chat_send) {
            let result = self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_chat_sending(s, &message),
                Guest::Horizon(_) | Guest::Server(..) => Ok(SendAction::Send),
            });
            match result {
                Some(SendAction::Cancel) => return None,
                Some(SendAction::Replace(text)) => message = clip_to(text, SEND_CHARS),
                Some(SendAction::Send) | None => {}
            }
        }
        Some(message)
    }
    /// Over the HUD: what the `hud` hooks drew.
    pub fn hud_render(&mut self, context: RenderContext) -> Vec<DrawOp> {
        let mut ops = Vec::new();
        for i in self.indices(|h| h.hud) {
            ops.extend(self.render(i, "hud", |m, s| {
                m.starling_mods_events().call_hud_render(s, context)
            }));
        }
        ops
    }
    /// Whether a running mod draws an element at `part` with `placement`.
    pub fn has_hud_element(&self, part: HudPart, placement: Placement) -> bool {
        self.wants(|h| {
            h.hud_elements
                .iter()
                .any(|e| e.part == part && e.placement == placement)
        })
    }
    /// The elements at `part` with `placement`, drawn.
    pub fn hud_elements(
        &mut self,
        part: HudPart,
        placement: Placement,
        context: RenderContext,
    ) -> Vec<DrawOp> {
        let mut ops = Vec::new();
        for i in 0..self.entries.len() {
            let ids: Vec<String> = self.entries[i]
                .run
                .as_ref()
                .map(|r| {
                    r.hooks
                        .hud_elements
                        .iter()
                        .filter(|e| e.part == part && e.placement == placement)
                        .map(|e| e.id.clone())
                        .collect()
                })
                .unwrap_or_default();
            for id in ids {
                ops.extend(self.render(i, "element", |m, s| {
                    m.starling_mods_events()
                        .call_render_hud_element(s, &id, context)
                }));
            }
        }
        ops
    }
    /// The mods a screen's events go to: a mod's own screen to that mod, the game's to the
    /// mods asking for screen events.
    fn screen_mods(&self, screen: &str) -> Vec<usize> {
        match screen.split_once(':').filter(|(id, _)| !id.contains('/')) {
            Some((id, _)) if self.by_id(id).is_some() => self.by_id(id).into_iter().collect(),
            _ => self.indices(|h| h.screens),
        }
    }
    pub fn screen_opened(&mut self, screen: &str) {
        for i in self.screen_mods(screen) {
            self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_screen_opened(s, screen),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            });
        }
    }
    pub fn screen_closed(&mut self, screen: &str) {
        for i in self.screen_mods(screen) {
            self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_screen_closed(s, screen),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            });
        }
    }
    /// After a screen drew: what the mods drew over it.
    pub fn screen_render(&mut self, screen: &str, context: RenderContext) -> Vec<DrawOp> {
        let mut ops = Vec::new();
        for i in self.screen_mods(screen) {
            ops.extend(self.render(i, "screen", |m, s| {
                m.starling_mods_events()
                    .call_screen_render(s, screen, context)
            }));
        }
        ops
    }
    /// True when a mod used the event.
    pub fn screen_mouse(&mut self, screen: &str, event: MouseEvent) -> bool {
        for i in self.screen_mods(screen) {
            let used = self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_screen_mouse(s, screen, event),
                Guest::Horizon(_) | Guest::Server(..) => Ok(false),
            });
            if used == Some(true) {
                return true;
            }
        }
        false
    }
    pub fn screen_key(&mut self, screen: &str, event: &KeyEvent) -> bool {
        for i in self.screen_mods(screen) {
            let used = self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_screen_key(s, screen, event),
                Guest::Horizon(_) | Guest::Server(..) => Ok(false),
            });
            if used == Some(true) {
                return true;
            }
        }
        false
    }
    /// Lines the mods add under `stack`'s tooltip (rich text).
    pub fn tooltip(&mut self, stack: &ItemStack) -> Vec<String> {
        self.tooltip_grids(stack).0
    }
    /// The lines mods add under `stack`'s tooltip, the grids (`tooltips.add-grid`) and whether
    /// one stands instead of the game's lines of a container's contents.
    pub fn tooltip_grids(&mut self, stack: &ItemStack) -> (Vec<String>, Vec<ItemGrid>, bool) {
        let (mut lines, mut grids, mut instead) = (Vec::new(), Vec::new(), false);
        for i in self.indices(|h| h.tooltips) {
            if let Some(run) = self.entries[i].run.as_mut() {
                run.store.data_mut().grids = Some(Vec::new());
            }
            if let Some(more) = self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_tooltip(s, stack),
                Guest::Horizon(_) | Guest::Server(..) => Ok(Vec::new()),
            }) {
                lines.extend(more.into_iter().take(32).map(|l| clip_to(l, TEXT_CHARS)));
            }
            let added = self.entries[i]
                .run
                .as_mut()
                .and_then(|r| r.store.data_mut().grids.take())
                .unwrap_or_default();
            for (grid, over) in added {
                if grids.len() < TOOLTIP_GRIDS {
                    instead |= over;
                    grids.push(grid);
                }
            }
        }
        (lines, grids, instead)
    }
    /// Every running mod's keybinds, with the mod's id.
    pub fn keybinds(&self) -> Vec<(String, Keybind)> {
        self.entries
            .iter()
            .filter_map(|e| Some((e.manifest.as_ref()?.id.clone(), e.run.as_ref()?)))
            .flat_map(|(id, r)| {
                r.hooks
                    .keybinds
                    .iter()
                    .map(move |k| (id.clone(), k.clone()))
            })
            .collect()
    }
    /// Mod `id`'s keybind `key` pressed or released (Starling's `keybinds`; Horizon's `input`
    /// keys are [`Mods::key`]).
    pub fn keybind(&mut self, id: &str, key: &str, pressed: bool) {
        if let Some(i) = self.by_id(id) {
            self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_key(s, key, pressed),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            });
        }
    }
    /// Every channel a running mod uses, for the server's `minecraft:register`.
    pub fn channels(&self) -> Vec<String> {
        let mut all: Vec<String> = self
            .running()
            .filter(|r| r.capabilities.contains(&Capability::Net))
            .flat_map(|r| r.hooks.channels.iter().cloned())
            .collect();
        all.sort();
        all.dedup();
        all
    }
    /// A payload the server sent on `channel`.
    pub fn payload(&mut self, channel: &str, data: &[u8]) {
        let wanted = |h: &Hooks| h.channels.iter().any(|c| c == channel);
        self.each(FUEL_EVENT, wanted, |g, s| match g {
            Guest::Starling(m) => m.starling_mods_events().call_payload(s, channel, data),
            Guest::Horizon(_) | Guest::Server(..) => Ok(()),
        });
    }
    pub fn joined(&mut self, server: Option<&str>) {
        self.each(
            FUEL_EVENT,
            |h| h.connection,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_joined(s, server),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            },
        );
    }
    pub fn left(&mut self) {
        self.each(
            FUEL_EVENT,
            |h| h.connection,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_left(s),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            },
        );
    }
    pub fn dimension_changed(&mut self, dimension: &str) {
        self.each(
            FUEL_EVENT,
            |h| h.connection,
            |g, s| match g {
                Guest::Starling(m) => m
                    .starling_mods_events()
                    .call_dimension_changed(s, dimension),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            },
        );
    }
    /// False when a mod cancels it.
    pub fn use_item(&mut self, hand: Hand) -> bool {
        let allowed = self.each(
            FUEL_EVENT,
            |h| h.interaction,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_use_item(s, hand),
                Guest::Horizon(_) | Guest::Server(..) => Ok(true),
            },
        );
        allowed.into_iter().all(|a| a)
    }
    pub fn use_block(&mut self, pos: BlockPos, face: Direction, hand: Hand) -> bool {
        let allowed = self.each(
            FUEL_EVENT,
            |h| h.interaction,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_use_block(s, pos, face, hand),
                Guest::Horizon(_) | Guest::Server(..) => Ok(true),
            },
        );
        allowed.into_iter().all(|a| a)
    }
    pub fn attack_entity(&mut self, entity: i32) -> bool {
        let allowed = self.each(
            FUEL_EVENT,
            |h| h.interaction,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_attack_entity(s, entity),
                Guest::Horizon(_) | Guest::Server(..) => Ok(true),
            },
        );
        allowed.into_iter().all(|a| a)
    }
    pub fn attack_block(&mut self, pos: BlockPos, face: Direction) -> bool {
        let allowed = self.each(
            FUEL_EVENT,
            |h| h.interaction,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_attack_block(s, pos, face),
                Guest::Horizon(_) | Guest::Server(..) => Ok(true),
            },
        );
        allowed.into_iter().all(|a| a)
    }
    /// The player broke `block` at `pos` (Starling's `interaction` hooks; server mods hear
    /// [`Mods::block_broken`]).
    pub fn client_block_broken(&mut self, pos: BlockPos, block: &str) {
        self.each(
            FUEL_EVENT,
            |h| h.interaction,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_block_broken(s, pos, block),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            },
        );
    }
    pub fn player_hurt(&mut self, amount: f32) {
        self.each(
            FUEL_EVENT,
            |h| h.damage,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_player_hurt(s, amount),
                Guest::Horizon(_) | Guest::Server(..) => Ok(()),
            },
        );
    }
    /// Whether a running client mod owns `/name`.
    pub fn has_command(&self, name: &str) -> bool {
        self.command_owner(false, name).is_some()
    }
    fn command_owner(&self, server: bool, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| {
            e.run.as_ref().is_some_and(|r| {
                r.server().is_some() == server && r.hooks.commands.iter().any(|c| c == name)
            })
        })
    }
    /// Runs `line` (without its slash) if a client mod owns its command;
    /// false when none does.
    pub fn command(&mut self, line: &str) -> bool {
        let (name, args) = line.split_once(' ').unwrap_or((line, ""));
        let Some(i) = self.command_owner(false, name) else {
            return false;
        };
        let result = self.call(i, FUEL_EVENT, |g, s| match g {
            Guest::Starling(m) => m.starling_mods_events().call_command(s, name, args),
            Guest::Horizon(m) => m.horizon_mods_events().call_command(s, name, args),
            Guest::Server(..) => Ok(Ok(None)),
        });
        match result {
            Some(Ok(Some(text))) => self.out.push(Output::Feedback { text, error: false }),
            Some(Err(text)) => self.out.push(Output::Feedback { text, error: true }),
            Some(Ok(None)) | None => {}
        }
        true
    }
    /// The commands running client mods own, for suggestions.
    pub fn commands(&self) -> impl Iterator<Item = &str> {
        self.running()
            .filter(|r| r.server().is_none())
            .flat_map(|r| r.hooks.commands.iter().map(String::as_str))
    }

    /// A server tick.
    pub fn server_tick(&mut self, tick: u64) {
        self.each_server(FUEL_TICK, |h| h.tick, |m, s| m.call_tick(s, tick));
    }
    /// `player` joined the server (they are in the snapshot's players).
    pub fn player_joined(&mut self, player: &ServerPlayer) {
        self.each_server(
            FUEL_EVENT,
            |h| h.joins,
            |m, s| m.call_player_joined(s, player),
        );
    }
    /// `player` left the server (they are no longer in the snapshot's players).
    pub fn player_left(&mut self, player: &ServerPlayer) {
        self.each_server(
            FUEL_EVENT,
            |h| h.joins,
            |m, s| m.call_player_left(s, player),
        );
    }
    /// A chat message `player` sent, as plain text.
    pub fn player_chat(&mut self, player: &ServerPlayer, text: &str) {
        self.each_server(FUEL_EVENT, |h| h.chat, |m, s| m.call_chat(s, player, text));
    }
    /// `player` broke `state` at `pos`.
    pub fn block_broken(&mut self, player: &ServerPlayer, pos: ServerBlockPos, state: &str) {
        self.each_server(
            FUEL_EVENT,
            |h| h.blocks,
            |m, s| m.call_block_broken(s, player, pos, state),
        );
    }
    /// `player` placed `state` at `pos`.
    pub fn block_placed(&mut self, player: &ServerPlayer, pos: ServerBlockPos, state: &str) {
        self.each_server(
            FUEL_EVENT,
            |h| h.blocks,
            |m, s| m.call_block_placed(s, player, pos, state),
        );
    }
    /// The commands running server mods own and their arguments, for the
    /// server's command tree.
    pub fn server_commands(&self) -> impl Iterator<Item = &ServerCommand> {
        self.running()
            .filter_map(Running::server)
            .flat_map(|r| r.commands.iter())
    }
    /// Runs `line` (without its slash), typed by `player`, if a server mod
    /// owns its command; its feedback is an [`Output::Reply`]. False when
    /// none does.
    pub fn server_command(&mut self, player: &ServerPlayer, line: &str) -> bool {
        let (name, args) = line.split_once(' ').unwrap_or((line, ""));
        let Some(i) = self.command_owner(true, name) else {
            return false;
        };
        let reply = |text, error| Output::Reply {
            player: player.id.clone(),
            text,
            error,
        };
        let arguments = self.entries[i]
            .run
            .as_ref()
            .and_then(Running::server)
            .and_then(|r| r.commands.iter().find(|c| c.name == name))
            .map(|c| c.arguments.as_slice())
            .unwrap_or_default();
        let values = match crate::commands::parse(arguments, args, player, &self.snapshot.players) {
            Ok(values) => values,
            Err(e) => {
                self.out.push(reply(e, true));
                return true;
            }
        };
        let result = self.call(i, FUEL_EVENT, |g, s| match g {
            Guest::Server(m, _) => m
                .horizon_mods_server_events()
                .call_command(s, player, name, args, &values),
            Guest::Horizon(_) | Guest::Starling(_) => Ok(Ok(None)),
        });
        match result {
            Some(Ok(Some(text))) => self.out.push(reply(text, false)),
            Some(Err(text)) => self.out.push(reply(text, true)),
            Some(Ok(None)) | None => {}
        }
        true
    }

    /// The running mods' HUD panels.
    pub fn panels(&self) -> impl Iterator<Item = &Panel> {
        self.running().filter_map(|r| r.store.data().panel.as_ref())
    }
    /// The running mods' corner panels (Horizon's capability `render`), in load order.
    pub fn corner_panels(&self) -> impl Iterator<Item = &CornerPanel> {
        self.running()
            .filter_map(|r| r.store.data().corner.as_ref())
    }
    /// Some running mod calls Horizon's `game.raycast`: the snapshot needs a
    /// [`Snapshot::view_hit`].
    pub fn wants_raycast(&self) -> bool {
        self.running().any(|r| r.raycasts)
    }
    /// The panel beside container screens (`browser.set-items`): the first
    /// running mod with items or recipes, its index, the panel's version and
    /// the items.
    pub fn browser_items(&self) -> Option<(usize, u64, &[BrowserStack])> {
        self.entries.iter().enumerate().find_map(|(i, e)| {
            let b = &e.run.as_ref()?.store.data().browser;
            (!b.items.is_empty() || b.recipes.is_some()).then_some((
                i,
                b.version,
                b.items.as_slice(),
            ))
        })
    }
    /// What mod `owner`'s panel shows instead of its items
    /// (`browser.set-recipes`).
    pub fn browser_recipes(&self, owner: usize) -> Option<&RecipePage> {
        let b = &self.entries.get(owner)?.run.as_ref()?.store.data().browser;
        b.recipes.as_ref()
    }
    /// The player did `event` on mod `owner`'s panel.
    pub fn browser_event(&mut self, owner: usize, event: BrowserEvent) {
        if let Some(run) = self.entries.get_mut(owner).and_then(|e| e.run.as_mut()) {
            run.store.data_mut().browser.push(event);
        }
    }
    /// The key `key` went down with the pointer on `stack` in a container
    /// screen: each mod with a key on it hears its own name for it.
    pub fn browser_key(&mut self, key: &str, stack: &BrowserStack) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let data = run.store.data_mut();
            let names: Vec<String> = data
                .input
                .keys
                .iter()
                .filter(|k| k.key == key)
                .map(|k| k.name.clone())
                .collect();
            for key in names {
                data.browser.push(BrowserEvent::Key(BrowserItemKey {
                    key,
                    stack: stack.clone(),
                }));
            }
        }
    }
    /// Some running mod reads Horizon's capability `recipes`: the snapshot needs
    /// [`Snapshot::horizon_recipes`] and [`Snapshot::known_recipes`].
    pub fn wants_recipes(&self) -> bool {
        self.running().any(|r| r.reads_recipes)
    }
    /// Some running mod has a `frame` hook: the game refreshes the snapshot every frame for it.
    pub fn wants_frames(&self) -> bool {
        self.wants(|h| h.frame)
    }
    /// Some running mod claimed items (Horizon's capability `input`).
    pub fn claims_any(&self) -> bool {
        self.inputs().any(|i| !i.claimed.is_empty())
    }
    /// Some running mod claimed the item `id`.
    pub fn claims(&self, id: &str) -> bool {
        self.inputs().any(|i| i.claimed.iter().any(|c| c == id))
    }
    /// Some running mod registered a key (Horizon's `input.register-key`).
    pub fn has_keys(&self) -> bool {
        self.inputs().any(|i| !i.keys.is_empty())
    }
    /// The Horizon mods' keys: (mod name, key), for Controls.
    pub fn mod_keys(&self) -> impl Iterator<Item = (&str, &ModKey)> {
        self.running().flat_map(|r| {
            let d = r.store.data();
            d.input.keys.iter().map(move |k| (d.name.as_str(), k))
        })
    }
    /// Mod `i`'s keys, as it registered them (with the player's choices).
    pub fn keys_of(&self, i: usize) -> &[ModKey] {
        self.entries
            .get(i)
            .and_then(|e| e.run.as_ref())
            .map_or(&[], |r| r.store.data().input.keys.as_slice())
    }
    /// The keys the player chose (`ui`'s key widget), by mod id: each
    /// running mod's `name → key`. They replace the defaults registered
    /// so far and later ones. Keys that aren't keys are skipped.
    pub fn set_bindings<'a>(&mut self, chosen: impl Fn(&str) -> Vec<(&'a str, &'a str)>) {
        for e in &mut self.entries {
            let (Some(m), Some(run)) = (&e.manifest, e.run.as_mut()) else {
                continue;
            };
            let input = &mut run.store.data_mut().input;
            input.bindings = chosen(&m.id)
                .into_iter()
                .filter(|(_, key)| key_ok(key))
                .map(|(n, k)| (n.to_owned(), k.to_owned()))
                .collect();
            for (name, key) in input.bindings.clone() {
                input.rebind(&name, key);
            }
        }
    }
    /// The player binds mod `i`'s key `name` to `key` (`key.keyboard.x`,
    /// `key.mouse.4`, `key.keyboard.unknown` for none). False when the mod
    /// has no such key or `key` isn't one.
    pub fn rebind(&mut self, i: usize, name: &str, key: &str) -> bool {
        let Some(run) = self.entries.get_mut(i).and_then(|e| e.run.as_mut()) else {
            return false;
        };
        let input = &mut run.store.data_mut().input;
        if !key_ok(key) || !input.keys.iter().any(|k| k.name == name) {
            return false;
        }
        input.bindings.retain(|(n, _)| n != name);
        input.bindings.push((name.to_owned(), key.to_owned()));
        input.rebind(name, key.to_owned());
        true
    }
    /// The use button went down with `item`: `use-start` to each mod that claimed it.
    pub fn use_start(&mut self, item: &HeldItem) {
        let event = InputEvent::UseStart(ItemUse {
            item: item.clone(),
            ticks: 0,
        });
        self.push_input(|i| i.claimed.contains(&item.id), &event);
    }
    /// The claimed use ended after `ticks`.
    pub fn use_stop(&mut self, item: &HeldItem, ticks: u32) {
        let event = InputEvent::UseStop(ItemUse {
            item: item.clone(),
            ticks,
        });
        self.push_input(|i| i.claimed.contains(&item.id), &event);
    }
    /// The key `key` (`key.keyboard.r`) went down or up: each Horizon mod with a key on it hears
    /// its own name for it (Starling's keybinds are [`Mods::keybind`]).
    pub fn key(&mut self, key: &str, down: bool) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let input = &mut run.store.data_mut().input;
            let names: Vec<String> = input
                .keys
                .iter()
                .filter(|k| k.key == key)
                .map(|k| k.name.clone())
                .collect();
            for name in names {
                let held = input.down.iter().position(|n| *n == name);
                match (down, held) {
                    (true, None) => input.down.push(name.clone()),
                    (false, Some(i)) => {
                        input.down.swap_remove(i);
                    }
                    // Already let go when a screen opened.
                    (false, None) => continue,
                    (true, Some(_)) => {}
                }
                input.push(if down {
                    InputEvent::KeyDown(name)
                } else {
                    InputEvent::KeyUp(name)
                });
            }
        }
    }
    /// A screen opened, or the game stopped playing: every mod key held
    /// lets go (`key-up`), so a key needs a new press afterwards.
    pub fn release_keys(&mut self) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let input = &mut run.store.data_mut().input;
            for name in std::mem::take(&mut input.down) {
                input.push(InputEvent::KeyUp(name));
            }
        }
    }
    /// Some running mod holds the mouse wheel.
    pub fn wheel_claimed(&self) -> bool {
        self.inputs().any(|i| i.wheel)
    }
    /// Wheel steps while playing: `wheel` to the mods holding it; false
    /// when none does (the hotbar takes them).
    pub fn wheel(&mut self, steps: f32) -> bool {
        let event = InputEvent::Wheel(steps);
        let claimed = self.wheel_claimed();
        self.push_input(|i| i.wheel, &event);
        claimed
    }
    /// The strongest zoom a running mod asks for (Horizon's capability `render`).
    pub fn zoom(&self) -> Option<ModZoom> {
        self.running()
            .filter_map(|r| r.store.data().zoom)
            .max_by(|a, b| a.factor.total_cmp(&b.factor))
    }
    /// The minimap the first running mod shows (Horizon's capability `render`).
    pub fn minimap(&self) -> Option<ModMinimap> {
        self.running().find_map(|r| r.store.data().minimap)
    }
    /// The far terrain the first running mod draws (Horizon's capability `render`).
    pub fn far_terrain(&self) -> Option<ModFarTerrain> {
        self.running().find_map(|r| r.store.data().far_terrain)
    }
    fn inputs(&self) -> impl Iterator<Item = &InputState> {
        self.running().map(|r| &r.store.data().input)
    }
    fn push_input(&mut self, to: impl Fn(&InputState) -> bool, event: &InputEvent) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let input = &mut run.store.data_mut().input;
            if to(input) {
                input.push(event.clone());
            }
        }
    }
    /// The running mods' HUD shapes (Horizon's capability `render`), each mod's in its own
    /// order, mods in load order.
    pub fn hud_shapes(&self) -> impl Iterator<Item = &Shape> {
        self.running().flat_map(|r| &r.store.data().shapes)
    }
    /// The screen mod `i` last pushed with Horizon's `ui.open-screen`, while it is up.
    pub fn screen(&self, i: usize) -> Option<&UiScreen> {
        self.entries
            .get(i)?
            .run
            .as_ref()?
            .store
            .data()
            .ui
            .screen
            .as_ref()
    }
    /// The widgets Horizon's `ui.set-settings` pushed: mod `i`'s page in Options > Mods. Empty
    /// while it has none.
    pub fn settings_widgets(&self, i: usize) -> &[UiWidget] {
        self.entries
            .get(i)
            .and_then(|e| e.run.as_ref())
            .map_or(&[], |r| &r.store.data().ui.settings)
    }
    /// A `list` widget's row was picked on a surface of mod `i` (a button is row 0).
    pub fn ui_select(&mut self, i: usize, id: u32, row: u32) {
        self.ui_event(i, |ui, s| {
            ui.horizon_mods_ui_events().call_select(s, id, row)
        });
    }
    /// The player left mod `i`'s screen (Esc or Done).
    pub fn ui_closed(&mut self, i: usize) {
        self.ui_event(i, |ui, s| ui.horizon_mods_ui_events().call_closed(s));
    }
    /// The player opened mod `i`'s settings page.
    pub fn ui_settings_opened(&mut self, i: usize) {
        self.ui_event(i, |ui, s| {
            ui.horizon_mods_ui_events().call_settings_opened(s)
        });
    }
    /// A `ui-events` call on mod `i`; no export, no call (Starling's mods and Horizon's built
    /// before `ui` have none).
    fn ui_event(
        &mut self,
        i: usize,
        f: impl FnOnce(&UiHost, &mut Store<State>) -> wasmtime::Result<()>,
    ) {
        // Out of the entry for the call, which hands the closure the guest and store only.
        let Some(ui) = self
            .entries
            .get_mut(i)
            .and_then(|e| e.run.as_mut())
            .and_then(|r| r.ui.take())
        else {
            return;
        };
        self.call(i, FUEL_EVENT, |_, s| f(&ui, s));
        // A mod the call stopped has no run left to keep it.
        if let Some(run) = self.entries[i].run.as_mut() {
            run.ui = Some(ui);
        }
    }
    /// What mods asked to show since the last call.
    pub fn drain(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.out)
    }

    /// Starts again the mods that stopped on the quick compiler's code, once the full compile
    /// is kept (read back in milliseconds): true when any did. Called now and then.
    pub fn restart_on_full_code(&mut self, reserved: impl Fn(&str) -> bool) -> bool {
        if !self.entries.iter().any(|e| e.retry_full) || compiling_in_background() {
            return false;
        }
        finish_background_compiles();
        self.js = None;
        for i in 0..self.entries.len() {
            if !std::mem::take(&mut self.entries[i].retry_full) {
                continue;
            }
            let path = self.entries[i].path.clone();
            let started = Package::read(&path).and_then(|p| self.start(&path, &p, &reserved));
            let entry = &mut self.entries[i];
            let id = entry.manifest.as_ref().map_or("?", |m| m.id.as_str());
            match started {
                Ok(run) => {
                    log::info!("Horizon: mod {id} runs on full code");
                    entry.run = run;
                    entry.status = Status::Running;
                }
                Err(e) => {
                    log::warn!("Horizon: mod {id} did not start on full code: {e}");
                    entry.status = Status::Failed(e);
                }
            }
        }
        true
    }

    /// Restarts the mods whose file changed since they loaded (mod
    /// development: rebuild, and the game picks it up). True when any did.
    pub fn reload_changed(&mut self, reserved: impl Fn(&str) -> bool) -> bool {
        let mut any = false;
        for i in 0..self.entries.len() {
            let entry = &self.entries[i];
            let now = modified(&entry.path);
            let was_on = !matches!(entry.status, Status::Off | Status::Elsewhere);
            if now == entry.modified || !was_on {
                continue;
            }
            any = true;
            self.generation += 1;
            self.entries[i].modified = now;
            self.entries[i].run = None;
            let path = self.entries[i].path.clone();
            let started = Package::read(&path).and_then(|p| {
                self.entries[i].manifest = Some(p.manifest.clone());
                self.start(&path, &p, &reserved)
            });
            let entry = &mut self.entries[i];
            let id = entry.manifest.as_ref().map_or("?", |m| m.id.as_str());
            match started {
                Ok(run) => {
                    log::info!("Horizon: mod {id} reloaded");
                    entry.run = run;
                    entry.status = Status::Running;
                }
                Err(e) => {
                    log::warn!("Horizon: mod {id} did not reload: {e}");
                    entry.status = Status::Failed(e);
                }
            }
        }
        any
    }
}

/// A command's arguments are well formed: names given and distinct,
/// optional ones last, free text only last.
fn check_arguments(arguments: &[Argument]) -> Result<(), String> {
    for (i, a) in arguments.iter().enumerate() {
        if a.name.is_empty()
            || !a
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!("argument name '{}' isn't [A-Za-z0-9_]+", a.name));
        }
        if arguments[..i].iter().any(|b| b.name == a.name) {
            return Err(format!("two arguments are named {}", a.name));
        }
        if i > 0 && arguments[i - 1].optional && !a.optional {
            return Err(format!("{} is required after an optional argument", a.name));
        }
        if a.kind == ArgumentKind::Text && i + 1 < arguments.len() {
            return Err(format!(
                "{} takes the rest of the line, so it must be last",
                a.name
            ));
        }
    }
    Ok(())
}

/// A mod's component compiled, or read back from `cache` where it was compiled before.
fn compile(engine: &Engine, wasm: &[u8], cache: Option<&Path>) -> Result<Component, String> {
    compile_named(engine, &digest(wasm), || Ok(Cow::Borrowed(wasm)), cache)
}

/// [`compile`] for a component named by `digest`, whose bytes `wasm` gives only when it must be
/// compiled.
fn compile_named<'a>(
    engine: &Engine,
    digest: &str,
    wasm: impl FnOnce() -> Result<Cow<'a, [u8]>, String>,
    cache: Option<&Path>,
) -> Result<Component, String> {
    let fresh =
        |wasm: &[u8]| Component::new(engine, wasm).map_err(|e| format!("not a component: {e}"));
    let Some(dir) = cache else {
        return fresh(&wasm()?);
    };
    let file = dir.join(cached_name(digest, engine));
    if file.is_file() {
        // SAFETY: the cache is the game's own folder and holds only what `Component::serialize`
        // wrote below (renamed into place once whole); deserializing checks the file was made
        // by this build of wasmtime with this engine's settings.
        match unsafe { Component::deserialize_file(engine, &file) } {
            Ok(component) => return Ok(component),
            Err(e) => log::warn!("Horizon: compiling {} again: {e}", file.display()),
        }
    }
    let wasm = wasm()?;
    // A big component (the JavaScript engine) runs on quick code until its full code is kept.
    if wasm.len() >= QUICK_FIRST
        && quick_wanted()
        && let Some(component) = compile_quick_then_full(engine, wasm.to_vec(), file.clone())
    {
        return Ok(component);
    }
    let started = Instant::now();
    let component = fresh(&wasm)?;
    log::info!("Horizon: compiled a mod in {:?}", started.elapsed());
    if let Err(e) = keep_compiled(&component, &file) {
        log::warn!("Horizon: could not keep {}: {e}", file.display());
    }
    Ok(component)
}

/// Components this big start on the quick compiler's code (a mod's own is tens of kilobytes;
/// the JavaScript engine is 12 megabytes).
const QUICK_FIRST: usize = 2 << 20;

thread_local! {
    /// The mod being started (on this thread) runs on the quick compiler's code.
    static STARTED_QUICK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// ...and trapped as wrong code would ([`compiler_fault`]).
    static QUICK_TRAPPED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether `e` could be the compiler's fault: a trap (an `unreachable`, a memory access out of
/// bounds...), not a mod's own error (a missing capability, an error it returned) nor its fuel
/// running out (fuel counts instructions, which the compilers do not change).
fn compiler_fault(e: &wasmtime::Error) -> bool {
    matches!(e.downcast_ref::<Trap>(), Some(t) if *t != Trap::OutOfFuel)
}

/// A mod failed on the quick compiler's code: this run compiles fully.
static QUICK_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether big components start on the quick compiler's code, which compiles the JavaScript
/// engine in two seconds on one core where the full one takes fifteen. Wasmtime 36's Winch
/// crashed the engine on ARM (phones); 50's is on trial there, a mod failing on its code being
/// started again on full code. `STARLING_QUICK_COMPILER=off` turns it off.
fn quick_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let wanted =
        *WANTED.get_or_init(|| std::env::var("STARLING_QUICK_COMPILER").as_deref() != Ok("off"));
    wanted && !QUICK_FAILED.load(std::sync::atomic::Ordering::Relaxed)
}

/// A mod's start failing, as the player reads it; on the quick compiler's code the whole error
/// (with the WebAssembly backtrace) goes to the log too, which says what its code got wrong.
fn quick_detail(id: &str, e: &wasmtime::Error) -> String {
    if STARTED_QUICK.get() && compiler_fault(e) {
        QUICK_TRAPPED.set(true);
        log::warn!(
            "Horizon: mod {id} on quick code ({}): {e:?}",
            std::env::consts::ARCH
        );
    }
    reason(e)
}

fn keep_compiled(component: &Component, file: &Path) -> std::io::Result<()> {
    let bytes = component.serialize().map_err(std::io::Error::other)?;
    keep_bytes(&bytes, file)
}

/// Compiled code written to `file` whole (a part renamed into place).
fn keep_bytes(bytes: &[u8], file: &Path) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU32, Ordering};
    // Two loads at once (the player turning mods on and off quickly) each write their own.
    static PART: AtomicU32 = AtomicU32::new(0);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = file.with_extension(format!(
        "{}-{}.part",
        std::process::id(),
        PART.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })
}

/// Starling's shared JavaScript engine, which runs every JavaScript mod: embedded when the build
/// found it (build.rs), compressed.
mod js_engine {
    use super::{Component, Engine, Path};
    #[cfg(js_engine)]
    use super::{Cow, compile_named};

    #[cfg(js_engine)]
    mod built {
        include!(concat!(env!("OUT_DIR"), "/js_engine.rs"));
        pub const PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/js-engine.wasm.gz"));
    }

    /// The engine component's SHA-256, when the build has it.
    #[cfg(js_engine)]
    pub const DIGEST: Option<&str> = Some(built::DIGEST);
    #[cfg(not(js_engine))]
    pub const DIGEST: Option<&str> = None;

    /// The engine's component compiled, or read back from `cache`; unpacked only to compile it.
    pub fn component(engine: &Engine, cache: Option<&Path>) -> Result<Component, String> {
        #[cfg(js_engine)]
        {
            compile_named(
                engine,
                built::DIGEST,
                || {
                    use std::io::Read;
                    let mut wasm = Vec::new();
                    flate2::read::GzDecoder::new(built::PACKED)
                        .read_to_end(&mut wasm)
                        .map_err(|e| format!("the JavaScript engine: {e}"))?;
                    Ok(Cow::Owned(wasm))
                },
                cache,
            )
        }
        #[cfg(not(js_engine))]
        {
            let _ = (engine, cache);
            Err("this build has no JavaScript engine".into())
        }
    }
}

/// Whether this build embeds the shared JavaScript engine, which JavaScript mods need.
pub fn has_js_engine() -> bool {
    js_engine::DIGEST.is_some()
}

/// `js-draw.batch`: the engine's queued drawing, as the calls it stands for (an unknown opcode or a
/// short call ends it).
fn draw_batch(state: &mut State, ops: &[u8], texts: &[&str]) {
    use draw::Host;
    let slots: Vec<[u8; 4]> = ops.as_chunks::<4>().0.to_vec();
    let f = |i: usize| slots.get(i).map(|s| f32::from_le_bytes(*s));
    let n = |i: usize| slots.get(i).map(|s| i32::from_le_bytes(*s));
    let text = |i: usize| {
        n(i).and_then(|t| texts.get(usize::try_from(t).ok()?))
            .map(|t| (*t).to_owned())
    };
    let mut at = 0;
    while let Some(op) = n(at) {
        let drawn = match op {
            1 => (|| {
                let (x, y, w, h, c) = (f(at + 1)?, f(at + 2)?, f(at + 3)?, f(at + 4)?, n(at + 5)?);
                state.fill(x, y, w, h, c as u32);
                Some(6)
            })(),
            2 => (|| {
                let (x, y, w, h) = (f(at + 1)?, f(at + 2)?, f(at + 3)?, f(at + 4)?);
                let (top, bottom) = (n(at + 5)?, n(at + 6)?);
                state.gradient(x, y, w, h, top as u32, bottom as u32);
                Some(7)
            })(),
            // A text or sprite whose string was lost is left out; the calls after it are not.
            3 => (|| {
                let (x, y, c, shadow) = (f(at + 1)?, f(at + 2)?, n(at + 3)?, n(at + 4)?);
                n(at + 5)?;
                if let Some(t) = text(at + 5) {
                    state.text(x, y, t, c as u32, shadow != 0);
                }
                Some(6)
            })(),
            4 => (|| {
                let (x, y, w, h) = (f(at + 1)?, f(at + 2)?, f(at + 3)?, f(at + 4)?);
                n(at + 5)?;
                if let Some(s) = text(at + 5) {
                    state.sprite(x, y, w, h, s);
                }
                Some(6)
            })(),
            5 => (|| {
                state.push(f(at + 1)?, f(at + 2)?, f(at + 3)?);
                Some(4)
            })(),
            6 => {
                state.pop();
                Some(1)
            }
            // An item stack's icon: what `draw.item` draws of it (its id, count and components).
            7 => (|| {
                let (x, y, count, damage) = (f(at + 1)?, f(at + 2)?, n(at + 3)?, n(at + 4)?);
                n(at + 6)?;
                if let (Some(id), Some(components)) = (text(at + 5), text(at + 6)) {
                    let stack = ItemStack {
                        id,
                        count: count as u32,
                        damage: damage as u32,
                        max_damage: 0,
                        name: String::new(),
                        components,
                    };
                    state.item(x, y, stack);
                }
                Some(7)
            })(),
            8 => (|| {
                let (x, y) = (f(at + 1)?, f(at + 2)?);
                n(at + 3)?;
                if let Some(id) = text(at + 3) {
                    state.item_id(x, y, id);
                }
                Some(4)
            })(),
            _ => None,
        };
        match drawn {
            Some(len) => at += len,
            None => break,
        }
    }
}

/// A JavaScript mod's linker: the interfaces its manifest declares (and the ones every mod has),
/// the engine's `js-source` (and `js-draw` with `draw`, and so on), and a trap for every other import, which
/// stops the mod.
fn js_linker(
    engine: &Engine,
    capabilities: &[Capability],
    component: &Component,
) -> wasmtime::Result<Linker<State>> {
    use starling::starling::mods as api;
    use wasmtime::component::types::ComponentItem;
    let mut linker = Linker::new(engine);
    let mut interfaces = vec!["log", "mobile"];
    for c in capabilities {
        interfaces.extend(c.interfaces().iter().copied());
    }
    interfaces.sort_unstable();
    interfaces.dedup();
    for &interface in &interfaces {
        let l = &mut linker;
        match interface {
            "log" => api::log::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "mobile" => api::mobile::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "game" => api::game::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "level" => api::level::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "inventory" => api::inventory::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "slots" => api::slots::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "creative" => api::creative::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "recipes" => api::recipes::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "chat" => api::chat::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "chat-send" => api::chat_send::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "draw" => api::draw::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "hud" => api::hud::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "screens" => api::screens::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "widgets" => api::widgets::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "keys" => api::keys::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "input" => api::input::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "sound" => api::sound::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "net" => api::net::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "storage" => api::storage::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "world-draw" => api::world_draw::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            "tooltips" => api::tooltips::add_to_linker::<State, HasSelf<State>>(l, |s| s)?,
            _ => {}
        }
    }
    let mut js_source = linker.instance("starling:mods/js-source@0.1.0")?;
    js_source.func_wrap(
        "get",
        |store: wasmtime::StoreContextMut<'_, State>, (): ()| {
            Ok((store.data().js_source.clone().unwrap_or_default(),))
        },
    )?;
    js_source.func_wrap(
        "snapshot",
        |store: wasmtime::StoreContextMut<'_, State>, (): ()| Ok((store.data().snapshot_id,)),
    )?;
    if interfaces.contains(&"draw") {
        use api::draw::Host;
        type Colour = (f32, f32, f32, f32, i32);
        type Colours = (f32, f32, f32, f32, i32, i32);
        let mut draw = linker.instance("starling:mods/js-draw@0.1.0")?;
        draw.func_wrap(
            "fill",
            |mut store: wasmtime::StoreContextMut<'_, State>, (x, y, w, h, color): Colour| {
                store.data_mut().fill(x, y, w, h, color as u32);
                Ok(())
            },
        )?;
        draw.func_wrap(
            "gradient",
            |mut store: wasmtime::StoreContextMut<'_, State>,
             (x, y, w, h, top, bottom): Colours| {
                store
                    .data_mut()
                    .gradient(x, y, w, h, top as u32, bottom as u32);
                Ok(())
            },
        )?;
        draw.func_wrap(
            "batch",
            |mut store: wasmtime::StoreContextMut<'_, State>,
             (ops, count, texts): (Vec<u8>, u32, Vec<u8>)| {
                let texts = String::from_utf8_lossy(&texts);
                let parts: Vec<&str> = if count == 0 {
                    Vec::new()
                } else {
                    texts.split('\0').collect()
                };
                // A text holding a NUL split in two: which is which is lost.
                let parts = if parts.len() == count as usize {
                    parts
                } else {
                    Vec::new()
                };
                draw_batch(store.data_mut(), &ops, &parts);
                Ok(())
            },
        )?;
    }
    if interfaces.contains(&"keys") {
        linker.instance("starling:mods/js-keys@0.1.0")?.func_wrap(
            "held",
            |store: wasmtime::StoreContextMut<'_, State>, (): ()| {
                let bits = store
                    .data()
                    .controls
                    .iter()
                    .enumerate()
                    .filter(|(_, (down, _))| *down)
                    .fold(0u32, |bits, (i, _)| bits | 1 << i);
                Ok((bits,))
            },
        )?;
    }
    if interfaces.contains(&"tooltips") {
        use tooltips::Host;
        type Grid = (u32, Vec<Option<ItemStack>>, i32, bool);
        linker
            .instance("starling:mods/js-tooltips@0.1.0")?
            .func_wrap(
                "add-grid",
                |mut store: wasmtime::StoreContextMut<'_, State>,
                 (columns, stacks, color, instead): Grid| {
                    let grid = ItemGrid {
                        columns,
                        stacks,
                        color: color as u32,
                    };
                    Ok((store.data_mut().add_grid(grid, instead),))
                },
            )?;
    }
    // The other interfaces: each function says which capability the manifest lacks.
    for (name, item) in component.component_type().imports(engine) {
        let interface = name
            .strip_prefix("starling:mods/")
            .and_then(|rest| rest.split_once('@'))
            .map(|(interface, _)| interface);
        let (Some(interface), ComponentItem::ComponentInstance(instance)) = (interface, item.ty)
        else {
            continue;
        };
        // js-draw goes with draw, js-keys with keys, js-tooltips with tooltips.
        let granting = match interface {
            "js-draw" => "draw",
            "js-keys" => "keys",
            "js-tooltips" => "tooltips",
            other => other,
        };
        if interface == "js-source" || interfaces.contains(&granting) {
            continue;
        }
        let capability = Capability::ALL
            .into_iter()
            .find(|c| c.interfaces().contains(&granting))
            .map_or_else(|| interface.to_owned(), |c| c.to_string());
        let mut functions = linker.instance(name)?;
        for (function, item) in instance.exports(engine) {
            if let ComponentItem::ComponentFunc(_) = item.ty {
                let message = format!(
                    "uses {interface}.{function}: capability {capability} is missing from its manifest"
                );
                functions.func_new(function, move |_, _, _, _| {
                    Err(wasmtime::Error::msg(message.clone()))
                })?;
            }
        }
    }
    Ok(linker)
}

/// SHA-256 of a mod's code, in hex.
fn digest(wasm: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(wasm)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The cache's name for a mod's code compiled by `engine` (its build and settings).
fn cached_name(digest: &str, engine: &Engine) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    engine.precompile_compatibility_hash().hash(&mut hasher);
    format!("{digest}-{:016x}.cwasm", hasher.finish())
}

/// Removes the compiled code in `dir` that is not one of `digests` compiled by `engine`: an
/// updated or removed mod, or another build of the game.
fn prune(dir: &Path, engine: &Engine, digests: &[String]) {
    let keep: Vec<String> = digests.iter().map(|d| cached_name(d, engine)).collect();
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for file in read.flatten() {
        let name = file.file_name().to_string_lossy().into_owned();
        let compiled = name.ends_with(".cwasm") || name.ends_with(".part");
        // A background compile writing its part (`compile_later`) is left to finish.
        let writing = name.ends_with(".part")
            && file
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(3600));
        if compiled && !keep.contains(&name) && !writing {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

/// An engine for mods: the component model, fuel, and `strategy`'s compiler (across every core
/// when `parallel`).
fn make_engine(strategy: wasmtime::Strategy, parallel: bool) -> Result<Engine, String> {
    let mut config = Config::new();
    config
        .wasm_component_model(true)
        .consume_fuel(true)
        .strategy(strategy)
        .parallel_compilation(parallel);
    Engine::new(&config).map_err(|e| e.to_string())
}

/// `wasm` compiled by the quick compiler (Winch: about six times faster to compile than
/// Cranelift, its code about two and a half times slower) for this session, while a thread
/// compiles it with `engine`'s full compiler on one core into `file` for the next start. None
/// where the quick compiler cannot compile it (the full one does, then).
fn compile_quick_then_full(engine: &Engine, wasm: Vec<u8>, file: PathBuf) -> Option<Component> {
    let quick = make_engine(wasmtime::Strategy::Winch, true).ok()?;
    let started = Instant::now();
    let component = Component::new(&quick, &wasm)
        .inspect_err(|e| log::warn!("Horizon: the quick compiler could not: {e}"))
        .ok()?;
    log::info!(
        "Horizon: compiled a mod quickly in {:?}; the full compile goes on in the background",
        started.elapsed()
    );
    // The full compiler's settings, on one core: the game keeps the others.
    let background = make_engine(wasmtime::Strategy::Cranelift, false);
    let _ = engine;
    let spawned = std::thread::Builder::new()
        .name("mod-compile".into())
        .spawn(move || {
            // Behind the game's threads: a phone with few cores keeps its frames.
            #[cfg(any(target_os = "linux", target_os = "android"))]
            // SAFETY: setpriority with `who` 0 changes only this thread's niceness.
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS as _, 0, 10);
            }
            let started = Instant::now();
            let Ok(background) = background else {
                return;
            };
            match background.precompile_component(&wasm) {
                Ok(bytes) => match keep_bytes(&bytes, &file) {
                    Ok(()) => log::info!(
                        "Horizon: the full compile is kept for the next start ({:?})",
                        started.elapsed()
                    ),
                    Err(e) => log::warn!("Horizon: could not keep {}: {e}", file.display()),
                },
                Err(e) => log::warn!("Horizon: the full compile failed: {e}"),
            }
        });
    match spawned {
        Ok(handle) => BACKGROUND
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(handle),
        Err(e) => log::warn!("Horizon: no background compile: {e}"),
    }
    Some(component)
}

/// The full compiles going on in the background.
static BACKGROUND: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>> =
    std::sync::Mutex::new(Vec::new());

/// Whether a full compile is still going on in the background.
pub fn compiling_in_background() -> bool {
    let handles = BACKGROUND.lock().unwrap_or_else(|e| e.into_inner());
    handles.iter().any(|h| !h.is_finished())
}

/// Waits for the full compiles going on in the background to be kept (a program about to exit,
/// such as `starling-mod run`, so that its next run has them). True when there were any.
pub fn finish_background_compiles() -> bool {
    let handles = std::mem::take(&mut *BACKGROUND.lock().unwrap_or_else(|e| e.into_inner()));
    let any = !handles.is_empty();
    for handle in handles {
        let _ = handle.join();
    }
    any
}

fn modified(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        // A folder mod changes when its component does, or its JavaScript.
        return [crate::manifest::WASM, crate::manifest::JS]
            .iter()
            .filter_map(|file| std::fs::metadata(path.join(file)).ok()?.modified().ok())
            .max();
    }
    meta.modified().ok()
}

/// A trap or error, as the player reads it.
fn reason(e: &wasmtime::Error) -> String {
    match e.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => "it took too long (an endless loop?)".into(),
        Some(Trap::UnreachableCodeReached) => "it crashed (a panic)".into(),
        Some(trap) => format!("it crashed: {trap}"),
        // A host call that refused (a JavaScript mod using a capability it lacks): its own
        // message, without the wasm backtrace around it.
        None => e.root_cause().to_string(),
    }
}

fn clip(text: String) -> String {
    clip_to(text, LINE_CHARS)
}
fn clip_to(text: String, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => text[..at].to_string(),
        None => text,
    }
}

/// A payload channel: `namespace:path`, lowercase identifier characters, not vanilla's.
fn channel_ok(c: &str) -> bool {
    let Some((ns, path)) = c.split_once(':') else {
        return false;
    };
    let ok = |s: &str, extra: &[u8]| {
        !s.is_empty()
            && s.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || b"_-.".contains(&b)
                    || extra.contains(&b)
            })
    };
    c.len() <= 128 && ns != "minecraft" && ok(ns, b"") && ok(path, b"/")
}

/// A mod screen's name: `[a-z0-9_./-]{1,64}`.
fn screen_name_ok(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_./-".contains(&b))
}

impl State {
    /// Records a drawing call while a render hook runs.
    fn draw(&mut self, op: DrawOp) {
        if let Some(ops) = self.draw.as_mut()
            && ops.len() < DRAW_OPS
        {
            ops.push(op);
        }
    }
    fn measure(&self, text: &str) -> f32 {
        match &self.snapshot.measure {
            Some(m) => m(text),
            None => text.chars().count() as f32 * 6.0,
        }
    }
}

impl types::Host for State {}
impl mod_log::Host for State {
    fn write(&mut self, level: mod_log::Level, message: String) {
        let level = match level {
            mod_log::Level::Debug => log::Level::Debug,
            mod_log::Level::Info => log::Level::Info,
            mod_log::Level::Warn => log::Level::Warn,
            mod_log::Level::Error => log::Level::Error,
        };
        log::log!(level, "[mod {}] {}", self.id, clip(message));
    }
}
impl game::Host for State {
    fn get_player(&mut self) -> Option<Player> {
        self.snapshot.player.clone()
    }
    fn get_effects(&mut self) -> Vec<Effect> {
        self.snapshot.effects.clone()
    }
    fn get_clock(&mut self) -> Option<Clock> {
        self.snapshot.clock
    }
    fn get_world_info(&mut self) -> Option<WorldInfo> {
        self.snapshot.world.clone()
    }
    fn get_target(&mut self) -> Target {
        self.snapshot.target.clone().unwrap_or(Target::Miss)
    }
    fn get_screen(&mut self) -> Option<String> {
        self.snapshot.screen.clone()
    }
    fn get_gui(&mut self) -> Gui {
        self.snapshot.gui.unwrap_or(Gui {
            width: 0,
            height: 0,
            scale: 1.0,
        })
    }
    fn get_fps(&mut self) -> u32 {
        self.snapshot.fps
    }
    fn get_mining(&mut self) -> Option<Mining> {
        self.snapshot.mining
    }
}
impl level::Host for State {
    fn get_block(&mut self, pos: BlockPos) -> Option<String> {
        self.snapshot.level.as_ref()?.block([pos.x, pos.y, pos.z])
    }
    fn get_block_info(&mut self, pos: BlockPos) -> Option<BlockInfo> {
        self.snapshot
            .level
            .as_ref()?
            .block_info([pos.x, pos.y, pos.z])
    }
    fn get_entity_info(&mut self, id: i32) -> Option<EntityInfo> {
        self.snapshot
            .entity_info
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }
    fn get_biome(&mut self, pos: BlockPos) -> Option<String> {
        self.snapshot.level.as_ref()?.biome([pos.x, pos.y, pos.z])
    }
    fn get_light(&mut self, pos: BlockPos) -> Option<(u8, u8)> {
        self.snapshot.level.as_ref()?.light([pos.x, pos.y, pos.z])
    }
    fn get_spawn_spots(&mut self, from: BlockPos, to: BlockPos) -> SpawnSpots {
        let Some(level) = self.snapshot.level.as_ref() else {
            return SpawnSpots::default();
        };
        let (from, to) = ([from.x, from.y, from.z], [to.x, to.y, to.z]);
        let lo: [i32; 3] = std::array::from_fn(|i| from[i].min(to[i]));
        let hi: [i32; 3] = std::array::from_fn(|i| {
            from[i]
                .max(to[i])
                .min(lo[i].saturating_add(SPAWN_SPOTS_SIDE - 1))
        });
        level.spawn_spots(lo, hi)
    }
    fn get_entities(&mut self, radius: f32) -> Vec<Entity> {
        let Some(p) = &self.snapshot.player else {
            return Vec::new();
        };
        let r2 = f64::from(radius.max(0.0)).powi(2);
        self.snapshot
            .entities
            .iter()
            .filter(|e| (e.x - p.x).powi(2) + (e.y - p.y).powi(2) + (e.z - p.z).powi(2) <= r2)
            .take(ENTITIES)
            .cloned()
            .collect()
    }
}
impl inventory::Host for State {
    fn get_slot(&mut self, slot: u32) -> Option<ItemStack> {
        self.snapshot
            .inventory
            .get(slot as usize)
            .cloned()
            .flatten()
    }
    fn get_held(&mut self, hand: Hand) -> Option<ItemStack> {
        let slot = match hand {
            Hand::MainHand => self.snapshot.player.as_ref()?.selected_slot,
            Hand::OffHand => 40,
        };
        self.get_slot(slot)
    }
    fn get_cursor(&mut self) -> Option<ItemStack> {
        self.snapshot.cursor.clone()
    }
    fn get_hovered(&mut self) -> Option<ItemStack> {
        self.snapshot.hovered.clone()
    }
    fn get_container(&mut self) -> Option<Container> {
        self.snapshot.container.clone()
    }
    fn get_item_info(&mut self, id: String) -> Option<ItemInfo> {
        self.snapshot.items.as_ref()?.info(&id)
    }
    fn get_item_order(&mut self, stack: ItemStack) -> Option<u32> {
        self.snapshot.creative.as_ref()?.order(&stack)
    }
}
impl State {
    /// Queues a chat line unless the call already sent [`CHAT_PER_CALL`].
    fn say(&mut self, line: impl FnOnce(String) -> Output) -> bool {
        let shown = self
            .out
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Output::Chat { .. } | Output::Tell { .. } | Output::Broadcast { .. }
                )
            })
            .count();
        if shown < CHAT_PER_CALL {
            let name = self.name.clone();
            self.out.push(line(name));
        }
        shown < CHAT_PER_CALL
    }
}
impl chat::Host for State {
    fn show(&mut self, text: String) {
        self.say(|name| Output::Chat {
            name,
            text: clip_to(text, TEXT_CHARS),
        });
    }
    fn show_overlay(&mut self, text: String) {
        self.out.retain(|o| !matches!(o, Output::Overlay { .. }));
        self.out.push(Output::Overlay {
            text: clip_to(text, TEXT_CHARS),
        });
    }
}
impl browser::Host for State {
    fn set_items(&mut self, items: Vec<BrowserStack>) {
        self.browser.items = items
            .into_iter()
            .take(BROWSER_ITEMS)
            .map(|s| BrowserStack {
                item: clip(s.item),
                components: clip_to(s.components, 4096),
                ..s
            })
            .collect();
        self.browser.version = BROWSER_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn set_recipes(
        &mut self,
        title: String,
        tabs: Vec<BrowserStack>,
        tab: u32,
        recipes: Vec<BrowserRecipe>,
    ) {
        let stack = |s: BrowserStack| BrowserStack {
            item: clip(s.item),
            components: clip_to(s.components, 4096),
            ..s
        };
        let cycle = |list: Vec<BrowserStack>| -> Vec<BrowserStack> {
            list.into_iter().take(SLOT_STACKS).map(stack).collect()
        };
        self.browser.recipes = (!recipes.is_empty()).then(|| {
            let recipes = recipes
                .into_iter()
                .take(BROWSER_RECIPES)
                .map(|r| BrowserRecipe {
                    width: r.width.clamp(1, 3),
                    height: r.height.clamp(1, 3),
                    slots: r.slots.into_iter().take(RECIPE_SLOTS).map(cycle).collect(),
                    results: cycle(r.results),
                    note: clip(r.note),
                    ..r
                })
                .collect();
            RecipePage {
                title: clip(title),
                tabs: tabs.into_iter().take(RECIPE_TABS).map(stack).collect(),
                tab,
                recipes,
            }
        });
        self.browser.version = BROWSER_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn poll(&mut self) -> Vec<BrowserEvent> {
        self.browser.events.drain(..).collect()
    }
}
impl horizon_recipes::Host for State {
    fn items(&mut self) -> Vec<RecipeItem> {
        self.snapshot
            .horizon_recipes
            .as_ref()
            .map_or_else(Vec::new, |r| r.items.clone())
    }
    fn all(&mut self) -> Vec<HorizonRecipe> {
        self.snapshot
            .horizon_recipes
            .as_ref()
            .map_or_else(Vec::new, |r| r.recipes.clone())
    }
    fn known(&mut self) -> Vec<i32> {
        self.snapshot.known_recipes.to_vec()
    }
    fn place(&mut self, book_id: i32, all: bool) -> Result<(), String> {
        self.recipe_action()?;
        if !self.snapshot.placing {
            return Err("no screen that takes recipes is open".into());
        }
        if !self.snapshot.known_recipes.contains(&book_id) {
            return Err(format!(
                "recipe {book_id} is not in the player's recipe book"
            ));
        }
        self.out.push(Output::PlaceRecipe { book_id, all });
        Ok(())
    }
    fn give(&mut self, stack: RecipeStack) -> Result<(), String> {
        self.recipe_action()?;
        if !self.snapshot.creative_mode {
            return Err("only in creative mode".into());
        }
        let item = self
            .snapshot
            .horizon_recipes
            .as_ref()
            .and_then(|r| r.items.get(stack.item as usize))
            .ok_or_else(|| format!("no item {}", stack.item))?
            .id
            .clone();
        self.out.push(Output::Give {
            item,
            count: stack.count.clamp(1, 99),
            components: clip_to(stack.components, 4096),
        });
        Ok(())
    }
}
impl State {
    /// `recipes.place` and `give` take at most [`RECIPE_ACTIONS`] a call.
    fn recipe_action(&self) -> Result<(), String> {
        let done = self
            .out
            .iter()
            .filter(|o| matches!(o, Output::PlaceRecipe { .. } | Output::Give { .. }))
            .count();
        if done < RECIPE_ACTIONS {
            Ok(())
        } else {
            Err(format!("at most {RECIPE_ACTIONS} places and gives a call"))
        }
    }
}
impl mod_world::Host for State {
    fn get_block(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<String> {
        let view = self.world_view.lock().ok()?;
        view.as_ref()?.block(&dimension, x, y, z)
    }
    fn get_biome(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<String> {
        let view = self.world_view.lock().ok()?;
        view.as_ref()?.biome(&dimension, x, y, z)
    }
    fn set_blocks(&mut self, dimension: String, blocks: Vec<BlockChange>) -> Result<(), String> {
        if blocks.len() > BLOCKS_PER_CALL {
            return Err(format!(
                "{} blocks: at most {BLOCKS_PER_CALL} per call",
                blocks.len()
            ));
        }
        if let Some(view) = self.world_view.lock().ok().as_ref().and_then(|v| v.as_ref()) {
            if !view.has_dimension(&dimension) {
                return Err(format!("no dimension {dimension}"));
            }
            for b in &blocks {
                view.check_state(&b.state)?;
            }
        }
        if !blocks.is_empty() {
            self.out.push(Output::SetBlocks {
                name: self.name.clone(),
                dimension,
                blocks,
            });
        }
        Ok(())
    }
}
impl players::Host for State {
    fn online(&mut self) -> Vec<ServerPlayer> {
        self.snapshot.players.clone()
    }
    fn tell(&mut self, player: String, text: String) -> bool {
        self.snapshot.players.iter().any(|p| p.id == player)
            && self.say(|name| Output::Tell {
                name,
                player,
                text: clip(text),
            })
    }
    fn broadcast(&mut self, text: String) {
        self.say(|name| Output::Broadcast {
            name,
            text: clip(text),
        });
    }
}
impl chat_send::Host for State {
    fn send(&mut self, message: String) -> Result<(), String> {
        let now = Instant::now();
        while self
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
        {
            self.sent.pop_front();
        }
        if self.sent.len() >= SENDS_PER_SECOND {
            return Err(format!("at most {SENDS_PER_SECOND} messages per second"));
        }
        let message = message.trim();
        if message.is_empty() || message.chars().any(|c| c.is_control() || c == '§') {
            return Err("an empty message, or one with control or § characters".into());
        }
        self.sent.push_back(now);
        self.out.push(Output::Send {
            message: clip_to(message.to_owned(), SEND_CHARS),
        });
        Ok(())
    }
}
impl draw::Host for State {
    fn fill(&mut self, x: f32, y: f32, width: f32, height: f32, color: u32) {
        self.draw(DrawOp::Fill {
            x,
            y,
            w: width,
            h: height,
            color,
        });
    }
    fn gradient(&mut self, x: f32, y: f32, width: f32, height: f32, top: u32, bottom: u32) {
        self.draw(DrawOp::Gradient {
            x,
            y,
            w: width,
            h: height,
            top,
            bottom,
        });
    }
    fn text(&mut self, x: f32, y: f32, text: String, color: u32, shadow: bool) -> f32 {
        let text = clip_to(text, TEXT_CHARS);
        let width = self.measure(&text);
        self.draw(DrawOp::Text {
            x,
            y,
            text,
            color,
            shadow,
        });
        width
    }
    fn text_width(&mut self, text: String) -> f32 {
        self.measure(&clip_to(text, TEXT_CHARS))
    }
    fn item(&mut self, x: f32, y: f32, stack: ItemStack) {
        self.draw(DrawOp::Item { x, y, stack });
    }
    fn item_id(&mut self, x: f32, y: f32, id: String) {
        self.draw(DrawOp::ItemId { x, y, id: clip(id) });
    }
    fn sprite(&mut self, x: f32, y: f32, width: f32, height: f32, sprite: String) {
        self.draw(DrawOp::Sprite {
            x,
            y,
            w: width,
            h: height,
            sprite: clip(sprite),
        });
    }
    fn tooltip(&mut self, x: f32, y: f32, lines: Vec<String>) {
        let lines = lines
            .into_iter()
            .take(64)
            .map(|l| clip_to(l, TEXT_CHARS))
            .collect();
        self.draw(DrawOp::Tooltip { x, y, lines });
    }
    fn button(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        label: String,
        hovered: bool,
        active: bool,
    ) {
        self.draw(DrawOp::Button {
            x,
            y,
            w: width,
            h: height,
            label: clip_to(label, TEXT_CHARS),
            hovered,
            active,
        });
    }
    fn panel(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.draw(DrawOp::Panel {
            x,
            y,
            w: width,
            h: height,
        });
    }
    fn push(&mut self, x: f32, y: f32, scale: f32) {
        self.draw(DrawOp::Push {
            x,
            y,
            scale: scale.clamp(0.05, 20.0),
        });
    }
    fn pop(&mut self) {
        self.draw(DrawOp::Pop);
    }
    fn player(&mut self, x: f32, y: f32, width: f32, height: f32, size: f32, turn: f32) {
        self.draw(DrawOp::Player {
            x,
            y,
            w: width,
            h: height,
            size,
            turn,
        });
    }
    fn item_tooltip(&mut self, x: f32, y: f32, stack: ItemStack) {
        self.draw(DrawOp::ItemTooltip { x, y, stack });
    }
}
impl hud::Host for State {
    fn set_panel(&mut self, corner: Corner, lines: Vec<Line>) {
        let lines = lines
            .into_iter()
            .take(HUD_LINES)
            .map(|l| Line {
                text: clip(l.text),
                color: l.color & 0xFF_FFFF,
            })
            .collect();
        self.panel = Some(Panel { corner, lines });
    }
    fn clear(&mut self) {
        self.panel = None;
    }
}
impl screens::Host for State {
    fn open(&mut self, name: String) {
        if screen_name_ok(&name) {
            self.out.push(Output::OpenScreen {
                screen: format!("{}:{name}", self.id),
            });
        } else {
            log::warn!(
                "[mod {}] screen name {:?} is not [a-z0-9_./-]{{1,64}}",
                self.id,
                clip(name)
            );
        }
    }
    fn close(&mut self) {
        self.out.push(Output::CloseScreen {
            id: self.id.clone(),
        });
    }
    fn set_config(&mut self, name: String) {
        if screen_name_ok(&name) {
            self.config = Some(format!("{}:{name}", self.id));
        } else {
            log::warn!(
                "[mod {}] config screen {:?} is not [a-z0-9_./-]{{1,64}}",
                self.id,
                clip(name)
            );
        }
    }
    fn replace(&mut self, screen: String, name: String) -> bool {
        if !REPLACEABLE_SCREENS.contains(&screen.as_str())
            || !(name.is_empty() || screen_name_ok(&name))
        {
            return false;
        }
        self.replaces.retain(|(game, _)| *game != screen);
        if !name.is_empty() {
            let own = format!("{}:{name}", self.id);
            self.replaces.push((screen, own));
        }
        true
    }
    fn set_slots(&mut self, slots: Vec<screens::ScreenSlot>) {
        let slots: Vec<(u32, f32, f32)> = slots
            .into_iter()
            .filter(|s| s.slot < 128 && s.x.is_finite() && s.y.is_finite())
            .take(128)
            .map(|s| (s.slot, s.x, s.y))
            .collect();
        // The last call's slots are the ones.
        let id = self.id.clone();
        self.out
            .retain(|o| !matches!(o, Output::ScreenSlots { id: owner, .. } if *owner == id));
        self.out.push(Output::ScreenSlots { id, slots });
    }
    fn get_window(&mut self) -> Option<(f32, f32, f32, f32)> {
        self.snapshot.window.map(|[x, y, w, h]| (x, y, w, h))
    }
}
/// `tooltips.add-grid`'s grids a tooltip, and stacks a grid.
const TOOLTIP_GRIDS: usize = 4;
const GRID_STACKS: usize = 64;
impl tooltips::Host for State {
    fn add_grid(&mut self, grid: ItemGrid, instead: bool) -> Result<(), String> {
        let Some(grids) = self.grids.as_mut() else {
            return Err("grids go in tooltips: add them from events.tooltip".into());
        };
        if !(1..=16).contains(&grid.columns) {
            return Err(format!("a grid has 1 to 16 columns, not {}", grid.columns));
        }
        if grid.stacks.len() > GRID_STACKS {
            return Err(format!("a grid has at most {GRID_STACKS} stacks"));
        }
        if grids.len() >= TOOLTIP_GRIDS {
            return Err(format!("a tooltip has at most {TOOLTIP_GRIDS} grids"));
        }
        grids.push((grid, instead));
        Ok(())
    }
}
/// `recipes.get-recipes`' page.
const RECIPE_PAGE: u32 = 16;
impl recipes::Host for State {
    fn count_recipes(&mut self, stack: ItemStack, lookup: Lookup) -> Vec<(RecipeKind, u32)> {
        self.snapshot
            .recipes
            .as_ref()
            .map(|r| r.count(&stack, lookup))
            .unwrap_or_default()
    }
    fn get_recipes(
        &mut self,
        stack: ItemStack,
        lookup: Lookup,
        kind: RecipeKind,
        start: u32,
        count: u32,
    ) -> Vec<Recipe> {
        self.snapshot
            .recipes
            .as_ref()
            .map(|r| {
                r.recipes(
                    &stack,
                    lookup,
                    kind,
                    start as usize,
                    count.min(RECIPE_PAGE) as usize,
                )
            })
            .unwrap_or_default()
    }
    fn fill(&mut self, id: String, all: bool) -> bool {
        let Some((kind, width, height)) = self.snapshot.recipes.as_ref().and_then(|r| r.shape(&id))
        else {
            return false;
        };
        let fits = match self.snapshot.crafting {
            // A shapeless recipe's inputs fill the grid in turn.
            Some(Crafting::Grid(n)) if height == 0 => {
                kind == RecipeKind::Crafting && width <= n * n
            }
            Some(Crafting::Grid(n)) => kind == RecipeKind::Crafting && width <= n && height <= n,
            Some(Crafting::Furnace(furnace)) => kind == furnace,
            None => false,
        };
        if fits {
            let owner = self.id.clone();
            // One fill a call: the last one asked.
            self.out
                .retain(|o| !matches!(o, Output::FillRecipe { id, .. } if *id == owner));
            self.out.push(Output::FillRecipe {
                id: owner,
                recipe: id,
                all,
            });
        }
        fits
    }
}
impl slots::Host for State {
    fn get_menu(&mut self) -> Option<Menu> {
        self.snapshot.menu.clone()
    }
    fn click(&mut self, slot: i32, button: u8, kind: ClickKind) -> Result<(), String> {
        if slot != -999 && !(0..128).contains(&slot) {
            return Err(format!("no menu has slot {slot}"));
        }
        self.room_for_click()?;
        let id = self.id.clone();
        self.out.push(Output::Click {
            id,
            slot,
            button,
            kind,
        });
        Ok(())
    }
    fn drag(&mut self, slots: Vec<i32>, button: u8) -> Result<(), String> {
        if button > 2 {
            return Err(format!("no drag with button {button}: 0 to 2"));
        }
        if let Some(slot) = slots.iter().find(|s| !(0..128).contains(*s)) {
            return Err(format!("no menu has slot {slot}"));
        }
        // Its start, a click a slot, its end.
        self.room_for_clicks(slots.len() + 2)?;
        let id = self.id.clone();
        self.out.push(Output::Drag { id, slots, button });
        Ok(())
    }
}

impl State {
    /// Whether one more click (of `slots` or `creative`) fits in this call.
    fn room_for_click(&self) -> Result<(), String> {
        self.room_for_clicks(1)
    }
    /// Whether `more` clicks fit in this call.
    fn room_for_clicks(&self, more: usize) -> Result<(), String> {
        let queued: usize = self
            .out
            .iter()
            .map(|o| match o {
                Output::Click { .. }
                | Output::CreativeClick { .. }
                | Output::CreativeSlot { .. }
                | Output::CreativeCarried { .. } => 1,
                Output::Drag { slots, .. } => slots.len() + 2,
                _ => 0,
            })
            .sum();
        if queued + more > CLICKS_PER_CALL {
            return Err(format!("at most {CLICKS_PER_CALL} clicks a call"));
        }
        Ok(())
    }
    /// `creative`'s takings need the player in creative mode.
    fn in_creative(&self) -> Result<(), String> {
        match &self.snapshot.player {
            Some(p) if p.game_mode == GameMode::Creative => Ok(()),
            _ => Err("only in creative mode".into()),
        }
    }
    /// A stack a mod gives: a known item, no more than it stacks to.
    fn stack_ok(&self, stack: &ItemStack) -> Result<(), String> {
        let Some(info) = self.snapshot.items.as_ref().and_then(|i| i.info(&stack.id)) else {
            return Err(format!("no item {}", clip(stack.id.clone())));
        };
        if stack.count == 0 || stack.count > info.max_stack_size {
            return Err(format!(
                "{} of {} (it stacks to {})",
                stack.count, stack.id, info.max_stack_size
            ));
        }
        Ok(())
    }
}

impl creative::Host for State {
    fn get_tabs(&mut self) -> Vec<Tab> {
        self.snapshot
            .creative
            .as_ref()
            .map(|c| c.tabs())
            .unwrap_or_default()
    }
    fn get_items(&mut self, tab: String, search: String, start: u32, count: u32) -> ItemPage {
        let empty = ItemPage {
            total: 0,
            items: Vec::new(),
        };
        let Some(c) = self.snapshot.creative.as_ref() else {
            return empty;
        };
        let search = clip(search);
        c.items(
            &tab,
            &search,
            start as usize,
            count.min(CREATIVE_PAGE) as usize,
        )
    }
    fn click(
        &mut self,
        tab: String,
        search: String,
        index: u32,
        button: u8,
        kind: CreativeClick,
    ) -> Result<(), String> {
        self.in_creative()?;
        self.room_for_click()?;
        let id = self.id.clone();
        self.out.push(Output::CreativeClick {
            id,
            tab: clip(tab),
            search: clip(search),
            index,
            button,
            kind,
        });
        Ok(())
    }
    fn set_slot(&mut self, slot: u32, stack: Option<ItemStack>) -> Result<(), String> {
        self.in_creative()?;
        if !(1..=45).contains(&slot) {
            return Err(format!(
                "the inventory menu's slot {slot} takes no item: 1 to 45 do"
            ));
        }
        if let Some(s) = &stack {
            self.stack_ok(s)?;
        }
        self.room_for_click()?;
        let id = self.id.clone();
        self.out.push(Output::CreativeSlot { id, slot, stack });
        Ok(())
    }
    fn set_carried(&mut self, stack: Option<ItemStack>) -> Result<(), String> {
        self.in_creative()?;
        if let Some(s) = &stack {
            self.stack_ok(s)?;
        }
        self.room_for_click()?;
        let id = self.id.clone();
        self.out.push(Output::CreativeCarried { id, stack });
        Ok(())
    }
}

/// Lines in a `world-draw` layer.
fn layer_lines(layer: &WorldLayer) -> usize {
    layer.lines.iter().map(|l| l.ends.len() / 6).sum()
}
/// Boxes in a `world-draw` layer.
fn layer_boxes(layer: &WorldLayer) -> usize {
    layer.boxes.iter().map(|b| b.corners.len() / 6).sum()
}
/// Texts in a `world-draw` layer.
fn layer_texts(layer: &WorldLayer) -> usize {
    layer.texts.iter().map(|t| t.places.len() / 3).sum()
}
impl world_draw::Host for State {
    fn set_layer(&mut self, id: String, layer: WorldLayer) -> Result<(), String> {
        if id.is_empty() || id.chars().count() > 64 {
            return Err(format!(
                "{:?} is not a layer's id (1 to 64 characters)",
                clip(id)
            ));
        }
        // Within the world border and then some.
        let point_ok = |p: &Point| {
            [p.x, p.y, p.z]
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 3.0e7)
        };
        for l in &layer.lines {
            if !point_ok(&l.origin)
                || l.ends.len() % 6 != 0
                || l.ends.iter().any(|n| !n.is_finite())
            {
                return Err("lines need a place and six finite numbers a line".into());
            }
            let count = l.ends.len() / 6;
            if l.colors.len() != 1 && l.colors.len() != count {
                return Err(format!(
                    "lines need one colour for all, or one a line ({} for {count})",
                    l.colors.len()
                ));
            }
            if !(l.width.is_finite() && l.width > 0.0) {
                return Err("lines need a width".into());
            }
        }
        for b in &layer.boxes {
            if !point_ok(&b.origin)
                || b.corners.len() % 6 != 0
                || b.corners.iter().any(|n| !n.is_finite())
            {
                return Err("boxes need a place and six finite numbers a box".into());
            }
            let count = b.corners.len() / 6;
            if b.colors.len() != 1 && b.colors.len() != count {
                return Err(format!(
                    "boxes need one colour for all, or one a box ({} for {count})",
                    b.colors.len()
                ));
            }
        }
        for t in &layer.texts {
            if !point_ok(&t.origin)
                || t.places.len() % 3 != 0
                || t.places.iter().any(|n| !n.is_finite())
                || !(t.scale.is_finite() && t.scale > 0.0)
            {
                return Err("texts need a place, a size and three finite numbers a text".into());
            }
            let count = t.places.len() / 3;
            let lines = if t.lines.is_empty() {
                0
            } else {
                t.lines.split('\n').count()
            };
            if lines != count {
                return Err(format!("{count} places for texts, {lines} lines"));
            }
            if t.colors.len() != 1 && t.colors.len() != count {
                return Err(format!(
                    "texts need one colour for all, or one a text ({} for {count})",
                    t.colors.len()
                ));
            }
        }
        if let Some(b) = layer
            .labels
            .iter()
            .find(|b| !point_ok(&b.at) || !(b.scale.is_finite() && b.scale > 0.0))
        {
            return Err(format!(
                "label {:?} needs a place and a size",
                clip_to(b.text.clone(), 32)
            ));
        }
        let others = self.world.iter().filter(|(k, _)| *k != id);
        let count = |l: &WorldLayer| {
            [
                layer_lines(l),
                layer_boxes(l),
                layer_texts(l),
                l.labels.len(),
            ]
        };
        let totals = others.fold(count(&layer), |t, (_, l)| {
            let c = count(l);
            std::array::from_fn(|i| t[i] + c[i])
        });
        let caps = [WORLD_LINES, WORLD_BOXES, WORLD_TEXTS, WORLD_LABELS];
        if totals.iter().zip(caps).any(|(n, cap)| *n > cap) {
            return Err(format!(
                "a mod draws at most {WORLD_LINES} lines, {WORLD_BOXES} boxes, {WORLD_TEXTS} \
                 texts and {WORLD_LABELS} labels in the world"
            ));
        }
        let layer = WorldLayer {
            lines: layer
                .lines
                .into_iter()
                .filter(|l| !l.ends.is_empty())
                .map(|l| WorldLines {
                    width: l.width.min(32.0),
                    ..l
                })
                .collect(),
            boxes: layer
                .boxes
                .into_iter()
                .filter(|b| !b.corners.is_empty())
                .collect(),
            texts: layer
                .texts
                .into_iter()
                .filter(|t| !t.places.is_empty())
                .map(|t| WorldTexts {
                    scale: t.scale.min(64.0),
                    ..t
                })
                .collect(),
            labels: layer
                .labels
                .into_iter()
                .map(|b| WorldLabel {
                    text: clip(b.text),
                    scale: b.scale.min(64.0),
                    ..b
                })
                .collect(),
            ..layer
        };
        let full = self.world.len() >= WORLD_LAYERS;
        match self.world.iter_mut().find(|(k, _)| *k == id) {
            Some((_, old)) => *old = layer,
            None if full => {
                return Err(format!(
                    "a mod has at most {WORLD_LAYERS} layers in the world"
                ));
            }
            None => self.world.push((id, layer)),
        }
        Ok(())
    }
    fn remove_layer(&mut self, id: String) {
        self.world.retain(|(k, _)| *k != id);
        self.icons.retain(|(k, _)| *k != id);
    }
    fn set_icons(&mut self, id: String, icons: Vec<WorldIcon>) -> Result<(), String> {
        if id.is_empty() || id.chars().count() > 64 {
            return Err(format!(
                "{:?} is not a layer's id (1 to 64 characters)",
                clip(id)
            ));
        }
        let size_ok = |n: f32| n.is_finite() && n > 0.0;
        if let Some(i) = icons.iter().find(|i| {
            let at = [i.at.x, i.at.y, i.at.z];
            !at.iter().all(|n| n.is_finite() && n.abs() <= 3.0e7)
                || !size_ok(i.size)
                || !size_ok(i.text_scale)
        }) {
            return Err(format!(
                "icon {:?} needs a place, a size and a text size",
                clip_to(i.sprite.clone(), 32)
            ));
        }
        let others: usize = self
            .icons
            .iter()
            .filter(|(k, _)| *k != id)
            .map(|(_, i)| i.len())
            .sum();
        if others + icons.len() > WORLD_ICONS {
            return Err(format!(
                "a mod draws at most {WORLD_ICONS} icons in the world"
            ));
        }
        let icons: Vec<WorldIcon> = icons
            .into_iter()
            .map(|i| WorldIcon {
                sprite: clip(i.sprite),
                above: clip(i.above),
                below: clip(i.below),
                size: i.size.min(256.0),
                text_scale: i.text_scale.min(64.0),
                ..i
            })
            .collect();
        let full = self.icons.len() >= WORLD_LAYERS;
        match self.icons.iter_mut().find(|(k, _)| *k == id) {
            Some((_, old)) => *old = icons,
            None if full => {
                return Err(format!(
                    "a mod has at most {WORLD_LAYERS} layers in the world"
                ));
            }
            None => self.icons.push((id, icons)),
        }
        Ok(())
    }
}
impl widgets::Host for State {
    fn add_screen_button(&mut self, b: ScreenButton) -> Result<(), String> {
        button_id_ok(&b.id)?;
        if !screen_name_ok(&b.screen.replace(':', "/")) {
            return Err(format!("{:?} is not a screen's name", clip(b.screen)));
        }
        let finite = [b.x, b.y, b.width, b.height].iter().all(|n| n.is_finite());
        if !finite || b.width < 1.0 || b.height < 1.0 {
            return Err(format!("button {} has no size", b.id));
        }
        let all = &mut self.widgets.screen;
        all.retain(|o| o.id != b.id);
        if all.len() >= SCREEN_BUTTONS {
            return Err(format!("a mod has at most {SCREEN_BUTTONS} screen buttons"));
        }
        all.push(ScreenButton {
            x: b.x.clamp(-4096.0, 4096.0),
            y: b.y.clamp(-4096.0, 4096.0),
            width: b.width.min(4096.0),
            height: b.height.min(4096.0),
            label: clip(b.label),
            icon: clip(b.icon),
            tooltip: b.tooltip.into_iter().take(8).map(clip).collect(),
            ..b
        });
        Ok(())
    }
    fn add_touch_button(&mut self, b: TouchButton) -> Result<(), String> {
        button_id_ok(&b.id)?;
        let all = &mut self.widgets.touch;
        all.retain(|o| o.id != b.id);
        if all.len() >= TOUCH_BUTTONS {
            return Err(format!("a mod has at most {TOUCH_BUTTONS} touch buttons"));
        }
        let or = |n: f32, d: f32| if n.is_finite() { n } else { d };
        all.push(TouchButton {
            label: clip_to(b.label, 24),
            icon: clip(b.icon),
            x: or(b.x, 0.5).clamp(0.0, 1.0),
            y: or(b.y, 0.5).clamp(0.0, 1.0),
            size: or(b.size, 0.5).clamp(0.3, 2.0),
            ..b
        });
        Ok(())
    }
    fn add_window_button(&mut self, b: WindowButton) -> Result<(), String> {
        button_id_ok(&b.id)?;
        let finite = [b.x, b.y, b.width, b.height].iter().all(|n| n.is_finite());
        if !finite || b.width < 1.0 || b.height < 1.0 {
            return Err(format!("button {} has no size", b.id));
        }
        if let Some(menu) = b
            .menus
            .iter()
            .find(|m| !screen_name_ok(&m.replace(':', "/")))
        {
            return Err(format!("{:?} is not a menu's name", clip(menu.clone())));
        }
        let all = &mut self.widgets.window;
        all.retain(|o| o.id != b.id);
        if all.len() >= WINDOW_BUTTONS {
            return Err(format!("a mod has at most {WINDOW_BUTTONS} window buttons"));
        }
        all.push(WindowButton {
            x: b.x.clamp(-512.0, 512.0),
            y: b.y.clamp(-512.0, 512.0),
            width: b.width.min(64.0),
            height: b.height.min(64.0),
            label: clip_to(b.label, 24),
            icon: clip(b.icon),
            tooltip: b.tooltip.into_iter().take(8).map(clip).collect(),
            menus: b.menus.into_iter().take(32).collect(),
            ..b
        });
        Ok(())
    }
    fn remove(&mut self, id: String) {
        self.widgets.screen.retain(|b| b.id != id);
        self.widgets.touch.retain(|b| b.id != id);
        self.widgets.window.retain(|b| b.id != id);
    }
    fn style_control(
        &mut self,
        control: String,
        label: String,
        icon: String,
        shape: ButtonShape,
    ) -> bool {
        if !TOUCH_CONTROLS.contains(&control.as_str()) {
            return false;
        }
        let styles = &mut self.widgets.styles;
        styles.retain(|s| s.control != control);
        styles.push(ControlStyle {
            control,
            label: clip_to(label, 24),
            icon: clip(icon),
            shape,
        });
        true
    }
}

/// A button's id: as a keybind's, `[a-z0-9_.-]{1,64}`.
fn button_id_ok(id: &str) -> Result<(), String> {
    let ok = (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "button id {:?} is not [a-z0-9_.-]{{1,64}}",
            clip(id.to_owned())
        ))
    }
}

impl mobile::Host for State {
    fn has_touch(&mut self) -> bool {
        self.snapshot.touch
    }
    fn keyboard(&mut self, shown: bool) {
        self.out.push(Output::Keyboard {
            id: self.id.clone(),
            shown,
        });
    }
    fn drag_areas(&mut self, areas: Vec<(f32, f32, f32, f32)>) {
        let areas: Vec<[f32; 4]> = areas
            .into_iter()
            .take(DRAG_AREAS)
            .map(|(x, y, w, h)| [x, y, w, h])
            .filter(|a| a.iter().all(|v| v.is_finite()) && a[2] > 0.0 && a[3] > 0.0)
            .collect();
        // The last call's areas are the ones.
        let id = self.id.clone();
        self.out
            .retain(|o| !matches!(o, Output::DragAreas { id: owner, .. } if *owner == id));
        self.out.push(Output::DragAreas { id, areas });
    }
}
impl keys::Host for State {
    fn is_down(&mut self, id: String) -> bool {
        let full = format!("{}:{id}", self.id);
        self.snapshot.keys_down.contains(&full)
    }
    fn is_control_down(&mut self, control: String) -> bool {
        let i = CONTROLS.iter().position(|c| *c == control);
        i.is_some_and(|i| self.controls[i].0)
    }
    fn presses(&mut self, control: String) -> u64 {
        let i = CONTROLS.iter().position(|c| *c == control);
        i.map_or(0, |i| self.controls[i].1)
    }
}
impl input::Host for State {
    fn hold(&mut self, control: String, held: bool) -> bool {
        if !matches!(control.as_str(), "key.sprint" | "key.sneak") {
            return false;
        }
        // The last word for each control is enough.
        self.out
            .retain(|o| !matches!(o, Output::Hold { control: c, .. } if *c == control));
        let id = self.id.clone();
        self.out.push(Output::Hold { id, control, held });
        true
    }
}
impl sound::Host for State {
    fn play(&mut self, id: String, volume: f32, pitch: f32) {
        if self
            .out
            .iter()
            .filter(|o| matches!(o, Output::Sound { .. }))
            .count()
            < CHAT_PER_CALL
        {
            self.out.push(Output::Sound {
                id: clip(id),
                volume: volume.clamp(0.0, 1.0),
                pitch: pitch.clamp(0.5, 2.0),
            });
        }
    }
}
impl net::Host for State {
    fn send(&mut self, channel: String, data: Vec<u8>) -> Result<(), String> {
        if !self.channels.contains(&channel) {
            return Err(format!("{channel} is not one of the mod's channels"));
        }
        if !self.snapshot.world.as_ref().is_some_and(|w| w.multiplayer) {
            return Err("not on a server".into());
        }
        if data.len() > PAYLOAD_BYTES {
            return Err(format!("over {} KiB", PAYLOAD_BYTES >> 10));
        }
        self.out.push(Output::Payload { channel, data });
        Ok(())
    }
}
impl storage::Host for State {
    fn get(&mut self, key: String) -> Option<Vec<u8>> {
        storage_key_ok(&key)
            .then(|| std::fs::read(self.data_dir.join(&key)).ok())
            .flatten()
    }
    fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
        if !storage_key_ok(&key) {
            return Err(format!("{key:?} is not [a-z0-9_.-]{{1,64}}"));
        }
        let len = value.len() as u64;
        if len > self.storage_budget {
            return Err("too much storage traffic in one call".into());
        }
        // Everything already on disk but the file this call replaces.
        let existing: Vec<std::fs::DirEntry> = std::fs::read_dir(&self.data_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy() != key)
            .collect();
        if existing.len() >= STORAGE_MAX_KEYS {
            return Err(format!("would hold over {STORAGE_MAX_KEYS} keys"));
        }
        let used: u64 = existing
            .iter()
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
        if used + len > STORAGE_QUOTA {
            return Err(format!(
                "would use over {} MiB of storage",
                STORAGE_QUOTA >> 20
            ));
        }
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        std::fs::write(self.data_dir.join(&key), value).map_err(|e| e.to_string())?;
        self.storage_budget -= len;
        Ok(())
    }
    fn delete(&mut self, key: String) {
        if storage_key_ok(&key) {
            let _ = std::fs::remove_file(self.data_dir.join(&key));
        }
    }
}

/// Widgets on a Horizon `ui` screen or settings page, and rows on one `list`.
const WIDGETS: usize = 64;
const LIST_ROWS: usize = 512;

fn clip_widgets(widgets: Vec<UiWidget>) -> Vec<UiWidget> {
    widgets
        .into_iter()
        .take(WIDGETS)
        .map(|w| match w {
            UiWidget::Text(mut t) => {
                t.text = clip(t.text);
                UiWidget::Text(t)
            }
            UiWidget::Button(mut b) => {
                b.label = clip(b.label);
                UiWidget::Button(b)
            }
            UiWidget::Item(mut i) => {
                i.item = clip(i.item);
                i.name = clip(i.name);
                UiWidget::Item(i)
            }
            UiWidget::List(mut l) => {
                l.rows = l.rows.into_iter().take(LIST_ROWS).map(clip).collect();
                l.selected = l.selected.filter(|&s| (s as usize) < l.rows.len());
                UiWidget::List(l)
            }
        })
        .collect()
}

/// A key a Horizon mod registered (capability `input`).
#[derive(Clone, Debug, PartialEq)]
pub struct ModKey {
    /// The mod's own name for it, echoed in `key-down`.
    pub name: String,
    /// What it does, for Controls.
    pub label: String,
    /// `key.keyboard.r`.
    pub key: String,
}

/// What Horizon's capability `input` holds for one mod.
#[derive(Default)]
struct InputState {
    claimed: Vec<String>,
    keys: Vec<ModKey>,
    events: VecDeque<InputEvent>,
    /// The mod's keys held now, by name.
    down: Vec<String>,
    /// It holds the mouse wheel.
    wheel: bool,
    /// The keys the player chose, `name → key`: they win over
    /// `register-key`'s.
    bindings: Vec<(String, String)>,
}
/// Events kept for `poll`, items claimed and keys, per mod.
const INPUT_EVENTS: usize = 256;
const CLAIMS: usize = 64;
const KEYS: usize = 16;

/// `key.keyboard.<name>` or `key.mouse.<name>`, as options.txt writes keys;
/// the game knows which names exist and ignores a key it has none for.
fn key_ok(key: &str) -> bool {
    let rest = key
        .strip_prefix("key.keyboard.")
        .or_else(|| key.strip_prefix("key.mouse."));
    rest.is_some_and(|r| {
        (1..=32).contains(&r.len())
            && r.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
    })
}

impl InputState {
    /// Key `name` is `key` now; held, it lets go first.
    fn rebind(&mut self, name: &str, key: String) {
        let Some(k) = self.keys.iter_mut().find(|k| k.name == name) else {
            return;
        };
        k.key = key;
        if let Some(i) = self.down.iter().position(|n| n == name) {
            let name = self.down.swap_remove(i);
            self.push(InputEvent::KeyUp(name));
        }
    }
    fn push(&mut self, event: InputEvent) {
        if self.events.len() == INPUT_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

/// HUD shapes one mod may keep (Horizon's capability `render`).
const SHAPES: usize = 1024;
/// The longest jolt `render.shake` makes, in seconds.
const SHAKE_SECONDS: f32 = 10.0;
/// `render.set-zoom`'s limits: `Camera.tickFov` keeps the modifier at 0.1
/// or more, and turning never stops.
const ZOOM_MAX: f32 = 10.0;
const TURN_MIN: f32 = 0.05;
/// `set-minimap`'s side in GUI pixels and its blocks per map pixel.
const MINIMAP_SIZE: (f32, f32) = (16.0, 256.0);
const MINIMAP_SCALE: (f32, f32) = (0.5, 4.0);
/// `set-far-terrain`'s distance in chunks, as the LOD Distance setting's.
const FAR_TERRAIN_DISTANCE: (u32, u32) = (64, 1024);

/// A Horizon mod's corner panel: shapes in a `width` × `height` box the game places in
/// `corner`, stacked with the other overlays.
#[derive(Clone, Debug)]
pub struct CornerPanel {
    pub corner: RenderCorner,
    pub width: f32,
    pub height: f32,
    pub shapes: Vec<Shape>,
}

fn clip_shapes(shapes: Vec<Shape>) -> Vec<Shape> {
    shapes
        .into_iter()
        .take(SHAPES)
        .map(|shape| match shape {
            Shape::Sprite(mut s) => {
                s.sprite = clip(s.sprite);
                Shape::Sprite(s)
            }
            Shape::Item(mut i) => {
                i.item = clip(i.item);
                i.scale = if i.scale.is_finite() {
                    i.scale.clamp(0.0, 16.0)
                } else {
                    1.0
                };
                Shape::Item(i)
            }
            Shape::Text(mut t) => {
                t.text = clip(t.text);
                t.color &= 0xFF_FFFF;
                Shape::Text(t)
            }
            other => other,
        })
        .collect()
}

/// Horizon's API (`horizon:mods` 0.1), for mods built with its SDK: the same state, Horizon's
/// shapes. Its 0.1 interfaces become Starling's; `command`, `ui`, `render` and `input` are
/// Horizon's own (Starling's API has its own `input`, `screens` and `draw`).
mod legacy {
    use super::{
        CHAT_PER_CALL, CLAIMS, CornerPanel, FAR_TERRAIN_DISTANCE, FrameRate, HeldItem, Hit,
        InputEvent, KEYS, MINIMAP_SCALE, MINIMAP_SIZE, ModFarTerrain, ModKey, ModMinimap, ModZoom,
        Output, Panel, QUIET_PER_CALL, RAYCAST_MAX, RenderCorner, SHAKE_SECONDS, Shape, State,
        TURN_MIN, ZOOM_MAX, clip, clip_shapes, clip_widgets, key_ok,
        render_bindings::horizon::mods::{input, render},
        ui_bindings::horizon::mods::ui,
        v1::horizon::mods as old,
    };
    impl old::command::Host for State {
        fn run(&mut self, line: String) {
            let queued = self
                .out
                .iter()
                .filter(|o| matches!(o, Output::Command { .. }))
                .count();
            if queued < CHAT_PER_CALL {
                self.out.push(Output::Command { line: clip(line) });
            }
        }
        fn run_quiet(&mut self, lines: Vec<String>) {
            let queued: usize = self
                .out
                .iter()
                .map(|o| match o {
                    Output::QuietCommands { lines, .. } => lines.len(),
                    _ => 0,
                })
                .sum();
            let room = QUIET_PER_CALL.saturating_sub(queued);
            if room > 0 && !lines.is_empty() {
                self.out.push(Output::QuietCommands {
                    mod_id: self.id.clone(),
                    lines: lines.into_iter().take(room).map(clip).collect(),
                });
            }
        }
    }
    impl ui::Host for State {
        fn open_screen(&mut self, mut screen: ui::Screen) {
            screen.title = clip(screen.title);
            screen.widgets = clip_widgets(screen.widgets);
            self.ui.screen = Some(screen);
            self.ui.open = true;
        }
        fn close_screen(&mut self) {
            self.ui.screen = None;
            self.ui.close = true;
        }
        fn set_settings(&mut self, widgets: Vec<ui::Widget>) {
            self.ui.settings = clip_widgets(widgets);
        }
    }
    impl input::Host for State {
        fn claim_items(&mut self, ids: Vec<String>) {
            self.input.claimed = ids.into_iter().take(CLAIMS).map(clip).collect();
        }
        fn register_key(&mut self, name: String, label: String, key: String) -> Result<(), String> {
            if !key_ok(&key) {
                return Err(format!(
                    "{key:?} is not a key as options.txt writes them (key.keyboard.r)"
                ));
            }
            let (name, label) = (clip(name), clip(label));
            let key = match self.input.bindings.iter().find(|(n, _)| *n == name) {
                Some((_, chosen)) => chosen.clone(),
                None => key,
            };
            if let Some(k) = self.input.keys.iter_mut().find(|k| k.name == name) {
                (k.label, k.key) = (label, key);
                return Ok(());
            }
            if self.input.keys.len() == KEYS {
                return Err(format!("a mod may register {KEYS} keys"));
            }
            self.input.keys.push(ModKey { name, label, key });
            Ok(())
        }
        fn held(&mut self) -> Option<HeldItem> {
            self.snapshot.held.clone()
        }
        fn poll(&mut self) -> Vec<InputEvent> {
            self.input.events.drain(..).collect()
        }
        fn mapping(&mut self, name: String) -> Option<input::MappingState> {
            let (_, label, down) = self.snapshot.mappings.iter().find(|m| m.0 == name)?;
            Some(input::MappingState {
                label: label.clone(),
                down: *down,
            })
        }
        fn clicks_per_second(&mut self) -> (u32, u32) {
            self.snapshot.cps
        }
        fn claim_wheel(&mut self, on: bool) {
            self.input.wheel = on;
        }
    }
    impl render::Host for State {
        fn set_hud(&mut self, shapes: Vec<Shape>) {
            self.shapes = clip_shapes(shapes);
        }
        fn set_corner(
            &mut self,
            corner: RenderCorner,
            width: f32,
            height: f32,
            shapes: Vec<Shape>,
        ) {
            let size = |v: f32| {
                if v.is_finite() {
                    v.clamp(0.0, 1024.0)
                } else {
                    0.0
                }
            };
            self.corner = Some(CornerPanel {
                corner,
                width: size(width),
                height: size(height),
                shapes: clip_shapes(shapes),
            });
        }
        fn clear_corner(&mut self) {
            self.corner = None;
        }
        fn text_width(&mut self, text: String) -> f32 {
            let widths = &self.snapshot.font_widths;
            text.chars()
                .map(|c| widths.get(c as usize).copied().unwrap_or(6.0))
                .sum::<f32>()
                .ceil()
        }
        fn set_zoom(&mut self, zoom: ModZoom) {
            let finite =
                |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
            let factor = finite(zoom.factor, 1.0, ZOOM_MAX);
            self.zoom = (factor > 1.0).then(|| ModZoom {
                factor,
                smooth: zoom.smooth,
                turn: if zoom.turn.is_finite() {
                    zoom.turn.clamp(TURN_MIN, 1.0)
                } else {
                    1.0
                },
            });
        }
        fn set_minimap(&mut self, minimap: Option<ModMinimap>) {
            let finite =
                |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
            self.minimap = minimap.map(|m| ModMinimap {
                size: finite(m.size, MINIMAP_SIZE.0, MINIMAP_SIZE.1),
                blocks_per_pixel: finite(m.blocks_per_pixel, MINIMAP_SCALE.0, MINIMAP_SCALE.1),
                ..m
            });
        }
        fn set_far_terrain(&mut self, far_terrain: Option<ModFarTerrain>) {
            self.far_terrain = far_terrain.map(|f| ModFarTerrain {
                distance: f
                    .distance
                    .clamp(FAR_TERRAIN_DISTANCE.0, FAR_TERRAIN_DISTANCE.1),
                ..f
            });
        }
        fn frame_stats(&mut self) -> FrameRate {
            let (fps, min, low) = self.snapshot.frame;
            FrameRate { fps, min, low }
        }
        fn gui_size(&mut self) -> (f32, f32) {
            self.snapshot.gui_size
        }
        fn shake(&mut self, strength: f32, seconds: f32) {
            let ok = |v: f32, max: f32| {
                if v.is_finite() {
                    v.clamp(0.0, max)
                } else {
                    0.0
                }
            };
            let (strength, seconds) = (ok(strength, 1.0), ok(seconds, SHAKE_SECONDS));
            if strength > 0.0 && seconds > 0.0 {
                self.out.push(Output::Shake { strength, seconds });
            }
        }
    }
    impl old::log::Host for State {
        fn write(&mut self, level: old::log::Level, message: String) {
            use super::mod_log::{Host, Level};
            let level = match level {
                old::log::Level::Debug => Level::Debug,
                old::log::Level::Info => Level::Info,
                old::log::Level::Warn => Level::Warn,
                old::log::Level::Error => Level::Error,
            };
            Host::write(self, level, message);
        }
    }
    impl old::game::Host for State {
        fn get_player(&mut self) -> Option<old::game::Player> {
            let p = self.snapshot.player.as_ref()?;
            Some(old::game::Player {
                x: p.x,
                y: p.y,
                z: p.z,
                yaw: p.yaw,
                pitch: p.pitch,
                health: p.health,
                food: p.food,
                dimension: p.dimension.clone(),
            })
        }
        fn get_clock(&mut self) -> Option<old::game::Clock> {
            self.snapshot.clock.map(|c| old::game::Clock {
                game_time: c.game_time,
                day_time: c.day_time,
            })
        }
        fn raycast(&mut self, distance: f32) -> Option<Hit> {
            let (at, hit) = self.snapshot.view_hit.as_ref()?;
            // NaN compares false: no hit.
            (*at <= distance.min(RAYCAST_MAX) && !distance.is_nan()).then(|| hit.clone())
        }
    }
    impl old::chat::Host for State {
        fn show(&mut self, text: String) {
            super::chat::Host::show(self, text);
        }
    }
    impl old::hud::Host for State {
        fn set_panel(&mut self, corner: old::hud::Corner, lines: Vec<old::hud::Line>) {
            use super::{Corner, Line};
            let corner = match corner {
                old::hud::Corner::TopLeft => Corner::TopLeft,
                old::hud::Corner::TopRight => Corner::TopRight,
                old::hud::Corner::BottomLeft => Corner::BottomLeft,
                old::hud::Corner::BottomRight => Corner::BottomRight,
            };
            let lines = lines
                .into_iter()
                .map(|l| Line {
                    text: l.text,
                    color: l.color,
                })
                .collect();
            super::hud::Host::set_panel(self, corner, lines);
        }
        fn clear(&mut self) {
            super::hud::Host::clear(self);
        }
    }
    impl old::storage::Host for State {
        fn get(&mut self, key: String) -> Option<Vec<u8>> {
            super::storage::Host::get(self, key)
        }
        fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
            super::storage::Host::set(self, key, value)
        }
        fn delete(&mut self, key: String) {
            super::storage::Host::delete(self, key);
        }
    }
    // Keeps the imports used when a build has no 0.1 shapes left to convert.
    #[allow(dead_code)]
    fn _types(_: Output, _: Panel) {}
}

/// A storage key: one path segment of `[a-z0-9_.-]{1,64}`, not `.` or `..`.
/// No `/`, so a key can never leave the mod's own directory.
fn storage_key_ok(key: &str) -> bool {
    let chars_ok = (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
    chars_ok && key != "." && key != ".."
}
