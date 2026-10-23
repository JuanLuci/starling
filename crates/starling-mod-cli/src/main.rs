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
//! `world::world`. A step is one of:
//! `{"tick": 20}`, `{"frame": 0.05}`, `{"command": "hello world"}`, `{"chat": "text"}`,
//! `{"send": "text"}`, `{"hud": {}}`, `{"tooltip": "minecraft:stick"}`,
//! `{"key": "<keybind id>", "pressed": true}`, `{"screen": "<mod id>:<name>"}` (opens and draws
//! it), `{"click": [x, y], "button": 0}`, `{"type": "text"}`, `{"press": "key.keyboard.enter"}`,
//! `{"close": true}`, `{"join": "server"}`, `{"leave": true}`, `{"hurt": 2.0}`,
//! `{"controls": ["key.forward", "key.attack"]}` (the game's controls held from then on, the
//! others let go; mods' `input.hold` adds sprint and sneak).
//! Without steps it ticks once and draws the HUD.
//!
//! Each run starts with the mod's storage empty; `--storage <dir>` keeps it there between runs.
//! `--watch` runs it again whenever the mod (a `.hmod`, or a folder's files) or the scenario
//! changes, with a `{"event": "changed"}` line before each new run, until stopped.
//! `--resources <dir>` is a prepared resources folder (the one holding `items.json` and
//! `assets/minecraft`): the pretend world gets its items and its font's text widths.
//! `--png <dir>` also draws each step that drew something to `<dir>/step<N>.png` (its path in the
//! step's line), with that folder's font and sprites, or plain shapes without one.
mod render;
mod world;

use horizon_mods::{
    CONTROLS, Controls, Corner, DrawOp, Gui, ItemCatalog, ItemStack, KeyEvent, Mods, MouseAction,
    MouseEvent, Output, RenderContext, Snapshot, Status,
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
    outputs(&mut mods, &mut None, &mut hands);
    if status != "running" {
        return Ok(());
    }
    let gui = Gui {
        width: scenario["gui"]["width"].as_u64().unwrap_or(427) as u32,
        height: scenario["gui"]["height"].as_u64().unwrap_or(240) as u32,
        scale: scenario["gui"]["scale"].as_f64().unwrap_or(2.0) as f32,
    };
    let data = options.resources.as_deref().and_then(world::Data::read);
    let data = Arc::new(data.unwrap_or_default());
    let mut resources = render::Resources::new(options.resources.clone());
    let mut world = world::world(&scenario, data.clone());
    world.gui = Some(gui);
    world.measure = resources.measure();
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
            None => play(&mut mods, &step, &mut screen, &mut ticks, &gui, &data),
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
        outputs(&mut mods, &mut screen, &mut hands);
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

/// What the mod asked of the game since the last look, a line each; a screen it opened or
/// closed is followed, and so are sprint and sneak held.
fn outputs(mods: &mut Mods, screen: &mut Option<String>, hands: &mut Hands) {
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
            Output::Hold { control, held, .. } => {
                hands.held[usize::from(control == "key.sprint")] = held;
                json!({ "event": "hold", "control": control, "held": held })
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
        return drawn(ops);
    }
    if let Some(id) = step["tooltip"].as_str() {
        let info = data.info(id);
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
        })
        .collect()
}
