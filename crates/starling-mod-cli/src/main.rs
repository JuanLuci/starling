//! `starling-mod`: run a Starling mod without the game, in a pretend world, and see what it does.
//!
//! `starling-mod run <mod> [--scenario <file.json>] [--step <json>]...` loads the mod (a folder
//! with `horizon-mod.toml`, or a `.hmod`) as the game would and plays steps on it: ticks,
//! commands, chat, keys, tooltips, its HUD and screens. Each thing that happens is a line of
//! JSON on stdout, for a person or a program (an AI fixing the mod) to read.
//!
//! A scenario is `{"player": {...}, "gui": {"width": 427, "height": 240}, "steps": [...]}` (the
//! GUI's size in GUI pixels, the HUD and screens drawn at it), and what the crosshair is on
//! (`"target"`), the block being broken (`"mining"`) and blocks around (`"blocks"`): see
//! `world::world`; `"player": {"inventory": {"39": {"id": "minecraft:diamond_helmet", "damage":
//! 120}}}` fills the player's slots. A step is one of:
//! `{"tick": 20}`, `{"frame": 0.05}`, `{"command": "hello world"}`, `{"chat": "text"}`,
//! `{"send": "text"}`, `{"hud": {}}`, `{"tooltip": "minecraft:stick"}` (with `"components"`, SNBT),
//! `{"key": "<keybind id>", "pressed": true}`, `{"screen": "<mod id>:<name>"}` (opens and draws
//! it; a game screen's name, `pause` say, draws the mod's buttons on it; `container` and
//! `inventory` a plain window of vanilla's layout, the menu's stacks and the mod's window
//! buttons on it: window.rs),
//! `{"click": [x, y], "button": 0}` (a mod's button on a game screen first), `{"type": "text"}`,
//! `{"press": "key.keyboard.enter"}`, `{"close": true}`, `{"join": "server"}`, `{"leave": true}`,
//! `{"hurt": 2.0}`, `{"touch": "<button id>", "pressed": true}` (a touch button, pressed and
//! let go without `pressed`), `{"controls": ["key.forward", "key.attack"]}` (the game's controls
//! held from then on, the others let go; mods' `input.hold` adds sprint and sneak).
//! `{"hud": {"touch": true}}` draws the mod's touch buttons too, where they start. `"menu"` (see
//! `world::world`) is the open menu `slots.get-menu` reads; each `slots.click` is a `click` line.
//! The creative tabs (`creative`) are the resources' `creative-inventory.json`, else a few made
//! up; `"player": {"game_mode": "creative"}` lets a mod take from them, each a `creative` line.
//! Pickup clicks (buttons 0 and 1, and -999 outside), Shift-clicks on a chest's or the
//! inventory's menu, drags (`slots.drag`), `creative`'s clicks and sets change the pretend menu
//! and cursor as the game's simplest clicks do, before the step draws; the other clicks are lines
//! only. `{"click": [x, y], "press": false}` is a press without its release (a
//! finger held), `{"release": [x, y]}` its release, `{"drag": [x, y]}` the mouse moved with the
//! button held, `{"scroll": [x, y], "lines": -1}` the wheel; `{"key-press": "key.keyboard.left.shift",
//! "pressed": true}` a key going down (false: up) without its text.
//! Without steps it ticks once and draws the HUD. The mod's buttons (`widgets`) are a `widgets`
//! line whenever they change, its drawing in the world (`world-draw`) a `world` line.
//!
//! Each run starts with the mod's storage empty; `--storage <dir>` keeps it there between runs.
//! `--watch` runs it again whenever the mod (a `.hmod`, or a folder's files) or the scenario
//! changes, with a `{"event": "changed"}` line before each new run, until stopped.
//! `--resources <dir>` is a prepared resources folder (the one holding `items.json` and
//! `assets/minecraft`): the pretend world gets its items and its font's text widths.
//! `--png <dir>` also draws each step that drew something to `<dir>/step<N>.png` (its path in the
//! step's line), with that folder's font and sprites, or plain shapes without one.
mod creative;
mod recipes;
mod render;
mod window;
mod world;

use horizon_mods::{
    Anchor, ButtonShape as Shape, CONTROLS, Controls, Corner, CreativeCatalog, CreativeClick,
    DrawOp, Facing, Gui, ItemCatalog, ItemStack, KeyEvent, Mods, MouseAction, MouseEvent, Output,
    RenderContext, ScreenButton, Snapshot, Status,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

const USAGE: &str = "usage: starling-mod run <mod folder or .hmod> [--scenario <file.json>] \
    [--step <json>]... [--storage <dir>] [--resources <dir>] [--png <dir>] [--watch]";

fn main() -> ExitCode {
    log::set_logger(&JSON_LOG).expect("the only logger");
    log::set_max_level(log::LevelFilter::Info);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        _ => Err(USAGE.into()),
    };
    // The JavaScript engine ran on quick code: its full code is kept for the next run.
    if horizon_mods::compiling_in_background() {
        emit(json!({ "event": "note", "text": "keeping the full compile for the next run" }));
    }
    horizon_mods::finish_background_compiles();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            emit(json!({ "event": "error", "message": e }));
            ExitCode::FAILURE
        }
    }
}

/// One line of output.
fn emit(value: Value) {
    println!("{value}");
}

/// The game's log (and the mod's `log.write`, as `[mod <id>] ...`) as JSON lines.
struct JsonLog;
static JSON_LOG: JsonLog = JsonLog;
impl log::Log for JsonLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            emit(json!({
                "event": "log",
                "level": record.level().as_str().to_lowercase(),
                "text": record.args().to_string(),
            }));
        }
    }
    fn flush(&self) {}
}

/// What `run` was asked.
struct Options {
    path: PathBuf,
    scenario: Option<PathBuf>,
    steps: Vec<Value>,
    png: Option<PathBuf>,
    resources: Option<PathBuf>,
    storage: Option<PathBuf>,
    watch: bool,
}

fn run(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .ok_or(USAGE)?;
    let mut options = Options {
        path,
        scenario: None,
        steps: Vec::new(),
        png: None,
        resources: None,
        storage: None,
        watch: false,
    };
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        if flag == "--watch" {
            options.watch = true;
            continue;
        }
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--scenario" => options.scenario = Some(PathBuf::from(value)),
            "--step" => options
                .steps
                .push(serde_json::from_str(value).map_err(|e| format!("--step {value}: {e}"))?),
            "--png" => options.png = Some(PathBuf::from(value)),
            "--resources" => options.resources = Some(PathBuf::from(value)),
            "--storage" => options.storage = Some(PathBuf::from(value)),
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    if !options.watch {
        return run_once(&options);
    }
    // Runs again whenever the mod or the scenario changes, until stopped (Ctrl+C).
    let mut seen = stamp(&options);
    loop {
        if let Err(e) = run_once(&options) {
            emit(json!({ "event": "error", "message": e }));
        }
        emit(json!({ "event": "watching" }));
        seen = changed(&options, &seen);
        emit(json!({ "event": "changed" }));
    }
}

/// The mod's and the scenario's files with their times and sizes, to see a change.
type Stamp = Vec<(PathBuf, Option<std::time::SystemTime>, u64)>;

fn stamp(options: &Options) -> Stamp {
    fn add(path: &Path, out: &mut Stamp) {
        let Ok(meta) = std::fs::metadata(path) else {
            out.push((path.to_owned(), None, 0));
            return;
        };
        if !meta.is_dir() {
            out.push((path.to_owned(), meta.modified().ok(), meta.len()));
            return;
        }
        let Ok(read) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<PathBuf> = read
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .collect();
        entries.sort();
        for entry in entries {
            add(&entry, out);
        }
    }
    let mut out = Vec::new();
    add(&options.path, &mut out);
    if let Some(scenario) = &options.scenario {
        add(scenario, &mut out);
    }
    out
}

/// Waits for the files to differ from `seen` and then hold still (a build or an editor writes in
/// parts); their new stamp.
fn changed(options: &Options, seen: &Stamp) -> Stamp {
    let look = || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        stamp(options)
    };
    loop {
        let now = look();
        if now != *seen && look() == now {
            return now;
        }
    }
}

/// One run of the mod: loaded fresh, the steps played on it.
fn run_once(options: &Options) -> Result<(), String> {
    let path = &options.path;
    let (scenario, mut steps) = match &options.scenario {
        Some(file) => {
            let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", file.display());
            let text = std::fs::read_to_string(file).map_err(|e| fail(&e))?;
            let scenario: Value = serde_json::from_str(&text).map_err(|e| fail(&e))?;
            let steps = scenario["steps"].as_array().cloned().unwrap_or_default();
            (scenario, steps)
        }
        None => (json!({}), Vec::new()),
    };
    steps.extend(options.steps.iter().cloned());
    if steps.is_empty() {
        steps = vec![json!({ "tick": 1 }), json!({ "hud": {} })];
    }

    // A mod keeps its storage beside itself (`.horizon-mod-data`), so it runs from a copy in the
    // storage folder: a new one for this run unless `--storage` names one.
    let place = Place::new(options.storage.clone())?;
    let copy = place.dir.join(path.file_name().ok_or(USAGE)?);
    copy_all(path, &copy)?;

    // The engine's compiled code is kept between runs, as the game keeps it; a folder for each
    // mod, since a load removes the compiled code of mods it did not load.
    let cache = std::env::temp_dir()
        .join("starling-mod-cache")
        .join(copy.file_name().ok_or(USAGE)?);
    let started = Instant::now();
    let mut mods = Mods::load_cached(
        std::slice::from_ref(&copy),
        |_| true,
        |_| false,
        Some(&cache),
    );
    let entry = &mods.entries()[0];
    let (status, reason) = match &entry.status {
        Status::Running => ("running", None),
        Status::Off => ("off", None),
        Status::Failed(reason) => ("failed", Some(reason.clone())),
            Status::Elsewhere => ("elsewhere", None),
    };
    let manifest = entry.manifest.as_ref();
    emit(json!({
        "event": "loaded",
        "id": manifest.map(|m| m.id.clone()),
        "name": manifest.map(|m| m.name.clone()),
        "status": status,
        "reason": reason,
        "seconds": started.elapsed().as_secs_f32(),
        "commands": mods.commands().collect::<Vec<_>>(),
        "keybinds": mods.keybinds().iter().map(|(_, k)| json!({ "id": k.id, "name": k.name, "key": k.key })).collect::<Vec<_>>(),
        "channels": mods.channels(),
        "config": manifest.and_then(|m| mods.config_screen(&m.id)),
    }));
    let mut hands = Hands::default();
    let mut world = Snapshot::default();
    outputs(&mut mods, &mut None, &mut hands, &mut world);
    if status != "running" {
        return Ok(());
    }
    let mut buttons = widgets_line(&mods, &mut Value::Null);
    let mut layers = world_line(&mods, &mut Value::Null);
    let gui = Gui {
        width: scenario["gui"]["width"].as_u64().unwrap_or(427) as u32,
        height: scenario["gui"]["height"].as_u64().unwrap_or(240) as u32,
        scale: scenario["gui"]["scale"].as_f64().unwrap_or(2.0) as f32,
    };
    let data = options.resources.as_deref().and_then(world::Data::read);
    let data = Arc::new(data.unwrap_or_default());
    let mut resources = render::Resources::new(options.resources.clone()).with_mod(path);
    let creative: Arc<dyn CreativeCatalog> =
        Arc::new(creative::Pretend::read(options.resources.as_deref(), &data));
    let mut world = world::world(&scenario, data.clone());
    world.gui = Some(gui);
    world.measure = resources.measure();
    world.creative = Some(creative);
    world.recipes = Some(Arc::new(recipes::Pretend::read(
        options.resources.as_deref(),
        &data,
    )));
    hands.apply(&mut mods, &world);
    let mut screen: Option<String> = None;
    let mut ticks = 0;
    let png = &options.png;
    if let Some(dir) = png {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // The last run's pictures, which this run may draw fewer of.
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("step") && name.ends_with(".png") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        if !resources.has_font() {
            emit(
                json!({ "event": "note", "text": "no font in --resources: text is drawn as boxes" }),
            );
        }
    }
    for (n, step) in steps.into_iter().enumerate() {
        let (result, drawn) = match step.get("controls") {
            Some(held) => (hands.hold(held, &mut mods, &world), None),
            None => {
                let mut sim = Sim {
                    screen: &mut screen,
                    hands: &mut hands,
                    world: &mut world,
                };
                play(&mut mods, &step, &mut sim, &mut ticks, &gui, &data)
            }
        };
        let file = match (png, drawn) {
            (Some(dir), Some(ops)) if !ops.is_empty() => {
                let scale = gui.scale.round().max(1.0) as u32;
                let file = dir.join(format!("step{n}.png"));
                render::render(&ops, gui.width, gui.height, scale, &mut resources).save(&file)?;
                Some(file)
            }
            _ => None,
        };
        emit(json!({ "event": "step", "step": step, "result": result, "png": file }));
        outputs(&mut mods, &mut screen, &mut hands, &mut world);
        buttons = widgets_line(&mods, &mut buttons);
        layers = world_line(&mods, &mut layers);
        hands.apply(&mut mods, &world);
        if !mods.is_running() {
            break;
        }
    }
    Ok(())
}

/// The folder a run's copy of the mod (and so its storage) is in, removed after the run when
/// it was made for it.
struct Place {
    dir: PathBuf,
    temporary: bool,
}

impl Place {
    fn new(storage: Option<PathBuf>) -> Result<Self, String> {
        let temporary = storage.is_none();
        let dir = storage.unwrap_or_else(|| {
            std::env::temp_dir().join(format!("starling-mod-run-{}", std::process::id()))
        });
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Place { dir, temporary })
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// `from` (a file or a folder) copied to `to`, over what was there.
fn copy_all(from: &Path, to: &Path) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", from.display());
    if from.is_dir() {
        if to.exists() {
            std::fs::remove_dir_all(to).map_err(fail)?;
        }
        std::fs::create_dir_all(to).map_err(fail)?;
        for entry in std::fs::read_dir(from).map_err(fail)? {
            let entry = entry.map_err(fail)?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(drop).map_err(fail)
    }
}

/// The pretend player's hands on the game's controls: what the scenario holds and what mods
/// hold (`input.hold`), with presses counted as the game counts them.
#[derive(Default)]
struct Hands {
    scenario: [bool; 9],
    /// Sneak and sprint, held by mods.
    held: [bool; 2],
    controls: Controls,
}

/// Where a control is in [`CONTROLS`].
fn control(name: &str) -> Option<usize> {
    CONTROLS.iter().position(|c| *c == name)
}

impl Hands {
    /// Holds the scenario's `list` of controls and lets go of the others; the controls held.
    fn hold(&mut self, list: &Value, mods: &mut Mods, world: &Snapshot) -> Value {
        self.scenario = [false; 9];
        for name in list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            match control(name) {
                Some(i) => self.scenario[i] = true,
                None => {
                    return json!({ "error": format!("no control {name}: one of {CONTROLS:?}") });
                }
            }
        }
        self.apply(mods, world);
        let held: Vec<&str> = (0..9)
            .filter(|&i| self.controls[i].0)
            .map(|i| CONTROLS[i])
            .collect();
        json!({ "held": held })
    }
    /// The controls as now held, told to the mods; the player sprints and sneaks with them.
    fn apply(&mut self, mods: &mut Mods, world: &Snapshot) {
        let at = |name| control(name).expect("one of CONTROLS");
        let (forward, sneak, sprint) = (at("key.forward"), at("key.sneak"), at("key.sprint"));
        for (i, control) in self.controls.iter_mut().enumerate() {
            let down =
                self.scenario[i] || (i == sneak && self.held[0]) || (i == sprint && self.held[1]);
            if down && !control.0 {
                control.1 += 1;
            }
            control.0 = down;
        }
        mods.set_controls(self.controls);
        let mut world = world.clone();
        if let Some(p) = &mut world.player {
            p.sneaking = self.controls[sneak].0;
            p.sprinting = self.controls[sprint].0 && self.controls[forward].0 && !p.sneaking;
        }
        mods.set_snapshot(world);
    }
}

/// What a step plays on: the open screen, the controls held and the pretend world.
struct Sim<'a> {
    screen: &'a mut Option<String>,
    hands: &'a mut Hands,
    world: &'a mut Snapshot,
}

impl Sim<'_> {
    /// What the mod asked for so far done, as the game does it after a hook returns: its lines,
    /// and the world changed by its clicks.
    fn settle(&mut self, mods: &mut Mods) {
        outputs(mods, self.screen, self.hands, self.world);
        self.hands.apply(mods, self.world);
    }
}

/// The most a stack of `id` holds in the pretend world.
fn max_stack(world: &Snapshot, id: &str) -> u32 {
    world
        .items
        .as_ref()
        .and_then(|i| i.info(id))
        .map_or(64, |i| i.max_stack_size.max(1))
}

fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.id == b.id && a.components == b.components
}

/// A pickup click (button 0 or 1) on the pretend menu's slot `slot` (-999 outside: a throw), as
/// vanilla's simplest clicks: take all or half, put all or one, merge, swap. False for one it
/// does not pretend (the inventory menu's crafting result).
fn pretend_pickup(world: &mut Snapshot, slot: i32, button: u8) -> bool {
    let Some(menu) = world.menu.as_mut() else {
        return false;
    };
    if button > 1 {
        return false;
    }
    if slot == -999 {
        if let Some(c) = menu.carried.as_mut() {
            if button == 0 || c.count <= 1 {
                menu.carried = None;
            } else {
                c.count -= 1;
            }
        }
        return true;
    }
    let Some(index) = usize::try_from(slot)
        .ok()
        .filter(|&i| i < menu.slots.len() && !(i == 0 && menu.kind == "minecraft:inventory"))
    else {
        return false;
    };
    let items = world.items.clone();
    let max = |id: &str| {
        items
            .as_ref()
            .and_then(|i| i.info(id))
            .map_or(64, |i| i.max_stack_size.max(1))
    };
    let menu = world.menu.as_mut().expect("there is a menu");
    let (here, carried) = (menu.slots[index].take(), menu.carried.take());
    let (here, carried) = match (here, carried) {
        (Some(mut s), None) => {
            let taken = if button == 0 {
                s.count
            } else {
                s.count.div_ceil(2)
            };
            let mut hand = s.clone();
            hand.count = taken;
            s.count -= taken;
            ((s.count > 0).then_some(s), Some(hand))
        }
        (None, Some(mut c)) => {
            let put = if button == 0 { c.count } else { 1 };
            let mut s = c.clone();
            s.count = put;
            c.count -= put;
            (Some(s), (c.count > 0).then_some(c))
        }
        (Some(mut s), Some(mut c)) if same_item(&s, &c) => {
            let room = max(&s.id).saturating_sub(s.count);
            let put = room.min(if button == 0 { c.count } else { 1 });
            s.count += put;
            c.count -= put;
            (Some(s), (c.count > 0).then_some(c))
        }
        (here, carried) => (carried, here),
    };
    let menu = world.menu.as_mut().expect("there is a menu");
    menu.slots[index] = here;
    menu.carried = carried;
    true
}

/// A quick-move (Shift-click) on the pretend menu's slot `slot`, as vanilla's chest menus and
/// the inventory menu move a stack (`moveItemStackTo`): from a chest's slots into the inventory,
/// the hotbar's end first; from the inventory into the chest; in the inventory menu between the
/// inventory and the hotbar. False for a menu or slot it does not pretend.
fn pretend_quick_move(world: &mut Snapshot, slot: i32) -> bool {
    let Some(menu) = world.menu.as_ref() else {
        return false;
    };
    let n = menu.slots.len();
    let Some(i) = usize::try_from(slot).ok().filter(|&i| i < n) else {
        return false;
    };
    let (to, reverse) = if menu.kind.starts_with("minecraft:generic_9x") && n > 36 {
        let own = n - 36;
        if i < own {
            (own..n, true)
        } else {
            (0..own, false)
        }
    } else if menu.kind == "minecraft:inventory" && n >= 45 {
        match i {
            9..=35 => (36..45, false),
            36..=44 => (9..36, false),
            _ => return false,
        }
    } else {
        return false;
    };
    let Some(mut moving) = menu.slots[i].clone() else {
        return true;
    };
    let full = max_stack(world, &moving.id);
    let menu = world.menu.as_mut().expect("there is a menu");
    let order: Vec<usize> = if reverse {
        to.rev().collect()
    } else {
        to.collect()
    };
    // Onto the same item first, then into the first empty slot.
    for &j in &order {
        if let Some(there) = menu.slots[j].as_mut()
            && same_item(there, &moving)
            && there.count < full
        {
            let put = (full - there.count).min(moving.count);
            there.count += put;
            moving.count -= put;
            if moving.count == 0 {
                break;
            }
        }
    }
    if moving.count > 0
        && let Some(&j) = order.iter().find(|&&j| menu.slots[j].is_none())
    {
        menu.slots[j] = Some(moving.clone());
        moving.count = 0;
    }
    menu.slots[i] = (moving.count > 0).then_some(moving);
    true
}

/// `slots.drag` on the pretend menu, as vanilla's QUICK_CRAFT ends: the cursor's stack shared
/// out evenly (button 0), one each (1) or a full stack each (2) among the slots that take it
/// (empty, or the same item with room; not the inventory menu's crafting result), the rest left
/// on the cursor.
/// Armour and other slots that take only some items are not pretended: any item goes.
fn pretend_drag(world: &mut Snapshot, slots: &[i32], button: u8) {
    let Some(carried) = world.menu.as_ref().and_then(|m| m.carried.clone()) else {
        return;
    };
    let full = max_stack(world, &carried.id);
    let menu = world.menu.as_mut().expect("there is a menu");
    let mut taking: Vec<usize> = Vec::new();
    for &s in slots {
        let Some(i) = usize::try_from(s)
            .ok()
            .filter(|&i| i < menu.slots.len() && !(i == 0 && menu.kind == "minecraft:inventory"))
        else {
            continue;
        };
        let takes = match &menu.slots[i] {
            None => true,
            Some(have) => same_item(have, &carried) && have.count < full,
        };
        // A drag adds a slot while the cursor has an item for it (AbstractContainerMenu).
        if takes && !taking.contains(&i) && (button == 2 || carried.count as usize > taking.len()) {
            taking.push(i);
        }
    }
    if taking.is_empty() {
        return;
    }
    let mut left = i64::from(carried.count);
    for &i in &taking {
        let have = menu.slots[i].as_ref().map_or(0, |s| s.count);
        let each = match button {
            0 => carried.count / taking.len() as u32,
            1 => 1,
            _ => full,
        };
        let count = (have + each).min(full);
        left -= i64::from(count) - i64::from(have);
        menu.slots[i] = Some(ItemStack {
            count,
            ..carried.clone()
        });
    }
    menu.carried = (left > 0).then_some(ItemStack {
        count: left as u32,
        ..carried
    });
}

/// `creative.click` on the pretend tabs, as `creative::click_display` does it; the hotbar slot
/// it fills, if any.
fn pretend_creative(
    world: &mut Snapshot,
    tab: &str,
    search: &str,
    index: u32,
    button: u8,
    kind: CreativeClick,
) {
    let Some(shown) = world
        .creative
        .as_ref()
        .and_then(|c| c.items(tab, search, index as usize, 1).items.pop())
    else {
        if kind == CreativeClick::Pickup
            && let Some(menu) = world.menu.as_mut()
            && let Some(c) = menu.carried.as_mut()
        {
            if button == 0 || c.count <= 1 {
                menu.carried = None;
            } else {
                c.count -= 1;
            }
        }
        return;
    };
    let full = max_stack(world, &shown.id);
    let Some(menu) = world.menu.as_mut() else {
        return;
    };
    let copy = |count: u32| ItemStack {
        count,
        ..shown.clone()
    };
    match kind {
        CreativeClick::Swap => {
            // A hotbar slot, or the off hand (40).
            let at = if button == 40 {
                45
            } else {
                36 + button as usize
            };
            if (button < 9 || button == 40) && at < menu.slots.len() {
                menu.slots[at] = Some(copy(full));
            }
        }
        CreativeClick::Clone => {
            if menu.carried.is_none() {
                menu.carried = Some(copy(full));
            }
        }
        CreativeClick::Throw => {}
        CreativeClick::Pickup | CreativeClick::QuickMove => {
            let quick = kind == CreativeClick::QuickMove;
            menu.carried = match menu.carried.take() {
                Some(mut c) if same_item(&c, &shown) => {
                    if button == 0 {
                        c.count = if quick { full } else { (c.count + 1).min(full) };
                    } else {
                        c.count -= 1;
                    }
                    (c.count > 0).then_some(c)
                }
                None => Some(copy(if quick { full } else { shown.count })),
                Some(mut c) => {
                    if button == 0 {
                        None
                    } else {
                        c.count -= 1;
                        (c.count > 0).then_some(c)
                    }
                }
            };
        }
    }
}

/// What the mod asked of the game since the last look, a line each; a screen it opened or
/// closed is followed, and so are sprint and sneak held, and its clicks change the pretend world.
fn outputs(mods: &mut Mods, screen: &mut Option<String>, hands: &mut Hands, world: &mut Snapshot) {
    for output in mods.drain() {
        let line = match output {
            Output::Chat { name, text } => json!({ "event": "chat", "name": name, "text": text }),
            Output::Overlay { text } => json!({ "event": "overlay", "text": text }),
            Output::Feedback { text, error } => {
                json!({ "event": "feedback", "text": text, "error": error })
            }
            Output::Stopped { name, reason } => {
                json!({ "event": "stopped", "name": name, "reason": reason })
            }
            Output::Send { message } => json!({ "event": "send", "message": message }),
            Output::Sound { id, volume, pitch } => {
                json!({ "event": "sound", "id": id, "volume": volume, "pitch": pitch })
            }
            Output::Payload { channel, data } => {
                json!({ "event": "payload", "channel": channel, "bytes": data.len() })
            }
            Output::OpenScreen { screen: name } => {
                *screen = Some(name.clone());
                json!({ "event": "open-screen", "screen": name })
            }
            Output::CloseScreen { id } => {
                *screen = None;
                json!({ "event": "close-screen", "id": id })
            }
            Output::Keyboard { id, shown } => {
                json!({ "event": "keyboard", "id": id, "shown": shown })
            }
            Output::Command { line } => json!({ "event": "command", "line": line }),
            Output::OpenUiScreen { owner } => json!({ "event": "open-ui-screen", "owner": owner }),
            Output::CloseUiScreen { owner } => {
                json!({ "event": "close-ui-screen", "owner": owner })
            }
            Output::Shake { strength, seconds } => {
                json!({ "event": "shake", "strength": strength, "seconds": seconds })
            }
            Output::Click {
                slot, button, kind, ..
            } => {
                use horizon_mods::ClickKind;
                let kind = match kind {
                    ClickKind::Pickup => "pickup",
                    ClickKind::QuickMove => "quick-move",
                    ClickKind::Swap => "swap",
                    ClickKind::Clone => "clone",
                    ClickKind::Throw => "throw",
                    ClickKind::PickupAll => "pickup-all",
                };
                let pretended = match kind {
                    "pickup" => pretend_pickup(world, slot, button),
                    "quick-move" => pretend_quick_move(world, slot),
                    _ => false,
                };
                json!({ "event": "click", "slot": slot, "button": button, "kind": kind, "pretended": pretended })
            }
            Output::Drag { slots, button, .. } => {
                pretend_drag(world, &slots, button);
                json!({ "event": "drag", "slots": slots, "button": button })
            }
            Output::DragAreas { areas, .. } => {
                json!({ "event": "drag-areas", "areas": areas })
            }
            Output::ScreenSlots { slots, .. } => {
                json!({ "event": "screen-slots", "slots": slots })
            }
            Output::FillRecipe { recipe, all, .. } => {
                json!({ "event": "fill", "recipe": recipe, "all": all })
            }
            Output::CreativeClick {
                tab,
                search,
                index,
                button,
                kind,
                ..
            } => {
                pretend_creative(world, &tab, &search, index, button, kind);
                let kind = match kind {
                    CreativeClick::Pickup => "pickup",
                    CreativeClick::QuickMove => "quick-move",
                    CreativeClick::Swap => "swap",
                    CreativeClick::Clone => "clone",
                    CreativeClick::Throw => "throw",
                };
                json!({ "event": "creative", "click": { "tab": tab, "search": search, "index": index, "button": button, "kind": kind } })
            }
            Output::CreativeSlot { slot, stack, .. } => {
                if let Some(menu) = world.menu.as_mut()
                    && let Some(s) = menu.slots.get_mut(slot as usize)
                {
                    *s = stack.clone();
                }
                json!({ "event": "creative", "slot": slot, "stack": stack.map(|s| json!({ "id": s.id, "count": s.count })) })
            }
            Output::CreativeCarried { stack, .. } => {
                if let Some(menu) = world.menu.as_mut() {
                    menu.carried = stack.clone();
                }
                json!({ "event": "creative", "carried": stack.map(|s| json!({ "id": s.id, "count": s.count })) })
            }
            Output::Hold { control, held, .. } => {
                hands.held[usize::from(control == "key.sprint")] = held;
                json!({ "event": "hold", "control": control, "held": held })
            }
        };
        emit(line);
    }
}

/// The mod's buttons as JSON, a `widgets` line when they are not `last` (and there are any).
fn widgets_line(mods: &Mods, last: &mut Value) -> Value {
    let Some((_, w)) = mods.widgets().next() else {
        return Value::Null;
    };
    let shape = |s: Shape| match s {
        Shape::Round => "round",
        Shape::Square => "square",
        Shape::Wide => "wide",
    };
    let anchor = |a: Anchor| match a {
        Anchor::TopLeft => "top-left",
        Anchor::TopMiddle => "top-middle",
        Anchor::TopRight => "top-right",
        Anchor::LeftMiddle => "left-middle",
        Anchor::Center => "center",
        Anchor::RightMiddle => "right-middle",
        Anchor::BottomLeft => "bottom-left",
        Anchor::BottomMiddle => "bottom-middle",
        Anchor::BottomRight => "bottom-right",
    };
    let now = json!({
        "screen": w.screen.iter().map(|b| json!({
            "id": b.id, "screen": b.screen, "anchor": anchor(b.anchor),
            "x": b.x, "y": b.y, "width": b.width, "height": b.height, "label": b.label,
            "icon": b.icon, "tooltip": b.tooltip, "active": b.active,
        })).collect::<Vec<_>>(),
        "touch": w.touch.iter().map(|b| json!({
            "id": b.id, "label": b.label, "icon": b.icon, "shape": shape(b.shape),
            "x": b.x, "y": b.y, "size": b.size,
        })).collect::<Vec<_>>(),
        "styles": w.styles.iter().map(|s| json!({
            "control": s.control, "label": s.label, "icon": s.icon, "shape": shape(s.shape),
        })).collect::<Vec<_>>(),
    });
    let empty = ["screen", "touch", "styles"]
        .iter()
        .all(|k| now[k].as_array().is_some_and(Vec::is_empty));
    if now != *last && !(empty && last.is_null()) {
        let mut line = now.clone();
        line["event"] = json!("widgets");
        emit(line);
    }
    now
}

/// The mod's layers in the world (`world-draw`) as JSON, a `world` line when they are not
/// `last`: each layer's lines and boxes counted by colour (ARGB in hex) and its texts by text,
/// with the box they span, and its labels.
fn world_line(mods: &Mods, last: &mut Value) -> Value {
    let Some((_, layers)) = mods.world_layers().next() else {
        return Value::Null;
    };
    let layers =
        json!(layers
        .iter()
        .map(|(id, l)| {
            let mut colors = std::collections::BTreeMap::new();
            let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for lines in &l.lines {
                let o = [lines.origin.x, lines.origin.y, lines.origin.z];
                for (i, e) in lines.ends.chunks_exact(6).enumerate() {
                    let c = lines.colors.get(i).or(lines.colors.first()).copied().unwrap_or(0);
                    *colors.entry(format!("{c:08X}")).or_insert(0) += 1;
                    for (k, v) in e.iter().enumerate() {
                        let v = o[k % 3] + f64::from(*v);
                        min[k % 3] = min[k % 3].min(v);
                        max[k % 3] = max[k % 3].max(v);
                    }
                }
            }
            let mut boxes = std::collections::BTreeMap::new();
            for b in &l.boxes {
                let o = [b.origin.x, b.origin.y, b.origin.z];
                for (i, c) in b.corners.chunks_exact(6).enumerate() {
                    let argb = b.colors.get(i).or(b.colors.first()).copied().unwrap_or(0);
                    *boxes.entry(format!("{argb:08X}")).or_insert(0) += 1;
                    for (k, v) in c.iter().enumerate() {
                        let v = o[k % 3] + f64::from(*v);
                        min[k % 3] = min[k % 3].min(v);
                        max[k % 3] = max[k % 3].max(v);
                    }
                }
            }
            let mut texts = std::collections::BTreeMap::new();
            for t in &l.texts {
                for line in t.lines.split('\n') {
                    *texts.entry(line.to_owned()).or_insert(0) += 1;
                }
                let o = [t.origin.x, t.origin.y, t.origin.z];
                for (k, v) in t.places.iter().enumerate() {
                    let v = o[k % 3] + f64::from(*v);
                    min[k % 3] = min[k % 3].min(v);
                    max[k % 3] = max[k % 3].max(v);
                }
            }
            json!({
                "id": id, "through_walls": l.through_walls,
                "lines": colors, "boxes": boxes, "texts": texts,
                "from": min.iter().all(|v| v.is_finite()).then_some(min),
                "to": max.iter().all(|v| v.is_finite()).then_some(max),
                "labels": l.labels.iter().map(|b| json!({
                    "at": [b.at.x, b.at.y, b.at.z], "text": b.text, "scale": b.scale,
                    "fixed_size": b.fixed_size, "background": format!("{:08X}", b.background),
                    "facing": match b.facing { Facing::Camera => "camera", Facing::Up => "up" },
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>());
    // The icons (`set-icons`), each with its layer's id.
    let icons: Vec<Value> =
        mods.world_icons()
            .next()
            .map_or(&[][..], |(_, l)| l)
            .iter()
            .flat_map(|(id, icons)| {
                icons.iter().map(move |i| json!({
                "layer": id, "at": [i.at.x, i.at.y, i.at.z], "sprite": i.sprite,
                "color": format!("{:08X}", i.color), "size": i.size, "above": i.above,
                "below": i.below, "fixed_size": i.fixed_size, "through_walls": i.through_walls,
            }))
            })
            .collect();
    let empty = layers.as_array().is_some_and(Vec::is_empty) && icons.is_empty();
    let mut event = json!({ "event": "world", "layers": layers });
    if !icons.is_empty() {
        event["icons"] = json!(icons);
    }
    let now = json!([layers, icons]);
    if now != *last && !(empty && last.is_null()) {
        emit(event);
    }
    now
}

/// A screen button's box on a GUI this size, as the game places it.
fn anchored(b: &ScreenButton, gui: &Gui) -> [f32; 4] {
    let (fx, fy) = match b.anchor {
        Anchor::TopLeft => (0.0, 0.0),
        Anchor::TopMiddle => (0.5, 0.0),
        Anchor::TopRight => (1.0, 0.0),
        Anchor::LeftMiddle => (0.0, 0.5),
        Anchor::Center => (0.5, 0.5),
        Anchor::RightMiddle => (1.0, 0.5),
        Anchor::BottomLeft => (0.0, 1.0),
        Anchor::BottomMiddle => (0.5, 1.0),
        Anchor::BottomRight => (1.0, 1.0),
    };
    let (w, h) = (b.width.floor(), b.height.floor());
    let (sw, sh) = (gui.width as f32, gui.height as f32);
    [
        ((sw - w) * fx + b.x).floor(),
        ((sh - h) * fy + b.y).floor(),
        w,
        h,
    ]
}

/// The mod's buttons on game screen `screen`: owner, button, box.
fn screen_buttons(mods: &Mods, screen: &str, gui: &Gui) -> Vec<(String, ScreenButton, [f32; 4])> {
    mods.widgets()
        .flat_map(|(id, w)| w.screen.iter().map(move |b| (id, b)))
        .filter(|(_, b)| b.screen == screen)
        .map(|(id, b)| (id.to_owned(), b.clone(), anchored(b, gui)))
        .collect()
}

/// A game screen with the mod's buttons on it, as the game draws them (vanilla's look): a dark
/// backdrop for the screen itself, which is not drawn here.
fn screen_button_ops(mods: &Mods, screen: &str, gui: &Gui) -> Vec<DrawOp> {
    let (w, h) = (gui.width as f32, gui.height as f32);
    let mut ops = vec![DrawOp::Gradient {
        x: 0.0,
        y: 0.0,
        w,
        h,
        top: 0xC010_1010,
        bottom: 0xD010_1010,
    }];
    for (_, b, r) in screen_buttons(mods, screen, gui) {
        let side = (r[3] - 4.0).clamp(0.0, 16.0);
        let with_icon = !b.icon.is_empty();
        ops.push(DrawOp::Button {
            x: r[0],
            y: r[1],
            w: r[2],
            h: r[3],
            label: if with_icon {
                format!("   {}", b.label)
            } else {
                b.label.clone()
            },
            hovered: false,
            active: b.active,
        });
        if with_icon {
            ops.push(DrawOp::Sprite {
                x: r[0] + 3.0,
                y: r[1] + ((r[3] - side) / 2.0).floor(),
                w: side,
                h: side,
                sprite: b.icon.clone(),
            });
        }
    }
    ops
}

/// The mod's touch buttons where they start, on a GUI this size: plates with their icon or
/// label (the game draws them rounder).
fn touch_ops(mods: &Mods, gui: &Gui) -> Vec<DrawOp> {
    let (w, h) = (gui.width as f32, gui.height as f32);
    // A ninth of the shorter side, the controls' unit.
    let unit = w.min(h) / 9.0;
    let mut ops = Vec::new();
    for (_, widgets) in mods.widgets() {
        for b in &widgets.touch {
            let r = if b.shape == Shape::Wide {
                8.0
            } else {
                b.size * unit
            };
            let half = if b.shape == Shape::Wide {
                (b.label.chars().count() as f32 * 3.0 + 5.0).max(r)
            } else {
                r
            };
            let (cx, cy) = (b.x * w, b.y * h);
            ops.push(DrawOp::Fill {
                x: cx - half,
                y: cy - r,
                w: 2.0 * half,
                h: 2.0 * r,
                color: 0x40FF_FFFF,
            });
            if b.icon.is_empty() || b.shape == Shape::Wide {
                ops.push(DrawOp::Text {
                    x: cx - half + 3.0,
                    y: cy - 4.0,
                    text: b.label.clone(),
                    color: 0xFFFFFF,
                    shadow: true,
                });
            } else {
                let side = (2.0 * r * 0.6).round();
                ops.push(DrawOp::Sprite {
                    x: (cx - side / 2.0).round(),
                    y: (cy - side / 2.0).round(),
                    w: side,
                    h: side,
                    sprite: b.icon.clone(),
                });
            }
        }
    }
    ops
}

/// One step on the mod; what its hook returned, and what it drew.
fn play(
    mods: &mut Mods,
    step: &Value,
    sim: &mut Sim,
    ticks: &mut u64,
    gui: &Gui,
    data: &world::Data,
) -> (Value, Option<Vec<DrawOp>>) {
    let drawn = |ops: Vec<DrawOp>| (draw_ops(&ops), Some(ops));
    let context = |v: &Value| RenderContext {
        width: v["width"].as_u64().map_or(gui.width, |w| w as u32),
        height: v["height"].as_u64().map_or(gui.height, |h| h as u32),
        mouse_x: v["mouse"][0].as_f64().unwrap_or(0.0) as f32,
        mouse_y: v["mouse"][1].as_f64().unwrap_or(0.0) as f32,
        delta: 0.05,
    };
    if let Some(n) = step["tick"].as_u64() {
        for _ in 0..n {
            *ticks += 1;
            mods.tick(*ticks);
        }
        return (json!({ "ticks": *ticks }), None);
    }
    if let Some(seconds) = step["frame"].as_f64() {
        mods.frame(seconds as f32);
        return (Value::Null, None);
    }
    if let Some(line) = step["command"].as_str() {
        return (json!({ "handled": mods.command(line) }), None);
    }
    if let Some(text) = step["chat"].as_str() {
        let verdict = mods.chat(text, &json!({ "text": text }).to_string());
        return (json!({ "verdict": format!("{verdict:?}") }), None);
    }
    if let Some(text) = step["send"].as_str() {
        return (json!({ "sent": mods.chat_sending(text) }), None);
    }
    if step.get("hud").is_some() {
        let mut ops = panel_ops(mods, gui);
        ops.extend(mods.hud_render(context(&step["hud"])));
        if step["hud"]["touch"].as_bool() == Some(true) {
            ops.extend(touch_ops(mods, gui));
        }
        return drawn(ops);
    }
    if let Some(id) = step["touch"].as_str() {
        let Some(owner) = touch_owner(mods, id) else {
            return (json!({ "error": format!("no touch button {id}") }), None);
        };
        match step["pressed"].as_bool() {
            Some(pressed) => mods.keybind(&owner, id, pressed),
            None => {
                mods.keybind(&owner, id, true);
                mods.keybind(&owner, id, false);
            }
        }
        return (Value::Null, None);
    }
    if let Some(id) = step["tooltip"].as_str() {
        let info = data.info(id);
        let stack = ItemStack {
            id: id.into(),
            count: step["count"].as_u64().map_or(1, |c| c as u32),
            damage: 0,
            max_damage: info.as_ref().map_or(0, |i| i.max_damage),
            name: info.map_or_else(
                || id.rsplit(':').next().unwrap_or(id).replace('_', " "),
                |i| i.name,
            ),
            // As the game gives them: SNBT, e.g. {"minecraft:container":[...]}.
            components: step["components"].as_str().unwrap_or_default().into(),
        };
        let (lines, grids, instead) = mods.tooltip_grids(&stack);
        // The item's tooltip as the game shows it: its name, then the mod's lines.
        let shown = [vec![stack.name.clone()], lines.clone()].concat();
        let ops = vec![DrawOp::Tooltip {
            x: 20.0,
            y: 32.0,
            lines: shown,
        }];
        if grids.is_empty() {
            return (json!({ "lines": lines }), Some(ops));
        }
        // Its grids (`tooltips.add-grid`), each stack as [id, count] or null.
        let grids: Vec<Value> = grids
            .iter()
            .map(|g| {
                let stacks: Vec<Value> = g
                    .stacks
                    .iter()
                    .map(|s| s.as_ref().map_or(Value::Null, |s| json!([s.id, s.count])))
                    .collect();
                json!({ "columns": g.columns, "color": format!("#{:08x}", g.color), "stacks": stacks })
            })
            .collect();
        return (
            json!({ "lines": lines, "grids": grids, "instead": instead }),
            Some(ops),
        );
    }
    if let Some(id) = step["key"].as_str() {
        let pressed = step["pressed"].as_bool().unwrap_or(true);
        let Some((owner, _)) = mods.keybinds().into_iter().find(|(_, k)| k.id == id) else {
            return (json!({ "error": format!("no keybind {id}") }), None);
        };
        mods.keybind(&owner, id, pressed);
        return (Value::Null, None);
    }
    if let Some(server) = step.get("join") {
        mods.joined(server.as_str());
        return (Value::Null, None);
    }
    if step.get("leave").is_some() {
        mods.left();
        return (Value::Null, None);
    }
    if let Some(amount) = step["hurt"].as_f64() {
        mods.player_hurt(amount as f32);
        return (Value::Null, None);
    }
    if let Some(name) = step["screen"].as_str() {
        if sim.screen.as_deref() != Some(name) {
            mods.screen_opened(name);
            *sim.screen = Some(name.to_owned());
        }
        // A game screen: the mod's buttons on it, then what the mod draws over it. A screen with
        // slots (`container`, `inventory`) is drawn as a plain window with the menu's stacks and
        // the mod's window buttons.
        let mut ops = Vec::new();
        // Its window (`screens.get-window`), and the inventory's 2 by 2 grid for `recipes.fill`.
        let window = window::Window::of(name, sim.world.menu.as_ref(), gui);
        sim.world.window = window.as_ref().map(window::Window::rect);
        sim.world.crafting = (name == "inventory").then_some(horizon_mods::Crafting::Grid(2));
        mods.set_snapshot(sim.world.clone());
        if let Some(w) = window {
            let mouse = context(step);
            ops = w.ops(sim.world.menu.as_ref());
            ops.extend(window::button_ops(mods, &w, (mouse.mouse_x, mouse.mouse_y)));
            ops.extend(screen_button_ops(mods, name, gui).into_iter().skip(1));
        } else if !name.contains(':') {
            ops = screen_button_ops(mods, name, gui);
        }
        ops.extend(mods.screen_render(name, context(step)));
        return drawn(ops);
    }
    let Some(open) = sim.screen.clone() else {
        let error = format!("unknown step, or one for a screen while none is open: {step}");
        return (json!({ "error": error }), None);
    };
    let at = |v: &Value| -> Option<(f32, f32)> {
        let a = v.as_array()?;
        Some((a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
    };
    let button = step["button"].as_u64().unwrap_or(0) as u8;
    // A held button's release, a drag, the wheel: the mouse as the game sends it, then the
    // screen drawn with the mouse there.
    let mouse = |action, key: &str| {
        at(&step[key]).map(|(x, y)| MouseEvent {
            action,
            x,
            y,
            button,
            scroll: if action == MouseAction::Scroll {
                step["lines"].as_f64().unwrap_or(-1.0) as f32
            } else {
                0.0
            },
        })
    };
    for (key, action) in [
        ("release", MouseAction::Release),
        ("drag", MouseAction::Drag),
        ("scroll", MouseAction::Scroll),
    ] {
        if let Some(event) = mouse(action, key) {
            let handled = mods.screen_mouse(&open, event);
            sim.settle(mods);
            let mut context = context(step);
            (context.mouse_x, context.mouse_y) = (event.x, event.y);
            let (then, ops) = drawn(mods.screen_render(&open, context));
            return (json!({ "handled": handled, "then": then }), ops);
        }
    }
    if let Some(name) = step["key-press"].as_str() {
        let event = KeyEvent {
            key: name.into(),
            pressed: step["pressed"].as_bool().unwrap_or(true),
            text: String::new(),
            shift: false,
            control: false,
            alt: false,
        };
        let handled = mods.screen_key(&open, &event);
        sim.settle(mods);
        return (json!({ "handled": handled }), None);
    }
    if let Some([x, y]) = step["click"].as_array().map(|a| a.as_slice()) {
        let event = |action| MouseEvent {
            action,
            x: x.as_f64().unwrap_or(0.0) as f32,
            y: y.as_f64().unwrap_or(0.0) as f32,
            button,
            scroll: 0.0,
        };
        // The mod's buttons on a game screen take a left click first, as in the game: its window
        // buttons on a screen with slots, then its screen buttons.
        let at = (event(MouseAction::Press).x, event(MouseAction::Press).y);
        let window = window::Window::of(&open, sim.world.menu.as_ref(), gui);
        let pressed = window.as_ref().and_then(|w| {
            window::buttons(mods, w)
                .into_iter()
                .rev()
                .find(|(_, _, r)| {
                    at.0 >= r[0] && at.0 < r[0] + r[2] && at.1 >= r[1] && at.1 < r[1] + r[3]
                })
        });
        if let (Some((owner, b, _)), 0) = (pressed, button) {
            mods.keybind(&owner, &b.id, true);
            mods.keybind(&owner, &b.id, false);
            sim.settle(mods);
            return (json!({ "button": b.id }), None);
        }
        let hit = screen_buttons(mods, &open, gui)
            .into_iter()
            .rev()
            .find(|(_, _, r)| {
                at.0 >= r[0] && at.0 < r[0] + r[2] && at.1 >= r[1] && at.1 < r[1] + r[3]
            });
        if let (Some((owner, b, _)), 0) = (hit, button) {
            if b.active {
                mods.keybind(&owner, &b.id, true);
                mods.keybind(&owner, &b.id, false);
            }
            return (json!({ "button": b.id, "active": b.active }), None);
        }
        let handled = mods.screen_mouse(&open, event(MouseAction::Press));
        if step["press"].as_bool() != Some(false) {
            mods.screen_mouse(&open, event(MouseAction::Release));
        }
        sim.settle(mods);
        let mut context = context(step);
        (context.mouse_x, context.mouse_y) = at;
        let (then, ops) = drawn(mods.screen_render(&open, context));
        return (json!({ "handled": handled, "then": then }), ops);
    }
    let key = |key: &str, text: &str| KeyEvent {
        key: key.into(),
        pressed: true,
        text: text.into(),
        shift: false,
        control: false,
        alt: false,
    };
    if let Some(text) = step["type"].as_str() {
        for c in text.chars() {
            mods.screen_key(
                &open,
                &key(
                    &format!("key.keyboard.{}", c.to_ascii_lowercase()),
                    &c.to_string(),
                ),
            );
        }
        sim.settle(mods);
        return drawn(mods.screen_render(&open, context(step)));
    }
    if let Some(name) = step["press"].as_str() {
        let handled = mods.screen_key(&open, &key(name, ""));
        sim.settle(mods);
        let (then, ops) = drawn(mods.screen_render(&open, context(step)));
        return (json!({ "handled": handled, "then": then }), ops);
    }
    if step.get("close").is_some() {
        mods.screen_closed(&open);
        *sim.screen = None;
        sim.world.window = None;
        sim.world.crafting = None;
        return (Value::Null, None);
    }
    (json!({ "error": format!("unknown step: {step}") }), None)
}

/// The mod owning touch button `id`: one of its own, or a keybind's (each has one).
fn touch_owner(mods: &Mods, id: &str) -> Option<String> {
    mods.widgets()
        .find(|(_, w)| w.touch.iter().any(|b| b.id == id))
        .map(|(owner, _)| owner.to_owned())
        .or_else(|| {
            mods.keybinds()
                .into_iter()
                .find(|(_, k)| k.id == id)
                .map(|(owner, _)| owner)
        })
}

/// The mods' HUD panels (`hud.set-panel`) as the game draws them: lines on a dark box in their
/// corner, each panel below (or above) the last in the same corner.
fn panel_ops(mods: &Mods, gui: &Gui) -> Vec<DrawOp> {
    const MARGIN: f32 = 2.0;
    let measure = mods.snapshot().measure.clone();
    let width = |text: &str| {
        measure
            .as_ref()
            .map_or(text.chars().count() as f32 * 6.0, |m| m(text))
    };
    let mut used = [0.0f32; 4];
    let mut ops = Vec::new();
    for panel in mods.panels().filter(|p| !p.lines.is_empty()) {
        let (i, left, top) = match panel.corner {
            Corner::TopLeft => (0, true, true),
            Corner::TopRight => (1, false, true),
            Corner::BottomLeft => (2, true, false),
            Corner::BottomRight => (3, false, false),
        };
        let w = panel
            .lines
            .iter()
            .map(|l| width(&l.text))
            .fold(0.0, f32::max)
            + 4.0;
        let h = 10.0 * panel.lines.len() as f32 + 2.0;
        let edge = used[i] + MARGIN;
        used[i] = edge + h;
        let x = if left {
            MARGIN
        } else {
            gui.width as f32 - MARGIN - w
        };
        let y = if top {
            edge
        } else {
            gui.height as f32 - edge - h
        };
        ops.push(DrawOp::Fill {
            x,
            y,
            w,
            h,
            color: 0x7800_0000,
        });
        for (n, line) in panel.lines.iter().enumerate() {
            ops.push(DrawOp::Text {
                x: x + 2.0,
                y: y + 2.0 + 10.0 * n as f32,
                text: line.text.clone(),
                color: line.color,
                shadow: true,
            });
        }
    }
    ops
}

/// What a render hook drew, an object a call.
fn draw_ops(ops: &[DrawOp]) -> Value {
    ops.iter()
        .map(|op| match op {
            DrawOp::Fill { x, y, w, h, color } => json!({ "fill": [x, y, w, h], "color": format!("#{color:08x}") }),
            DrawOp::Gradient { x, y, w, h, top, bottom } => json!({ "gradient": [x, y, w, h], "top": format!("#{top:08x}"), "bottom": format!("#{bottom:08x}") }),
            DrawOp::Text { x, y, text, color, shadow } => json!({ "text": text, "at": [x, y], "color": format!("#{color:06x}"), "shadow": shadow }),
            DrawOp::Item { x, y, stack } => json!({ "item": stack.id, "count": stack.count, "at": [x, y] }),
            DrawOp::ItemId { x, y, id } => json!({ "item": id, "at": [x, y] }),
            DrawOp::Sprite { x, y, w, h, sprite } => json!({ "sprite": sprite, "rect": [x, y, w, h] }),
            DrawOp::Tooltip { x, y, lines } => json!({ "tooltip": lines, "at": [x, y] }),
            DrawOp::Button { x, y, w, h, label, hovered, active } => json!({ "button": label, "rect": [x, y, w, h], "hovered": hovered, "active": active }),
            DrawOp::Panel { x, y, w, h } => json!({ "panel": [x, y, w, h] }),
            DrawOp::Push { x, y, scale } => json!({ "push": [x, y], "scale": scale }),
            DrawOp::Pop => json!({ "pop": true }),
            DrawOp::Player { x, y, w, h, size, turn } => json!({ "player": [x, y, w, h], "size": size, "turn": turn }),
            DrawOp::ItemTooltip { x, y, stack } => json!({ "item-tooltip": stack.id, "at": [x, y] }),
        })
        .collect()
}
