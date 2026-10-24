//! Loads real mods built with the SDK: the coords-hud example and a guest
//! that misbehaves on command. Needs `rustup target add wasm32-unknown-unknown`.
use horizon_mods::{
    ChatVerdict, Clock, Corner, DrawOp, GameMode, Hand, HudPart, ItemStack, Mods, MouseAction,
    MouseEvent, Output, Package, Placement, Player, RenderContext, Snapshot, Status, WorldInfo,
    manifest::componentize,
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
                "food-info",
                "-p",
                "waypoints",
                "-p",
                "toggle-sprint",
                "-p",
                "horizon-mods-test-guest",
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
        // coords-hud is Horizon's own (API 0.1); the others Starling's.
        let example = |sdk: &str, name: &str| {
            let path = format!("crates/{sdk}/examples/{name}/horizon-mod.toml");
            std::fs::read_to_string(root.join(path)).unwrap()
        };
        let mods = [
            ("coords_hud", example("horizon-mod-sdk", "coords-hud")),
            ("food_info", example("starling-mod-sdk", "food-info")),
            ("waypoints", example("starling-mod-sdk", "waypoints")),
            (
                "toggle_sprint",
                example("starling-mod-sdk", "toggle-sprint"),
            ),
            ("horizon_mods_test_guest", GUEST_MANIFEST.to_string()),
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
    mods.key("test_guest", "zoom", true);
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
    mods.key("waypoints", "open", true);
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
    // Kept on disk: a reload has it.
    let entries = Mods::load(&[dir.join("waypoints.hmod")], |_| true, |_| false);
    let mut reloaded = entries;
    reloaded.set_snapshot(player());
    reloaded.key("waypoints", "open", true);
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
    mods.key("test_guest", "home", true);
    mods.key("test_guest", "home", false);
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
    mods.key("elytra_swap", "swap", true);
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
    // Both declare id "test_guest" (the guest's own manifest): sharing a
    // `data_dir` would let the second read, overwrite or delete the
    // first's storage, so only the first-loaded one may start.
    let mods = Mods::load(&[first, second], |_| true, |_| false);
    assert_eq!(mods.entries()[0].status, Status::Running);
    let refused = Status::Failed("another running mod already uses id test_guest".into());
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
    mods.key("toggle_sprint", "sprint", true);
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
}
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

/// Bedrock Inventory stands in for the creative screen and takes from the tabs with the mouse.
#[test]
fn bedrock_inventory_takes_from_the_creative_tabs() {
    if !horizon_mods::has_js_engine() {
        eprintln!("no JavaScript engine in this build: skipped");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example = root.join("crates/starling-mod-sdk/examples/bedrock-inventory");
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
    // A phone's GUI: the creative window (the building blocks' two items, the hotbar row) and the
    // inventory beside it.
    let phone = RenderContext {
        width: 533,
        height: 240,
        mouse_x: 0.0,
        mouse_y: 0.0,
        delta: 0.016,
    };
    let ops = mods.screen_render(screen, phone);
    let items: Vec<(String, f32, f32)> = ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Item { x, y, stack } => Some((stack.id.clone(), *x, *y)),
            _ => None,
        })
        .collect();
    for shown in [
        ("minecraft:stone", 88.0, 70.0),
        ("minecraft:dirt", 106.0, 70.0),
        ("minecraft:bow", 88.0, 164.0),
        ("minecraft:bow", 286.0, 179.0),
    ] {
        assert!(
            items.contains(&(shown.0.into(), shown.1, shown.2)),
            "{shown:?} in {items:?}"
        );
    }
    assert!(ops.iter().any(|op| matches!(op, DrawOp::Player { .. })));
    // A click on the dirt takes it onto the cursor, as the creative screen's grid does.
    let press = |x, y, action| MouseEvent {
        action,
        x,
        y,
        button: 0,
        scroll: 0.0,
    };
    mods.screen_mouse(screen, press(112.0, 76.0, MouseAction::Press));
    mods.screen_mouse(screen, press(112.0, 76.0, MouseAction::Release));
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
    mods.screen_mouse(screen, press(294.0, 128.0, MouseAction::Press));
    assert_eq!(mods.drain(), []);
    mods.screen_mouse(screen, press(294.0, 128.0, MouseAction::Release));
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
