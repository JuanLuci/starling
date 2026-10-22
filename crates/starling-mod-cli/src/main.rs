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
use horizon_mods::{
    Clock, DrawOp, GameMode, Gui, ItemStack, KeyEvent, Mods, MouseAction, MouseEvent, Output,
    Player, RenderContext, Snapshot, Status,
};
use serde_json::{Value, json};
use std::{path::PathBuf, process::ExitCode, time::Instant};

const USAGE: &str =
    "usage: starling-mod run <mod folder or .hmod> [--scenario <file.json>] [--step <json>]...";

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
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    if steps.is_empty() {
        steps = vec![json!({ "tick": 1 }), json!({ "hud": {} })];
    }

    // The engine's compiled code is kept between runs, as the game keeps it.
    let cache = std::env::temp_dir().join("starling-mod-cache");
    let started = Instant::now();
    let mut mods = Mods::load_cached(
        std::slice::from_ref(&path),
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
    let mut world = snapshot(&scenario["player"]);
    world.gui = Some(gui);
    mods.set_snapshot(world);
    let mut screen: Option<String> = None;
    let mut ticks = 0;
    for step in steps {
        let result = play(&mut mods, &step, &mut screen, &mut ticks, &gui);
        emit(json!({ "event": "step", "step": step, "result": result }));
        outputs(&mut mods, &mut screen);
        if !mods.is_running() {
            break;
        }
    }
    Ok(())
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

/// One step on the mod; what its hook returned.
fn play(
    mods: &mut Mods,
    step: &Value,
    screen: &mut Option<String>,
    ticks: &mut u64,
    gui: &Gui,
) -> Value {
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
        return json!({ "ticks": *ticks });
    }
    if let Some(seconds) = step["frame"].as_f64() {
        mods.frame(seconds as f32);
        return Value::Null;
    }
    if let Some(line) = step["command"].as_str() {
        return json!({ "handled": mods.command(line) });
    }
    if let Some(text) = step["chat"].as_str() {
        return json!({ "verdict": format!("{:?}", mods.chat(text, &json!({ "text": text }).to_string())) });
    }
    if let Some(text) = step["send"].as_str() {
        return json!({ "sent": mods.chat_sending(text) });
    }
    if step.get("hud").is_some() {
        return draw_ops(&mods.hud_render(context(&step["hud"])));
    }
    if let Some(id) = step["tooltip"].as_str() {
        let stack = ItemStack {
            id: id.into(),
            count: 1,
            damage: 0,
            max_damage: 0,
            name: id.rsplit(':').next().unwrap_or(id).replace('_', " "),
            components: String::new(),
        };
        return json!({ "lines": mods.tooltip(&stack) });
    }
    if let Some(id) = step["key"].as_str() {
        let pressed = step["pressed"].as_bool().unwrap_or(true);
        let Some((owner, _)) = mods.keybinds().into_iter().find(|(_, k)| k.id == id) else {
            return json!({ "error": format!("no keybind {id}") });
        };
        mods.key(&owner, id, pressed);
        return Value::Null;
    }
    if let Some(name) = step["screen"].as_str() {
        if screen.as_deref() != Some(name) {
            mods.screen_opened(name);
            *screen = Some(name.to_owned());
        }
        return draw_ops(&mods.screen_render(name, context(step)));
    }
    let Some(open) = screen.clone() else {
        return json!({ "error": format!("unknown step, or one for a screen while none is open: {step}") });
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
        return json!({ "handled": handled, "then": draw_ops(&mods.screen_render(&open, context(step))) });
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
        return draw_ops(&mods.screen_render(&open, context(step)));
    }
    if let Some(name) = step["press"].as_str() {
        let handled = mods.screen_key(&open, &key(name, ""));
        return json!({ "handled": handled, "then": draw_ops(&mods.screen_render(&open, context(step))) });
    }
    if step.get("close").is_some() {
        mods.screen_closed(&open);
        *screen = None;
        return Value::Null;
    }
    json!({ "error": format!("unknown step: {step}") })
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
