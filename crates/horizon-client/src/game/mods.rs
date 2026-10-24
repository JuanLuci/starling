//! Horizon mods in the game (`docs/mods.md`): loading the ones turned on, the snapshot they
//! read each tick, their hooks (tick, frame, chat, commands, connection, keys, payloads, damage)
//! and what they ask for (chat lines, sounds, payloads, screens). With no mod on,
//! `horizon_mods::Mods` holds no engine and every hook here returns after one check. Drawing
//! and screens are in `mod_ui.rs`.
use super::{Game, Screen};
use horizon_client::{
    command::{Dispatcher, vanilla},
    extras::Corner,
    extras_hud::{self, Corners},
    hud::HudBatch,
    items::ItemStack,
    mobs::finalize,
    mod_list::{self, Installed},
    text::{self, Text},
};
use horizon_mods::{
    BlockInfo, Capability, ChatVerdict, Clock, Controls, CreativeCatalog, Effect, Entity,
    EntityInfo, GameMode, Gui, ItemCatalog, ItemInfo, Measure, Mods, Output, Player, Snapshot,
    Status, Target, WorldInfo,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};
use winit::keyboard::KeyCode;

#[derive(Default)]
pub(super) struct ModsState {
    pub(super) mods: Mods,
    /// `--dev-mod PATH`: always on, reloaded when rebuilt.
    pub(super) dev: Vec<PathBuf>,
    /// When dev mods were last checked for a rebuild.
    checked: Option<Instant>,
    /// When the mods stopped on quick code were last looked at ([`Mods::restart_on_full_code`]).
    retried: Option<Instant>,
    /// The chat's command tree while a running mod owns commands.
    tree: Option<Dispatcher>,
    /// The mod screen open now, `<mod id>:<name>`, over everything else.
    pub(super) screen: Option<String>,
    /// That screen stands in for the creative screen (`screens.replace`).
    pub(super) replacing: bool,
    /// The screen the mods were last told of (`screen-opened` / `screen-closed`).
    pub(super) shown: Option<String>,
    /// The mouse button held on a screen, for drag events.
    pub(super) held: Option<u8>,
    /// The mod screen asked for the on-screen keyboard (`screens.keyboard`).
    pub(super) keyboard: bool,
    /// Where on the mod screen a finger drags instead of scrolling (`mobile.drag-areas`), in
    /// GUI pixels.
    pub(super) drag_areas: Vec<[f32; 4]>,
    /// The world the mods were last told of: None out of a world, Some(server address).
    joined: Option<Option<String>>,
    dimension: Option<String>,
    /// The player's health last tick, for `player-hurt`.
    health: Option<f32>,
    /// The mods' keybinds held now, as `<mod id>:<keybind id>`.
    pub(super) keys_down: Vec<String>,
    /// A mod's button on the game's screen held down by the mouse: mod id, button id.
    pub(super) pressed_button: Option<(String, String)>,
    /// The game's controls for `keys.is-control-down` and `keys.presses`, as last framed.
    controls: Controls,
    /// Controls whose press since the last frame was counted as it came ([`ModsState::press`]).
    counted: [bool; 9],
    /// The mods holding sneak (slot 0) or sprint (slot 1) for the player (`input.hold`), by id.
    holds: Vec<(String, usize)>,
    /// Every block state's name, for `world-read` (made once).
    states: Option<Arc<Vec<String>>>,
    /// What `level.get-block-info` says of every block state, made once for the language its
    /// names are in.
    block_facts: Option<(String, Arc<Vec<BlockFact>>)>,
    /// The blocks needing a tool that the held item (its id here) harvests.
    harvest: Option<(String, Arc<HashSet<Arc<str>>>)>,
    biomes: Option<Arc<Vec<String>>>,
    items: Option<Arc<dyn ItemCatalog>>,
    /// The creative tabs for `creative`, made once for the language their names are in.
    pub(super) creative: Option<(String, Arc<dyn CreativeCatalog>)>,
    measure: Option<Measure>,
    /// Mods loading on their own thread ([`Game::start_mods`]), and since when.
    loading: Option<(Receiver<Mods>, Instant)>,
    /// The mods folder as last looked at (each mod with its size and time), and when: a mod
    /// copied in, replaced or removed (`cargo horizon-mod push`, a file manager) restarts them.
    folder: Option<Vec<FolderMod>>,
    /// A change seen once: the mods restart when the next look finds the folder the same
    /// (replacing a file on Android's shared storage shows it missing for a moment).
    changed: Option<Vec<FolderMod>>,
    watched: Option<Instant>,
    /// The load running restarts the mods after the folder changed: say so when it ends.
    reloading: bool,
    /// The "Starting mods" note is up.
    announced: bool,
}
impl ModsState {
    /// The mods are starting (on their own thread): a world waits for them to open.
    pub(super) fn starting(&self) -> bool {
        self.loading.is_some()
    }
    /// A press of `control` (one of [`horizon_mods::CONTROLS`]) counted as it comes: a click can
    /// start and end between two frames.
    pub(super) fn press(&mut self, control: &str) {
        if let Some(i) = horizon_mods::CONTROLS.iter().position(|c| *c == control) {
            self.controls[i].1 += 1;
            self.counted[i] = true;
        }
    }
    /// A mod's screen is open in the creative screen's place.
    pub(super) fn standing_in(&self) -> bool {
        self.replacing && self.screen.is_some()
    }
    /// A mod holds sneak (slot 0) or sprint (slot 1): the player's toggle state, as it were.
    pub(super) fn holding(&self, slot: usize) -> bool {
        self.holds.iter().any(|(_, s)| *s == slot)
    }
    /// The chat's command tree: vanilla's, plus the running mods' commands.
    pub(super) fn dispatcher(&self) -> &Dispatcher {
        self.tree
            .as_ref()
            .unwrap_or_else(|| vanilla::game_dispatcher())
    }
}
/// A mod in the folder as the watcher saw it: path, size, file id and change time. On Android's
/// shared storage a replaced file's modification time goes back to the old file's a moment
/// later (and the change time's fraction of a second with it), so Unix uses the inode and the
/// change time's seconds, which a replacement sets.
type FolderMod = (PathBuf, u64, u64, i128);

/// How often dev mods are checked for a rebuild.
const RELOAD_CHECK: Duration = Duration::from_secs(1);
/// How long mods may take to start before a note says so (a large one compiling the first time).
const SLOW_START: Duration = Duration::from_millis(700);
/// How far around the player `world-read` mods see blocks.
const LEVEL_CHUNKS: i32 = 2;

/// A vanilla command name: mods may not take it.
fn vanilla_command(name: &str) -> bool {
    let d = vanilla::game_dispatcher();
    d.nodes[0]
        .children
        .iter()
        .any(|&c| d.nodes[c].name() == name)
}

/// The loaded chunks around the player, for `world-read` mods.
struct LevelView {
    chunks: Vec<((i32, i32), Arc<horizon_client::chunk::Chunk>)>,
    states: Arc<Vec<String>>,
    biomes: Arc<Vec<String>>,
    facts: Arc<Vec<BlockFact>>,
    harvest: Arc<HashSet<Arc<str>>>,
}
/// A block state as `level.get-block-info` tells it, but for the held item.
struct BlockFact {
    /// `minecraft:stone`.
    block: Arc<str>,
    name: String,
    item: Option<String>,
    hardness: f32,
    tool: Option<&'static str>,
    needs_tool: bool,
}
impl LevelView {
    fn chunk(&self, pos: [i32; 3]) -> Option<&horizon_client::chunk::Chunk> {
        let key = (pos[0] >> 4, pos[2] >> 4);
        self.chunks
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, c)| &**c)
    }
}
impl horizon_mods::LevelView for LevelView {
    fn block(&self, pos: [i32; 3]) -> Option<String> {
        let state = self.chunk(pos)?.block(pos[0] & 15, pos[1], pos[2] & 15);
        self.states.get(state.0 as usize).cloned()
    }
    fn biome(&self, pos: [i32; 3]) -> Option<String> {
        let biome = self.chunk(pos)?.biome(pos[0], pos[1], pos[2]);
        self.biomes.get(biome as usize).cloned()
    }
    fn light(&self, pos: [i32; 3]) -> Option<(u8, u8)> {
        let light = self.chunk(pos)?.light(pos[0] & 15, pos[1], pos[2] & 15);
        Some((light & 15, light >> 4))
    }
    fn block_info(&self, pos: [i32; 3]) -> Option<BlockInfo> {
        let state = self.chunk(pos)?.block(pos[0] & 15, pos[1], pos[2] & 15).0 as usize;
        let fact = self.facts.get(state)?;
        Some(BlockInfo {
            block: self.states.get(state)?.clone(),
            name: fact.name.clone(),
            item: fact.item.clone(),
            hardness: fact.hardness,
            tool: fact.tool.map(str::to_owned),
            needs_tool: fact.needs_tool,
            harvestable: !fact.needs_tool || self.harvest.contains(&fact.block),
        })
    }
}

/// The game's items, for `inventory.get-item-info`.
struct Catalog(Vec<(String, ItemInfo)>);
impl ItemCatalog for Catalog {
    fn info(&self, id: &str) -> Option<ItemInfo> {
        let id = if id.contains(':') {
            id.to_owned()
        } else {
            format!("minecraft:{id}")
        };
        self.0
            .binary_search_by(|(k, _)| k.as_str().cmp(&id))
            .ok()
            .map(|i| self.0[i].1.clone())
    }
}

/// `root` with the content mods that are on layered over it
/// (docs/modpacks.md): their blocks, items, data and assets. Read once at
/// start, before the resources load; a mod whose content is refused is left
/// out here and its row on the Mods page says why.
pub(super) fn content_root(
    root: &std::path::Path,
    enabled: &std::collections::BTreeSet<String>,
    dev: &[std::path::PathBuf],
) -> std::path::PathBuf {
    use horizon_client::mod_content::{self, ContentMod};
    use horizon_mods::{Content, Package};
    let here = std::env::current_dir().unwrap_or_default();
    let mut paths = horizon_mods::find(&here.join(mod_list::DIR));
    paths.retain(|p| !dev.contains(p));
    paths.extend(dev.iter().cloned());
    let mut mods: Vec<ContentMod> = Vec::new();
    for path in &paths {
        let Ok(package) = Package::read(path) else {
            continue;
        };
        let m = &package.manifest;
        let on = enabled.contains(&m.id) || dev.contains(path);
        if !on || mods.iter().any(|c| c.id == m.id) {
            continue;
        }
        // A mod without registries brings its own GUI sprites at most (its buttons' icons).
        let content = Content::read(path, m).and_then(|content| {
            // The first of two mods owning one namespace stays on.
            let own = (m.id.as_str(), content.namespaces.as_slice());
            match mods
                .iter()
                .find_map(|c| horizon_mods::content::clash((&c.id, &c.content.namespaces), own))
            {
                Some(e) => Err(e),
                None => Ok(content),
            }
        });
        match content {
            Ok(content) if content.is_empty() => {}
            Ok(content) => mods.push(ContentMod {
                id: m.id.clone(),
                content,
            }),
            Err(e) => log::warn!("Horizon: mod {} content not loaded: {e}", m.id),
        }
    }
    match mod_content::resource_root(root, &mods, &horizon_client::logging::cache_dir()) {
        Ok(out) => {
            if out != root {
                log::info!(
                    "Horizon: content from {} mod(s) in {}",
                    mods.len(),
                    out.display()
                );
            }
            out
        }
        Err(e) => {
            log::warn!("Horizon: content mods not loaded: {e}");
            root.to_owned()
        }
    }
}

/// A UUID as text, with dashes.
fn uuid_text(id: u128) -> String {
    let h = format!("{id:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Rich text from a mod: a JSON text component when it starts with `{` or `[`, else plain text
/// with legacy `§` codes.
pub(super) fn rich_text(text: &str) -> Text {
    let t = text.trim_start();
    if (t.starts_with('{') || t.starts_with('['))
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(t)
    {
        return horizon_client::scoreboard::comp_text(&v);
    }
    if text.contains('§') {
        Text::legacy(text)
    } else {
        Text::literal(text)
    }
}

impl Game<'_> {
    /// Reads `horizon-mods/` and the dev mods, and starts the ones turned on, on a thread of
    /// their own: compiling a mod takes seconds the first time (its code is kept in the cache
    /// after), which on the render thread would freeze the game and get it closed on Android.
    /// The mods running until then keep running; [`Game::poll_mods`] puts the new ones in place.
    pub(super) fn start_mods(&mut self) {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        let dev = self.mods.dev.clone();
        let enabled = self.settings.mods.enabled.clone();
        let cache = horizon_client::logging::cache_dir().join("compiled-mods");
        let (send, receive) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("mods".into())
            .spawn(move || {
                let mut paths = horizon_mods::find(&dir);
                paths.retain(|p| !dev.contains(p));
                paths.extend(dev.iter().cloned());
                // A dev mod is on whatever the list says.
                let dev_ids: Vec<String> = dev
                    .iter()
                    .filter_map(|p| horizon_mods::Package::read(p).ok())
                    .map(|p| p.manifest.id)
                    .collect();
                let on = |id: &str| enabled.contains(id) || dev_ids.iter().any(|d| d == id);
                let _ = send.send(Mods::load_cached(&paths, on, vanilla_command, Some(&cache)));
            });
        match spawned {
            // A load already running is dropped: this one reads the newer list.
            Ok(_) => self.mods.loading = Some((receive, Instant::now())),
            Err(e) => log::error!("Horizon: could not start mods: {e}"),
        }
    }
    /// The mods folder's mods, each with its size and time (a folder mod's by its component).
    fn mods_folder() -> Vec<FolderMod> {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        horizon_mods::find(&dir)
            .into_iter()
            .map(|path| {
                let file = if path.is_dir() {
                    path.join(horizon_mods::manifest::WASM)
                } else {
                    path.clone()
                };
                let Ok(meta) = std::fs::metadata(&file) else {
                    return (path, 0, 0, 0);
                };
                #[cfg(unix)]
                let (id, time) = {
                    use std::os::unix::fs::MetadataExt;
                    // Whole seconds: the fraction too changes back a moment after a copy.
                    (meta.ino(), i128::from(meta.ctime()))
                };
                #[cfg(not(unix))]
                let (id, time) = (
                    0,
                    meta.modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos() as i128),
                );
                (path, meta.len(), id, time)
            })
            .collect()
    }
    /// Once a second, in menus and in a world: the mods start again when the folder changed.
    fn watch_mods_folder(&mut self) {
        if self
            .mods
            .watched
            .is_some_and(|t| t.elapsed() < RELOAD_CHECK)
        {
            return;
        }
        self.mods.watched = Some(Instant::now());
        if self.mods.loading.is_some() {
            return;
        }
        let now = Self::mods_folder();
        match &self.mods.folder {
            Some(was) if *was != now => {
                if self.mods.changed.as_ref() != Some(&now) {
                    self.mods.changed = Some(now);
                    return;
                }
                log::info!("Horizon: the mods folder changed; starting the mods again");
                self.mods.changed = None;
                self.mods.folder = Some(now);
                self.mods.reloading = true;
                self.start_mods();
            }
            Some(_) => self.mods.changed = None,
            None => self.mods.folder = Some(now),
        }
    }
    /// Each frame: the mods a load finished, in place of the ones running.
    pub(super) fn poll_mods(&mut self) {
        self.watch_mods_folder();
        let Some((receive, started)) = &self.mods.loading else {
            return;
        };
        let mods = match receive.try_recv() {
            Ok(mods) => mods,
            Err(TryRecvError::Empty) => {
                if !self.mods.announced && started.elapsed() > SLOW_START {
                    self.mods.announced = true;
                    self.imports.working("Starting mods...".into());
                }
                return;
            }
            Err(TryRecvError::Disconnected) => {
                log::error!("Horizon: the mods' thread stopped");
                self.mods.loading = None;
                return;
            }
        };
        log::info!(
            "Horizon: mods started in {:?}",
            self.mods
                .loading
                .take()
                .map_or(Duration::ZERO, |(_, t)| t.elapsed())
        );
        if std::mem::take(&mut self.mods.reloading) {
            self.imports.say("Mods reloaded".into());
            self.mods.announced = false;
        } else if std::mem::take(&mut self.mods.announced) {
            self.imports.say("Mods started".into());
        }
        self.mods.mods = mods;
        self.mods.screen = None;
        self.mods.joined = None;
        self.release_mod_holds(|_| false);
        self.list_mods();
        self.sync_mod_commands();
        self.register_mod_channels();
        // What they read until the next tick, now: a game paused while they started has none.
        self.refresh_mod_snapshot();
        self.show_mod_output();
    }
    /// The Mods page's rows, from what loaded.
    fn list_mods(&mut self) {
        let dev = &self.mods.dev;
        self.settings.mods.installed = self
            .mods
            .mods
            .entries()
            .iter()
            .map(|e| {
                let file = e.path.file_name().unwrap_or_default().to_string_lossy();
                let problem = match &e.status {
                    Status::Failed(reason) => Some(format!("Stopped: {reason}")),
                    _ => None,
                };
                let Some(m) = &e.manifest else {
                    return Installed {
                        id: String::new(),
                        title: file.into_owned(),
                        description: String::new(),
                        summary: String::new(),
                        config: None,
                        problem,
                        dev: false,
                    };
                };
                // What it may do first: the row cuts long descriptions.
                let may: Vec<&str> = m.capabilities.iter().map(|c| c.describe()).collect();
                let may = if may.is_empty() {
                    "Asks for nothing.".to_owned()
                } else {
                    format!("May {}.", may.join(", "))
                };
                Installed {
                    id: m.id.clone(),
                    title: format!("{} {}", m.name, m.version),
                    description: format!("{may} {}", m.description).trim().to_owned(),
                    summary: m.description.clone(),
                    config: self.mods.mods.config_screen(&m.id),
                    problem,
                    dev: dev.contains(&e.path),
                }
            })
            .collect();
    }
    /// The chat's tree after the running mods changed.
    fn sync_mod_commands(&mut self) {
        let mut names = self.mods.mods.commands().peekable();
        self.mods.tree = names
            .peek()
            .is_some()
            .then(|| vanilla::game_dispatcher().with_mod_commands(names));
        self.merge_remote_commands();
    }
    /// On a server, the chat's tree: the server's, with the running mods' commands it does not
    /// have (the server's own win).
    pub(super) fn merge_remote_commands(&mut self) {
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        r.commands = r.server_commands.as_ref().map(|tree| {
            let mut names = self
                .mods
                .mods
                .commands()
                .filter(|name| tree.find(name).is_none())
                .peekable();
            match names.peek() {
                Some(_) => tree.with_mod_commands(names),
                None => tree.clone(),
            }
        });
    }
    /// `/command` typed in chat on a server: true when a mod owns it and the server has no
    /// command of that name.
    pub(super) fn remote_mod_command(&mut self, command: &str) -> bool {
        let name = command.split_whitespace().next().unwrap_or_default();
        let server_has = self
            .remote
            .as_ref()
            .and_then(|r| r.server_commands.as_ref())
            .is_some_and(|tree| tree.find(name).is_some());
        !server_has && self.mod_command(command)
    }
    /// On a server, tells it the mods' payload channels (`minecraft:register`, NUL-separated),
    /// so its plugins send on them.
    pub(super) fn register_mod_channels(&mut self) {
        let channels = self.mods.mods.channels();
        if channels.is_empty() || self.remote.is_none() {
            return;
        }
        self.send(horizon_protocol::packets::play::Serverbound::CustomPayload(
            horizon_protocol::packets::configuration::CustomPayload {
                channel: "minecraft:register".into(),
                data: channels.join("\0").into_bytes(),
            },
        ));
    }
    /// After the options changed: restart the mods if the list of the ones
    /// turned on did.
    pub(super) fn sync_mods(&mut self) {
        let running: Vec<&str> = self
            .mods
            .mods
            .entries()
            .iter()
            .filter(|e| e.status != Status::Off)
            .filter_map(|e| Some(e.manifest.as_ref()?.id.as_str()))
            .collect();
        let wanted: Vec<&str> = self
            .settings
            .mods
            .installed
            .iter()
            .enumerate()
            .filter(|&(i, _)| self.settings.mods.is_on(i))
            .map(|(_, m)| m.id.as_str())
            .collect();
        if running != wanted {
            self.start_mods();
        }
    }
    /// A game stack as mods see it.
    pub(super) fn mod_stack(&self, s: &ItemStack) -> horizon_mods::ItemStack {
        let components = horizon_client::item_data::encode_components(s)
            .map(|c| horizon_client::item_data::canon_tag(&c))
            .unwrap_or_default();
        horizon_mods::ItemStack {
            id: s.item.to_string(),
            count: s.count,
            damage: s.damage,
            max_damage: self
                .res
                .items()
                .items
                .get(&*s.item)
                .map_or(0, |i| i.max_damage),
            name: self.stack_name(s).plain(),
            components,
        }
    }
    /// A mod's stack as a game stack (its saved components read back).
    pub(super) fn game_stack(&self, s: &horizon_mods::ItemStack) -> Option<ItemStack> {
        let components = if s.components.is_empty() {
            String::new()
        } else {
            format!(",components:{}", s.components)
        };
        let snbt = format!(
            "{{id:\"{}\",count:{}{components}}}",
            s.id.replace('"', ""),
            s.count.max(1)
        );
        horizon_client::item_data::parse_stack(&snbt)
            .ok()
            .flatten()
            .or_else(|| {
                self.res
                    .items()
                    .items
                    .contains_key(s.id.as_str())
                    .then(|| ItemStack::new(&s.id, s.count.max(1)))
            })
    }
    /// What mods read, taken now (between ticks), once the world's resources have loaded: mods
    /// cached from an earlier start run before they do, on the loading menus.
    pub(super) fn refresh_mod_snapshot(&mut self) {
        if self.mods.mods.is_running() && self.res.rest_ready() {
            let snapshot = self.mod_snapshot();
            self.mods.mods.set_snapshot(snapshot);
        }
    }
    /// What mods read this tick: the parts their capabilities reach.
    fn mod_snapshot(&mut self) -> Snapshot {
        let mods = &self.mods.mods;
        let (inventory_read, world_read, game_read) = (
            mods.uses(Capability::InventoryRead),
            mods.uses(Capability::WorldRead),
            mods.uses(Capability::GameRead) || mods.uses(Capability::Net),
        );
        let remote = self.remote.as_ref();
        let (name, uuid) = match remote {
            Some(r) => (r.session.name.clone(), uuid_text(r.profile)),
            None => (
                super::chat::PLAYER_NAME.to_owned(),
                self.level
                    .player_uuid
                    .map(|u| {
                        uuid_text(
                            u.iter()
                                .fold(0u128, |acc, &w| acc << 32 | u128::from(w as u32)),
                        )
                    })
                    .unwrap_or_default(),
            ),
        };
        // LivingEntity.getMaxHealth / getArmorValue (attributes read through the live map).
        let (max_health, armor) = self.player.as_mut().map_or((20.0, 0.0), |p| {
            (
                p.attribute("minecraft:max_health").unwrap_or(20.0),
                p.attribute("minecraft:armor")
                    .or_else(|_| p.attribute("armor"))
                    .unwrap_or(0.0),
            )
        });
        let player = self.player.as_ref().map(|p| Player {
            x: p.position.x,
            y: p.position.y,
            z: p.position.z,
            velocity_x: p.velocity.x,
            velocity_y: p.velocity.y,
            velocity_z: p.velocity.z,
            yaw: self.yaw,
            pitch: self.pitch,
            health: p.health.health(),
            max_health: max_health as f32,
            absorption: p.health.absorption(),
            food: p.health.food.food.max(0) as u32,
            saturation: p.health.food.saturation,
            exhaustion: if remote.is_some() {
                0.0
            } else {
                p.health.food.exhaustion
            },
            air: p.air,
            max_air: 300,
            armor: armor.max(0.0) as u32,
            experience_level: self.experience.level.max(0) as u32,
            experience_progress: self.experience.progress,
            selected_slot: self.selected as u32,
            game_mode: match self.mode_id() {
                1 => GameMode::Creative,
                2 => GameMode::Adventure,
                3 => GameMode::Spectator,
                _ => GameMode::Survival,
            },
            on_ground: p.ground.on_ground,
            sneaking: p.shift_key_down,
            sprinting: p.sprinting,
            swimming: p.swimming,
            flying: p.flying,
            gliding: p.fall_flying,
            dimension: self.dim.id().to_owned(),
            name,
            uuid,
        });
        let in_world = player.is_some();
        let clock = in_world.then(|| Clock {
            game_time: self.world.game_time.max(0) as u64,
            day_time: self.world.day_time.max(0) as u64,
        });
        let effects = if game_read && in_world {
            self.potions
                .effects
                .list
                .iter()
                .map(|e| Effect {
                    id: format!("minecraft:{}", e.id),
                    amplifier: e.amplifier.max(0) as u32,
                    duration: e.duration,
                    ambient: e.ambient,
                    visible: e.visible,
                })
                .collect()
        } else {
            Vec::new()
        };
        let weather = &self.world.weather;
        let world = in_world.then(|| WorldInfo {
            multiplayer: remote.is_some() && self.integrated.is_none(),
            server: remote
                .filter(|_| self.integrated.is_none())
                .map(|r| r.session.address.clone()),
            difficulty: u32::from(self.ui.difficulty),
            hardcore: self.level.hardcore,
            raining: weather.rain_level.1 > 0.2,
            thundering: weather.thunder_level.1 > 0.9,
        });
        let target = (game_read && in_world).then(|| self.mod_target()).flatten();
        let mining = self
            .mining
            .as_ref()
            .filter(|_| game_read && in_world)
            .map(|m| horizon_mods::Mining {
                pos: horizon_mods::BlockPos {
                    x: m.pos[0],
                    y: m.pos[1],
                    z: m.pos[2],
                },
                progress: m.progress.clamp(0.0, 1.0),
            });
        let (gw, gh) = self.gui_size();
        let gui = Gui {
            width: gw as u32,
            height: gh as u32,
            scale: self.renderer.as_ref().map_or(1.0, |r| {
                HudBatch::new([r.config.width, r.config.height]).scale
            }),
        };
        let mut snapshot = Snapshot {
            player,
            effects,
            clock,
            world,
            target,
            mining,
            screen: self.mod_screen_name(),
            gui: Some(gui),
            fps: self.fps.round().max(0.0) as u32,
            keys_down: self.mods.keys_down.clone(),
            touch: self.has_touch(),
            measure: Some(self.mod_measure()),
            ..Snapshot::default()
        };
        if inventory_read {
            let mut slots: Vec<Option<horizon_mods::ItemStack>> = self.inventory[..36]
                .iter()
                .map(|s| s.as_ref().map(|s| self.mod_stack(s)))
                .collect();
            // Feet to head, as vanilla's Inventory and the API say (the game keeps head first).
            slots.extend(
                self.armor
                    .iter()
                    .rev()
                    .map(|s| s.as_ref().map(|s| self.mod_stack(s))),
            );
            slots.push(self.offhand.as_ref().map(|s| self.mod_stack(s)));
            snapshot.inventory = slots;
            snapshot.cursor = self.carried.as_ref().map(|s| self.mod_stack(s));
            snapshot.hovered = self.hovered_stack().map(|s| self.mod_stack(&s));
            snapshot.container = self.mod_container();
        }
        let creative = self.mods.mods.uses(Capability::Creative);
        // `creative` checks the stacks it is given against the items.
        if inventory_read || creative {
            snapshot.items = Some(self.mod_items());
        }
        // The tabs answer `creative` and `inventory.get-item-order`.
        if (creative || inventory_read) && self.res.rest_ready() {
            snapshot.creative = Some(self.mod_creative_tabs());
        }
        if self.mods.mods.uses(Capability::InventoryClick) {
            snapshot.menu = self.mod_menu();
        }
        if world_read && in_world {
            snapshot.level = Some(self.mod_level());
            (snapshot.entities, snapshot.entity_info) = self.mod_entities().into_iter().unzip();
        }
        snapshot
    }
    /// What the crosshair is on, for mods.
    fn mod_target(&self) -> Option<Target> {
        use horizon_mods::{BlockHit, BlockPos, Direction, EntityHit};
        let block = self.pick().map(|hit| {
            let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
            let face = match hit.face {
                [0, -1, 0] => Direction::Down,
                [0, 1, 0] => Direction::Up,
                [0, 0, -1] => Direction::North,
                [0, 0, 1] => Direction::South,
                [-1, 0, 0] => Direction::West,
                _ => Direction::East,
            };
            let eye = self.eye_position().unwrap_or(hit.point);
            let d = ((hit.point[0] - eye[0]).powi(2)
                + (hit.point[1] - eye[1]).powi(2)
                + (hit.point[2] - eye[2]).powi(2))
            .sqrt();
            (
                BlockHit {
                    pos: BlockPos {
                        x: hit.pos[0],
                        y: hit.pos[1],
                        z: hit.pos[2],
                    },
                    face,
                    block: self.state_string(state),
                },
                d,
            )
        });
        let entity = self.mob_hit().map(|(i, d)| {
            let m = &self.mobs[i];
            (
                EntityHit {
                    id: m.id as i32,
                    kind: m.kind.id.to_string(),
                    x: m.position.x,
                    y: m.position.y,
                    z: m.position.z,
                },
                d,
            )
        });
        match (block, entity) {
            (Some((_, bd)), Some((e, ed))) if ed <= bd => Some(Target::Entity(e)),
            (None, Some((e, _))) => Some(Target::Entity(e)),
            (Some((b, _)), _) => Some(Target::Block(b)),
            (None, None) => None,
        }
    }
    /// The open container's own slots, for mods.
    fn mod_container(&mut self) -> Option<horizon_mods::Container> {
        if self.screen != Screen::Inventory {
            return None;
        }
        let (kind, title) = match self.remote.as_ref().and_then(|r| r.menu.clone()) {
            // The server names it in full already (`minecraft:generic_9x3`).
            Some((_, menu)) => (
                menu,
                self.remote
                    .as_ref()
                    .and_then(|r| r.menu_title.clone())
                    .unwrap_or_default(),
            ),
            None if self.container.is_some() => ("minecraft:generic_9x3".to_owned(), String::new()),
            None => return None,
        };
        let own: Vec<usize> = self
            .slots()
            .into_iter()
            .filter(|i| (300..500).contains(i))
            .collect();
        let mut slots = Vec::with_capacity(own.len());
        for i in own {
            let stack = self.slot_view(i);
            slots.push(stack.map(|s| self.mod_stack(&s)));
        }
        Some(horizon_mods::Container { kind, title, slots })
    }
    /// The open menu by vanilla's slot ids (`slots.get-menu`): the screen's, or the player's
    /// inventory with none open; None on a screen without slots.
    fn mod_menu(&mut self) -> Option<horizon_mods::Menu> {
        let slotted = matches!(self.screen, Screen::Inventory | Screen::Creative)
            || (self.screen == Screen::Playing && self.player.is_some());
        if !slotted || self.title.is_some() {
            return None;
        }
        let mut read = None;
        // Read, nothing kept.
        self.menu_session(|m, _, _| {
            read = Some((m.kind(), super::remote::menu_items(m)));
            Some(false)
        });
        let (kind, items) = read?;
        let kind = match self.remote.as_ref().and_then(|r| r.menu.as_ref()) {
            Some((_, name)) if self.screen == Screen::Inventory => name.clone(),
            _ => kind.registry_name(),
        };
        Some(horizon_mods::Menu {
            kind,
            slots: items
                .iter()
                .map(|s| s.as_ref().map(|s| self.mod_stack(s)))
                .collect(),
            carried: self.carried.as_ref().map(|s| self.mod_stack(s)),
        })
    }
    /// `slots.click`: a click on the open menu's slot `slot` (vanilla's id), as the mouse's: on
    /// a server predicted and sent, here done on the menu.
    pub(super) fn mod_slot_click(&mut self, slot: i32, button: u8, kind: horizon_mods::ClickKind) {
        use horizon_client::menu::Input;
        use horizon_mods::ClickKind;
        let input = match kind {
            ClickKind::Pickup => Input::Pickup,
            ClickKind::QuickMove => Input::QuickMove,
            ClickKind::Swap => Input::Swap,
            ClickKind::Clone => Input::Clone,
            ClickKind::Throw => Input::Throw,
            ClickKind::PickupAll => Input::PickupAll,
        };
        let button = i32::from(button);
        // The creative screen's clicks are this client's, sent as set slots (as vanilla's).
        if self.remote.is_some() && !self.creative_menu_open() {
            let Ok(slot) = i16::try_from(slot) else {
                return;
            };
            self.remote_menu_clicks(&[(slot, button, input)]);
            return;
        }
        let done = self.menu_session(|m, env, _| Some(m.click(env, slot, button, input).is_ok()));
        if done && let Some(key) = self.container {
            self.container_changed(key);
        }
    }
    /// `slots.drag`: the mouse's drag across slots (AbstractContainerScreen's QUICK_CRAFT
    /// clicks: its start outside the window, each slot, its end).
    pub(super) fn mod_slot_drag(&mut self, slots: &[i32], button: u8) {
        use horizon_client::menu::Input;
        // AbstractContainerMenu.getQuickcraftMask: the phase, and the drag's kind above it.
        let mask = |phase: i32| phase | (i32::from(button) << 2);
        let clicks: Vec<(i32, i32)> = std::iter::once((-999, mask(0)))
            .chain(slots.iter().map(|&s| (s, mask(1))))
            .chain(std::iter::once((-999, mask(2))))
            .collect();
        if self.remote.is_some() && !self.creative_menu_open() {
            let clicks: Vec<(i16, i32, Input)> = clicks
                .iter()
                .filter_map(|&(s, b)| Some((i16::try_from(s).ok()?, b, Input::QuickCraft)))
                .collect();
            self.remote_menu_clicks(&clicks);
            return;
        }
        let done = self.menu_session(|m, env, _| {
            let mut done = false;
            for &(slot, b) in &clicks {
                done |= m.click(env, slot, b, Input::QuickCraft).is_ok();
            }
            Some(done)
        });
        if done && let Some(key) = self.container {
            self.container_changed(key);
        }
    }
    /// The stack under the cursor in a screen.
    fn hovered_stack(&mut self) -> Option<ItemStack> {
        if !matches!(self.screen, Screen::Inventory | Screen::Creative)
            || self.mods.screen.is_some()
        {
            return None;
        }
        let renderer = self.renderer.as_ref()?;
        let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
        let slot = self.slot_at(&batch, self.cursor.0, self.cursor.1)?;
        self.slot_view(slot)
    }
    fn mod_items(&mut self) -> Arc<dyn ItemCatalog> {
        if let Some(items) = &self.mods.items {
            return items.clone();
        }
        let mut list: Vec<(String, ItemInfo)> = self
            .res
            .items()
            .items
            .iter()
            .map(|(id, info)| {
                let food = info.food.map(|(nutrition, saturation)| horizon_mods::Food {
                    nutrition,
                    saturation,
                    can_always_eat: info.can_always_eat,
                });
                (
                    id.to_string(),
                    ItemInfo {
                        id: id.to_string(),
                        name: self.item_display_name(id).plain(),
                        max_stack_size: info.max_stack.max(1),
                        max_damage: info.max_damage,
                        food,
                    },
                )
            })
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        let items: Arc<dyn ItemCatalog> = Arc::new(Catalog(list));
        self.mods.items = Some(items.clone());
        items
    }
    fn mod_measure(&mut self) -> Measure {
        if let Some(m) = &self.mods.measure {
            return m.clone();
        }
        // The font is the render thread's: mods measure with a copy of its advances
        // (Basic Multilingual Plane), bold adding a pixel as Font.width does.
        let font = &self.gui.atlas.font;
        let advances: Vec<f32> = (0..0x3000u32)
            .map(|c| char::from_u32(c).map_or(0.0, |ch| font.width(ch.encode_utf8(&mut [0; 4]))))
            .collect();
        let measure: Measure = Arc::new(move |s: &str| {
            rich_text(s)
                .resolve_legacy()
                .chars()
                .map(|(ch, style)| {
                    let advance = advances.get(ch as usize).copied().unwrap_or(6.0);
                    advance + if style.is_bold() { 1.0 } else { 0.0 }
                })
                .sum()
        });
        self.mods.measure = Some(measure.clone());
        measure
    }
    fn mod_level(&mut self) -> Arc<dyn horizon_mods::LevelView> {
        let states = match &self.mods.states {
            Some(s) => s.clone(),
            None => {
                let registry = &self.pack.assets.registry;
                let names: Vec<String> = (0..registry.len())
                    .map(|i| self.state_string(horizon_world::registry::BlockStateId(i as u32)))
                    .collect();
                let names = Arc::new(names);
                self.mods.states = Some(names.clone());
                names
            }
        };
        let biomes = self
            .mods
            .biomes
            .get_or_insert_with(|| {
                Arc::new(
                    self.res
                        .biomes()
                        .list
                        .iter()
                        .map(|b| b.name.clone())
                        .collect(),
                )
            })
            .clone();
        let (cx, cz) = self.player.as_ref().map_or((0, 0), |p| {
            (
                (p.position.x.floor() as i32) >> 4,
                (p.position.z.floor() as i32) >> 4,
            )
        });
        let mut chunks = Vec::new();
        for x in cx - LEVEL_CHUNKS..=cx + LEVEL_CHUNKS {
            for z in cz - LEVEL_CHUNKS..=cz + LEVEL_CHUNKS {
                if let Some(c) = self.world.chunks.get(&(x, z)) {
                    chunks.push(((x, z), c.clone()));
                }
            }
        }
        Arc::new(LevelView {
            chunks,
            states,
            biomes,
            facts: self.mod_block_facts(),
            harvest: self.mod_harvest(),
        })
    }
    /// Every block state's name, item, hardness and tool, in the player's language.
    fn mod_block_facts(&mut self) -> Arc<Vec<BlockFact>> {
        let language = horizon_client::lang::code();
        if let Some((made_in, facts)) = &self.mods.block_facts
            && *made_in == language
        {
            return facts.clone();
        }
        let tags = &self.res.items().block_tags;
        let tool = |block: &str| {
            ["pickaxe", "axe", "shovel", "hoe"]
                .into_iter()
                .find(|tool| {
                    tags.get(&format!("minecraft:mineable/{tool}"))
                        .is_some_and(|set| set.contains(block))
                })
        };
        let registry = &self.pack.assets.registry;
        let facts: Vec<BlockFact> = (0..registry.len())
            .map(|i| {
                let f = self
                    .res
                    .facts()
                    .get(horizon_world::registry::BlockStateId(i as u32));
                let (namespace, path) = f.name.split_once(':').unwrap_or(("minecraft", &f.name));
                BlockFact {
                    block: f.name.clone(),
                    name: horizon_client::lang::get(&format!("block.{namespace}.{path}")),
                    item: (&*f.item != "minecraft:air").then(|| f.item.to_string()),
                    hardness: f.hardness,
                    tool: tool(&f.name),
                    needs_tool: f.requires_tool,
                }
            })
            .collect();
        let facts = Arc::new(facts);
        self.mods.block_facts = Some((language, facts.clone()));
        facts
    }
    /// The blocks needing a tool that the held item would make drop (`Tool.isCorrectForDrops`),
    /// made again when the held item changes.
    fn mod_harvest(&mut self) -> Arc<HashSet<Arc<str>>> {
        let held = self.held().map_or_else(String::new, |s| s.item.to_string());
        if let Some((item, set)) = &self.mods.harvest
            && *item == held
        {
            return set.clone();
        }
        let facts = self.mod_block_facts();
        let items = self.res.items();
        let stack = self.held();
        let set: HashSet<Arc<str>> = facts
            .iter()
            .filter(|f| f.needs_tool && items.correct_for_drops(stack, &f.block))
            .map(|f| f.block.clone())
            .collect();
        let set = Arc::new(set);
        self.mods.harvest = Some((held, set.clone()));
        set
    }
    /// Entities near the player, nearest first, for mods, with what the game shows of them.
    fn mod_entities(&self) -> Vec<(Entity, EntityInfo)> {
        let Some(p) = self.player.as_ref() else {
            return Vec::new();
        };
        let mut list: Vec<(f64, (Entity, EntityInfo))> = self
            .mobs
            .iter()
            .map(|m| {
                let d = m.position.distance_squared(p.position);
                let entity = Entity {
                    id: m.id as i32,
                    kind: m.kind.id.to_string(),
                    x: m.position.x,
                    y: m.position.y,
                    z: m.position.z,
                    yaw: m.yaw,
                    health: Some(m.health),
                    name: m.custom_name.clone(),
                };
                let info = EntityInfo {
                    id: m.id as i32,
                    kind: m.kind.id.to_string(),
                    name: m.custom_name.clone().unwrap_or_else(|| {
                        horizon_client::lang::get(&format!(
                            "entity.{}",
                            m.kind.id.replace(':', ".")
                        ))
                    }),
                    health: Some(m.health),
                    max_health: Some(finalize::max_health(m)),
                    armor: finalize::value(m, finalize::Attr::Armor) as f32,
                    baby: m.is_baby(),
                };
                (d, (entity, info))
            })
            .filter(|(d, _)| *d <= 64.0 * 64.0)
            .collect();
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        list.truncate(128);
        list.into_iter().map(|(_, e)| e).collect()
    }
    /// A game tick: the snapshot, the connection and damage events, then the tick hook.
    pub(super) fn tick_mods(&mut self) {
        // A mod that stopped lets go of what it held.
        if !self.mods.holds.is_empty() {
            let running: Vec<String> = self
                .mods
                .mods
                .entries()
                .iter()
                .filter(|e| e.status == Status::Running)
                .filter_map(|e| e.manifest.as_ref().map(|m| m.id.clone()))
                .collect();
            self.release_mod_holds(|id| running.iter().any(|r| r == id));
        }
        if !self.mods.mods.is_running() {
            return;
        }
        let snapshot = self.mod_snapshot();
        let joined = snapshot
            .player
            .as_ref()
            .map(|_| snapshot.world.as_ref().and_then(|w| w.server.clone()));
        let dimension = snapshot.player.as_ref().map(|p| p.dimension.clone());
        let health = snapshot.player.as_ref().map(|p| p.health);
        self.mods.mods.set_snapshot(snapshot);
        // ClientPlayConnectionEvents: the world entered or left, a dimension changed.
        if joined != self.mods.joined {
            if self.mods.joined.is_some() {
                self.mods.mods.left();
            }
            if let Some(server) = &joined {
                self.mods.mods.joined(server.as_deref());
            }
            self.mods.joined = joined;
            self.mods.dimension = None;
        }
        if dimension.is_some() && dimension != self.mods.dimension {
            if self.mods.dimension.is_some()
                && let Some(d) = &dimension
            {
                self.mods.mods.dimension_changed(d);
            }
            self.mods.dimension = dimension;
        }
        if let (Some(before), Some(now)) = (self.mods.health, health)
            && now < before
        {
            self.mods.mods.player_hurt(before - now);
        }
        self.mods.health = health;
        self.mods.mods.tick(self.ticks);
        self.show_mod_output();
    }
    /// Once per frame: the frame hook, and dev mods' rebuilds.
    pub(super) fn mods_frame(&mut self, elapsed: Duration) {
        if !self.mods.dev.is_empty()
            && self
                .mods
                .checked
                .is_none_or(|t| t.elapsed() >= RELOAD_CHECK)
        {
            self.mods.checked = Some(Instant::now());
            if self.mods.mods.reload_changed(vanilla_command) {
                self.list_mods();
                self.sync_mod_commands();
                self.system_message(Text::literal("Mods reloaded").with_color(text::GRAY));
            }
        }
        // A mod stopped on the quick compiler's code starts again on full code once it is kept.
        if self
            .mods
            .retried
            .is_none_or(|t| t.elapsed() >= RELOAD_CHECK)
        {
            self.mods.retried = Some(Instant::now());
            if self.mods.mods.restart_on_full_code(vanilla_command) {
                self.list_mods();
                self.sync_mod_commands();
            }
        }
        self.swap_creative_screen();
        if !self.mods.mods.is_running() {
            return;
        }
        self.mods_screen_events();
        self.frame_mod_controls();
        self.mods.mods.frame(elapsed.as_secs_f32());
        self.show_mod_output();
    }
    /// The game's controls for mods this frame: held (behind a screen, none), and a press counted
    /// each time one goes down.
    fn frame_mod_controls(&mut self) {
        let playing = self.screen == Screen::Playing && self.title.is_none();
        let key = |k| playing && self.keys.contains(&k);
        let [left, forward] = self.pad_move().filter(|_| playing).unwrap_or([0.0, 0.0]);
        let held = [
            key(KeyCode::KeyW) || forward > 0.3,
            key(KeyCode::KeyS) || forward < -0.3,
            key(KeyCode::KeyA) || left > 0.3,
            key(KeyCode::KeyD) || left < -0.3,
            key(KeyCode::Space),
            key(KeyCode::ShiftLeft),
            key(KeyCode::ControlLeft),
            playing && self.left,
            playing && self.right,
        ];
        let counted = std::mem::take(&mut self.mods.counted);
        for ((control, down), counted) in self.mods.controls.iter_mut().zip(held).zip(counted) {
            if down && !control.0 && !counted {
                control.1 += 1;
            }
            control.0 = down;
        }
        self.mods.mods.set_controls(self.mods.controls);
    }
    /// Lets go of what mods hold for the player but those `keep` says (when they stop or reload).
    pub(super) fn release_mod_holds(&mut self, keep: impl Fn(&str) -> bool) {
        let before = self.mods.holds.len();
        self.mods.holds.retain(|(id, _)| keep(id));
        if self.mods.holds.len() != before {
            self.sync_toggle(0);
            self.sync_toggle(1);
        }
    }
    /// `/command` typed in chat: true when a mod owns it.
    pub(super) fn mod_command(&mut self, command: &str) -> bool {
        if !self.mods.mods.is_running() {
            return false;
        }
        // The command may follow a teleport in the same tick.
        let snapshot = self.mod_snapshot();
        self.mods.mods.set_snapshot(snapshot);
        if !self.mods.mods.command(command) {
            return false;
        }
        self.show_mod_output();
        true
    }
    /// A chat line the player receives: what the mods make of it (None: hidden).
    pub(super) fn mods_chat(&mut self, line: Text) -> Option<Text> {
        if !self.mods.mods.is_running() {
            return Some(line);
        }
        let component = line.component().to_string();
        let verdict = self.mods.mods.chat(&line.plain(), &component);
        self.show_mod_output();
        match verdict {
            ChatVerdict::Keep => Some(line),
            ChatVerdict::Hide => None,
            ChatVerdict::Replace(text) => Some(rich_text(&text)),
        }
    }
    /// A line the player is about to send: what the mods make of it (None: cancelled).
    pub(super) fn mods_chat_sending(&mut self, message: &str) -> Option<String> {
        if !self.mods.mods.is_running() {
            return Some(message.to_owned());
        }
        let out = self.mods.mods.chat_sending(message);
        self.show_mod_output();
        out
    }
    /// A custom payload from the server, for the mods on its channel.
    pub(super) fn mods_payload(&mut self, channel: &str, data: &[u8]) {
        if !self.mods.mods.is_running() {
            return;
        }
        self.mods.mods.payload(channel, data);
        self.show_mod_output();
    }
    /// Lines the mods add under `stack`'s tooltip.
    pub(super) fn mods_tooltip(&mut self, stack: &ItemStack) -> Vec<Text> {
        if !self.mods.mods.is_running() || !self.mods.mods.wants(|h| h.tooltips) {
            return Vec::new();
        }
        let stack = self.mod_stack(stack);
        let lines = self.mods.mods.tooltip(&stack);
        self.show_mod_output();
        lines.iter().map(|l| rich_text(l)).collect()
    }
    /// A key or mouse button for the mods' keybinds (while playing): pressed or released.
    pub(super) fn mods_key(&mut self, name: &str, pressed: bool) {
        if !self.mods.mods.is_running() {
            return;
        }
        let binds = self.mods.mods.keybinds();
        for (id, bind) in binds.iter().filter(|(_, b)| b.key == name) {
            self.mod_bind(id, &bind.id, pressed);
        }
        self.show_mod_output();
    }
    /// Mod `id`'s keybind `bind` held or let go (a key, or its touch button).
    pub(super) fn mod_bind(&mut self, id: &str, bind: &str, pressed: bool) {
        let full = format!("{id}:{bind}");
        let held = self.mods.keys_down.contains(&full);
        if pressed == held {
            return;
        }
        if pressed {
            self.mods.keys_down.push(full);
            // A press on a paused game's screen (a mod's button there) comes between no ticks.
            self.refresh_mod_snapshot();
        } else {
            self.mods.keys_down.retain(|k| *k != full);
        }
        self.mods.mods.key(id, bind, pressed);
        self.show_mod_output();
    }
    /// Minecraft.startAttack: whether the mods let it go ahead, on the entity or block under
    /// the crosshair.
    pub(super) fn mods_allow_attack(&mut self) -> bool {
        if !self.mods.mods.wants(|h| h.interaction) {
            return true;
        }
        if let Some((i, distance)) = self.mob_hit()
            && self.before_block(distance)
        {
            let id = self.mobs[i].id as i32;
            return self.mods_attack_entity(id);
        }
        match self.pick() {
            Some(hit) => self.mods_attack_block(hit.pos, hit.face),
            None => true,
        }
    }
    /// Minecraft.startUseItem: whether the mods let it go ahead, on the block under the
    /// crosshair or with the held item.
    pub(super) fn mods_allow_use(&mut self) -> bool {
        if !self.mods.mods.wants(|h| h.interaction) {
            return true;
        }
        if let Some(hit) = self.pick()
            && !self.entity_targeted()
            && !self.mods_use_block(hit.pos, hit.face, false)
        {
            return false;
        }
        self.mods_use_item(false)
    }
    /// `UseItemCallback`: false when a mod cancels it.
    pub(super) fn mods_use_item(&mut self, off_hand: bool) -> bool {
        if !self.mods.mods.wants(|h| h.interaction) {
            return true;
        }
        let hand = if off_hand {
            horizon_mods::Hand::OffHand
        } else {
            horizon_mods::Hand::MainHand
        };
        let allowed = self.mods.mods.use_item(hand);
        self.show_mod_output();
        allowed
    }
    /// `UseBlockCallback`.
    pub(super) fn mods_use_block(&mut self, pos: [i32; 3], face: [i32; 3], off_hand: bool) -> bool {
        if !self.mods.mods.wants(|h| h.interaction) {
            return true;
        }
        let hand = if off_hand {
            horizon_mods::Hand::OffHand
        } else {
            horizon_mods::Hand::MainHand
        };
        let allowed = self
            .mods
            .mods
            .use_block(block_pos(pos), direction(face), hand);
        self.show_mod_output();
        allowed
    }
    /// `AttackEntityCallback`.
    pub(super) fn mods_attack_entity(&mut self, id: i32) -> bool {
        if !self.mods.mods.wants(|h| h.interaction) {
            return true;
        }
        let allowed = self.mods.mods.attack_entity(id);
        self.show_mod_output();
        allowed
    }
    /// `AttackBlockCallback`.
    pub(super) fn mods_attack_block(&mut self, pos: [i32; 3], face: [i32; 3]) -> bool {
        if !self.mods.mods.wants(|h| h.interaction) {
            return true;
        }
        let allowed = self.mods.mods.attack_block(block_pos(pos), direction(face));
        self.show_mod_output();
        allowed
    }
    /// `ClientPlayerBlockBreakEvents.AFTER`.
    pub(super) fn mods_block_broken(
        &mut self,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    ) {
        if !self.mods.mods.wants(|h| h.interaction) {
            return;
        }
        let block = self.state_string(state);
        self.mods.mods.block_broken(block_pos(pos), &block);
        self.show_mod_output();
    }
    /// Chat lines, command feedback, stopped mods, and what mods asked for.
    pub(super) fn show_mod_output(&mut self) {
        // Slots a mod clicked or set: its next hook reads them as they are now.
        let mut clicked = false;
        for output in self.mods.mods.drain() {
            let line =
                match output {
                    Output::Chat { name, text } => {
                        let mut line = Text::literal(format!("[{name}] ")).with_color(text::GRAY);
                        line.push(rich_text(&text));
                        line
                    }
                    Output::Feedback { text, error: false } => Text::literal(text),
                    Output::Feedback { text, error: true } => {
                        Text::literal(text).with_color(text::RED)
                    }
                    Output::Stopped { name, reason } => {
                        self.list_mods();
                        self.sync_mod_commands();
                        if self
                            .mods
                            .screen
                            .as_ref()
                            .is_some_and(|s| !self.mods.mods.wants(|_| true) || s.is_empty())
                        {
                            self.mods.screen = None;
                        }
                        Text::literal(format!("Mod {name} stopped: {reason}")).with_color(text::RED)
                    }
                    Output::Overlay { text } => {
                        self.titles.set_overlay(rich_text(&text), false);
                        continue;
                    }
                    Output::Send { message } => {
                        self.mod_send(&message);
                        continue;
                    }
                    Output::Sound { id, volume, pitch } => {
                        self.audio.play(&id, None, volume, pitch);
                        continue;
                    }
                    Output::Payload { channel, data } => {
                        self.send(horizon_protocol::packets::play::Serverbound::CustomPayload(
                            horizon_protocol::packets::configuration::CustomPayload {
                                channel,
                                data,
                            },
                        ));
                        continue;
                    }
                    Output::OpenScreen { screen } => {
                        self.open_mod_screen(screen);
                        continue;
                    }
                    Output::Click {
                        slot, button, kind, ..
                    } => {
                        self.mod_slot_click(slot, button, kind);
                        clicked = true;
                        continue;
                    }
                    Output::Drag { slots, button, .. } => {
                        self.mod_slot_drag(&slots, button);
                        clicked = true;
                        continue;
                    }
                    Output::DragAreas { id, areas } => {
                        if self.mods.screen.as_ref().is_some_and(|s| {
                            s.split_once(':').is_some_and(|(owner, _)| owner == id)
                        }) {
                            self.mods.drag_areas = areas;
                        }
                        continue;
                    }
                    Output::CreativeClick {
                        tab,
                        search,
                        index,
                        button,
                        kind,
                        ..
                    } => {
                        self.mod_creative_click(&tab, &search, index, button, kind);
                        clicked = true;
                        continue;
                    }
                    Output::CreativeSlot { slot, stack, .. } => {
                        self.mod_creative_slot(slot, stack.as_ref());
                        clicked = true;
                        continue;
                    }
                    Output::CreativeCarried { stack, .. } => {
                        self.mod_creative_carried(stack.as_ref());
                        clicked = true;
                        continue;
                    }
                    Output::Hold { id, control, held } => {
                        let slot = usize::from(control == "key.sprint");
                        self.mods
                            .holds
                            .retain(|(owner, s)| !(*owner == id && *s == slot));
                        if held {
                            self.mods.holds.push((id, slot));
                        }
                        self.sync_toggle(slot);
                        continue;
                    }
                    Output::Keyboard { id, shown } => {
                        if self.mods.screen.as_ref().is_some_and(|s| {
                            s.split_once(':').is_some_and(|(owner, _)| owner == id)
                        }) {
                            self.mods.keyboard = shown;
                        }
                        continue;
                    }
                    Output::CloseScreen { id } => {
                        if self.mods.screen.as_ref().is_some_and(|s| {
                            s.split_once(':').is_some_and(|(owner, _)| owner == id)
                        }) {
                            self.close_mod_screen();
                        }
                        continue;
                    }
                };
            // Not through system_message: a mod must not hear its own lines.
            let focused = self.screen == Screen::Chat;
            self.ui
                .chat
                .add(&self.gui.atlas, line, None, self.ticks, focused);
        }
        if clicked {
            self.refresh_mod_snapshot();
        }
    }
    /// `chat-send.send`: a line or `/command` as the player typed it, past the chat hooks.
    fn mod_send(&mut self, message: &str) {
        if self.remote.is_some() {
            self.send_chat(message);
        } else if let Some(command) = message.strip_prefix('/') {
            self.run_command(command);
        } else {
            self.handle_chat_input(message);
        }
    }
    /// The mods' HUD panels, stacked with the extras in their corners.
    pub(super) fn draw_mod_panels(&self, batch: &mut HudBatch, corners: &mut Corners) {
        for panel in self.mods.mods.panels() {
            let corner = match panel.corner {
                horizon_mods::Corner::TopLeft => Corner::TopLeft,
                horizon_mods::Corner::TopRight => Corner::TopRight,
                horizon_mods::Corner::BottomLeft => Corner::BottomLeft,
                horizon_mods::Corner::BottomRight => Corner::BottomRight,
            };
            // F3 takes the top left, as it does from the extras.
            if self.debug && corner == Corner::TopLeft {
                continue;
            }
            let lines: Vec<(&str, u32)> = panel
                .lines
                .iter()
                .map(|l| (l.text.as_str(), l.color))
                .collect();
            extras_hud::draw_lines(batch, &self.gui.atlas, corners, corner, &lines);
        }
    }
}

fn block_pos(p: [i32; 3]) -> horizon_mods::BlockPos {
    horizon_mods::BlockPos {
        x: p[0],
        y: p[1],
        z: p[2],
    }
}
fn direction(face: [i32; 3]) -> horizon_mods::Direction {
    use horizon_mods::Direction;
    match face {
        [0, -1, 0] => Direction::Down,
        [0, 1, 0] => Direction::Up,
        [0, 0, -1] => Direction::North,
        [0, 0, 1] => Direction::South,
        [-1, 0, 0] => Direction::West,
        _ => Direction::East,
    }
}
