//! Loads real mods built with the SDKs: Horizon's coords-hud and predator-missile examples,
//! Starling's, and guests that misbehave on command. Needs
//! `rustup target add wasm32-unknown-unknown`.
use horizon_mods::{
    ChatVerdict, Clock, Corner, DrawOp, GameMode, Hand, HeldItem, Hit, HudPart, ItemStack, Mods,
    MouseAction, MouseEvent, Output, Package, Placement, Player, RenderContext, Runs,
    ServerBlockPos, ServerGameMode, ServerPlayer, Shape, Snapshot, Status, WorldInfo,
    manifest::componentize,
    testing::{Script, TestGame},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Instant,
};

const GUEST_MANIFEST: &str = r#"
id = "test_guest"
name = "Test Guest"
version = "0.1.0"
api = "starling-0.1"
capabilities = ["chat", "storage", "hud", "ui", "chat-send", "net", "game-read"]
"#;
const UI_MANIFEST: &str = r#"
id = "test_ui_guest"
name = "Test UI Guest"
version = "0.1.0"
api = "0.1"
capabilities = ["game-read", "chat", "command", "ui", "render", "input", "recipes"]
experimental = ["ui", "render", "input", "recipes"]
"#;

/// Builds the test mods once and writes their `.hmod` files; returns the folder.
fn mods_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = root.join("target/mods-test");
        let status = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
            .args([
                "-p",
                "coords-hud",
                "-p",
                "predator-missile",
                "-p",
                "welcome",
                "-p",
                "beacons",
                "-p",
                "food-info",
                "-p",
                "waypoints",
                "-p",
                "toggle-sprint",
                "-p",
                "horizon-mods-test-guest",
                "-p",
                "horizon-mods-test-ui-guest",
                "--target-dir",
            ])
            .arg(&target)
            .status()
            .expect("cargo runs");
        assert!(
            status.success(),
            "building the test mods needs `rustup target add wasm32-unknown-unknown`"
        );
        let built = target.join("wasm32-unknown-unknown/release");
        let dir = target.join("mods");
        std::fs::create_dir_all(&dir).unwrap();
        // coords-hud and predator-missile are Horizon's own (API 0.1), welcome and beacons its
        // server mods (API 0.3); the others Starling's.
        let example = |sdk: &str, name: &str| {
            let path = format!("crates/{sdk}/examples/{name}/horizon-mod.toml");
            std::fs::read_to_string(root.join(path)).unwrap()
        };
        let mods = [
            ("coords_hud", example("horizon-mod-sdk", "coords-hud")),
            (
                "predator_missile",
                example("horizon-mod-sdk", "predator-missile"),
            ),
            ("welcome", example("horizon-mod-sdk", "welcome")),
            ("beacons", example("horizon-mod-sdk", "beacons")),
            ("food_info", example("starling-mod-sdk", "food-info")),
            ("waypoints", example("starling-mod-sdk", "waypoints")),
            (
                "toggle_sprint",
                example("starling-mod-sdk", "toggle-sprint"),
            ),
            ("horizon_mods_test_guest", GUEST_MANIFEST.to_string()),
            ("horizon_mods_test_ui_guest", UI_MANIFEST.to_string()),
        ];
        for (name, manifest) in mods {
            let core = std::fs::read(built.join(format!("{name}.wasm"))).unwrap();
            let hmod = Package::write(&manifest, &componentize(&core).unwrap()).unwrap();
            std::fs::write(dir.join(format!("{name}.hmod")), hmod).unwrap();
        }
        dir
    })
}

fn load(name: &str) -> Mods {
    Mods::load(&[mods_dir().join(name)], |_| true, |c| c == "give")
}

fn player() -> Snapshot {
    Snapshot {
        player: Some(Player {
            x: 12.5,
            y: 64.0,
            z: -3.25,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            yaw: 180.0,
            pitch: 0.0,
            health: 20.0,
            max_health: 20.0,
            absorption: 0.0,
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            air: 300,
            max_air: 300,
            armor: 0,
            experience_level: 0,
            experience_progress: 0.0,
            selected_slot: 0,
            game_mode: GameMode::Survival,
            on_ground: true,
            sneaking: false,
            sprinting: false,
            swimming: false,
            flying: false,
            gliding: false,
            dimension: "minecraft:overworld".into(),
            name: "Player".into(),
            uuid: "00000000-0000-0000-0000-000000000000".into(),
        }),
        clock: Some(Clock {
            game_time: 100,
            day_time: 24000 + 6000,
        }),
        gui_size: (427.0, 240.0),
        ..Snapshot::default()
    }
}

fn context() -> RenderContext {
    RenderContext {
        width: 320,
        height: 240,
        mouse_x: 20.0,
        mouse_y: 15.0,
        delta: 0.016,
    }
}

/// Capability `command`: `/missile` queues vanilla commands, and the chat
/// line its impact check prints sets off the warhead.
#[test]
fn predator_missile_runs_commands() {
    let mut mods = load("predator_missile.hmod");
    assert_eq!(mods.entries()[0].status, Status::Running);
    mods.set_snapshot(player());
    let commands = |mods: &mut Mods| -> Vec<String> {
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Command { line } => Some(line),
                _ => None,
            })
            .collect()
    };
    assert!(mods.command("missile"));
    let launch = commands(&mut mods);
    assert_eq!(launch.len(), 6);
    assert!(launch.contains(&"gamemode spectator".to_string()));
    mods.tick(1);
    mods.tick(2);
    let flight = commands(&mut mods);
    assert!(flight.iter().any(|l| l.ends_with("run tp @s ^ ^ ^1.5")));
    mods.chat("Predator: impact", "{}");
    let blast = commands(&mut mods);
    assert_eq!(blast.len(), 4);
    assert!(blast.iter().all(|l| l.contains("summon tnt")));
}

#[test]
fn coords_hud_draws_and_answers() {
    let mut mods = load("coords_hud.hmod");
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert_eq!(mods.commands().collect::<Vec<_>>(), ["where", "coords"]);
    mods.set_snapshot(player());
    mods.tick(1);
    let panel = mods.panels().next().expect("a panel after a tick");
    assert_eq!(panel.corner, Corner::TopLeft);
    let lines: Vec<&str> = panel.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        lines,
        [
            "XYZ 12.5 / 64.0 / -3.2",
            "Facing North (-Z)",
            "Day 2, 12:00"
        ]
    );

    assert!(mods.command("where"));
    let chat = Output::Chat {
        name: "Coordinates HUD".into(),
        text: "12, 64, -4 in minecraft:overworld".into(),
    };
    assert_eq!(mods.drain(), [chat]);
    assert!(mods.command("coords"));
    mods.tick(2);
    assert_eq!(mods.panels().count(), 0);
    let hidden = Output::Feedback {
        text: "Coordinates panel hidden".into(),
        error: false,
    };
    assert_eq!(mods.drain(), [hidden]);
    assert!(!mods.command("gamemode creative"));
}

/// A mod built with the 0.1 SDK (the fixture is coords-hud as API 0.1 built it) runs on the
/// 0.1 interfaces, as before.
#[test]
fn api_0_1_mods_still_run() {
    let dir = mods_dir().join("api_0_1");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("coords_hud.hmod");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coords_hud-api-0.1.hmod"),
        &path,
    )
    .unwrap();
    let mut mods = Mods::load(&[path], |_| true, |c| c == "give");
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert_eq!(mods.commands().collect::<Vec<_>>(), ["where", "coords"]);
    mods.set_snapshot(player());
    mods.tick(1);
    let panel = mods.panels().next().expect("a panel after a tick");
    let lines: Vec<&str> = panel.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        lines,
        [
            "XYZ 12.5 / 64.0 / -3.2",
            "Facing North (-Z)",
            "Day 2, 12:00"
        ]
    );
    assert!(mods.command("where"));
    let chat = Output::Chat {
        name: "Coordinates HUD".into(),
        text: "12, 64, -4 in minecraft:overworld".into(),
    };
    assert_eq!(mods.drain(), [chat]);
    // Starling's hooks are not 0.1's: nothing to draw, every action allowed.
    assert!(mods.hud_render(context()).is_empty());
    assert!(mods.use_item(Hand::OffHand));
}

#[test]
fn hud_and_element_hooks_draw() {
    let mut mods = load("horizon_mods_test_guest.hmod");
    mods.set_snapshot(player());
    let ops = mods.hud_render(context());
    assert!(matches!(
        ops[0],
        DrawOp::Fill {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
            color: 0xFF0000
        }
    ));
    assert!(matches!(&ops[1], DrawOp::Text { text, shadow: true, .. } if text == "hi"));
    // Without the game's font, a character is 6 pixels.
    assert!(matches!(
        ops[2],
        DrawOp::Fill {
            x: 12.0,
            w: 320.0,
            ..
        }
    ));
    assert!(mods.has_hud_element(HudPart::Hotbar, Placement::After));
    assert!(mods.has_hud_element(HudPart::Food, Placement::Replace));
    assert!(!mods.has_hud_element(HudPart::Food, Placement::Before));
    let ops = mods.hud_elements(HudPart::Hotbar, Placement::After, context());
    assert!(matches!(&ops[..], [DrawOp::Sprite { sprite, .. }] if sprite == "hud/after_hotbar"));
    // Drawing outside a render hook draws nothing.
    assert!(mods.command("hello"));
    assert!(
        mods.hud_elements(HudPart::Health, Placement::After, context())
            .is_empty()
    );
}

#[test]
fn a_mod_screen_draws_and_takes_clicks() {
    let mut mods = load("horizon_mods_test_guest.hmod");
    assert!(mods.command("open"));
    let screen = "test_guest:menu".to_string();
    assert_eq!(
        mods.drain(),
        [Output::OpenScreen {
            screen: screen.clone()
        }]
    );
    let ops = mods.screen_render(&screen, context());
    assert!(matches!(
        ops[0],
        DrawOp::Panel {
            w: 320.0,
            h: 240.0,
            ..
        }
    ));
    assert!(matches!(&ops[1], DrawOp::Button { label, hovered: true, .. } if *label == screen));
    let click = |x, y| MouseEvent {
        action: MouseAction::Press,
        x,
        y,
        button: 0,
        scroll: 0.0,
    };
    assert!(!mods.screen_mouse(&screen, click(100.0, 100.0)));
    assert!(mods.screen_mouse(&screen, click(20.0, 15.0)));
    assert_eq!(
        mods.drain(),
        [Output::CloseScreen {
            id: "test_guest".into()
        }]
    );
    // The game's own screens go only to mods asking for screen events.
    assert!(mods.screen_render("inventory", context()).is_empty());
}

#[test]
fn chat_hooks_filter_lines() {
    let mut mods = load("horizon_mods_test_guest.hmod");
    assert_eq!(mods.chat("a secret word", "{}"), ChatVerdict::Hide);
    assert_eq!(
        mods.chat("swap it", "{}"),
        ChatVerdict::Replace("swapped it".into())
    );
    assert_eq!(mods.chat("plain", "{}"), ChatVerdict::Keep);
    assert_eq!(mods.chat_sending("/blocked"), None);
    assert_eq!(mods.chat_sending("lower").as_deref(), Some("LOWER"));
    assert_eq!(mods.chat_sending("hi").as_deref(), Some("hi"));
}

#[test]
fn tooltips_keys_sends_and_payloads() {
    let mut mods = load("horizon_mods_test_guest.hmod");
    let mut snapshot = player();
    snapshot.world = Some(WorldInfo {
        multiplayer: true,
        server: Some("example.org".into()),
        difficulty: 2,
        hardcore: false,
        raining: false,
        thundering: false,
    });
    mods.set_snapshot(snapshot);
    let apple = ItemStack {
        id: "minecraft:apple".into(),
        count: 3,
        damage: 0,
        max_damage: 0,
        name: "Apple".into(),
        components: String::new(),
    };
    assert_eq!(mods.tooltip(&apple), ["minecraft:apple x3 at 20"]);
    let binds = mods.keybinds();
    assert_eq!(binds.len(), 1);
    assert_eq!(
        (binds[0].0.as_str(), binds[0].1.id.as_str()),
        ("test_guest", "zoom")
    );
    assert_eq!(binds[0].1.key, "key.keyboard.c");
    mods.keybind("test_guest", "zoom", true);
    assert!(mods.command("send hello there"));
    assert_eq!(
        mods.drain(),
        [
            Output::Send {
                message: "hello there".into()
            },
            Output::Feedback {
                text: "keys zoom:true".into(),
                error: false
            }
        ]
    );
    // At most 4 a second.
    for _ in 0..3 {
        assert!(mods.command("send again"));
    }
    mods.drain();
    assert!(mods.command("send one too many"));
    assert!(matches!(
        &mods.drain()[..],
        [Output::Feedback { error: true, .. }]
    ));
    // The badly named channel was dropped.
    assert_eq!(mods.channels(), ["test:echo"]);
    mods.payload("test:echo", b"ping");
    assert_eq!(
        mods.drain(),
        [Output::Payload {
            channel: "test:echo".into(),
            data: b"ping".to_vec()
        }]
    );
    mods.payload("test:other", b"ping");
    assert!(mods.drain().is_empty());
    assert!(mods.use_item(Hand::MainHand));
    assert!(!mods.use_item(Hand::OffHand));
}

/// The game's apple, for `inventory.get-item-info`.
struct Apples;
impl horizon_mods::ItemCatalog for Apples {
    fn info(&self, id: &str) -> Option<horizon_mods::ItemInfo> {
        (id == "minecraft:apple").then(|| horizon_mods::ItemInfo {
            id: id.into(),
            name: "Apple".into(),
            max_stack_size: 64,
            max_damage: 0,
            food: Some(horizon_mods::Food {
                nutrition: 4,
                saturation: 2.4,
                can_always_eat: false,
            }),
        })
    }
}

/// The AppleSkin-style example: saturation outlines and the held food on the hunger bar, food
/// values in tooltips.
#[test]
fn food_info_draws_around_the_hunger_bar() {
    let mut mods = load("food_info.hmod");
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert!(mods.has_hud_element(HudPart::Food, Placement::After));
    assert!(mods.has_hud_element(HudPart::Food, Placement::Before));
    let apple = ItemStack {
        id: "minecraft:apple".into(),
        count: 1,
        damage: 0,
        max_damage: 0,
        name: "Apple".into(),
        components: String::new(),
    };
    let mut snapshot = player();
    if let Some(p) = snapshot.player.as_mut() {
        p.food = 10;
        p.saturation = 3.0;
        p.exhaustion = 2.0;
    }
    snapshot.inventory = vec![None; 41];
    snapshot.inventory[0] = Some(apple.clone());
    snapshot.items = Some(std::sync::Arc::new(Apples));
    mods.set_snapshot(snapshot);
    let mut context = context();
    context.delta = 0.0;
    // Exhaustion 2 of 4: half the bar's 81 pixels, from its right end (320 / 2 + 91).
    let behind = mods.hud_elements(HudPart::Food, Placement::Before, context);
    assert!(matches!(
        behind[..],
        [DrawOp::Fill {
            x: 210.5,
            w: 40.5,
            h: 9.0,
            ..
        }]
    ));
    let over = mods.hud_elements(HudPart::Food, Placement::After, context);
    // Saturation 3: one full outline (4 fills) and half of the next (3), then the two
    // icons the apple fills (food 10 to 14).
    let fills = over
        .iter()
        .filter(|o| matches!(o, DrawOp::Fill { .. }))
        .count();
    assert_eq!(fills, 7);
    let sprites: Vec<&str> = over
        .iter()
        .filter_map(|o| match o {
            DrawOp::Sprite { sprite, .. } => Some(sprite.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sprites, ["hud/food_full", "hud/food_full"]);
    assert_eq!(
        mods.tooltip(&apple),
        [r#"{"text":"+4 hunger, +2.4 saturation","color":"gold"}"#]
    );
}

/// The screen example: its keybind opens its screen, "Add here" keeps the place (storage), a
/// press on the place picks it and the HUD then points at it.
#[test]
fn waypoints_screen_keeps_and_points() {
    let dir = mods_dir().join("waypoints_run");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("waypoints.hmod");
    std::fs::copy(mods_dir().join("waypoints.hmod"), &path).unwrap();
    let mut mods = Mods::load(&[path], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    let mut snapshot = player();
    snapshot.gui = Some(horizon_mods::Gui {
        width: 320,
        height: 240,
        scale: 2.0,
    });
    mods.set_snapshot(snapshot);
    let binds = mods.keybinds();
    assert_eq!(binds[0].1.key, "key.keyboard.j");
    mods.keybind("waypoints", "open", true);
    let screen = "waypoints:list".to_string();
    assert_eq!(
        mods.drain(),
        [Output::OpenScreen {
            screen: screen.clone()
        }]
    );
    let labels = |ops: &[DrawOp]| -> Vec<String> {
        ops.iter()
            .filter_map(|o| match o {
                DrawOp::Button { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect()
    };
    let ops = mods.screen_render(&screen, context());
    assert_eq!(labels(&ops), ["Add here", "Rename", "Close"]);
    // The panel is 220x176 in the middle of 320x240: "Add here" at (60, 178).
    let press = |x, y| MouseEvent {
        action: MouseAction::Press,
        x,
        y,
        button: 0,
        scroll: 0.0,
    };
    assert!(mods.screen_mouse(&screen, press(70.0, 185.0)));
    let ops = mods.screen_render(&screen, context());
    assert_eq!(
        labels(&ops),
        ["Place 1 (0 m)", "x", "Add here", "Rename", "Close"]
    );
    assert!(mods.hud_render(context()).is_empty());
    // Pick it: the HUD points at it.
    assert!(mods.screen_mouse(&screen, press(70.0, 62.0)));
    let hud = mods.hud_render(context());
    assert!(
        hud.iter()
            .any(|o| matches!(o, DrawOp::Text { text, .. } if text.contains("Place 1 0 m")))
    );
    // Rename it: the keyboard comes up, typed text and Enter name it.
    assert!(mods.screen_mouse(&screen, press(140.0, 185.0)));
    assert_eq!(
        mods.drain(),
        [Output::Keyboard {
            id: "waypoints".into(),
            shown: true
        }]
    );
    let key = |key: &str, text: &str| horizon_mods::KeyEvent {
        key: key.into(),
        pressed: true,
        text: text.into(),
        shift: false,
        control: false,
        alt: false,
    };
    for _ in 0.."Place 1".len() {
        assert!(mods.screen_key(&screen, &key("key.keyboard.backspace", "")));
    }
    assert!(mods.screen_key(&screen, &key("key.keyboard.h", "Home")));
    assert!(mods.screen_key(&screen, &key("key.keyboard.enter", "")));
    let ops = mods.screen_render(&screen, context());
    assert_eq!(labels(&ops)[0], "> Home (0 m)");
    // In the world: a beam up from it, its diamond with its name over it and its distance under
    // it seen through hills, the distance by the metre as the player walks away; the chosen
    // place ringed, the ring with the texts.
    let mut away = player();
    away.player.as_mut().unwrap().x += 30.4;
    mods.set_snapshot(away);
    mods.tick(1);
    let layers: Vec<(String, horizon_mods::WorldLayer)> =
        mods.world_layers().flat_map(|(_, l)| l.to_vec()).collect();
    let names: Vec<&str> = layers.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(names, ["beams"]);
    let beam = &layers[0].1.lines[0];
    assert_eq!(
        (beam.origin.x, beam.origin.y, beam.origin.z),
        (12.5, 64.0, -3.25)
    );
    assert_eq!(beam.ends, [0.0, 0.0, 0.0, 0.0, 256.0, 0.0]);
    assert_eq!(
        (beam.colors[0], layers[0].1.through_walls),
        (0xC0FF_5555, false)
    );
    let icons: Vec<(String, Vec<horizon_mods::WorldIcon>)> =
        mods.world_icons().flat_map(|(_, l)| l.to_vec()).collect();
    assert_eq!(icons.len(), 1);
    let (layer, marks) = &icons[0];
    assert_eq!(layer, "marks");
    let sprites: Vec<&str> = marks.iter().map(|i| i.sprite.as_str()).collect();
    assert_eq!(sprites, ["waypoints:diamond_ring", "waypoints:diamond"]);
    let (ring, diamond) = (&marks[0], &marks[1]);
    assert_eq!(ring.above, r##"{"text":"Home","color":"#FF5555"}"##);
    assert_eq!(ring.below, r##"{"text":"30 m","color":"#E0E0E0"}"##);
    assert!(ring.fixed_size && ring.through_walls);
    assert_eq!((ring.size, diamond.size), (17.0, 11.0));
    assert_eq!((ring.color, diamond.color), (0xFFFF_FFFF, 0xFFFF_5555));
    assert!(diamond.above.is_empty() && diamond.below.is_empty());
    assert_eq!(
        (diamond.at.x, diamond.at.y, diamond.at.z),
        (12.5, 65.2, -3.25)
    );
    // Kept on disk: a reload has it.
    let entries = Mods::load(&[dir.join("waypoints.hmod")], |_| true, |_| false);
    let mut reloaded = entries;
    reloaded.set_snapshot(player());
    reloaded.keybind("waypoints", "open", true);
    let ops = reloaded.screen_render(&screen, context());
    assert!(labels(&ops).iter().any(|l| l.starts_with("Home")));
}

/// A mod's buttons (`widgets`): on a game screen, replaced by id and removed, on the touch
/// controls kept in bounds, the game's own restyled; wrong ones refused; a press is its keybind.
#[test]
fn mods_add_buttons_to_screens_and_touch_controls() {
    let mut mods = load("horizon_mods_test_guest.hmod");
    assert!(mods.command("buttons"));
    assert_eq!(
        mods.drain(),
        [Output::Feedback {
            text: "refused 2; laser false, jump true".into(),
            error: false
        }]
    );
    let (id, w) = mods.widgets().next().unwrap();
    assert_eq!(id, "test_guest");
    let labels: Vec<_> = w.screen.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["Home again"]);
    let zoom = &w.touch[0];
    assert_eq!((zoom.x, zoom.size), (1.0, 2.0));
    assert_eq!(w.styles[0].control, "jump");
    assert_eq!(w.styles[0].label, "Hop");
    // A press reaches the mod as its keybinds do.
    mods.keybind("test_guest", "home", true);
    mods.keybind("test_guest", "home", false);
    assert!(mods.command("send hi"));
    assert!(mods.drain().iter().any(|o| matches!(
        o,
        Output::Feedback { text, .. } if text == "keys home:true,home:false"
    )));
}

/// The Waypoints example's buttons: one on the pause menu, one on the touch controls, both
/// opening its screen.
#[test]
fn waypoints_has_a_pause_button_and_a_touch_button() {
    let mods = load("waypoints.hmod");
    let (_, w) = mods.widgets().next().unwrap();
    assert_eq!(w.screen[0].screen, "pause");
    assert_eq!(w.screen[0].id, "open");
    assert_eq!(w.touch[0].icon, "item/compass_16");
}

/// With nothing kept, the JavaScript engine starts on the quick compiler's code at once, and
/// its full code is kept in the background for the next start, which reads it back.
#[test]
fn javascript_starts_quickly_and_keeps_its_full_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example = root.join("crates/starling-mod-sdk/examples/hello-js");
    let cache = mods_dir().join("quick_cache");
    let _ = std::fs::remove_dir_all(&cache);
    let started = Instant::now();
    let mods = Mods::load_cached(
        std::slice::from_ref(&example),
        |_| true,
        |_| false,
        Some(&cache),
    );
    assert_eq!(mods.entries()[0].status, Status::Running);
    let quick = started.elapsed();
    assert!(mods.on_quick_code());
    // (Other tests compile in the background at the same time: the list is everyone's.)
    horizon_mods::finish_background_compiles();
    let kept = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".cwasm"))
        .count();
    assert!(kept >= 1, "the full code was not kept");
    let started = Instant::now();
    let again = Mods::load_cached(&[example], |_| true, |_| false, Some(&cache));
    assert_eq!(again.entries()[0].status, Status::Running);
    assert!(!again.on_quick_code(), "the full code was not read back");
    eprintln!(
        "quick start {quick:?}, from the kept code {:?}",
        started.elapsed()
    );
}

/// The Elytra Swap example on the player's inventory menu (`slots`): R takes the elytra from
/// the inventory onto the chest in three vanilla clicks, and says so.
#[test]
fn elytra_swap_clicks_the_elytra_on() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example = root.join("crates/starling-mod-sdk/examples/elytra-swap");
    let mut mods = Mods::load(&[example], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    let stack = |id: &str| ItemStack {
        id: id.into(),
        count: 1,
        damage: 0,
        max_damage: 0,
        name: id.into(),
        components: String::new(),
    };
    let mut slots = vec![None; 46];
    slots[6] = Some(stack("minecraft:iron_chestplate"));
    slots[22] = Some(stack("minecraft:elytra"));
    let mut snapshot = player();
    snapshot.menu = Some(horizon_mods::Menu {
        kind: "minecraft:inventory".into(),
        slots,
        carried: None,
    });
    mods.set_snapshot(snapshot);
    mods.keybind("elytra_swap", "swap", true);
    let clicks: Vec<(i32, u8)> = mods
        .drain()
        .into_iter()
        .filter_map(|o| match o {
            Output::Click {
                slot,
                button,
                kind: horizon_mods::ClickKind::Pickup,
                ..
            } => Some((slot, button)),
            Output::Overlay { text } => {
                assert_eq!(text, "Elytra on");
                None
            }
            _ => None,
        })
        .collect();
    assert_eq!(clicks, [(22, 0), (6, 0), (22, 0)]);
}

#[test]
fn off_mods_cost_nothing() {
    let mods = Mods::load(&[mods_dir().join("coords_hud.hmod")], |_| false, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Off);
    assert!(!mods.is_running());
    assert_eq!(mods.commands().count(), 0);
}

#[test]
fn undeclared_capabilities_are_refused() {
    let dir = mods_dir().join("undeclared");
    std::fs::create_dir_all(&dir).unwrap();
    let wasm = Package::read(&mods_dir().join("horizon_mods_test_guest.hmod"))
        .unwrap()
        .wasm
        .expect("the guest has code");
    let manifest = GUEST_MANIFEST.replace("\"chat\", ", "");
    std::fs::write(dir.join("horizon-mod.toml"), manifest).unwrap();
    std::fs::write(dir.join("mod.wasm"), wasm).unwrap();
    let mods = Mods::load(&[dir], |_| true, |_| false);
    let refused = Status::Failed("uses capability chat, missing from horizon-mod.toml".into());
    assert_eq!(mods.entries()[0].status, refused);
}

/// A mod calling a function this Horizon doesn't have is refused with
/// "update Horizon", not a linking error; the test mods' levels are read
/// from the functions they import.
#[test]
fn a_mod_for_a_newer_horizon_is_refused() {
    use horizon_mods::api_level::{LEVEL, imports, needed};
    use wit_parser::{ManglingAndAbi, Resolve};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wit = std::fs::read_to_string(root.join("crates/horizon-mod-sdk/wit/horizon.wit"))
        .unwrap()
        .replace(
            "    set-minimap: func(minimap: option<minimap>);",
            "    set-minimap: func(minimap: option<minimap>);\n    set-sky: func(on: bool);",
        );
    let mut resolve = Resolve::default();
    let package = resolve.push_str("horizon.wit", &wit).unwrap();
    let world = resolve
        .select_world(&[package], Some("horizon-ui-mod"))
        .unwrap();
    let mut module = wit_component::dummy_module(&resolve, world, ManglingAndAbi::Standard32);
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .unwrap();
    let wasm = wit_component::ComponentEncoder::default()
        .module(&module)
        .unwrap()
        .encode()
        .unwrap();
    let functions = imports(&wasm).unwrap();
    assert!(functions.iter().any(|f| f == "render.set-sky"));
    let dir = mods_dir().join("too_new");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = r#"
id = "too_new"
name = "Too New"
version = "0.1.0"
api = "0.1"
capabilities = ["game-read", "chat", "hud", "storage", "command", "ui", "render", "input", "recipes"]
experimental = ["ui", "render", "input", "recipes"]
"#;
    std::fs::write(dir.join("horizon-mod.toml"), manifest).unwrap();
    std::fs::write(dir.join("mod.wasm"), wasm).unwrap();
    let mods = Mods::load(&[dir], |_| true, |_| false);
    let refused = "made for a newer Horizon (it uses render.set-sky): update Horizon to run it";
    assert_eq!(mods.entries()[0].status, Status::Failed(refused.into()));

    let level = |file: &str| {
        let wasm = Package::read(&mods_dir().join(file)).unwrap().wasm.unwrap();
        needed(imports(&wasm).unwrap().iter().map(String::as_str)).0
    };
    assert_eq!(level("coords_hud.hmod"), 0);
    // The UI guest draws far terrain (`render.set-far-terrain`).
    assert_eq!(level("horizon_mods_test_ui_guest.hmod"), LEVEL);
}

#[test]
fn of_two_copies_the_newest_runs() {
    let wasm = Package::read(&mods_dir().join("horizon_mods_test_guest.hmod"))
        .unwrap()
        .wasm
        .expect("the guest has code");
    let copy = |name: &str, age: u64| {
        let dir = mods_dir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("horizon-mod.toml"), GUEST_MANIFEST).unwrap();
        std::fs::write(dir.join("mod.wasm"), &wasm).unwrap();
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(dir.join("mod.wasm"))
            .unwrap()
            .set_modified(when)
            .unwrap();
        dir
    };
    // Whichever name comes first.
    for (older, newer) in [("copy_a", "copy_b"), ("copy_d", "copy_c")] {
        let (older, newer) = (copy(older, 600), copy(newer, 60));
        let mut paths = [older.clone(), newer.clone()];
        paths.sort();
        let mods = Mods::load(&paths, |_| true, |_| false);
        let status = |dir: &PathBuf| {
            let entry = mods.entries().iter().find(|e| &e.path == dir).unwrap();
            entry.status.clone()
        };
        assert_eq!(status(&newer), Status::Running);
        let name = newer.file_name().unwrap().to_string_lossy();
        assert_eq!(
            status(&older),
            Status::Failed(format!("an older copy: {name} runs"))
        );
    }
}

#[test]
fn vanilla_commands_win() {
    let mods = Mods::load(
        &[mods_dir().join("coords_hud.hmod")],
        |_| true,
        |c| c == "where",
    );
    assert_eq!(mods.commands().collect::<Vec<_>>(), ["coords"]);
}

#[test]
fn a_misbehaving_mod_is_stopped_alone() {
    for (command, reason) in [
        ("spin", "it took too long (an endless loop?)"),
        ("panic", "it crashed (a panic)"),
        (
            "grow",
            "it ran out of memory (it asked for 1025 MiB; a mod has 64 MiB)",
        ),
    ] {
        let paths = [
            mods_dir().join("coords_hud.hmod"),
            mods_dir().join("horizon_mods_test_guest.hmod"),
        ];
        let mut mods = Mods::load(&paths, |_| true, |_| false);
        let started = Instant::now();
        assert!(mods.command(command));
        assert!(started.elapsed().as_secs() < 5, "/{command} was cut short");
        let stopped = Output::Stopped {
            name: "Test Guest".into(),
            reason: reason.into(),
        };
        assert_eq!(mods.drain(), [stopped], "/{command}");
        assert_eq!(mods.entries()[1].status, Status::Failed(reason.into()));
        // The other mod runs on, and the stopped one's command is gone.
        assert!(mods.is_running());
        assert!(!mods.command("hello"));
        mods.set_snapshot(player());
        mods.tick(1);
        assert_eq!(mods.panels().count(), 1);
    }
}

#[test]
fn hot_reload_restarts_a_changed_mod() {
    let dir = mods_dir().join("reload");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("guest.hmod");
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &path).unwrap();
    let mut mods = Mods::load(std::slice::from_ref(&path), |_| true, |_| false);
    mods.tick(1);
    assert!(!mods.reload_changed(|_| false));
    // A rebuild: same bytes, newer file.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(mods.reload_changed(|_| false));
    assert!(mods.command("hello world"));
    let chat = Output::Chat {
        name: "Test Guest".into(),
        text: "hello world after 0 ticks".into(),
    };
    assert_eq!(mods.drain(), [chat]);
}

#[test]
fn compiled_mods_are_kept_for_the_next_start() {
    let cache = mods_dir().join("compiled");
    let _ = std::fs::remove_dir_all(&cache);
    std::fs::create_dir_all(&cache).unwrap();
    // A build of a mod since removed.
    let stale = cache.join("0000-0000.cwasm");
    std::fs::write(&stale, b"old").unwrap();
    let path = mods_dir().join("horizon_mods_test_guest.hmod");
    let start = || {
        let mut mods = Mods::load_cached(
            std::slice::from_ref(&path),
            |_| true,
            |_| false,
            Some(&cache),
        );
        assert_eq!(mods.entries()[0].status, Status::Running);
        assert!(mods.command("hello cached"));
        mods.drain();
    };
    start();
    let kept: Vec<_> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].extension().is_some_and(|e| e == "cwasm"));
    // The second start reads it back; a damaged file is compiled again.
    start();
    std::fs::write(&kept[0], b"damaged").unwrap();
    start();
    assert!(std::fs::metadata(&kept[0]).unwrap().len() > 1000);
}

/// A JavaScript mod (its manifest and `mod.js`) runs in the shared engine; the build embeds the
/// engine once it is built (crates/starling-mod-sdk/js-engine).
#[test]
fn javascript_mods_run_in_the_shared_engine() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example = root.join("crates/starling-mod-sdk/examples/hello-js");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[example], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert_eq!(mods.commands().collect::<Vec<_>>(), ["hello_js"]);
    mods.set_snapshot(player());
    assert!(mods.command("hello_js"));
    let said = mods.drain();
    assert!(
        matches!(&said[..], [Output::Chat { text, .. }] if text.starts_with("Hello from JS at 12.5 64.0")),
        "{said:?}"
    );
    let ops = mods.hud_render(context());
    assert!(
        matches!(&ops[..], [DrawOp::Text { text, .. }] if text.starts_with("JS ")),
        "{ops:?}"
    );
    let stack = ItemStack {
        id: "minecraft:stick".into(),
        count: 1,
        damage: 0,
        max_damage: 0,
        name: "Stick".into(),
        components: String::new(),
    };
    assert_eq!(mods.tooltip(&stack), ["§8minecraft:stick"]);
}

/// A JavaScript mod gets only the interfaces its manifest declares: calling another stops it.
#[test]
fn javascript_mods_get_only_their_capabilities() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = mods_dir().join("js-undeclared");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"sneaky\"
name = \"Sneaky\"
version = \"1\"
api = \"starling-0.1\"
",
    )
    .unwrap();
    std::fs::write(
        dir.join("mod.js"),
        r#"import { show } from "starling:mods/chat@0.1.0";
export const events = {
  init() {
    return {
      commands: ["sneak"], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  command(name, args) { show("not allowed"); },
};
"#,
    )
    .unwrap();
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    mods.command("sneak");
    let reason = "uses chat.show: capability chat is missing from its manifest";
    assert_eq!(
        mods.drain(),
        [Output::Stopped {
            name: "Sneaky".into(),
            reason: reason.into()
        }]
    );
    assert_eq!(mods.entries()[0].status, Status::Failed(reason.into()));
}

#[test]
fn storage_persists_across_reload() {
    let dir = mods_dir().join("storage_reload");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("guest.hmod");
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &path).unwrap();
    let mut mods = Mods::load(std::slice::from_ref(&path), |_| true, |_| false);
    assert!(mods.command("save remembered"));
    let saved = Output::Feedback {
        text: "saved".into(),
        error: false,
    };
    assert_eq!(mods.drain(), [saved]);

    // A rebuild (hot_reload_restarts_a_changed_mod's pattern): a fresh Store,
    // but the same on-disk data directory.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(mods.reload_changed(|_| false));

    assert!(mods.command("load"));
    let chat = Output::Chat {
        name: "Test Guest".into(),
        text: "remembered".into(),
    };
    assert_eq!(mods.drain(), [chat]);
}

#[test]
fn storage_write_budget_limits_one_call() {
    let dir = mods_dir().join("storage_budget");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("guest.hmod");
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &path).unwrap();
    let mut mods = Mods::load(std::slice::from_ref(&path), |_| true, |_| false);
    // Under the 4 MiB on-disk quota, but over the 1 MiB a single call may
    // write: a loop replacing one key can never bypass the quota by
    // writing a lot in one call, only across many.
    let over_budget = "a".repeat((1 << 20) + 1024);
    assert!(mods.command(&format!("save {over_budget}")));
    let refused = Output::Feedback {
        text: "too much storage traffic in one call".into(),
        error: true,
    };
    assert_eq!(mods.drain(), [refused]);
}

#[test]
fn storage_quota_is_enforced() {
    let dir = mods_dir().join("storage_quota");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("guest.hmod");
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &path).unwrap();
    let mut mods = Mods::load(std::slice::from_ref(&path), |_| true, |_| false);
    // Each call is under the per-call write budget; distinct keys so the
    // quota (not the fixed "note" key being overwritten) is what's tested.
    let chunk = "a".repeat(900 << 10);
    for i in 0..4 {
        assert!(mods.command(&format!("save_key k{i} {chunk}")));
        assert_eq!(
            mods.drain(),
            [Output::Feedback {
                text: "saved".into(),
                error: false,
            }]
        );
    }
    assert!(mods.command(&format!("save_key k4 {chunk}")));
    let refused = Output::Feedback {
        text: "would use over 4 MiB of storage".into(),
        error: true,
    };
    assert_eq!(mods.drain(), [refused]);
}

#[test]
fn storage_key_count_is_bounded() {
    let dir = mods_dir().join("storage_keys");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("guest.hmod");
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &path).unwrap();
    let mut mods = Mods::load(std::slice::from_ref(&path), |_| true, |_| false);
    for i in 0..64 {
        assert!(mods.command(&format!("save_key k{i} x")));
        assert_eq!(
            mods.drain(),
            [Output::Feedback {
                text: "saved".into(),
                error: false,
            }],
            "key {i}"
        );
    }
    assert!(mods.command("save_key k64 x"));
    let refused = Output::Feedback {
        text: "would hold over 64 keys".into(),
        error: true,
    };
    assert_eq!(mods.drain(), [refused]);
}

#[test]
fn duplicate_mod_ids_are_refused() {
    let dir = mods_dir().join("duplicate_ids");
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.hmod");
    let second = dir.join("second.hmod");
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &first).unwrap();
    std::fs::copy(mods_dir().join("horizon_mods_test_guest.hmod"), &second).unwrap();
    // The second an hour older: of copies the newest runs, and two copied in a row may differ
    // by nanoseconds either way.
    let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&second)
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();
    // Both declare id "test_guest" (the guest's own manifest): sharing a
    // `data_dir` would let the second read, overwrite or delete the
    // first's storage, so only one of them may start.
    let mods = Mods::load(&[first, second], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    let refused = Status::Failed("an older copy: first.hmod runs".into());
    assert_eq!(mods.entries()[1].status, refused);
}

/// `cargo test -p horizon-mods --release -- --ignored --nocapture`: the
/// cost of the per-tick and per-frame hooks (docs/mods.md, Performance).
#[test]
#[ignore]
fn hook_overhead() {
    let time = |mods: &mut Mods, frame: bool| {
        let n = 200_000;
        for i in 0..1000 {
            mods.tick(i);
        }
        let started = Instant::now();
        for i in 0..n {
            if frame {
                mods.frame(0.016);
            } else {
                mods.tick(i);
            }
        }
        started.elapsed().as_nanos() as f64 / n as f64
    };
    let mut guest = load("horizon_mods_test_guest.hmod");
    let mut hud = load("coords_hud.hmod");
    hud.set_snapshot(player());
    let mut none = Mods::default();
    println!("no mods, tick:        {:8.1} ns", time(&mut none, false));
    println!("empty tick hook:      {:8.1} ns", time(&mut guest, false));
    println!("empty frame hook:     {:8.1} ns", time(&mut guest, true));
    println!("coords-hud tick hook: {:8.1} ns", time(&mut hud, false));
    let started = Instant::now();
    let _ = load("coords_hud.hmod");
    println!(
        "load coords-hud:      {:8.1} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

/// What a JavaScript mod's calls into the game cost, a frame of 40 of each (docs/mod-porting.md,
/// JavaScript speed): `cargo test --release -p horizon-mods --test sandbox javascript_call_costs
/// -- --ignored --nocapture`.
#[test]
#[ignore]
fn javascript_call_costs() {
    const MOD_JS: &str = r#"
import { getPlayer, getGui, getTarget } from "starling:mods/game@0.1.0";
import { text, fill, textWidth } from "starling:mods/draw@0.1.0";
import { isControlDown } from "starling:mods/keys@0.1.0";
let mode = 0;
const calls = [
  () => {},
  () => { for (let i = 0; i < 40; i++) getPlayer(); },
  () => { for (let i = 0; i < 40; i++) getGui(); },
  () => { for (let i = 0; i < 40; i++) getTarget(); },
  () => { for (let i = 0; i < 40; i++) isControlDown("key.forward"); },
  () => { for (let i = 0; i < 40; i++) fill(4, 4, 120, 9, 0x80000000); },
  () => { for (let i = 0; i < 40; i++) text(6, 5, `Line ${i}: 533x240`, 0xFFFFFF, true); },
  () => { for (let i = 0; i < 40; i++) textWidth("Line 12: 533x240"); },
];
export const events = {
  init() {
    return { commands: ["mode"], tick: false, frame: false, chat: false, chatSend: false,
      hud: true, hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false };
  },
  command(_name, args) { mode = Number(args); return undefined; },
  hudRender(_context) { calls[mode](); },
};
"#;
    let dir = mods_dir().join("call_costs");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"call_costs\"\nname = \"Call Costs\"\nversion = \"0.1.0\"\n\
         api = \"starling-0.1\"\ncapabilities = [\"game-read\", \"hud\", \"keys\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("mod.js"), MOD_JS).unwrap();
    let mut mods = Mods::load(&[dir], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    mods.set_snapshot(player());
    let names = [
        "empty hook",
        "40 getPlayer",
        "40 getGui",
        "40 getTarget",
        "40 isControlDown",
        "40 fill",
        "40 text",
        "40 textWidth (cached)",
    ];
    for (mode, name) in names.iter().enumerate() {
        assert!(mods.command(&format!("mode {mode}")));
        for _ in 0..100 {
            mods.hud_render(context());
        }
        let started = Instant::now();
        for _ in 0..1000 {
            mods.hud_render(context());
        }
        println!(
            "{name:24} {:8.1} us a frame",
            started.elapsed().as_secs_f64() * 1e3
        );
    }
    assert!(
        !mods
            .drain()
            .iter()
            .any(|o| matches!(o, Output::Stopped { .. })),
        "the mod stopped"
    );
}

#[test]
fn ui_screens_settings_and_events() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert!(!mods.entries()[0].has_ui_settings());
    // `/ui_screen` pushes the screen; the host clips nothing of this one.
    assert!(mods.command("ui_screen"));
    assert_eq!(mods.drain(), [Output::OpenUiScreen { owner: 0 }]);
    let screen = mods.screen(0).expect("a pushed screen");
    assert_eq!(screen.title, "Guest's screen");
    assert_eq!(screen.widgets.len(), 3);
    // What the player did comes back through `ui-events`.
    mods.ui_select(0, 7, 2);
    mods.ui_closed(0);
    assert_eq!(
        mods.drain(),
        [
            Output::Chat {
                name: "Test UI Guest".into(),
                text: "selected 7:2".into(),
            },
            Output::Chat {
                name: "Test UI Guest".into(),
                text: "closed".into(),
            },
        ]
    );
    // The settings page: empty until the mod fills it, refreshed on open.
    assert!(mods.command("ui_settings"));
    assert_eq!(mods.settings_widgets(0).len(), 3);
    assert!(mods.entries()[0].has_ui_settings());
    mods.ui_settings_opened(0);
    assert_eq!(mods.settings_widgets(0).len(), 2);
    // `/ui_close` closes the mod's own screen.
    assert!(mods.command("ui_close"));
    assert!(mods.drain().contains(&Output::CloseUiScreen { owner: 0 }));
    assert!(mods.screen(0).is_none());
}

#[test]
fn render_shapes_and_shakes() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    mods.set_snapshot(Snapshot {
        gui_size: (427.0, 240.0),
        ..Snapshot::default()
    });
    assert_eq!(mods.hud_shapes().count(), 0);
    assert!(mods.command("render_hud"));
    let shapes: Vec<_> = mods.hud_shapes().cloned().collect();
    assert_eq!(shapes.len(), 5);
    // The guest read `gui-size`; text is clipped to 256 characters.
    let Shape::Text(t) = &shapes[4] else {
        panic!("{:?}", shapes[4])
    };
    assert!(t.text.starts_with("gui 427x240 "), "{}", t.text);
    assert_eq!(t.text.chars().count(), 256);
    assert!(mods.command("render_clear"));
    assert_eq!(mods.hud_shapes().count(), 0);
    // One jolt, clamped; zero and NaN shake nothing.
    mods.drain();
    assert!(mods.command("render_shake"));
    assert_eq!(
        mods.drain(),
        [Output::Shake {
            strength: 1.0,
            seconds: 10.0
        }]
    );
}

#[test]
fn input_claims_keys_and_events() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let chat = |mods: &mut Mods| -> Vec<String> {
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Chat { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    assert!(!mods.claims_any() && !mods.has_keys());
    assert!(mods.command("input_claim"));
    let said = chat(&mut mods);
    assert!(
        said[0].starts_with("claimed; \"R\" is not a key"),
        "{said:?}"
    );
    assert!(mods.claims("minecraft:stick") && !mods.claims("minecraft:stone"));
    let keys: Vec<_> = mods
        .mod_keys()
        .map(|(m, k)| (m.to_owned(), k.key.clone()))
        .collect();
    assert_eq!(
        keys,
        [("Test UI Guest".to_owned(), "key.keyboard.r".to_owned())]
    );
    // The game feeds the events; the mod reads them in order with `poll`.
    let stick = HeldItem {
        id: "minecraft:stick".into(),
        count: 3,
        slot: 2,
    };
    mods.use_start(&stick);
    mods.key("key.keyboard.r", true);
    mods.key("key.keyboard.t", true);
    mods.key("key.keyboard.r", false);
    mods.use_stop(&stick, 12);
    assert!(mods.command("input_poll"));
    assert_eq!(
        chat(&mut mods),
        [
            "use-start minecraft:stick 0",
            "key-down wheel",
            "key-up wheel",
            "use-stop minecraft:stick 12"
        ]
    );
    // `held` reads the snapshot.
    mods.set_snapshot(Snapshot {
        held: Some(stick),
        ..Snapshot::default()
    });
    assert!(mods.command("input_held"));
    assert_eq!(chat(&mut mods), ["holding minecraft:stick x3 in 2"]);
    // A screen opening lets held keys up, once; the release that follows
    // is not sent again.
    mods.key("key.keyboard.r", true);
    mods.release_keys();
    mods.release_keys();
    mods.key("key.keyboard.r", false);
    assert!(mods.command("input_poll"));
    assert_eq!(chat(&mut mods), ["key-down wheel", "key-up wheel"]);
}

#[test]
fn the_players_key_choice_wins() {
    let chat = |mods: &mut Mods| -> Vec<String> {
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Chat { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    assert!(mods.command("input_claim"));
    // Its keys alone give it a page in Options > Mods, where the game lists them.
    assert!(mods.settings_widgets(0).is_empty() && mods.entries()[0].has_ui_settings());
    // Rebinding while held lets the old key go.
    mods.key("key.keyboard.r", true);
    assert!(mods.rebind(0, "wheel", "key.mouse.4"));
    assert_eq!(mods.keys_of(0)[0].key, "key.mouse.4");
    mods.key("key.keyboard.r", false);
    mods.key("key.mouse.4", true);
    assert!(!mods.rebind(0, "wheel", "R") && !mods.rebind(0, "nope", "key.keyboard.g"));
    chat(&mut mods);
    assert!(mods.command("input_poll"));
    assert_eq!(
        chat(&mut mods),
        ["key-down wheel", "key-up wheel", "key-down wheel"]
    );
    // A choice saved earlier replaces the default, registered before or after.
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    mods.set_bindings(|_| vec![("wheel", "key.keyboard.g"), ("bad", "G")]);
    assert!(mods.command("input_claim"));
    assert_eq!(mods.keys_of(0)[0].key, "key.keyboard.g");
    mods.set_bindings(|_| vec![("wheel", "key.keyboard.h")]);
    assert_eq!(mods.keys_of(0)[0].key, "key.keyboard.h");
}

#[test]
fn input_wheel_is_the_mods_while_claimed() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let chat = |mods: &mut Mods| -> Vec<String> {
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Chat { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    // Unclaimed, the hotbar keeps the wheel and the mod hears nothing.
    assert!(!mods.wheel_claimed() && !mods.wheel(1.0));
    assert!(mods.command("input_wheel"));
    assert!(mods.wheel_claimed());
    assert!(mods.wheel(1.0) && mods.wheel(-2.0));
    assert!(mods.command("input_poll"));
    assert_eq!(chat(&mut mods), ["wheel 1", "wheel -2"]);
    assert!(mods.command("input_wheel off"));
    assert!(!mods.wheel(1.0));
}

#[test]
fn render_zoom_is_clamped_and_the_strongest_counts() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let zoom = |mods: &mut Mods, args: &str| {
        assert!(mods.command(&format!("zoom {args}")));
        mods.zoom().map(|z| (z.factor, z.smooth, z.turn))
    };
    assert!(mods.zoom().is_none());
    assert_eq!(zoom(&mut mods, "4 1 0.25"), Some((4.0, true, 0.25)));
    // Over 10 and a turn under 0.05 are clamped.
    assert_eq!(zoom(&mut mods, "50 0 0"), Some((10.0, false, 0.05)));
    // A NaN turn keeps the player's sensitivity.
    assert_eq!(zoom(&mut mods, "2 0 x"), Some((2.0, false, 1.0)));
    // 1, under 1 and NaN end the zoom.
    assert_eq!(zoom(&mut mods, "1 0 1"), None);
    assert_eq!(zoom(&mut mods, "0.5 0 1"), None);
    assert_eq!(zoom(&mut mods, "x 0 1"), None);
}

#[test]
fn render_minimap_is_clamped_and_hidden() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let map = |mods: &mut Mods, args: &str| {
        assert!(mods.command(&format!("minimap {args}")));
        mods.minimap()
            .map(|m| (m.corner, m.size, m.blocks_per_pixel, m.rotate, m.round))
    };
    assert!(mods.minimap().is_none());
    let corner = horizon_mods::RenderCorner::BottomLeft;
    assert_eq!(
        map(&mut mods, "96 2 1 0"),
        Some((corner, 96.0, 2.0, true, false))
    );
    // The side and the scale are clamped; NaN takes the smallest.
    assert_eq!(
        map(&mut mods, "999 0.1 0 1"),
        Some((corner, 256.0, 0.5, false, true))
    );
    assert_eq!(
        map(&mut mods, "x 9 0 0"),
        Some((corner, 16.0, 4.0, false, false))
    );
    assert_eq!(map(&mut mods, "off"), None);
}

#[test]
fn render_far_terrain_is_clamped_and_ended() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let far = |mods: &mut Mods, args: &str| {
        assert!(mods.command(&format!("far_terrain {args}")));
        mods.far_terrain().map(|f| (f.distance, f.reduced))
    };
    assert!(mods.far_terrain().is_none());
    assert_eq!(far(&mut mods, "256 1"), Some((256, true)));
    // The distance is clamped to LOD Distance's 64 to 1024 chunks.
    assert_eq!(far(&mut mods, "8 0"), Some((64, false)));
    assert_eq!(far(&mut mods, "99999 0"), Some((1024, false)));
    assert_eq!(far(&mut mods, "off"), None);
}

/// `game.raycast`: the view's hit the game worked out, up to the distance
/// asked for; the game works it out only while a mod imports the call.
#[test]
fn game_raycast_sees_the_view_hit_within_its_distance() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    assert!(mods.wants_raycast());
    assert!(!load("coords_hud.hmod").wants_raycast());
    let said = |mods: &mut Mods, args: &str| -> Vec<String> {
        assert!(mods.command(&format!("raycast {args}")));
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Chat { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    assert_eq!(said(&mut mods, "100"), ["no hit"]);
    mods.set_snapshot(Snapshot {
        view_hit: Some((
            40.5,
            Hit {
                block_x: 12,
                block_y: 63,
                block_z: -44,
                x: 12.5,
                y: 64.0,
                z: -43.75,
                block: "minecraft:grass_block".into(),
            },
        )),
        ..player()
    });
    let hit = "hit minecraft:grass_block at 12 63 -44 (12.5 64.0 -43.8)";
    assert_eq!(said(&mut mods, "100"), [hit]);
    assert_eq!(said(&mut mods, "40"), ["no hit"]);
    // NaN asks for nothing; past 1024 is 1024.
    assert_eq!(said(&mut mods, "x"), ["no hit"]);
    assert_eq!(said(&mut mods, "1e9"), [hit]);
}

#[test]
fn quiet_commands_come_out_as_one_batch_capped() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let quiet = |mods: &mut Mods, command: &str| -> Vec<(String, Vec<String>)> {
        assert!(mods.command(command));
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::QuietCommands { mod_id, lines } => Some((mod_id, lines)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        quiet(&mut mods, "quiet setblock 0 0 0 stone;say hi"),
        [(
            "test_ui_guest".to_string(),
            vec!["setblock 0 0 0 stone".to_string(), "say hi".to_string()]
        )]
    );
    let batches = quiet(&mut mods, "quiet_many 300");
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].1.len(), 256);
    assert_eq!(batches[0].1[255], "setblock 255 300 0 stone");
}

#[test]
fn corner_panels_text_width_and_readings() {
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    // A font of 5-pixel glyphs, 3-pixel spaces.
    let mut widths = vec![5.0f32; 0x250];
    widths[' ' as usize] = 3.0;
    mods.set_snapshot(Snapshot {
        frame: (60, 41, 52),
        mappings: vec![("key.forward", "W".into(), true)],
        cps: (7, 2),
        font_widths: widths.into(),
        ..Snapshot::default()
    });
    assert!(mods.command("corner"));
    let panels: Vec<_> = mods.corner_panels().cloned().collect();
    assert_eq!(panels.len(), 1);
    // "60 fps": five glyphs and a space, plus the box's 4.
    assert_eq!(
        (panels[0].width, panels[0].height),
        (5.0 * 5.0 + 3.0 + 4.0, 12.0)
    );
    assert_eq!(panels[0].shapes.len(), 2);
    assert!(matches!(
        panels[0].corner,
        horizon_mods::RenderCorner::TopRight
    ));
    assert!(matches!(
        &mods.settings_widgets(0)[0],
        horizon_mods::UiWidget::Button(b) if b.id == 3 && b.label == "Corner: Top Right"
    ));
    mods.drain();
    assert!(mods.command("readings"));
    assert_eq!(
        mods.drain(),
        [Output::Chat {
            name: "Test UI Guest".into(),
            text: "60 41 52; forward W true; cps 7 2; other true".into(),
        }]
    );
}

#[test]
fn content_only_mods_load_without_an_engine() {
    let manifest = r#"
id = "ruby"
name = "Ruby"
version = "0.1.0"
api = "0.2"
side = "both"
capabilities = ["registries"]
"#;
    let block: &[u8] = br#"{"template": "minecraft:iron_block"}"#;
    let dir = mods_dir().join("content");
    std::fs::create_dir_all(&dir).unwrap();
    let ok = dir.join("ruby.hmod");
    let files: &[(&str, &[u8])] = &[("data/ruby/horizon/block/ruby_block.json", block)];
    std::fs::write(&ok, Package::write_files(manifest, None, files).unwrap()).unwrap();
    let bad = dir.join("bad.hmod");
    let files: &[(&str, &[u8])] = &[("data/minecraft/recipe/stone.json", b"{}")];
    let bad_manifest = manifest.replace("\"ruby\"", "\"bad\"");
    std::fs::write(
        &bad,
        Package::write_files(&bad_manifest, None, files).unwrap(),
    )
    .unwrap();
    let mods = Mods::load(&[ok, bad], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert!(matches!(&mods.entries()[1].status, Status::Failed(e) if e.contains("own namespace")));
    // Content has no hooks: the game's per-tick mod work stays skipped.
    assert!(!mods.is_running());
}

/// Toggle Sprint holds sprint for the player on its key, and reads the game's controls: its
/// panel says sneak is held with the key.
#[test]
fn toggle_sprint_holds_and_reads_controls() {
    let dir = mods_dir().join("toggle_sprint_run");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("toggle_sprint.hmod");
    std::fs::copy(mods_dir().join("toggle_sprint.hmod"), &path).unwrap();
    let mut mods = Mods::load(&[path], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    let hold = |control: &str, held| Output::Hold {
        id: "toggle_sprint".into(),
        control: control.into(),
        held,
    };
    // Nothing saved yet: both let go.
    assert_eq!(
        mods.drain(),
        [hold("key.sprint", false), hold("key.sneak", false)]
    );
    mods.keybind("toggle_sprint", "sprint", true);
    assert_eq!(
        mods.drain(),
        [hold("key.sprint", true), hold("key.sneak", false)]
    );
    let mut controls = horizon_mods::Controls::default();
    let sneak = horizon_mods::CONTROLS
        .iter()
        .position(|c| *c == "key.sneak")
        .unwrap();
    controls[sneak] = (true, 1);
    mods.set_snapshot(player());
    mods.set_controls(controls);
    mods.tick(1);
    let lines: Vec<String> = mods
        .panels()
        .flat_map(|p| p.lines.iter().map(|l| l.text.clone()))
        .collect();
    assert_eq!(lines, ["[Sneaking (Key Held)]", "[Sprinting (Toggled)]"]);
}

/// A JavaScript mod passing the game strings every frame keeps its memory: with componentize-js
/// before 0.23 each string a call took leaked, and a mod drawing text ran out in minutes.
#[test]
fn javascript_mods_keep_their_memory() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = mods_dir().join("js-strings");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"strings\"
name = \"Strings\"
version = \"1\"
api = \"starling-0.1\"
capabilities = [\"hud\"]
",
    )
    .unwrap();
    std::fs::write(
        dir.join("mod.js"),
        r#"import { textWidth } from "starling:mods/draw@0.1.0";
export const events = {
  init() {
    return {
      commands: [], tick: false, frame: false, chat: false, chatSend: false, hud: true,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  hudRender(_context) { for (let i = 0; i < 100; i++) textWidth("Clicks per second"); },
};
"#,
    )
    .unwrap();
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    // 400 000 strings: about 50 MiB leaked before.
    for _ in 0..4000 {
        mods.hud_render(context());
    }
    assert_eq!(mods.drain(), []);
    assert_eq!(mods.entries()[0].status, Status::Running);
}

/// The creative tabs a test mod reads: two of building blocks, and the search over them.
struct Tabs;
impl horizon_mods::CreativeCatalog for Tabs {
    fn tabs(&self) -> Vec<horizon_mods::Tab> {
        let tab = |id: &str, kind, column| horizon_mods::Tab {
            id: id.into(),
            title: id.into(),
            icon: "minecraft:stone".into(),
            kind,
            top: true,
            column,
            size: 2,
        };
        vec![
            tab(
                "minecraft:building_blocks",
                horizon_mods::TabKind::Category,
                0,
            ),
            tab("minecraft:search", horizon_mods::TabKind::Search, 6),
            tab("minecraft:inventory", horizon_mods::TabKind::Inventory, 6),
        ]
    }
    fn items(&self, tab: &str, search: &str, start: usize, count: usize) -> horizon_mods::ItemPage {
        let all: Vec<ItemStack> = ["stone", "dirt"]
            .iter()
            .filter(|id| tab == "minecraft:building_blocks" || id.contains(search))
            .map(|id| stack(&format!("minecraft:{id}"), 1))
            .collect();
        horizon_mods::ItemPage {
            total: all.len() as u32,
            items: all.into_iter().skip(start).take(count).collect(),
        }
    }
    /// The search tab's order, past its two items too (for sorting).
    fn order(&self, stack: &ItemStack) -> Option<u32> {
        ORDER
            .iter()
            .position(|id| stack.id == format!("minecraft:{id}"))
            .map(|p| p as u32)
    }
}
/// The order the test tabs give items (`inventory.get-item-order`).
const ORDER: [&str; 8] = [
    "stone",
    "dirt",
    "cobblestone",
    "oak_log",
    "oak_planks",
    "bow",
    "arrow",
    "bread",
];
/// Items stacking to 64, for the `creative` checks.
struct Items;
impl horizon_mods::ItemCatalog for Items {
    fn info(&self, id: &str) -> Option<horizon_mods::ItemInfo> {
        Some(horizon_mods::ItemInfo {
            id: id.into(),
            name: id.into(),
            max_stack_size: 64,
            max_damage: 0,
            food: None,
        })
    }
}
fn stack(id: &str, count: u32) -> ItemStack {
    ItemStack {
        id: id.into(),
        count,
        damage: 0,
        max_damage: 0,
        name: id.into(),
        components: String::new(),
    }
}
/// A player in `mode` with the test tabs, the items and an empty inventory menu.
fn creative_world(mode: GameMode) -> Snapshot {
    let mut world = player();
    if let Some(p) = &mut world.player {
        p.game_mode = mode;
    }
    world.creative = Some(std::sync::Arc::new(Tabs));
    world.items = Some(std::sync::Arc::new(Items));
    world.menu = Some(horizon_mods::Menu {
        kind: "minecraft:inventory".into(),
        slots: vec![None; 46],
        carried: None,
    });
    world
}

/// `creative`: the tabs and the search read, items taken only in creative mode and only as the
/// creative screen may (slots 1 to 45, no more than a stack); `screens.replace` takes only the
/// creative screen; `draw.player` and `draw.item-tooltip` drawn.
#[test]
fn creative_takes_items_as_the_creative_screen_does() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = mods_dir().join("js-creative");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"probe\"
name = \"Probe\"
version = \"1\"
api = \"starling-0.1\"
capabilities = [\"creative\", \"hud\", \"ui\"]
",
    )
    .unwrap();
    std::fs::write(
        dir.join("mod.js"),
        r#"import { getTabs, getItems, click, setSlot, setCarried } from "starling:mods/creative@0.1.0";
import { replace } from "starling:mods/screens@0.1.0";
import { fill, item, itemId, player, itemTooltip } from "starling:mods/draw@0.1.0";
const tryIt = (f) => { try { f(); return "ok"; } catch (e) { return `${e.payload === undefined ? e : e.payload}`; } };
export const events = {
  init() {
    return {
      commands: ["probe"], tick: false, frame: false, chat: false, chatSend: false, hud: true,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  command(name, args) {
    const stone = { id: "minecraft:stone", count: 64, damage: 0, maxDamage: 0, name: "Stone", components: "" };
    switch (args) {
      case "read": {
        const tabs = getTabs().map((t) => `${t.id}:${t.kind}`).join(",");
        const page = getItems("minecraft:search", "dir", 0, 10);
        const all = getItems("minecraft:building_blocks", "ignored", 1, 10);
        return `${tabs}|${page.total} ${page.items.map((s) => s.id)}|${all.total} ${all.items.map((s) => s.id)}`;
      }
      case "take":
        return [
          tryIt(() => click("minecraft:building_blocks", "", 1, 0, "quick-move")),
          tryIt(() => setSlot(36, stone)),
          tryIt(() => setSlot(0, stone)),
          tryIt(() => setSlot(46, stone)),
          tryIt(() => setSlot(9, { ...stone, count: 65 })),
          tryIt(() => setCarried(undefined)),
        ].join(",");
      case "replace":
        return [replace("pause", "x"), replace("creative", "Bad Name"), replace("creative", "inv")].join(",");
      case "give back":
        return `${replace("creative", "")}`;
    }
  },
  hudRender(context) {
    const stone = { id: "minecraft:stone", count: 3, damage: 0, maxDamage: 0, name: "Stone", components: "{\"minecraft:rarity\":\"epic\"}" };
    fill(0, 0, 1, 1, 0xFF000000);
    item(1, 2, stone);
    itemId(3, 4, "minecraft:dirt");
    fill(5, 6, 1, 1, 0xFF000000);
    player(10, 20, 30, 40, 25, 15);
    itemTooltip(7, 8, stone);
  },
};
"#,
    )
    .unwrap();
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    let feedback = |mods: &mut Mods, args: &str| -> (String, Vec<Output>) {
        assert!(mods.command(&format!("probe {args}")));
        let mut said = None;
        let rest = mods
            .drain()
            .into_iter()
            .filter(|o| match o {
                Output::Feedback { text, .. } => {
                    said = Some(text.clone());
                    false
                }
                _ => true,
            })
            .collect();
        (said.unwrap_or_default(), rest)
    };

    mods.set_snapshot(creative_world(GameMode::Survival));
    let (read, _) = feedback(&mut mods, "read");
    assert_eq!(
        read,
        "minecraft:building_blocks:category,minecraft:search:search,minecraft:inventory:inventory\
         |1 minecraft:dirt|2 minecraft:dirt"
    );
    let (took, outputs) = feedback(&mut mods, "take");
    let survival = "only in creative mode";
    assert_eq!(took, [survival; 6].join(","));
    assert_eq!(outputs, []);

    mods.set_snapshot(creative_world(GameMode::Creative));
    let (took, outputs) = feedback(&mut mods, "take");
    assert!(
        took.starts_with("ok,ok,the inventory menu's slot 0"),
        "{took}"
    );
    assert!(took.contains(",the inventory menu's slot 46"), "{took}");
    assert!(
        took.ends_with(",65 of minecraft:stone (it stacks to 64),ok"),
        "{took}"
    );
    assert_eq!(
        outputs,
        [
            Output::CreativeClick {
                id: "probe".into(),
                tab: "minecraft:building_blocks".into(),
                search: String::new(),
                index: 1,
                button: 0,
                kind: horizon_mods::CreativeClick::QuickMove,
            },
            Output::CreativeSlot {
                id: "probe".into(),
                slot: 36,
                stack: Some(ItemStack {
                    name: "Stone".into(),
                    ..stack("minecraft:stone", 64)
                }),
            },
            Output::CreativeCarried {
                id: "probe".into(),
                stack: None,
            },
        ]
    );

    let (replaced, _) = feedback(&mut mods, "replace");
    assert_eq!(replaced, "false,false,true");
    assert_eq!(mods.replacement("creative").as_deref(), Some("probe:inv"));
    assert_eq!(mods.replacement("pause"), None);
    let (given, _) = feedback(&mut mods, "give back");
    assert_eq!(given, "true");
    assert_eq!(mods.replacement("creative"), None);

    // Items go in the render hook's queue, in order with the fills, as their calls drew them.
    let ops = mods.hud_render(context());
    let summary: Vec<String> = ops
        .iter()
        .map(|op| match op {
            DrawOp::Fill { x, .. } => format!("fill {x}"),
            DrawOp::Item { x, y, stack } => format!(
                "item {x} {y} {} {} {}",
                stack.id, stack.count, stack.components
            ),
            DrawOp::ItemId { x, y, id } => format!("item-id {x} {y} {id}"),
            DrawOp::Player {
                x,
                y,
                w,
                h,
                size,
                turn,
            } => format!("player {x} {y} {w} {h} {size} {turn}"),
            DrawOp::ItemTooltip { x, y, stack } => format!("tooltip {x} {y} {}", stack.id),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        summary,
        [
            "fill 0",
            "item 1 2 minecraft:stone 3 {\"minecraft:rarity\":\"epic\"}",
            "item-id 3 4 minecraft:dirt",
            "fill 5",
            "player 10 20 30 40 25 15",
            "tooltip 7 8 minecraft:stone",
        ]
    );
}

/// `slots.drag`: the mouse's drag across slots (QUICK_CRAFT), its clicks (its start, a slot each,
/// its end) counted with a call's other clicks; a button past 2 or a slot no menu has refused.
/// `mobile.drag-areas`: the last call's boxes, the bad ones left out. `widgets.add-window-button`
/// refuses a button without a size or with a bad menu name; `inventory.get-item-order` is the
/// creative search tab's place.
#[test]
fn slots_drag_is_the_mouses_drag() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = mods_dir().join("js-drag");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"probe\"
name = \"Probe\"
version = \"1\"
api = \"starling-0.1\"
capabilities = [\"inventory-click\", \"inventory-read\", \"ui\"]
",
    )
    .unwrap();
    std::fs::write(
        dir.join("mod.js"),
        r#"import { click, drag } from "starling:mods/slots@0.1.0";
import { dragAreas } from "starling:mods/mobile@0.1.0";
import { getItemOrder } from "starling:mods/inventory@0.1.0";
import { addWindowButton } from "starling:mods/widgets@0.1.0";
const tryIt = (f) => { try { f(); return "ok"; } catch (e) { return `${e.payload === undefined ? e : e.payload}`; } };
export const events = {
  init() {
    return {
      commands: ["probe"], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  command(name, args) {
    if (args === "areas") {
      dragAreas([[0, 0, 10, 10], [1, 2, -3, 4]]);
      dragAreas([[5, 6, 7, 8], [1, 2, 0, 4]]);
      return "ok";
    }
    if (args === "window") {
      const one = (id) => ({ id, label: "", tooltip: [], icon: "", menus: [] });
      const order = (id) => `${getItemOrder({ id, count: 1, damage: 0, maxDamage: 0, name: "", components: "" })}`;
      return [
        tryIt(() => addWindowButton({ ...one("a"), part: "container", anchor: "top-right", x: -1, y: -13, width: 12, height: 12 })),
        tryIt(() => addWindowButton({ ...one("b"), part: "inventory", anchor: "top-right", x: -1, y: -13, width: 0, height: 12 })),
        tryIt(() => addWindowButton({ ...one("c"), part: "inventory", anchor: "top-right", x: 0, y: 0, width: 4, height: 4, menus: ["Not A Menu"] })),
        order("minecraft:dirt"), order("minecraft:bread"), order("minecraft:barrier"),
      ].join(",");
    }
    const many = Array.from({ length: 60 }, (_, i) => i + 1);
    return [
      tryIt(() => drag([1, 2, 3], 3)),
      tryIt(() => drag([1, 200], 0)),
      tryIt(() => drag([1, 2, 3], 1)),
      tryIt(() => click(5, 0, "pickup")),
      tryIt(() => drag(many, 0)),
    ].join(",");
  },
};
"#,
    )
    .unwrap();
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert!(mods.command("probe drag"));
    let mut said = None;
    let rest: Vec<Output> = mods
        .drain()
        .into_iter()
        .filter(|o| match o {
            Output::Feedback { text, .. } => {
                said = Some(text.clone());
                false
            }
            _ => true,
        })
        .collect();
    // 5 clicks queued (3 slots and the drag's start and end) and 1: 60 more and 2 are too many.
    assert_eq!(
        said.as_deref(),
        Some("no drag with button 3: 0 to 2,no menu has slot 200,ok,ok,at most 64 clicks a call")
    );
    assert_eq!(
        rest,
        [
            Output::Drag {
                id: "probe".into(),
                slots: vec![1, 2, 3],
                button: 1,
            },
            Output::Click {
                id: "probe".into(),
                slot: 5,
                button: 0,
                kind: horizon_mods::ClickKind::Pickup,
            },
        ]
    );
    let world = creative_world(GameMode::Survival);
    mods.set_snapshot(world);
    assert!(mods.command("probe window"));
    let said: Vec<String> = mods
        .drain()
        .into_iter()
        .filter_map(|o| match o {
            Output::Feedback { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        said,
        ["ok,button b has no size,\"Not A Menu\" is not a menu's name,1,7,undefined"]
    );
    assert_eq!(
        mods.widgets()
            .flat_map(|(_, w)| w.window.iter().map(|b| b.id.clone()))
            .collect::<Vec<_>>(),
        ["a"]
    );
    assert!(mods.command("probe areas"));
    let areas: Vec<Output> = mods
        .drain()
        .into_iter()
        .filter(|o| matches!(o, Output::DragAreas { .. }))
        .collect();
    assert_eq!(
        areas,
        [Output::DragAreas {
            id: "probe".into(),
            areas: vec![[5.0, 6.0, 7.0, 8.0]],
        }]
    );
}

/// A level of stone floor at y 63 under air, lit as the mod is told.
struct Floor;
impl horizon_mods::LevelView for Floor {
    fn block(&self, pos: [i32; 3]) -> Option<String> {
        Some(
            if pos[1] == 63 {
                "minecraft:stone"
            } else {
                "minecraft:air"
            }
            .into(),
        )
    }
    fn biome(&self, _: [i32; 3]) -> Option<String> {
        Some("minecraft:plains".into())
    }
    fn light(&self, pos: [i32; 3]) -> Option<(u8, u8)> {
        Some((0, if pos[0] < 0 { 0 } else { 15 }))
    }
    fn spawn_spots(&self, from: [i32; 3], to: [i32; 3]) -> horizon_mods::SpawnSpots {
        let mut out = horizon_mods::SpawnSpots::default();
        for y in from[1]..=to[1] {
            for z in from[2]..=to[2] {
                for x in from[0]..=to[0] {
                    if y == 64 {
                        let (block_light, sky_light) = self.light([x, y, z]).unwrap();
                        out.push([x, y, z], block_light, sky_light, 0.0);
                    }
                }
            }
        }
        out
    }
}

#[test]
fn world_layers_keep_their_lines_and_labels() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = mods_dir().join("js-world-draw");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"probe\"
name = \"Probe\"
version = \"1\"
api = \"starling-0.1\"
capabilities = [\"world-draw\", \"world-read\"]
",
    )
    .unwrap();
    std::fs::write(
        dir.join("mod.js"),
        r#"import { setLayer, removeLayer, setIcons } from "starling:mods/world-draw@0.1.0";
import { getSpawnSpots } from "starling:mods/level@0.1.0";
const tryIt = (f) => { try { f(); return "ok"; } catch (e) { return `${e.payload === undefined ? e : e.payload}`; } };
export const events = {
  init() {
    return {
      commands: ["probe"], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  command(name, args) {
    if (args === "spots") {
      const s = getSpawnSpots({ x: 1, y: 70, z: 2 }, { x: -1, y: 60, z: 1000 });
      const n = s.skyLight.length, p = s.positions;
      return `${n},${p.length},${p[0]},${p[2]},${s.skyLight[0]},${p[p.length - 1]},${p.constructor.name},${s.tops.constructor.name}`;
    }
    if (args === "remove") {
      removeLayer("a");
      return "ok";
    }
    const origin = { x: 10, y: 64, z: -5 };
    return [
      tryIt(() => setLayer("a", {
        lines: [{ origin, ends: [0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1], colors: [0xFFFF0000, 0x80FFFF00] }],
        boxes: [{ origin, corners: [0, 0, 0, 1, 0.0625, 1], colors: [0x7F00FF00] }],
        texts: [{ origin, places: [0.5, 0, 0.5, 1.5, 0, 0.5], lines: ["7", "15"], colors: [0xFFFF6600], facing: "up" }],
        labels: [{ at: origin, text: "§aHome", fixedSize: true, background: 0xC0000000 }],
        throughWalls: true,
      })),
      tryIt(() => setLayer("e", { texts: [{ origin, places: [0, 0, 0, 1, 0, 0], lines: ["a"], colors: [1] }] })),
      tryIt(() => setLayer("f", { boxes: [{ origin, corners: [0, 0, 0, 1, 1, 1], colors: [1, 2] }] })),
      tryIt(() => setLayer("b", { lines: [{ origin, ends: [0, 0, 0, 1, 0], colors: [1] }] })),
      tryIt(() => setLayer("b", { lines: [{ origin, ends: [0, 0, 0, 1, 0, 1], colors: [1, 2] }] })),
      tryIt(() => setLayer("", {})),
      tryIt(() => setLayer("c", { lines: [{ origin, ends: new Float32Array(6 * 70000), colors: [1] }] })),
      tryIt(() => setLayer("d", { labels: [{ at: { x: NaN, y: 0, z: 0 }, text: "x" }] })),
      tryIt(() => setIcons("a", [{ at: origin, sprite: "probe:mark", color: 0xFF00FF00, size: 9,
        above: "Up", below: "Down", fixedSize: true, throughWalls: true, background: 0x80000000 }])),
      tryIt(() => setIcons("g", [{ at: origin, size: 0 }])),
    ].join(",");
  },
};
"#,
    )
    .unwrap();
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    let said = |mods: &mut Mods| -> Vec<String> {
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Feedback { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    assert!(mods.command("probe draw"));
    assert_eq!(
        said(&mut mods),
        [
            "ok,2 places for texts, 1 lines,boxes need one colour for all, or one a box (2 for 1),\
lines need a place and six finite numbers a line,lines need one colour for all, or one a line (2 for 1),\
\"\" is not a layer's id (1 to 64 characters),\
a mod draws at most 65536 lines, 16384 boxes, 16384 texts and 256 labels in the world,\
label \"x\" needs a place and a size,ok,icon \"\" needs a place, a size and a text size"
        ]
    );
    let layers: Vec<_> = mods
        .world_layers()
        .flat_map(|(id, l)| {
            l.iter()
                .map(move |(k, v)| (id.to_owned(), k.clone(), v.clone()))
        })
        .collect();
    assert_eq!(layers.len(), 1);
    let (id, name, layer) = &layers[0];
    assert_eq!((id.as_str(), name.as_str()), ("probe", "a"));
    assert!(layer.through_walls);
    assert_eq!(
        layer.lines[0].ends,
        [0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(layer.lines[0].colors, [0xFFFF_0000, 0x80FF_FF00]);
    assert_eq!(layer.lines[0].width, 1.0);
    let label = &layer.labels[0];
    assert_eq!(
        (label.text.as_str(), label.scale, label.fixed_size),
        ("§aHome", 1.0, true)
    );
    assert_eq!(label.background, 0xC000_0000);
    assert_eq!((label.at.x, label.at.y, label.at.z), (10.0, 64.0, -5.0));
    assert_eq!(label.facing, horizon_mods::Facing::Camera);
    assert_eq!(layer.boxes[0].corners, [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]);
    assert_eq!(layer.boxes[0].colors, [0x7F00_FF00]);
    let texts = &layer.texts[0];
    assert_eq!(
        (texts.lines.as_str(), texts.facing),
        ("7\n15", horizon_mods::Facing::Up)
    );
    assert_eq!(
        (texts.places.len(), texts.colors[0], texts.scale),
        (6, 0xFFFF_6600, 1.0)
    );
    let icons: Vec<_> = mods.world_icons().flat_map(|(_, l)| l.to_vec()).collect();
    assert_eq!(icons.len(), 1);
    let icon = &icons[0].1[0];
    assert_eq!(
        (
            icons[0].0.as_str(),
            icon.sprite.as_str(),
            icon.color,
            icon.size
        ),
        ("a", "probe:mark", 0xFF00_FF00, 9.0)
    );
    assert_eq!((icon.above.as_str(), icon.below.as_str()), ("Up", "Down"));
    assert_eq!((icon.text_scale, icon.background), (1.0, 0x8000_0000));
    assert!(icon.fixed_size && icon.through_walls);
    // Removing a layer removes its icons.
    assert!(mods.command("probe remove"));
    said(&mut mods);
    assert_eq!(mods.world_layers().map(|(_, l)| l.len()).sum::<usize>(), 0);
    assert_eq!(mods.world_icons().map(|(_, l)| l.len()).sum::<usize>(), 0);
    // The spots of a box at most 64 a side, the corners in any order, the lowest first.
    let mut world = player();
    world.level = Some(std::sync::Arc::new(Floor));
    mods.set_snapshot(world);
    assert!(mods.command("probe spots"));
    assert_eq!(
        said(&mut mods),
        ["192,576,-1,2,0,65,Int32Array,Float32Array"]
    );
}

/// Floors of stone at `floors` under air, dark west of x 12 (sky light 0) and lit by a torch's
/// 3 north of z -3, the rest in the sky's light; mushroom fields from x 20.
struct Floors {
    floors: Vec<i32>,
}
impl horizon_mods::LevelView for Floors {
    fn block(&self, pos: [i32; 3]) -> Option<String> {
        Some(
            if self.floors.contains(&(pos[1] - 1)) {
                "minecraft:air"
            } else {
                "minecraft:stone"
            }
            .into(),
        )
    }
    fn biome(&self, pos: [i32; 3]) -> Option<String> {
        // From a biome cell's edge, as the game's are.
        Some(
            if pos[0] >= 20 {
                "minecraft:mushroom_fields"
            } else {
                "minecraft:plains"
            }
            .into(),
        )
    }
    fn light(&self, pos: [i32; 3]) -> Option<(u8, u8)> {
        Some((
            if pos[2] < -3 { 3 } else { 0 },
            if pos[0] < 12 { 0 } else { 15 },
        ))
    }
    fn spawn_spots(&self, from: [i32; 3], to: [i32; 3]) -> horizon_mods::SpawnSpots {
        let mut out = horizon_mods::SpawnSpots::default();
        for y in from[1]..=to[1] {
            for z in from[2]..=to[2] {
                for x in from[0]..=to[0] {
                    if self.floors.contains(&(y - 1)) {
                        let (block_light, sky_light) = self.light([x, y, z]).unwrap();
                        let top = if x == 12 && z == -3 { 0.125 } else { 0.0 };
                        out.push([x, y, z], block_light, sky_light, top);
                    }
                }
            }
        }
        out
    }
}

/// A copy of an example in a folder of its own, its storage empty.
fn example(name: &str, test: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/starling-mod-sdk/examples")
        .join(name);
    // Its storage beside it fresh too.
    let home = mods_dir().join(format!("{name}-{test}"));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for file in std::fs::read_dir(&from).unwrap() {
        let file = file.unwrap();
        if file.file_type().unwrap().is_file() {
            std::fs::copy(file.path(), dir.join(file.file_name())).unwrap();
        }
    }
    dir
}

/// Light Overlay marks the places monsters spawn in Lighty's colours, within its reach, in each
/// of its modes, and keeps within a tick's time on open ground.
#[test]
fn light_overlay_marks_where_monsters_spawn() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = example("light-overlay", "marks");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(
        std::slice::from_ref(&dir),
        |_| true,
        |_| false,
        Some(&cache),
    );
    assert_eq!(mods.entries()[0].status, Status::Running);
    let mut world = player();
    world.level = Some(std::sync::Arc::new(Floors { floors: vec![63] }));
    mods.set_snapshot(world.clone());
    mods.keybind("light_overlay", "toggle", true);
    let layer = |mods: &Mods| -> horizon_mods::WorldLayer {
        let layers: Vec<_> = mods.world_layers().flat_map(|(_, l)| l.to_vec()).collect();
        assert_eq!(layers.len(), 1, "{:?}", mods.entries()[0].status);
        layers[0].1.clone()
    };
    let crosses = layer(&mods);
    let lines = &crosses.lines[0];
    // From the player's block (12, 64, -4): the floor's places closer than 12 blocks, but for
    // the mushroom fields from x 20.
    assert_eq!(
        (lines.origin.x, lines.origin.y, lines.origin.z),
        (12.0, 64.0, -4.0)
    );
    let places = (-11..=11)
        .flat_map(|dx: i32| (-11..=11).map(move |dz: i32| (dx, dz)))
        .filter(|(dx, dz)| dx * dx + dz * dz < 144 && 12 + dx < 20)
        .count();
    assert_eq!(lines.ends.len() / 12, places);
    assert_eq!(lines.width, 2.0);
    // A cross: corner to corner, a hair above the ground (a snow layer's top at (12, 64, -3)).
    let at = |dx: f32, dz: f32| {
        (0..places)
            .find(|&i| lines.ends[i * 12] == dx + 0.01 && lines.ends[i * 12 + 2] == dz + 0.01)
    };
    let i = at(0.0, 1.0).unwrap();
    assert_eq!(
        &lines.ends[i * 12..i * 12 + 12],
        &[
            0.01, 0.13, 1.01, 0.99, 0.13, 1.99, 0.99, 0.13, 1.01, 0.01, 0.13, 1.99
        ]
    );
    // Red in the dark, green by the torch, orange under the sky.
    let color = |dx: f32, dz: f32| lines.colors[at(dx, dz).unwrap() * 2];
    assert_eq!(color(-1.0, 2.0), 0xFFFF_0000);
    assert_eq!(color(-1.0, -2.0), 0xFF00_FF00);
    assert_eq!(color(1.0, 2.0), 0xFFFF_6600);
    // The settings: the mode button (centred on a 320 by 240 GUI) goes to the next mode.
    mods.keybind("light_overlay", "settings", true);
    let opened = mods
        .drain()
        .into_iter()
        .any(|o| matches!(o, Output::OpenScreen { screen } if screen == "light_overlay:settings"));
    assert!(opened);
    let screen = "light_overlay:settings";
    mods.screen_render(screen, context());
    let press = |mods: &mut Mods, x: f32, y: f32| {
        let event = |action| MouseEvent {
            action,
            x,
            y,
            button: 0,
            scroll: 0.0,
        };
        mods.screen_mouse(screen, event(MouseAction::Press));
        mods.screen_mouse(screen, event(MouseAction::Release));
    };
    // Numbers: the block light over the sky light, lying face up.
    press(&mut mods, 100.0, 90.0);
    let numbers = layer(&mods);
    assert!(numbers.lines.is_empty() && numbers.boxes.is_empty());
    let texts = &numbers.texts[0];
    assert_eq!(texts.places.len() / 3, places * 2);
    assert_eq!(
        (texts.facing, texts.scale),
        (horizon_mods::Facing::Up, 1.25)
    );
    let lines: Vec<&str> = texts.lines.split('\n').collect();
    // The player looks north (yaw 180): the block light beyond, the sky light before.
    let k = (0..places * 2)
        .find(|&k| texts.places[k * 3] == -0.5 && texts.places[k * 3 + 2] == -1.5 - 0.1875)
        .unwrap();
    assert_eq!(
        (lines[k], lines[k + 1], texts.colors[k]),
        ("3", "0", 0xFF00_FF00)
    );
    assert_eq!(texts.places[(k + 1) * 3 + 2], -1.5 + 0.1875);
    // Tiles: a see-through pixel a hair above the ground, its top's edges solid.
    press(&mut mods, 100.0, 90.0);
    let tiles = layer(&mods);
    let boxes = &tiles.boxes[0];
    assert_eq!(boxes.corners.len() / 6, places);
    let b = (0..places)
        .find(|&b| boxes.corners[b * 6] == -1.0 && boxes.corners[b * 6 + 2] == 2.0)
        .unwrap();
    assert_eq!(
        &boxes.corners[b * 6..b * 6 + 6],
        &[-1.0, 0.002, 2.0, 0.0, 0.0645, 3.0]
    );
    assert_eq!(boxes.colors[b], 0x4BFF_0000);
    assert_eq!(tiles.lines[0].ends.len() / 6, places * 4);
    // Markers: a see-through block.
    press(&mut mods, 100.0, 90.0);
    let markers = layer(&mods);
    let boxes = &markers.boxes[0];
    assert_eq!(
        &boxes.corners[b * 6..b * 6 + 6],
        &[-1.0, 0.002, 2.0, 0.0, 1.002, 3.0]
    );
    assert_eq!(boxes.colors[b], 0x36FF_0000);
    // Green hidden: the torch's places go.
    press(&mut mods, 100.0, 114.0);
    let lit = (-11..=11)
        .flat_map(|dx: i32| (-11..=11).map(move |dz: i32| (dx, dz)))
        .filter(|(dx, dz)| dx * dx + dz * dz < 144 && 12 + dx < 20 && -4 + dz < -3)
        .count();
    assert_eq!(layer(&mods).boxes[0].corners.len() / 6, places - lit);
    // Kept for the next start; toggled off, the marks go.
    press(&mut mods, 100.0, 114.0);
    press(&mut mods, 100.0, 90.0);
    mods.keybind("light_overlay", "toggle", true);
    assert_eq!(mods.world_layers().map(|(_, l)| l.len()).sum::<usize>(), 0);
    drop(mods);
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    // Three floors in reach, back in the cross mode: a look each tick the player gets to another
    // block, each within a tick's fuel.
    let mut world = player();
    world.level = Some(std::sync::Arc::new(Floors {
        floors: vec![55, 63, 70],
    }));
    mods.set_snapshot(world.clone());
    mods.keybind("light_overlay", "toggle", true);
    assert_eq!(layer(&mods).lines[0].width, 2.0);
    let start = Instant::now();
    for t in 0..10 {
        world.player.as_mut().unwrap().x -= 1.0;
        mods.set_snapshot(world.clone());
        mods.tick(t);
    }
    eprintln!("light overlay: {:?} a look", start.elapsed() / 10);
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert_eq!(layer(&mods).lines[0].origin.x, 2.0);
    world.player.as_mut().unwrap().x = 12.5;
    mods.set_snapshot(world.clone());
    mods.tick(10);
    let crosses = layer(&mods).lines[0].ends.len() / 12;
    let expected = [-8i32, 0, 7]
        .into_iter()
        .flat_map(|dy| (-11..=11).flat_map(move |dx| (-11..=11).map(move |dz| (dx, dy, dz))))
        .filter(|(dx, dy, dz)| dx * dx + dy * dy + dz * dz < 144 && 12 + dx < 20)
        .count();
    assert_eq!(crosses, expected);
}

/// A copy of the Bedrock Inventory example in a folder of its own, its storage (kept beside the
/// mod: the settings a game with it as a dev mod saved) fresh.
fn bedrock_inventory(test: &str) -> PathBuf {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/starling-mod-sdk/examples/bedrock-inventory");
    let home = mods_dir().join(format!("bedrock-{test}"));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("bedrock-inventory");
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["horizon-mod.toml", "mod.js"] {
        std::fs::copy(example.join(file), dir.join(file)).unwrap();
    }
    dir
}

/// Bedrock Inventory stands in for the creative screen and takes from the tabs with the mouse.
#[test]
fn bedrock_inventory_takes_from_the_creative_tabs() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let example = bedrock_inventory("takes");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[example], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert_eq!(
        mods.replacement("creative").as_deref(),
        Some("bedrock_inventory:inventory")
    );
    let mut world = creative_world(GameMode::Creative);
    if let Some(menu) = &mut world.menu {
        menu.slots[36] = Some(stack("minecraft:bow", 1));
    }
    mods.set_snapshot(world);
    let screen = "bedrock_inventory:inventory";
    mods.screen_opened(screen);
    // A phone's GUI: the creative window (the building blocks' two items, no hotbar row: the
    // hotbar is the inventory's) and its inventory tab's window beside it.
    let phone = RenderContext {
        width: 533,
        height: 240,
        mouse_x: 0.0,
        mouse_y: 0.0,
        delta: 0.016,
    };
    let ops = mods.screen_render(screen, phone);
    // The screen says where it draws the slots (screens.set-slots): the inventory's first at
    // (277, 106), for other mods' window buttons.
    let declared = mods.drain();
    assert!(
        declared.iter().any(
            |o| matches!(o, Output::ScreenSlots { slots, .. } if slots.contains(&(9, 277.0, 106.0)))
        ),
        "{declared:?}"
    );
    let items: Vec<(String, f32, f32)> = ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Item { x, y, stack } => Some((stack.id.clone(), *x, *y)),
            _ => None,
        })
        .collect();
    for shown in [
        ("minecraft:stone", 78.0, 70.0),
        ("minecraft:dirt", 96.0, 70.0),
        ("minecraft:bow", 277.0, 164.0),
    ] {
        assert!(
            items.contains(&(shown.0.into(), shown.1, shown.2)),
            "{shown:?} in {items:?}"
        );
    }
    assert_eq!(
        items.iter().filter(|i| i.0 == "minecraft:bow").count(),
        1,
        "{items:?}"
    );
    assert!(ops.iter().any(|op| matches!(op, DrawOp::Player { .. })));
    // A click on the dirt takes it onto the cursor, as the creative screen's grid does.
    let press = |x, y, action| MouseEvent {
        action,
        x,
        y,
        button: 0,
        scroll: 0.0,
    };
    mods.screen_mouse(screen, press(100.0, 76.0, MouseAction::Press));
    mods.screen_mouse(screen, press(100.0, 76.0, MouseAction::Release));
    assert_eq!(
        mods.drain(),
        [Output::CreativeClick {
            id: "bedrock_inventory".into(),
            tab: "minecraft:building_blocks".into(),
            search: String::new(),
            index: 1,
            button: 0,
            kind: horizon_mods::CreativeClick::Pickup,
        }]
    );
    // Carried, it goes into the slot the button is let go over.
    let mut world = creative_world(GameMode::Creative);
    if let Some(menu) = &mut world.menu {
        menu.carried = Some(stack("minecraft:dirt", 1));
    }
    mods.set_snapshot(world);
    mods.screen_mouse(screen, press(283.0, 112.0, MouseAction::Press));
    assert_eq!(mods.drain(), []);
    mods.screen_mouse(screen, press(283.0, 112.0, MouseAction::Release));
    let clicked: Vec<(i32, u8)> = mods
        .drain()
        .into_iter()
        .filter_map(|o| match o {
            Output::Click {
                slot,
                button,
                kind: horizon_mods::ClickKind::Pickup,
                ..
            } => Some((slot, button)),
            _ => None,
        })
        .collect();
    assert_eq!(clicked, [(9, 0)]);
}

/// Bedrock Inventory on a touch screen: a finger held on a stack chooses how much of it, a picked
/// stack drawn across slots is shared out among them (`slots.drag`), and the tabs' items take
/// away a stack put on them.
#[test]
fn bedrock_inventory_fingers_choose_spread_and_take_away() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let example = bedrock_inventory("fingers");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[example], |_| true, |_| false, Some(&cache));
    let mut world = creative_world(GameMode::Creative);
    world.touch = true;
    if let Some(menu) = &mut world.menu {
        menu.slots[36] = Some(stack("minecraft:oak_log", 16));
        menu.slots[37] = Some(stack("minecraft:oak_planks", 64));
        menu.slots[9] = Some(stack("minecraft:bow", 1));
    }
    mods.set_snapshot(world);
    let screen = "bedrock_inventory:inventory";
    mods.screen_opened(screen);
    let phone = RenderContext {
        width: 533,
        height: 240,
        mouse_x: 0.0,
        mouse_y: 0.0,
        delta: 0.05,
    };
    let finger = |action, x, y| MouseEvent {
        action,
        x,
        y,
        button: 0,
        scroll: 0.0,
    };
    let tap = |mods: &mut Mods, x, y| {
        mods.screen_mouse(screen, finger(MouseAction::Press, x, y));
        mods.screen_mouse(screen, finger(MouseAction::Release, x, y));
    };
    mods.screen_render(screen, phone);
    // Fingers drag (not scroll) on the inventory's window and on the scrollbar.
    assert!(
        mods.drain()
            .iter()
            .any(|o| matches!(o, Output::DragAreas { areas, .. } if areas.len() == 2))
    );
    let id = || "bedrock_inventory".to_owned();
    // A tap picks the logs; a finger drawn across the crafting grid shares them out.
    tap(&mut mods, 277.0, 164.0);
    mods.screen_render(screen, phone);
    mods.screen_mouse(screen, finger(MouseAction::Press, 397.0, 59.0));
    for (x, y) in [(415.0, 59.0), (397.0, 77.0), (415.0, 77.0)] {
        mods.screen_mouse(screen, finger(MouseAction::Drag, x, y));
    }
    mods.screen_mouse(screen, finger(MouseAction::Release, 415.0, 77.0));
    assert_eq!(
        mods.drain(),
        [
            Output::Click {
                id: id(),
                slot: 36,
                button: 0,
                kind: horizon_mods::ClickKind::Pickup,
            },
            Output::Drag {
                id: id(),
                slots: vec![1, 2, 3, 4],
                button: 0,
            },
        ]
    );
    // A finger resting on the planks for a second chooses 9 of them (one, then faster), and a
    // tap puts those 9 in another slot.
    mods.screen_mouse(screen, finger(MouseAction::Press, 295.0, 164.0));
    for _ in 0..14 {
        mods.screen_render(screen, phone);
    }
    mods.screen_mouse(screen, finger(MouseAction::Release, 295.0, 164.0));
    mods.screen_render(screen, phone);
    tap(&mut mods, 313.0, 106.0);
    assert_eq!(
        mods.drain(),
        [
            Output::CreativeSlot {
                id: id(),
                slot: 37,
                stack: Some(stack("minecraft:oak_planks", 55)),
            },
            Output::CreativeCarried {
                id: id(),
                stack: Some(stack("minecraft:oak_planks", 9)),
            },
            Output::Click {
                id: id(),
                slot: 11,
                button: 0,
                kind: horizon_mods::ClickKind::Pickup,
            },
        ]
    );
    // The bow tapped onto a tab's item goes away.
    tap(&mut mods, 277.0, 106.0);
    tap(&mut mods, 114.0, 88.0);
    assert_eq!(
        mods.drain(),
        [Output::CreativeSlot {
            id: id(),
            slot: 9,
            stack: None,
        }]
    );
}

/// A left click (PICKUP, button 0) on a slot as the game does it: take, put, merge (64 a stack),
/// swap.
fn left_click(menu: &mut horizon_mods::Menu, slot: usize) {
    let (here, hand) = (menu.slots[slot].take(), menu.carried.take());
    let same = |a: &ItemStack, b: &ItemStack| a.id == b.id && a.components == b.components;
    (menu.slots[slot], menu.carried) = match (here, hand) {
        (here, None) => (None, here),
        (None, hand) => (hand, None),
        (Some(mut h), Some(mut c)) if same(&h, &c) => {
            let put = (64 - h.count).min(c.count);
            h.count += put;
            c.count -= put;
            (Some(h), (c.count > 0).then_some(c))
        }
        (here, hand) => (hand, here),
    };
}

/// Inventory Buttons on a chest: its four buttons on the window's two parts; Sort merges and
/// orders the chest by the creative order (over as many ticks as its clicks need, each tick's
/// clicks leaving the hand empty), the inventory left alone; Sort inventory leaves the hotbar;
/// Take all and Deposit all Shift-click each stack of their part.
#[test]
fn inventory_buttons_sort_take_and_deposit() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/starling-mod-sdk/examples/inventory-buttons");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[example], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    let buttons: Vec<(String, horizon_mods::WindowPart, i32)> = mods
        .widgets()
        .flat_map(|(_, w)| w.window.iter())
        .map(|b| (b.id.clone(), b.part, b.x as i32))
        .collect();
    use horizon_mods::WindowPart::{Container, Inventory};
    assert_eq!(
        buttons,
        [
            ("sort_container".to_owned(), Container, -1),
            ("take_all".to_owned(), Container, -15),
            ("sort_inventory".to_owned(), Inventory, -1),
            ("deposit_all".to_owned(), Inventory, -15),
        ]
    );
    let mut world = creative_world(GameMode::Survival);
    let chest = |slots: &[(usize, &str, u32)]| {
        let mut all = vec![None; 63];
        for &(i, id, n) in slots {
            all[i] = Some(stack(&format!("minecraft:{id}"), n));
        }
        horizon_mods::Menu {
            kind: "minecraft:generic_9x3".into(),
            slots: all,
            carried: None,
        }
    };
    world.menu = Some(chest(&[
        (0, "bread", 3),
        (2, "dirt", 40),
        (5, "stone", 10),
        (7, "dirt", 40),
        (11, "bow", 1),
        (13, "dirt", 5),
        (20, "stone", 64),
        (26, "oak_log", 7),
        // The inventory's.
        (30, "dirt", 9),
        (60, "arrow", 16),
    ]));
    let press = |mods: &mut Mods, id: &str| {
        mods.keybind("inventory_buttons", id, true);
        mods.keybind("inventory_buttons", id, false);
    };
    // The game does each tick's clicks and shows the mod the menu they left.
    let run = |mods: &mut Mods, world: &mut Snapshot| {
        for tick in 0..10 {
            let menu = world.menu.as_mut().unwrap();
            for o in mods.drain() {
                if let Output::Click {
                    slot,
                    button: 0,
                    kind: horizon_mods::ClickKind::Pickup,
                    ..
                } = o
                {
                    left_click(menu, slot as usize);
                }
            }
            assert!(
                menu.carried.is_none(),
                "the hand holds a stack after tick {tick}"
            );
            mods.set_snapshot(world.clone());
            mods.tick(tick);
        }
    };
    mods.set_snapshot(world.clone());
    press(&mut mods, "sort_container");
    run(&mut mods, &mut world);
    let shown = |world: &Snapshot, range: std::ops::Range<usize>| -> Vec<String> {
        let menu = world.menu.as_ref().unwrap();
        range
            .filter_map(|i| {
                menu.slots[i]
                    .as_ref()
                    .map(|s| format!("{i}:{}x{}", &s.id[10..], s.count))
            })
            .collect()
    };
    assert_eq!(
        shown(&world, 0..27),
        [
            "0:stonex64",
            "1:stonex10",
            "2:dirtx64",
            "3:dirtx21",
            "4:oak_logx7",
            "5:bowx1",
            "6:breadx3"
        ]
    );
    assert_eq!(shown(&world, 27..63), ["30:dirtx9", "60:arrowx16"]);
    // The inventory sorted, the hotbar (54 to 62 here) left.
    press(&mut mods, "sort_inventory");
    run(&mut mods, &mut world);
    assert_eq!(shown(&world, 27..63), ["27:dirtx9", "60:arrowx16"]);
    // Take all: a Shift-click on each of the chest's stacks; Deposit all: on the inventory's.
    let quick = |mods: &mut Mods| -> Vec<i32> {
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Click {
                    slot,
                    kind: horizon_mods::ClickKind::QuickMove,
                    ..
                } => Some(slot),
                _ => None,
            })
            .collect()
    };
    mods.set_snapshot(world.clone());
    press(&mut mods, "take_all");
    assert_eq!(quick(&mut mods), [0, 1, 2, 3, 4, 5, 6]);
    press(&mut mods, "deposit_all");
    assert_eq!(quick(&mut mods), [27]);
}

/// What a frame of Bedrock Inventory costs: the creative window full (a tab of 475 items), the
/// inventory full; `cargo test --release -p horizon-mods --test sandbox bedrock_inventory_frame
/// -- --ignored --nocapture`.
#[test]
#[ignore]
fn bedrock_inventory_frame_cost() {
    struct Big;
    impl horizon_mods::CreativeCatalog for Big {
        fn tabs(&self) -> Vec<horizon_mods::Tab> {
            Tabs.tabs()
        }
        fn items(&self, _: &str, _: &str, start: usize, count: usize) -> horizon_mods::ItemPage {
            let all: Vec<ItemStack> = (0..475)
                .map(|i| stack(&format!("minecraft:block_number_{i}"), 1))
                .collect();
            horizon_mods::ItemPage {
                total: 475,
                items: all.into_iter().skip(start).take(count).collect(),
            }
        }
        fn order(&self, stack: &ItemStack) -> Option<u32> {
            Tabs.order(stack)
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // BENCH_MOD: another folder (a variant of the mod) to measure.
    let example = std::env::var_os("BENCH_MOD").map_or_else(
        || root.join("crates/starling-mod-sdk/examples/bedrock-inventory"),
        PathBuf::from,
    );
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[example], |_| true, |_| false, Some(&cache));
    let mut world = creative_world(GameMode::Creative);
    world.creative = Some(std::sync::Arc::new(Big));
    if let Some(menu) = &mut world.menu {
        for (i, slot) in menu.slots.iter_mut().enumerate().skip(1) {
            *slot = Some(stack("minecraft:oak_planks", i as u32));
        }
    }
    let screen = "bedrock_inventory:inventory";
    mods.screen_opened(screen);
    let phone = RenderContext {
        width: 533,
        height: 240,
        mouse_x: 100.0,
        mouse_y: 76.0,
        delta: 0.016,
    };
    for snapshot_each_frame in [false, true] {
        mods.set_snapshot(world.clone());
        let first = Instant::now();
        mods.screen_render(screen, phone);
        println!("the first frame: {} us", first.elapsed().as_micros());
        let started = Instant::now();
        for _ in 0..200 {
            if snapshot_each_frame {
                mods.set_snapshot(world.clone());
            }
            mods.screen_render(screen, phone);
        }
        let us = started.elapsed().as_secs_f64() * 1e6 / 200.0;
        println!("a frame, new snapshot each: {snapshot_each_frame}: {us:.0} us");
        // Nothing but where the screen draws its slots.
        let out = mods.drain();
        assert!(
            out.iter().all(|o| matches!(o, Output::ScreenSlots { .. })),
            "{out:?}"
        );
    }
    assert_eq!(mods.entries()[0].status, Status::Running);
}

/// A cookbook of three: sticks from planks (shapeless), a torch (1 by 2), and a Potion of
/// Healing brewed.
struct Cookbook;
impl Cookbook {
    fn all() -> Vec<horizon_mods::Recipe> {
        use horizon_mods::{Recipe, RecipeKind};
        let healing = ItemStack {
            components: "{\"minecraft:potion_contents\":{potion:\"minecraft:healing\"}}".into(),
            ..stack("minecraft:potion", 1)
        };
        let planks = vec![
            stack("minecraft:oak_planks", 1),
            stack("minecraft:spruce_planks", 1),
        ];
        let recipe = |id: &str, kind, width, height, inputs, outputs, station: &str| Recipe {
            id: id.into(),
            kind,
            width,
            height,
            inputs,
            outputs,
            station: station.into(),
            time: 0,
            experience: 0.0,
        };
        vec![
            recipe(
                "minecraft:stick",
                RecipeKind::Crafting,
                0,
                0,
                vec![planks.clone(), planks],
                vec![stack("minecraft:stick", 4)],
                "minecraft:crafting_table",
            ),
            recipe(
                "minecraft:torch",
                RecipeKind::Crafting,
                1,
                2,
                vec![
                    vec![stack("minecraft:coal", 1)],
                    vec![stack("minecraft:stick", 1)],
                ],
                vec![stack("minecraft:torch", 4)],
                "minecraft:crafting_table",
            ),
            recipe(
                "minecraft:brewing/healing",
                RecipeKind::Brewing,
                0,
                0,
                vec![
                    vec![stack("minecraft:potion", 1)],
                    vec![stack("minecraft:glistering_melon_slice", 1)],
                ],
                vec![healing],
                "minecraft:brewing_stand",
            ),
        ]
    }
    fn found(stack: &ItemStack, lookup: horizon_mods::Lookup) -> Vec<horizon_mods::Recipe> {
        let same = |s: &ItemStack| {
            s.id == stack.id && (stack.components.is_empty() || s.components == stack.components)
        };
        Self::all()
            .into_iter()
            .filter(|r| match lookup {
                horizon_mods::Lookup::Recipes => r.outputs.iter().any(same),
                horizon_mods::Lookup::Uses => r.inputs.iter().flatten().any(same),
            })
            .collect()
    }
}
impl horizon_mods::RecipeCatalog for Cookbook {
    fn count(
        &self,
        stack: &ItemStack,
        lookup: horizon_mods::Lookup,
    ) -> Vec<(horizon_mods::RecipeKind, u32)> {
        let mut out: Vec<(horizon_mods::RecipeKind, u32)> = Vec::new();
        for r in Self::found(stack, lookup) {
            match out.iter_mut().find(|(k, _)| *k == r.kind) {
                Some((_, n)) => *n += 1,
                None => out.push((r.kind, 1)),
            }
        }
        out
    }
    fn recipes(
        &self,
        stack: &ItemStack,
        lookup: horizon_mods::Lookup,
        kind: horizon_mods::RecipeKind,
        start: usize,
        count: usize,
    ) -> Vec<horizon_mods::Recipe> {
        Self::found(stack, lookup)
            .into_iter()
            .filter(|r| r.kind == kind)
            .skip(start)
            .take(count)
            .collect()
    }
    fn shape(&self, id: &str) -> Option<(horizon_mods::RecipeKind, u32, u32)> {
        let r = Self::all().into_iter().find(|r| r.id == id)?;
        Some(match r.height {
            0 => (r.kind, r.inputs.len() as u32, 0),
            h => (r.kind, r.width, h),
        })
    }
}

/// `recipes`: found by what they make and take (a stack with components only its own), paged;
/// `fill` asks the game only for a recipe the open grid takes; `screens.get-window`.
#[test]
fn recipes_are_found_and_filled() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = mods_dir().join("js-recipes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("horizon-mod.toml"),
        "id = \"probe\"\nname = \"Probe\"\nversion = \"1\"\napi = \"starling-0.1\"\ncapabilities = [\"recipes\", \"ui\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("mod.js"),
        r#"import { countRecipes, getRecipes, fill } from "starling:mods/recipes@0.1.0";
import { getWindow } from "starling:mods/screens@0.1.0";
const s = (id, components = "") => ({ id, count: 1, damage: 0, maxDamage: 0, name: id, components });
export const events = {
  init() {
    return { commands: ["probe"], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false };
  },
  command(name, args) {
    if (args === "fill") return [fill("minecraft:stick", true), fill("minecraft:torch", false), fill("minecraft:nothing", false)].join(",");
    const sticks = getRecipes(s("minecraft:stick"), "recipes", "crafting", 0, 8);
    const r = sticks[0];
    const potion = s("minecraft:potion", "{\"minecraft:potion_contents\":{potion:\"minecraft:healing\"}}");
    return [
      JSON.stringify(countRecipes(s("minecraft:stick"), "recipes")),
      JSON.stringify(countRecipes(s("minecraft:stick"), "uses")),
      `${r.id} ${r.kind} ${r.width}x${r.height} ${r.inputs.length} ${r.inputs[0].map((x) => x.id).join("+")} ${r.outputs[0].count} ${r.station}`,
      getRecipes(s("minecraft:oak_planks"), "uses", "crafting", 1, 8).length,
      JSON.stringify(countRecipes(s("minecraft:potion"), "recipes")),
      JSON.stringify(countRecipes(potion, "recipes")),
      JSON.stringify(countRecipes(s("minecraft:potion", "{other}"), "recipes")),
      JSON.stringify(getWindow()),
    ].join(" | ");
  },
};
"#,
    )
    .unwrap();
    let mut mods = Mods::load_cached(
        &[dir],
        |_| true,
        |_| false,
        Some(&mods_dir().join("js-cache")),
    );
    assert_eq!(mods.entries()[0].status, Status::Running);
    let said = |mods: &mut Mods| -> Vec<Output> { mods.drain() };
    let mut world = player();
    world.recipes = Some(std::sync::Arc::new(Cookbook));
    world.window = Some([125.0, 37.0, 176.0, 166.0]);
    mods.set_snapshot(world.clone());
    assert!(mods.command("probe find"));
    assert_eq!(
        said(&mut mods),
        [Output::Feedback {
            text: "[[\"crafting\",1]] | [[\"crafting\",1]] | \
minecraft:stick crafting 0x0 2 minecraft:oak_planks+minecraft:spruce_planks 4 minecraft:crafting_table | 0 | \
[[\"brewing\",1]] | [[\"brewing\",1]] | [] | [125,37,176,166]"
                .into(),
            error: false,
        }]
    );
    // No grid open: nothing filled.
    assert!(mods.command("probe fill"));
    assert_eq!(
        said(&mut mods),
        [Output::Feedback {
            text: "false,false,false".into(),
            error: false
        }]
    );
    // The inventory's 2 by 2 takes sticks (2 shapeless) and a torch (1 by 2): one fill a call,
    // the last asked.
    world.crafting = Some(horizon_mods::Crafting::Grid(2));
    mods.set_snapshot(world.clone());
    assert!(mods.command("probe fill"));
    assert_eq!(
        said(&mut mods),
        [
            Output::FillRecipe {
                id: "probe".into(),
                recipe: "minecraft:torch".into(),
                all: false,
            },
            Output::Feedback {
                text: "true,true,false".into(),
                error: false
            },
        ]
    );
    // A furnace takes none of them.
    world.crafting = Some(horizon_mods::Crafting::Furnace(
        horizon_mods::RecipeKind::Smelting,
    ));
    mods.set_snapshot(world);
    assert!(mods.command("probe fill"));
    assert_eq!(
        said(&mut mods),
        [Output::Feedback {
            text: "false,false,false".into(),
            error: false
        }]
    );
}

/// The creative search lists planks, a stick and a torch.
struct Shelf;
impl horizon_mods::CreativeCatalog for Shelf {
    fn tabs(&self) -> Vec<horizon_mods::Tab> {
        vec![horizon_mods::Tab {
            id: "minecraft:search".into(),
            title: "Search".into(),
            icon: "minecraft:compass".into(),
            kind: horizon_mods::TabKind::Search,
            top: true,
            column: 6,
            size: 3,
        }]
    }
    fn items(
        &self,
        _tab: &str,
        search: &str,
        start: usize,
        count: usize,
    ) -> horizon_mods::ItemPage {
        let all: Vec<ItemStack> = ["minecraft:oak_planks", "minecraft:stick", "minecraft:torch"]
            .iter()
            .filter(|id| id.contains(search))
            .map(|id| stack(id, 1))
            .collect();
        horizon_mods::ItemPage {
            total: all.len() as u32,
            items: all.into_iter().skip(start).take(count).collect(),
        }
    }
    fn order(&self, _: &ItemStack) -> Option<u32> {
        None
    }
}

/// EMI: the index beside the inventory's window, searched; a click opens an item's recipes, a
/// right click its uses, on its recipes screen; + fills the grid and closes it; Backspace goes
/// back.
#[test]
fn emi_shows_recipes_and_uses_beside_the_inventory() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = example("emi", "index");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(
        std::slice::from_ref(&dir),
        |_| true,
        |_| false,
        Some(&cache),
    );
    assert_eq!(mods.entries()[0].status, Status::Running);
    let mut world = player();
    world.recipes = Some(std::sync::Arc::new(Cookbook));
    world.creative = Some(std::sync::Arc::new(Shelf));
    world.window = Some([72.0, 37.0, 176.0, 166.0]);
    world.crafting = Some(horizon_mods::Crafting::Grid(2));
    mods.set_snapshot(world);
    let ctx = |x: f32, y: f32| RenderContext {
        width: 427,
        height: 240,
        mouse_x: x,
        mouse_y: y,
        delta: 0.05,
    };
    // Right of the window (from 252): 9 columns from 263, rows from 16.
    let ops = mods.screen_render("inventory", ctx(0.0, 0.0));
    let items: Vec<&str> = ops
        .iter()
        .filter_map(|o| match o {
            DrawOp::Item { stack, .. } => Some(stack.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        items,
        ["minecraft:oak_planks", "minecraft:stick", "minecraft:torch"]
    );
    let item_at = |ops: &[DrawOp], id: &str| {
        ops.iter()
            .find_map(|o| match o {
                DrawOp::Item { x, y, stack } if stack.id == id => Some((*x, *y)),
                _ => None,
            })
            .unwrap()
    };
    let (sx, sy) = item_at(&ops, "minecraft:stick");
    let press = |x: f32, y: f32, button: u8| MouseEvent {
        action: MouseAction::Press,
        x,
        y,
        button,
        scroll: 0.0,
    };
    // A click on the stick: its recipes.
    assert!(mods.screen_mouse("inventory", press(sx + 4.0, sy + 4.0, 0)));
    assert!(
        mods.drain()
            .iter()
            .any(|o| matches!(o, Output::OpenScreen { screen } if screen == "emi:recipes"))
    );
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    let texts: Vec<&str> = ops
        .iter()
        .filter_map(|o| match o {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    // As EMI's: the station's bar and the page's, + and ++ right of the recipe; the index
    // beside it, searched under it.
    assert_eq!(texts, ["Crafting", "1/1", "+", "++", "1/1", "§7Search"]);
    // In the panel (176 wide from 126): the shapeless recipe in the 3 by 3 grid, planks in the
    // first two cells, 4 sticks out; the stick looked up in the row of buttons. The crafting
    // table in the strip left of it.
    let shown: Vec<&str> = ops
        .iter()
        .filter_map(|o| match o {
            DrawOp::Item { x, stack, .. } if (126.0..302.0).contains(x) => Some(stack.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [
            "minecraft:oak_planks",
            "minecraft:oak_planks",
            "minecraft:stick",
            "minecraft:stick"
        ]
    );
    assert_eq!(item_at(&ops, "minecraft:crafting_table"), (109.0, 37.0));
    // Right click on the planks in the recipe: their uses; Backspace: the stick's recipes again.
    let (px, py) = item_at(&ops, "minecraft:oak_planks");
    mods.screen_mouse("emi:recipes", press(px + 2.0, py + 2.0, 1));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    assert!(
        ops.iter()
            .any(|o| matches!(o, DrawOp::Button { label, active: false, .. } if label == "Uses"))
    );
    let key = |k: &str| horizon_mods::KeyEvent {
        key: k.into(),
        pressed: true,
        text: String::new(),
        shift: false,
        control: false,
        alt: false,
    };
    assert!(mods.screen_key("emi:recipes", &key("key.keyboard.backspace")));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    assert!(
        ops.iter().any(
            |o| matches!(o, DrawOp::Button { label, active: false, .. } if label == "Recipes")
        )
    );
    let button_at = |ops: &[DrawOp], name: &str| {
        ops.iter().find_map(|o| match o {
            DrawOp::Button { x, y, label, .. } if label == name => Some((*x, *y)),
            _ => None,
        })
    };
    // Every key has a button, for a phone: Back for Backspace (shown with somewhere to go back
    // to), the star for A.
    assert!(
        ops.iter()
            .any(|o| matches!(o, DrawOp::Button { label, active: false, .. } if label == "Back"))
    );
    mods.screen_mouse("emi:recipes", press(px + 2.0, py + 2.0, 1));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    let (bx, by) = button_at(&ops, "Back").unwrap();
    mods.screen_mouse("emi:recipes", press(bx + 2.0, by + 2.0, 0));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    assert!(
        ops.iter().any(
            |o| matches!(o, DrawOp::Button { label, active: false, .. } if label == "Recipes")
        )
    );
    let star = |ops: &[DrawOp]| {
        ops.iter()
            .find_map(|o| match o {
                DrawOp::ItemId { x, y, id } if id == "minecraft:nether_star" => Some((*x, *y)),
                _ => None,
            })
            .unwrap()
    };
    let (tx, ty) = star(&ops);
    mods.screen_mouse("emi:recipes", press(tx + 2.0, ty + 2.0, 0));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    // Framed in gold now: the stick is a favourite, left of the window.
    assert!(ops.iter().any(|o| matches!(
        o,
        DrawOp::Fill {
            color: 0xFFFF_D700,
            ..
        }
    )));
    let beside = mods.screen_render("inventory", ctx(0.0, 0.0));
    let left: Vec<&str> = beside
        .iter()
        .filter_map(|o| match o {
            DrawOp::Item { x, stack, .. } if *x < 72.0 => Some(stack.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(left, ["minecraft:stick"]);
    mods.screen_mouse("emi:recipes", press(tx + 2.0, ty + 2.0, 0));
    let beside = mods.screen_render("inventory", ctx(0.0, 0.0));
    assert!(
        !beside
            .iter()
            .any(|o| matches!(o, DrawOp::Item { x, .. } if *x < 72.0))
    );
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    // EMI's buttons right of the recipe, + under ++: ++ fills as many as can be, as Shift and +.
    let small: Vec<(f32, f32)> = ops
        .iter()
        .filter_map(|o| match o {
            DrawOp::Button { x, y, w: 16.0, .. } => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(small.len(), 2);
    assert_eq!(small[0].0, small[1].0);
    assert_eq!(small[0].1 - small[1].1, 14.0);
    let (ax, ay) = small[1];
    mods.drain();
    mods.screen_mouse("emi:recipes", press(ax + 2.0, ay + 2.0, 0));
    let out = mods.drain();
    assert!(out.iter().any(|o| matches!(o, Output::FillRecipe { recipe, all: true, .. } if recipe == "minecraft:stick")), "{out:?}");
    // The stick's recipes again, for +.
    mods.screen_mouse("inventory", press(sx + 4.0, sy + 4.0, 0));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    // + fills the grid with the recipe and closes the screen.
    let plus = ops
        .iter()
        .find_map(|o| match o {
            DrawOp::Button { x, y, w: 16.0, .. } => Some((*x, *y)),
            _ => None,
        })
        .unwrap();
    mods.drain();
    mods.screen_mouse("emi:recipes", press(plus.0 + 2.0, plus.1 + 2.0, 0));
    let out = mods.drain();
    assert!(out.iter().any(|o| matches!(o, Output::FillRecipe { recipe, all: false, .. } if recipe == "minecraft:stick")), "{out:?}");
    assert!(out.iter().any(|o| matches!(o, Output::CloseScreen { .. })));
    // On a touch screen (no hover): a tap picks a stack and shows its tooltip, a second tap on it
    // its recipes; picked in a recipe, it is what the buttons under the recipes act on.
    let mut world = player();
    world.recipes = Some(std::sync::Arc::new(Cookbook));
    world.creative = Some(std::sync::Arc::new(Shelf));
    world.window = Some([72.0, 37.0, 176.0, 166.0]);
    world.crafting = Some(horizon_mods::Crafting::Grid(2));
    world.touch = true;
    mods.set_snapshot(world);
    mods.screen_opened("inventory");
    mods.drain();
    let tooltip_of = |ops: &[DrawOp]| {
        ops.iter().find_map(|o| match o {
            DrawOp::ItemTooltip { stack, .. } => Some(stack.id.clone()),
            _ => None,
        })
    };
    // The index under the game's close button there (from 40, not 16), its arrows under it.
    let ops = mods.screen_render("inventory", ctx(0.0, 0.0));
    let (sx, sy) = item_at(&ops, "minecraft:stick");
    assert_eq!(sy, 41.0);
    assert!(
        ops.iter()
            .any(|o| matches!(o, DrawOp::Sprite { y, sprite, .. }
        if sprite.starts_with("recipe_book/page_forward") && *y == 220.0))
    );
    assert!(mods.screen_mouse("inventory", press(sx + 4.0, sy + 4.0, 0)));
    assert!(mods.drain().is_empty());
    let ops = mods.screen_render("inventory", ctx(0.0, 0.0));
    assert_eq!(tooltip_of(&ops).as_deref(), Some("minecraft:stick"));
    assert!(mods.screen_mouse("inventory", press(sx + 4.0, sy + 4.0, 0)));
    assert!(
        mods.drain()
            .iter()
            .any(|o| matches!(o, Output::OpenScreen { screen } if screen == "emi:recipes"))
    );
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    assert_eq!(tooltip_of(&ops), None);
    let (px, py) = item_at(&ops, "minecraft:oak_planks");
    mods.screen_mouse("emi:recipes", press(px + 2.0, py + 2.0, 0));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    assert_eq!(tooltip_of(&ops).as_deref(), Some("minecraft:oak_planks"));
    // The planks in the row of buttons, Recipes and Uses both theirs to show.
    let subject = ops
        .iter()
        .filter_map(|o| match o {
            DrawOp::Item { y, stack, .. } if *y > 180.0 && (126.0..302.0).contains(&stack_x(o)) => {
                Some(stack.id.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(subject, ["minecraft:oak_planks"]);
    let (ux, uy) = button_at(&ops, "Uses").unwrap();
    mods.screen_mouse("emi:recipes", press(ux + 2.0, uy + 2.0, 0));
    let ops = mods.screen_render("emi:recipes", ctx(0.0, 0.0));
    assert!(
        ops.iter()
            .any(|o| matches!(o, DrawOp::Button { label, active: false, .. } if label == "Uses"))
    );
    assert_eq!(tooltip_of(&ops), None);
}

/// Where an item op is drawn across.
fn stack_x(o: &DrawOp) -> f32 {
    match o {
        DrawOp::Item { x, .. } => *x,
        _ => f32::NAN,
    }
}

/// ShulkerBoxTooltip: a box's contents, from its components' SNBT, as a grid on its colour in
/// place of the game's lines; each kind once, or with Shift each slot where it is; `add-grid`
/// only from the tooltip hook.
#[test]
fn shulker_box_tooltip_shows_the_contents_as_slots() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let dir = example("shulker-box-tooltip", "grid");
    let cache = mods_dir().join("js-cache");
    let mut mods = Mods::load_cached(&[dir], |_| true, |_| false, Some(&cache));
    assert_eq!(mods.entries()[0].status, Status::Running);
    mods.set_snapshot(player());
    // As the game writes a box holding stone twice, a sharp sword and dirt.
    let components = "{minecraft:container:[{item:{count:64,id:\"minecraft:stone\"},slot:0},\
{item:{components:{minecraft:enchantments:{\"minecraft:sharpness\":5}},count:1,id:\"minecraft:diamond_sword\"},slot:4},\
{item:{count:10,id:\"minecraft:stone\"},slot:9},{item:{count:3,id:\"minecraft:dirt\"},slot:26}]}";
    let boxed = ItemStack {
        components: components.into(),
        ..stack("minecraft:red_shulker_box", 1)
    };
    let (lines, grids, instead) = mods.tooltip_grids(&boxed);
    assert_eq!(lines, ["§8Shift: each slot"]);
    assert!(instead);
    let grid = &grids[0];
    assert_eq!((grid.columns, grid.color), (3, 0xFFB0_2E26));
    let kinds: Vec<(String, u32, String)> = grid
        .stacks
        .iter()
        .flatten()
        .map(|s| (s.id.clone(), s.count, s.components.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("minecraft:stone".into(), 74, String::new()),
            (
                "minecraft:diamond_sword".into(),
                1,
                "{minecraft:enchantments:{\"minecraft:sharpness\":5}}".into()
            ),
            ("minecraft:dirt".into(), 3, String::new()),
        ]
    );
    // Shift: the box's 27 slots, 9 a row, each stack in its own.
    let key = |pressed| horizon_mods::KeyEvent {
        key: "key.keyboard.left.shift".into(),
        pressed,
        text: String::new(),
        shift: pressed,
        control: false,
        alt: false,
    };
    mods.screen_key("inventory", &key(true));
    let (lines, grids, _) = mods.tooltip_grids(&boxed);
    assert!(lines.is_empty());
    let grid = &grids[0];
    assert_eq!((grid.columns, grid.stacks.len()), (9, 27));
    let at = |i: usize| grid.stacks[i].as_ref().map(|s| (s.id.as_str(), s.count));
    assert_eq!(at(0), Some(("minecraft:stone", 64)));
    assert_eq!(at(1), None);
    assert_eq!(at(9), Some(("minecraft:stone", 10)));
    assert_eq!(at(26), Some(("minecraft:dirt", 3)));
    // A phone has no Shift: the Config screen picks the preview shown without it, kept; Shift
    // then shows the other.
    mods.screen_key("inventory", &key(false));
    assert_eq!(
        mods.config_screen("shulker_box_tooltip").as_deref(),
        Some("shulker_box_tooltip:settings")
    );
    let settings = "shulker_box_tooltip:settings";
    let ctx = RenderContext {
        width: 427,
        height: 240,
        mouse_x: 0.0,
        mouse_y: 0.0,
        delta: 0.05,
    };
    let preview = |ops: &[DrawOp]| {
        ops.iter()
            .find_map(|o| match o {
                DrawOp::Button { x, y, label, .. } if label.starts_with("Preview") => {
                    Some((*x, *y, label.clone()))
                }
                _ => None,
            })
            .unwrap()
    };
    let (bx, by, label) = preview(&mods.screen_render(settings, ctx));
    assert_eq!(label, "Preview: Compact");
    let tap = MouseEvent {
        action: MouseAction::Press,
        x: bx + 4.0,
        y: by + 4.0,
        button: 0,
        scroll: 0.0,
    };
    assert!(mods.screen_mouse(settings, tap));
    assert_eq!(
        preview(&mods.screen_render(settings, ctx)).2,
        "Preview: Each slot"
    );
    let (lines, grids, _) = mods.tooltip_grids(&boxed);
    assert_eq!(lines, ["§8Shift: together"]);
    assert_eq!(grids[0].stacks.len(), 27);
    mods.screen_key("inventory", &key(true));
    let (_, grids, _) = mods.tooltip_grids(&boxed);
    assert_eq!(grids[0].stacks.len(), 3);
    // An empty box, and a stick: the game's own tooltip.
    let (lines, grids, instead) = mods.tooltip_grids(&stack("minecraft:shulker_box", 1));
    assert!(lines.is_empty() && grids.is_empty() && !instead);
}

/// `cargo horizon-mod test` on the examples: their own `tests/*.toml` pass.
#[test]
fn the_examples_test_scripts_pass() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../horizon-mod-sdk/examples");
    let mut ran = 0;
    let examples_and_files = [
        ("coords-hud", "coords_hud"),
        ("welcome", "welcome"),
        ("beacons", "beacons"),
    ];
    for (example, hmod) in examples_and_files {
        for entry in std::fs::read_dir(examples.join(example).join("tests")).unwrap() {
            let path = entry.unwrap().path();
            let script = Script::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let mut game = TestGame::load(&mods_dir().join(format!("{hmod}.hmod"))).unwrap();
            script
                .run(&mut game)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            ran += 1;
        }
    }
    assert_eq!(ran, 5);
}

/// Capability `world` without a world lent (the registry's trial run):
/// `get-block` finds nothing, `set-blocks` checks its count and queues.
#[test]
fn a_server_mod_without_a_world_reads_nothing() {
    let mut game = TestGame::load(&mods_dir().join("beacons.hmod")).unwrap();
    game.join("Alex");
    let alex = game.players[0].clone();
    assert!(game.player_command(&alex, "ground"));
    let out = game.drain();
    assert!(
        out.iter()
            .any(|o| matches!(o, Output::Reply { text, error: true, .. }
            if text == "That block isn't loaded")),
        "{out:?}"
    );
}

#[test]
fn a_test_script_says_which_step_failed_and_why() {
    let script = Script::parse(
        r#"
        [[step]]
        ticks = 3
        command = "hello there"
        expect.chat = ["hello there after 2 ticks"]
        "#,
    )
    .unwrap();
    let mut game = TestGame::load(&mods_dir().join("horizon_mods_test_guest.hmod")).unwrap();
    assert_eq!(
        script.run(&mut game).unwrap_err(),
        "step 1 (3 ticks, /hello there): chat\n    expected: [\"hello there after 2 ticks\"]\n    \
         got:      [\"hello there after 3 ticks\"]"
    );
    assert!(
        Script::parse("[[step]]\ntick = 1").is_err(),
        "a typo is an error"
    );

    let mut game = TestGame::load(&mods_dir().join("welcome.hmod")).unwrap();
    let script = "[[step]]\njoin = \"Alex\"\n\n[[step]]\nas = \"Steve\"\ncommand = \"mined\"";
    assert_eq!(
        Script::parse(script).unwrap().run(&mut game).unwrap_err(),
        "step 2 (/mined): Steve is not on the server"
    );
}

#[test]
fn a_test_script_reloads_and_expects_a_stop() {
    let script = Script::parse(
        r#"
        [[step]]
        command = "save kept"
        expect.feedback = "saved"

        [[step]]
        reload = true
        command = "load"
        expect.chat = ["kept"]

        [[step]]
        command = "panic"
        expect.stopped = "panic"

        [[step]]
        reload = true
        command = "nope"
        "#,
    )
    .unwrap();
    let mut game = TestGame::load(&mods_dir().join("horizon_mods_test_guest.hmod")).unwrap();
    let dir = game.hmod().parent().unwrap().to_path_buf();
    assert_eq!(
        script.run(&mut game).unwrap_err(),
        "step 4 (reload, /nope): the mod owns no command /nope"
    );
    drop(game);
    assert!(
        !dir.exists(),
        "the scratch copy and its storage are removed"
    );
}

fn alex() -> ServerPlayer {
    ServerPlayer {
        id: "00000000-0000-4000-8000-000000000001".into(),
        name: "Alex".into(),
        x: 0.5,
        y: 64.0,
        z: 0.5,
        dimension: "minecraft:the_nether".into(),
        game_mode: ServerGameMode::Survival,
        health: 20.0,
    }
}

#[test]
fn a_server_mod_runs_only_where_the_world_does() {
    // A fresh folder: what the mod stored in an earlier run is beside it.
    let dir = mods_dir().join("server");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("welcome.hmod");
    std::fs::copy(mods_dir().join("welcome.hmod"), &path).unwrap();
    let paths = std::slice::from_ref(&path);
    let window = Mods::load_for(Runs::Client, paths, |_| true, |_| false);
    assert_eq!(window.entries()[0].status, Status::Elsewhere);
    assert!(!window.is_running());

    let mut mods = Mods::load_for(Runs::Server, paths, |_| true, |c| c == "give");
    assert!(mods.runs_server());
    let commands: Vec<_> = mods.server_commands().map(|c| c.name.as_str()).collect();
    assert_eq!(commands, ["online", "mined"]);
    // `/mined [player]`: its argument travels with it, for the command tree.
    let mined = mods.server_commands().nth(1).unwrap();
    assert_eq!(mined.arguments[0].kind, horizon_mods::ArgumentKind::Player);
    assert!(mined.arguments[0].optional);
    assert_eq!(mods.commands().count(), 0, "not the window's chat commands");
    let alex = alex();
    mods.set_snapshot(Snapshot {
        players: vec![alex.clone()],
        ..Snapshot::default()
    });
    mods.player_joined(&alex);
    let greeted = [
        Output::Tell {
            name: "Welcome".into(),
            player: alex.id.clone(),
            text: "Welcome, Alex! 0 other player(s) online.".into(),
        },
        Output::Broadcast {
            name: "Welcome".into(),
            text: "Alex is here for the first time".into(),
        },
    ];
    assert_eq!(mods.drain(), greeted);
    let stone = ServerBlockPos { x: 1, y: 63, z: 0 };
    mods.block_broken(&alex, stone, "minecraft:stone");
    assert!(mods.server_command(&alex, "mined"));
    assert!(mods.server_command(&alex, "online"));
    let reply = |text: &str| Output::Reply {
        player: alex.id.clone(),
        text: text.into(),
        error: false,
    };
    assert_eq!(
        mods.drain(),
        [reply("You broke 1 block(s)"), reply("Alex (the_nether)")]
    );
    assert!(!mods.server_command(&alex, "where"));
    assert!(
        !mods.command("online"),
        "a server command isn't the window's"
    );
}

/// Capability `recipes`: the guest reads tables as big as vanilla's (1,500
/// items, 1,600 recipes of nine five-item slots) within one command's fuel.
#[test]
fn recipes_reads_the_tables() {
    use horizon_mods::{
        HorizonRecipe as Recipe, HorizonRecipeKind as RecipeKind, RecipeItem, RecipeStack, Recipes,
    };
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let stack = |item| RecipeStack {
        item,
        count: 1,
        components: String::new(),
    };
    let items = (0..1500)
        .map(|i| RecipeItem {
            id: format!("minecraft:item_{i}"),
            name: format!("Item {i}"),
            tags: vec!["planks".into(), "logs".into()],
        })
        .collect();
    let mut recipes: Vec<Recipe> = (0..1600)
        .map(|i| Recipe {
            id: format!("minecraft:recipe_{i}"),
            kind: RecipeKind::CraftingShaped,
            category: "crafting_misc".into(),
            group: String::new(),
            width: 3,
            height: 3,
            slots: (0..9)
                .map(|k| (0..5).map(|j| stack((i + k + j) % 1500)).collect())
                .collect(),
            results: vec![RecipeStack {
                item: 7,
                count: 4,
                components: String::new(),
            }],
            cooking_time: 0,
            experience: 0.0,
            book_id: i as i32,
        })
        .collect();
    recipes[1].kind = RecipeKind::Brewing;
    recipes[1].results[0].components =
        "[minecraft:potion_contents={potion:\"minecraft:swiftness\"}]".into();
    let mods_recipes = Some(std::sync::Arc::new(Recipes { items, recipes }));
    mods.set_snapshot(Snapshot {
        horizon_recipes: mods_recipes.clone(),
        known_recipes: vec![1, 5, 9].into(),
        ..Snapshot::default()
    });
    let said = |mods: &mut Mods, command: &str| {
        assert!(mods.command(command));
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Chat { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        said(&mut mods, "recipes"),
        ["1500 items, 1600 recipes, 3 known, 1 brewing"]
    );
    assert_eq!(
        said(&mut mods, "recipes minecraft:recipe_1"),
        [
            "minecraft:recipe_1 Kind::Brewing 3x3 0t slots 9, the first 1 minecraft:item_1 -> \
          4 minecraft:item_7[minecraft:potion_contents={potion:\"minecraft:swiftness\"}] known true"
        ]
    );
    assert!(
        mods.entries()
            .iter()
            .all(|e| e.status == horizon_mods::Status::Running),
        "it ran out of fuel"
    );
    // Actions: refused without a crafting screen, for an unknown recipe,
    // and outside creative mode.
    assert_eq!(
        said(&mut mods, "recipe_place minecraft:recipe_5"),
        ["place minecraft:recipe_5: no screen that takes recipes is open"]
    );
    let mut snap = Snapshot {
        horizon_recipes: mods_recipes.clone(),
        known_recipes: vec![1, 5, 9].into(),
        placing: true,
        ..Snapshot::default()
    };
    mods.set_snapshot(snap.clone());
    assert_eq!(
        said(&mut mods, "recipe_place minecraft:recipe_6"),
        ["place minecraft:recipe_6: recipe 6 is not in the player's recipe book"]
    );
    assert!(mods.command("recipe_place minecraft:recipe_5 all"));
    assert_eq!(
        mods.drain()[0],
        Output::PlaceRecipe {
            book_id: 5,
            all: true
        }
    );
    assert_eq!(
        said(&mut mods, "recipe_give minecraft:item_7 3"),
        ["give minecraft:item_7: only in creative mode"]
    );
    snap.creative_mode = true;
    mods.set_snapshot(snap);
    assert!(mods.command("recipe_give minecraft:item_7 3"));
    assert_eq!(
        mods.drain()[0],
        Output::Give {
            item: "minecraft:item_7".into(),
            count: 3,
            components: String::new()
        }
    );
    // Without the game's tables (no mod read them yet): empty, not a trap.
    mods.set_snapshot(Snapshot::default());
    assert_eq!(
        said(&mut mods, "recipes"),
        ["0 items, 0 recipes, 0 known, 0 brewing"]
    );
}

#[test]
fn browser_lists_items_and_hears_the_player() {
    use horizon_mods::{BrowserClick, BrowserEvent, BrowserStack};
    let mut mods = load("horizon_mods_test_ui_guest.hmod");
    let said = |mods: &mut Mods, command: &str| {
        assert!(mods.command(command));
        mods.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Chat { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(mods.browser_items().is_none());
    let potion = "[minecraft:potion_contents={potion:\"minecraft:swiftness\"}]";
    assert_eq!(
        said(
            &mut mods,
            &format!("browser minecraft:stone minecraft:potion{potion}")
        ),
        ["panel of 2"]
    );
    let (owner, version, items) = mods.browser_items().unwrap();
    assert_eq!(
        (items[0].item.as_str(), items[1].components.as_str()),
        ("minecraft:stone", potion)
    );
    mods.browser_event(
        owner,
        BrowserEvent::Clicked(BrowserClick {
            index: 1,
            right: true,
        }),
    );
    mods.browser_event(owner, BrowserEvent::Search("oak".into()));
    let log = BrowserStack {
        item: "minecraft:oak_log".into(),
        count: 3,
        components: String::new(),
    };
    // R is the guest's `recipe`; X is none of its keys.
    mods.browser_key("key.keyboard.r", &log);
    mods.browser_key("key.keyboard.x", &log);
    assert_eq!(
        said(&mut mods, "browser_poll"),
        [
            "3 events",
            "clicked 1 right",
            "search \"oak\"",
            "key recipe on minecraft:oak_log x3"
        ]
    );
    // Each `set-items` is a new version; an empty list hides the panel.
    said(&mut mods, "browser minecraft:dirt");
    assert_ne!(mods.browser_items().unwrap().1, version);
    assert_eq!(said(&mut mods, "browser"), ["panel of 0"]);
    assert!(mods.browser_items().is_none());
    // Recipes in the panel's place: the guest shows those making planks.
    use horizon_mods::{
        BrowserLayout, HorizonRecipe as Recipe, HorizonRecipeKind as RecipeKind, RecipeItem,
        RecipeStack, Recipes,
    };
    let item = |id: &str| RecipeItem {
        id: id.into(),
        name: id.into(),
        tags: Vec::new(),
    };
    let stack = |item| RecipeStack {
        item,
        count: 1,
        components: String::new(),
    };
    mods.set_snapshot(Snapshot {
        horizon_recipes: Some(std::sync::Arc::new(Recipes {
            items: vec![item("minecraft:oak_log"), item("minecraft:oak_planks")],
            recipes: vec![Recipe {
                id: "minecraft:oak_planks".into(),
                kind: RecipeKind::CraftingShapeless,
                category: "building".into(),
                group: "planks".into(),
                width: 0,
                height: 0,
                slots: vec![vec![stack(0)]],
                results: vec![RecipeStack {
                    count: 4,
                    ..stack(1)
                }],
                cooking_time: 0,
                experience: 0.0,
                book_id: 1,
            }],
        })),
        ..Snapshot::default()
    });
    said(&mut mods, "browser minecraft:dirt");
    assert_eq!(
        said(&mut mods, "browser_recipes minecraft:oak_planks"),
        ["recipes shown: 1"]
    );
    let page = mods.browser_recipes(owner).unwrap();
    assert_eq!(page.title, "Recipes for minecraft:oak_planks");
    assert_eq!(
        (page.tabs[0].item.as_str(), page.tab),
        ("minecraft:crafting_table", 0)
    );
    let r = &page.recipes[0];
    assert_eq!(
        (r.layout, r.width, r.height, r.action),
        (BrowserLayout::Grid, 1, 1, Some(false))
    );
    assert_eq!(
        (r.slots[0][0].item.as_str(), r.results[0].count),
        ("minecraft:oak_log", 4)
    );
    assert_eq!(said(&mut mods, "browser_recipes"), ["recipes shown: 0"]);
    assert!(mods.browser_recipes(owner).is_none());
    // At most 256 events wait for a poll: the oldest go.
    for _ in 0..300 {
        mods.browser_event(owner, BrowserEvent::Page(1));
    }
    assert_eq!(said(&mut mods, "browser_poll")[0], "256 events");
}
