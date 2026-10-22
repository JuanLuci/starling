//! `starling-mod`: run a Starling mod without the game, in a pretend world, and see what it does.
//!
//! `starling-mod run <mod> [--scenario <file.json>] [--step <json>]...` loads the mod (a folder
//! with `horizon-mod.toml`, or a `.hmod`) as the game would and plays steps on it: ticks,
//! commands, chat, keys, tooltips, its HUD and screens. Each thing that happens is a line of
//! JSON on stdout, for a person or a program (an AI fixing the mod) to read.
//!
//! A scenario is `{"player": {...}, "gui": {"width": 427, "height": 240}, "steps": [...]}` (the
//! GUI's size in GUI pixels, the HUD and screens drawn at it); a step is one of:
//! `{"tick": 20}`, `{"frame": 0.05}`, `{"command": "hello world"}`, `{"chat": "text"}`,
//! `{"send": "text"}`, `{"hud": {}}`, `{"tooltip": "minecraft:stick"}`,
//! `{"key": "<keybind id>", "pressed": true}`, `{"screen": "<mod id>:<name>"}` (opens and draws
//! it), `{"click": [x, y], "button": 0}`, `{"type": "text"}`, `{"press": "key.keyboard.enter"}`,
//! `{"close": true}`, `{"join": "server"}`, `{"leave": true}`, `{"hurt": 2.0}`.
//! Without steps it ticks once and draws the HUD.
//!
//! Each run starts with the mod's storage empty; `--storage <dir>` keeps it there between runs.
//! `--resources <dir>` is a prepared resources folder (the one holding `items.json` and
//! `assets/minecraft`): the pretend world gets its items and its font's text widths.
//! `--png <dir>` also draws each step that drew something to `<dir>/step<N>.png` (its path in the
//! step's line), with that folder's font and sprites, or plain shapes without one.
mod render;

use horizon_mods::{
    Clock, DrawOp, Food, GameMode, Gui, ItemCatalog, ItemInfo, ItemStack, KeyEvent, Mods,
    MouseAction, MouseEvent, Output, Player, RenderContext, Snapshot, Status,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

const USAGE: &str = "usage: starling-mod run <mod folder or .hmod> [--scenario <file.json>] \
    [--step <json>]... [--storage <dir>] [--resources <dir>] [--png <dir>]";

fn main() -> ExitCode {
    log::set_logger(&JSON_LOG).expect("the only logger");
    log::set_max_level(log::LevelFilter::Info);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        _ => Err(USAGE.into()),
    };
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

fn run(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .ok_or(USAGE)?;
    let mut scenario = json!({});
    let mut steps: Vec<Value> = Vec::new();
    let (mut png, mut resources, mut storage) = (None, None, None);
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--scenario" => {
                let text = std::fs::read_to_string(value).map_err(|e| format!("{value}: {e}"))?;
                scenario = serde_json::from_str(&text).map_err(|e| format!("{value}: {e}"))?;
                if let Some(list) = scenario["steps"].as_array() {
                    steps.extend(list.iter().cloned());
                }
            }
            "--step" => {
                steps.push(serde_json::from_str(value).map_err(|e| format!("--step {value}: {e}"))?)
            }
            "--png" => png = Some(PathBuf::from(value)),
            "--resources" => resources = Some(PathBuf::from(value)),
            "--storage" => storage = Some(PathBuf::from(value)),
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    if steps.is_empty() {
        steps = vec![json!({ "tick": 1 }), json!({ "hud": {} })];
    }

    // A mod keeps its storage beside itself (`.horizon-mod-data`), so it runs from a copy in the
    // storage folder: a new one for this run unless `--storage` names one.
    let place = Place::new(storage)?;
    let copy = place.dir.join(path.file_name().ok_or(USAGE)?);
    copy_all(&path, &copy)?;

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
    }));
    outputs(&mut mods, &mut None);
    if status != "running" {
        return Ok(());
    }
    let gui = Gui {
        width: scenario["gui"]["width"].as_u64().unwrap_or(427) as u32,
        height: scenario["gui"]["height"].as_u64().unwrap_or(240) as u32,
        scale: scenario["gui"]["scale"].as_f64().unwrap_or(2.0) as f32,
    };
    let items = resources.as_deref().and_then(catalog).map(Arc::new);
    let mut resources = render::Resources::new(resources);
    let mut world = snapshot(&scenario["player"]);
    world.gui = Some(gui);
    world.items = items.clone().map(|c| c as Arc<dyn ItemCatalog>);
    world.measure = resources.measure();
    mods.set_snapshot(world);
    let mut screen: Option<String> = None;
    let mut ticks = 0;
    if let Some(dir) = &png {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        if !resources.has_font() {
            emit(
                json!({ "event": "note", "text": "no font in --resources: text is drawn as boxes" }),
            );
        }
    }
    let items = items.as_deref();
    for (n, step) in steps.into_iter().enumerate() {
        let (result, drawn) = play(&mut mods, &step, &mut screen, &mut ticks, &gui, items);
        let file = match (&png, drawn) {
            (Some(dir), Some(ops)) if !ops.is_empty() => {
                let scale = gui.scale.round().max(1.0) as u32;
                let file = dir.join(format!("step{n}.png"));
                render::render(&ops, gui.width, gui.height, scale, &mut resources).save(&file)?;
                Some(file)
            }
            _ => None,
        };
        emit(json!({ "event": "step", "step": step, "result": result, "png": file }));
        outputs(&mut mods, &mut screen);
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

/// What the mod asked of the game since the last look, a line each; a screen it opened or
/// closed is followed.
fn outputs(mods: &mut Mods, screen: &mut Option<String>) {
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
        };
        emit(line);
    }
}

/// One step on the mod; what its hook returned, and what it drew.
fn play(
    mods: &mut Mods,
    step: &Value,
    screen: &mut Option<String>,
    ticks: &mut u64,
    gui: &Gui,
    items: Option<&Catalog>,
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
        return drawn(mods.hud_render(context(&step["hud"])));
    }
    if let Some(id) = step["tooltip"].as_str() {
        let info = items.and_then(|c| c.info(id));
        let stack = ItemStack {
            id: id.into(),
            count: 1,
            damage: 0,
            max_damage: info.as_ref().map_or(0, |i| i.max_damage),
            name: info.map_or_else(
                || id.rsplit(':').next().unwrap_or(id).replace('_', " "),
                |i| i.name,
            ),
            components: String::new(),
        };
        let lines = mods.tooltip(&stack);
        // The item's tooltip as the game shows it: its name, then the mod's lines.
        let shown = [vec![stack.name.clone()], lines.clone()].concat();
        let ops = vec![DrawOp::Tooltip {
            x: 20.0,
            y: 32.0,
            lines: shown,
        }];
        return (json!({ "lines": lines }), Some(ops));
    }
    if let Some(id) = step["key"].as_str() {
        let pressed = step["pressed"].as_bool().unwrap_or(true);
        let Some((owner, _)) = mods.keybinds().into_iter().find(|(_, k)| k.id == id) else {
            return (json!({ "error": format!("no keybind {id}") }), None);
        };
        mods.key(&owner, id, pressed);
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
        if screen.as_deref() != Some(name) {
            mods.screen_opened(name);
            *screen = Some(name.to_owned());
        }
        return drawn(mods.screen_render(name, context(step)));
    }
    let Some(open) = screen.clone() else {
        let error = format!("unknown step, or one for a screen while none is open: {step}");
        return (json!({ "error": error }), None);
    };
    if let Some([x, y]) = step["click"].as_array().map(|a| a.as_slice()) {
        let button = step["button"].as_u64().unwrap_or(0) as u8;
        let event = |action| MouseEvent {
            action,
            x: x.as_f64().unwrap_or(0.0) as f32,
            y: y.as_f64().unwrap_or(0.0) as f32,
            button,
            scroll: 0.0,
        };
        let handled = mods.screen_mouse(&open, event(MouseAction::Press));
        mods.screen_mouse(&open, event(MouseAction::Release));
        let (then, ops) = drawn(mods.screen_render(&open, context(step)));
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
        return drawn(mods.screen_render(&open, context(step)));
    }
    if let Some(name) = step["press"].as_str() {
        let handled = mods.screen_key(&open, &key(name, ""));
        let (then, ops) = drawn(mods.screen_render(&open, context(step)));
        return (json!({ "handled": handled, "then": then }), ops);
    }
    if step.get("close").is_some() {
        mods.screen_closed(&open);
        *screen = None;
        return (Value::Null, None);
    }
    (json!({ "error": format!("unknown step: {step}") }), None)
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
        })
        .collect()
}

/// The game's items, from a prepared resources folder's `items.json`, named from its
/// `en_us.json`.
struct Catalog(HashMap<String, ItemInfo>);

impl ItemCatalog for Catalog {
    fn info(&self, id: &str) -> Option<ItemInfo> {
        self.0.get(id).cloned()
    }
}

fn catalog(root: &Path) -> Option<Catalog> {
    let read = |path: &str| -> Option<Value> {
        serde_json::from_slice(&std::fs::read(root.join(path)).ok()?).ok()
    };
    let items = read("items.json")?;
    let lang = read("assets/minecraft/lang/en_us.json").unwrap_or_default();
    let items = items.as_object()?.iter().map(|(id, c)| {
        let key = c["minecraft:item_name"]["translate"]
            .as_str()
            .unwrap_or_default();
        let food = c.get("minecraft:food").map(|f| Food {
            nutrition: f["nutrition"].as_u64().unwrap_or(0) as u32,
            saturation: f["saturation"].as_f64().unwrap_or(0.0) as f32,
            can_always_eat: f["can_always_eat"].as_bool().unwrap_or(false),
        });
        let info = ItemInfo {
            id: id.clone(),
            name: lang[key].as_str().unwrap_or(id).to_owned(),
            max_stack_size: c["minecraft:max_stack_size"].as_u64().unwrap_or(64) as u32,
            max_damage: c["minecraft:max_damage"].as_u64().unwrap_or(0) as u32,
            food,
        };
        (id.clone(), info)
    });
    Some(Catalog(items.collect()))
}

/// The pretend world: a player standing in the overworld at noon, with the scenario's
/// `player` fields over the defaults.
fn snapshot(over: &Value) -> Snapshot {
    let f = |name: &str, default: f64| over[name].as_f64().unwrap_or(default);
    let u = |name: &str, default: u64| over[name].as_u64().unwrap_or(default);
    Snapshot {
        player: Some(Player {
            x: f("x", 0.5),
            y: f("y", 64.0),
            z: f("z", 0.5),
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            yaw: f("yaw", 0.0) as f32,
            pitch: f("pitch", 0.0) as f32,
            health: f("health", 20.0) as f32,
            max_health: 20.0,
            absorption: 0.0,
            food: u("food", 20) as u32,
            saturation: f("saturation", 5.0) as f32,
            exhaustion: 0.0,
            air: 300,
            max_air: 300,
            armor: u("armor", 0) as u32,
            experience_level: u("level", 0) as u32,
            experience_progress: 0.0,
            selected_slot: 0,
            game_mode: match over["game_mode"].as_str() {
                Some("creative") => GameMode::Creative,
                Some("adventure") => GameMode::Adventure,
                Some("spectator") => GameMode::Spectator,
                _ => GameMode::Survival,
            },
            on_ground: true,
            sneaking: false,
            sprinting: false,
            swimming: false,
            flying: false,
            gliding: false,
            dimension: over["dimension"]
                .as_str()
                .unwrap_or("minecraft:overworld")
                .into(),
            name: over["name"].as_str().unwrap_or("Player").into(),
            uuid: "00000000-0000-0000-0000-000000000000".into(),
        }),
        clock: Some(Clock {
            game_time: 0,
            day_time: u("day_time", 6000),
        }),
        touch: over["touch"].as_bool().unwrap_or(false),
        ..Snapshot::default()
    }
}
