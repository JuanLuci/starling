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
api = "0.3"
capabilities = ["chat", "storage", "hud", "ui", "chat-send", "net", "game-read"]
"#;

/// Builds both mods once and writes their `.hmod` files; returns the folder.
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
        let example = |name: &str| {
            let path = format!("crates/horizon-mod-sdk/examples/{name}/horizon-mod.toml");
            std::fs::read_to_string(root.join(path)).unwrap()
        };
        let mods = [
            ("coords_hud", example("coords-hud")),
            ("food_info", example("food-info")),
            ("waypoints", example("waypoints")),
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
    // 0.3's hooks are not 0.1's: nothing to draw, every action allowed.
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
    assert_eq!(labels(&ops), ["Add here", "Close"]);
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
    assert_eq!(labels(&ops), ["Place 1 (0 m)", "x", "Add here", "Close"]);
    assert!(mods.hud_render(context()).is_empty());
    // Pick it: the HUD points at it.
    assert!(mods.screen_mouse(&screen, press(70.0, 62.0)));
    let hud = mods.hud_render(context());
    assert!(
        hud.iter()
            .any(|o| matches!(o, DrawOp::Text { text, .. } if text.contains("Place 1 0 m")))
    );
    // Kept on disk: a reload has it.
    let entries = Mods::load(&[dir.join("waypoints.hmod")], |_| true, |_| false);
    let mut reloaded = entries;
    reloaded.set_snapshot(player());
    reloaded.key("waypoints", "open", true);
    let ops = reloaded.screen_render(&screen, context());
    assert!(labels(&ops).iter().any(|l| l.starts_with("Place 1")));
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
        ("grow", "it crashed (a panic)"),
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
