//! Loading and calling mods with wasmtime's component model. A Starling mod links
//! `starling:mods@0.1.0` (starling-mod-sdk/wit); one built for Horizon links its `horizon:mods`
//! 0.1 interfaces (horizon-mod-sdk/wit, Horizon's own), which this host offers too, and keeps 0.1's
//! hooks.
use crate::manifest::{Capability, Manifest, Package, Side};
use std::{
    borrow::Cow,
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
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
pub use starling::exports::starling::mods::events::{
    ChatAction, HudElement, HudPart, KeyEvent, Keybind, MouseAction, MouseEvent, Placement,
    RenderContext, SendAction,
};
pub use starling::starling::mods::{
    creative::{ClickKind as CreativeClick, ItemPage, Tab, TabKind},
    game::{BlockHit, Clock, Effect, EntityHit, Gui, Mining, Player, Target, WorldInfo},
    hud::{Corner, Line},
    inventory::{Container, Food, ItemInfo},
    level::{BlockInfo, Entity, EntityInfo},
    slots::{ClickKind, Menu},
    types::{BlockPos, Direction, GameMode, Hand, ItemStack},
    widgets::{Anchor, ScreenButton, Shape, TouchButton, WindowButton, WindowPart},
};
use starling::{
    exports::starling::mods::events::Registration,
    starling::mods::{
        chat, chat_send, creative, draw, game, hud, input, inventory, keys, level, log as mod_log,
        mobile, net, screens, slots, sound, storage, types, widgets,
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
/// Chat lines a mod may show per call, HUD lines and their length.
const CHAT_PER_CALL: usize = 8;
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
/// A mod's data, beside its `horizon-mods` folder (`find`'s `path.parent()`
/// never has a manifest there, so `find` never mistakes it for a mod).
const STORAGE_DIR: &str = ".horizon-mod-data";

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
    pub measure: Option<Measure>,
    /// The mods' keybinds held now, as `<mod id>:<keybind id>`.
    pub keys_down: Vec<String>,
    /// The player plays with a touch screen (`mobile.has-touch`).
    pub touch: bool,
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
    /// The mod was turned off: it trapped, overran or failed to start.
    Stopped { name: String, reason: String },
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
    pub shape: Shape,
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
}

struct Running {
    store: Store<State>,
    guest: Guest,
    hooks: Hooks,
    capabilities: Vec<Capability>,
    /// The render hooks called already (`hud`, `element`, `screen`).
    rendered: Vec<&'static str>,
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
    /// Its settings screen (`screens.set-config`), `<mod id>:<name>`.
    config: Option<String>,
    /// The game's screens its own stand in for (`screens.replace`): the game's name, then
    /// `<mod id>:<name>`.
    replaces: Vec<(String, String)>,
    widgets: Widgets,
    /// The mod's payload channels.
    channels: Vec<String>,
    /// When `chat-send.send` last sent, within the last second.
    sent: VecDeque<Instant>,
    /// Capability `storage`'s folder; made on first `set`.
    data_dir: PathBuf,
    /// Bytes `storage::set` may still write before this hook call ends
    /// (reset per call in [`Mods::call`]): fuel bounds a mod's CPU time but
    /// not disk I/O time, so a loop of small-fuel, large-payload `set`s
    /// needs its own budget.
    storage_budget: u64,
    limits: Limits,
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
}

// The game loads mods on a thread of their own: compiling one can take seconds.
const _: fn() = || {
    fn send<T: Send>() {}
    send::<Mods>();
};

/// The `.hmod` files and mod folders in `dir`, by name.
pub fn find(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "hmod")
                || p.join(crate::manifest::MANIFEST).is_file()
        })
        .collect();
    found.sort();
    found
}

/// What the chat hooks made of a received line.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatVerdict {
    Keep,
    Hide,
    /// Shown instead (rich text).
    Replace(String),
}

impl Mods {
    /// Reads every mod at `paths` and starts those `enabled` says are on.
    /// A command that `reserved` (vanilla's commands) or an earlier mod
    /// already uses is refused.
    pub fn load(
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
    ) -> Mods {
        Self::load_cached(paths, enabled, reserved, None)
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
        let mut mods = Mods {
            cache: cache.map(Path::to_path_buf),
            ..Mods::default()
        };
        // The digests of every installed mod's code, on or off, kept in the cache.
        let mut digests = Vec::new();
        for path in paths {
            let modified = modified(path);
            let mut entry = Entry {
                path: path.clone(),
                manifest: None,
                status: Status::Off,
                cost: Duration::ZERO,
                run: None,
                modified,
                retry_full: false,
            };
            match Package::read(path) {
                Err(e) => entry.status = Status::Failed(e),
                Ok(package) => {
                    let on = enabled(&package.manifest.id);
                    entry.manifest = Some(package.manifest.clone());
                    if let (Some(_), Some(wasm)) = (cache, &package.wasm) {
                        digests.push(digest(wasm));
                    }
                    if let (Some(_), Some(_), Some(engine)) =
                        (cache, &package.js, js_engine::DIGEST)
                    {
                        digests.push(engine.to_owned());
                    }
                    if on {
                        // Content is read before the world loads; a mod whose
                        // content is refused stays off, with the reason.
                        let content = crate::Content::read(path, &package.manifest);
                        entry.status =
                            match content.and_then(|_| mods.start(path, &package, &reserved)) {
                                Ok(run) => {
                                    entry.run = run;
                                    Status::Running
                                }
                                Err(e) => Status::Failed(e),
                            };
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
        if m.side == Side::Server {
            return Err("server mods do not run yet".into());
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
        let taken: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .flat_map(|r| r.hooks.commands.iter().cloned())
            .collect();
        let snapshot = self.snapshot.clone();
        let snapshot_id = self.snapshot_id;
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
            config: None,
            replaces: Vec::new(),
            widgets: Widgets::default(),
            channels: Vec::new(),
            sent: VecDeque::new(),
            data_dir,
            storage_budget: STORAGE_WRITE_BUDGET,
            limits: Limits {
                inner: StoreLimitsBuilder::new()
                    .memory_size(MEMORY)
                    .instances(16)
                    .build(),
                refused: None,
            },
        };
        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(FUEL_INIT).map_err(|e| e.to_string())?;
        let (guest, mut hooks): (Guest, Hooks) = if for_starling {
            let exports = starling::StarlingMod::instantiate(&mut store, &component, linker)
                .map_err(|e| quick_detail(&m.id, &e))?;
            let hooks = exports
                .starling_mods_events()
                .call_init(&mut store)
                .map_err(|e| quick_detail(&m.id, &e))??;
            (Guest::Starling(exports), hooks.into())
        } else {
            let exports = v1::HorizonMod::instantiate(&mut store, &component, linker)
                .map_err(|e| quick_detail(&m.id, &e))?;
            let hooks = exports
                .horizon_mods_events()
                .call_init(&mut store)
                .map_err(|e| quick_detail(&m.id, &e))??;
            (Guest::Horizon(exports), hooks.into())
        };
        hooks.commands.retain(|c| {
            let free = !reserved(c) && !taken.contains(c);
            if !free {
                log::warn!("Horizon: mod {} cannot take /{c}: already a command", m.id);
            }
            free
        });
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
        self.out.append(&mut store.data_mut().out);
        Ok(Some(Running {
            store,
            guest,
            hooks,
            capabilities: m.capabilities.clone(),
            rendered: Vec::new(),
        }))
    }

    /// True when at least one mod runs: the game skips its mod work otherwise.
    pub fn is_running(&self) -> bool {
        self.entries.iter().any(|e| e.run.is_some())
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
        self.out.append(&mut run.store.data_mut().out);
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
            Guest::Horizon(_) => Ok(()),
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
                Guest::Horizon(_) => Ok(SendAction::Send),
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
                Guest::Horizon(_) => Ok(()),
            });
        }
    }
    pub fn screen_closed(&mut self, screen: &str) {
        for i in self.screen_mods(screen) {
            self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_screen_closed(s, screen),
                Guest::Horizon(_) => Ok(()),
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
                Guest::Horizon(_) => Ok(false),
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
                Guest::Horizon(_) => Ok(false),
            });
            if used == Some(true) {
                return true;
            }
        }
        false
    }
    /// Lines the mods add under `stack`'s tooltip (rich text).
    pub fn tooltip(&mut self, stack: &ItemStack) -> Vec<String> {
        let mut lines = Vec::new();
        for i in self.indices(|h| h.tooltips) {
            if let Some(more) = self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_tooltip(s, stack),
                Guest::Horizon(_) => Ok(Vec::new()),
            }) {
                lines.extend(more.into_iter().take(32).map(|l| clip_to(l, TEXT_CHARS)));
            }
        }
        lines
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
    /// Mod `id`'s keybind `key` pressed or released.
    pub fn key(&mut self, id: &str, key: &str, pressed: bool) {
        if let Some(i) = self.by_id(id) {
            self.call(i, FUEL_EVENT, |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_key(s, key, pressed),
                Guest::Horizon(_) => Ok(()),
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
            Guest::Horizon(_) => Ok(()),
        });
    }
    pub fn joined(&mut self, server: Option<&str>) {
        self.each(
            FUEL_EVENT,
            |h| h.connection,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_joined(s, server),
                Guest::Horizon(_) => Ok(()),
            },
        );
    }
    pub fn left(&mut self) {
        self.each(
            FUEL_EVENT,
            |h| h.connection,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_left(s),
                Guest::Horizon(_) => Ok(()),
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
                Guest::Horizon(_) => Ok(()),
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
                Guest::Horizon(_) => Ok(true),
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
                Guest::Horizon(_) => Ok(true),
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
                Guest::Horizon(_) => Ok(true),
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
                Guest::Horizon(_) => Ok(true),
            },
        );
        allowed.into_iter().all(|a| a)
    }
    pub fn block_broken(&mut self, pos: BlockPos, block: &str) {
        self.each(
            FUEL_EVENT,
            |h| h.interaction,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_block_broken(s, pos, block),
                Guest::Horizon(_) => Ok(()),
            },
        );
    }
    pub fn player_hurt(&mut self, amount: f32) {
        self.each(
            FUEL_EVENT,
            |h| h.damage,
            |g, s| match g {
                Guest::Starling(m) => m.starling_mods_events().call_player_hurt(s, amount),
                Guest::Horizon(_) => Ok(()),
            },
        );
    }
    /// Whether a running mod owns `/name`.
    pub fn has_command(&self, name: &str) -> bool {
        self.command_owner(name).is_some()
    }
    fn command_owner(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| {
            e.run
                .as_ref()
                .is_some_and(|r| r.hooks.commands.iter().any(|c| c == name))
        })
    }
    /// Runs `line` (without its slash) if a mod owns its command; false
    /// when none does.
    pub fn command(&mut self, line: &str) -> bool {
        let (name, args) = line.split_once(' ').unwrap_or((line, ""));
        let Some(i) = self.command_owner(name) else {
            return false;
        };
        let result = self.call(i, FUEL_EVENT, |g, s| match g {
            Guest::Starling(m) => m.starling_mods_events().call_command(s, name, args),
            Guest::Horizon(m) => m.horizon_mods_events().call_command(s, name, args),
        });
        match result {
            Some(Ok(Some(text))) => self.out.push(Output::Feedback { text, error: false }),
            Some(Err(text)) => self.out.push(Output::Feedback { text, error: true }),
            Some(Ok(None)) | None => {}
        }
        true
    }
    /// The commands running mods own, for suggestions.
    pub fn commands(&self) -> impl Iterator<Item = &str> {
        self.running()
            .flat_map(|r| r.hooks.commands.iter().map(String::as_str))
    }

    /// The running mods' HUD panels.
    pub fn panels(&self) -> impl Iterator<Item = &Panel> {
        self.running().filter_map(|r| r.store.data().panel.as_ref())
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
            let was_on = entry.status != Status::Off;
            if now == entry.modified || !was_on {
                continue;
            }
            any = true;
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
    let slots: Vec<[u8; 4]> = ops
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
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
/// the engine's `js-source` (and `js-draw` with `draw`), and a trap for every other import, which
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
        // js-draw goes with draw, js-keys with keys.
        let granting = match interface {
            "js-draw" => "draw",
            "js-keys" => "keys",
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
impl chat::Host for State {
    fn show(&mut self, text: String) {
        let shown = self
            .out
            .iter()
            .filter(|o| matches!(o, Output::Chat { .. }))
            .count();
        if shown < CHAT_PER_CALL {
            let name = self.name.clone();
            self.out.push(Output::Chat {
                name,
                text: clip_to(text, TEXT_CHARS),
            });
        }
    }
    fn show_overlay(&mut self, text: String) {
        self.out.retain(|o| !matches!(o, Output::Overlay { .. }));
        self.out.push(Output::Overlay {
            text: clip_to(text, TEXT_CHARS),
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
        shape: Shape,
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

/// API 0.1's interfaces, for mods built with a 0.1 SDK: the same state, older shapes.
mod legacy {
    use super::{Output, Panel, State, v1::horizon::mods as old};
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
