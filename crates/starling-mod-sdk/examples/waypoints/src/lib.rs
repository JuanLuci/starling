//! Example Horizon mod (`docs/mods.md`) for the screen API: J, its touch button (a compass) or
//! its button on the pause menu opens a screen of saved places drawn with vanilla buttons; "Add
//! here" saves where the player stands,
//! a place's button picks it, "x" forgets it and "Rename" names the picked one (bringing up the
//! on-screen keyboard on a phone; Enter keeps the name). The chosen place's distance and
//! direction show at the top of the HUD. In the world each place of the dimension has a beam of
//! its colour up from it and a diamond of its colour on the beam, its name over the diamond and
//! its distance under it, seen through hills and the same size at any distance (JourneyMap's
//! beacons and diamonds, Xaero's waypoints), with `world-draw`; the chosen one ringed in white.
//! The diamonds are the mod's own sprites (`assets/`). Places are kept with `storage`, one per
//! line.
use starling_mod_sdk::{
    Anchor, KeyEvent, Mod, MouseAction, MouseEvent, Registration, RenderContext, ScreenButton,
    Shape, TouchButton, draw, game, mobile, screens, storage, widgets,
    world_draw::{self, Icon, Layer, Lines, Point},
};

struct Place {
    name: String,
    pos: [f64; 3],
    dimension: String,
}

struct Waypoints {
    places: Vec<Place>,
    chosen: Option<usize>,
    /// The new name being typed for the chosen place.
    renaming: Option<String>,
    /// What the world shows now (the icons' texts and places), to set it again only when it
    /// changes.
    shown: String,
}

const SCREEN: &str = "waypoints:list";
/// The panel and its rows, in GUI pixels from the panel's corner.
const PANEL: [f32; 2] = [220.0, 176.0];
const ROW: f32 = 22.0;
const ROWS: usize = 5;
/// The places' colours, one each in turn (chat's red, green, blue, gold, light purple).
const COLORS: [u32; 5] = [0xFF5555, 0x55FF55, 0x5555FF, 0xFFAA00, 0xFF55FF];
/// A beam's height above its place and its width in pixels.
const BEAM: f32 = 256.0;
const BEAM_WIDTH: f32 = 3.0;
/// The diamond's height above a place.
const DIAMOND_HEIGHT: f64 = 1.2;

/// What a press on the screen means.
enum Hit {
    Pick(usize),
    Forget(usize),
    Add,
    Rename,
    Close,
}

impl Waypoints {
    fn load() -> Vec<Place> {
        let Some(bytes) = storage::get("places") else {
            return Vec::new();
        };
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|line| {
                let mut f = line.split('|');
                let name = f.next()?.to_owned();
                let mut n = || f.next()?.parse::<f64>().ok();
                let pos = [n()?, n()?, n()?];
                let dimension = f.next()?.to_owned();
                Some(Place {
                    name,
                    pos,
                    dimension,
                })
            })
            .collect()
    }
    fn save(&self) {
        let text: String = self
            .places
            .iter()
            .map(|p| {
                let [x, y, z] = p.pos;
                format!("{}|{x:.1}|{y:.1}|{z:.1}|{}\n", p.name, p.dimension)
            })
            .collect();
        let _ = storage::set("places", text.as_bytes());
    }
    /// The panel's corner, centred.
    fn origin(context: &RenderContext) -> [f32; 2] {
        [
            ((context.width as f32 - PANEL[0]) / 2.0).floor(),
            ((context.height as f32 - PANEL[1]) / 2.0).floor(),
        ]
    }
    /// Every button: (x, y, w, h) and what it does.
    fn buttons(&self, context: &RenderContext) -> Vec<([f32; 4], Hit)> {
        let [ox, oy] = Self::origin(context);
        let mut out = Vec::new();
        for i in 0..self.places.len().min(ROWS) {
            let y = oy + 24.0 + i as f32 * ROW;
            out.push(([ox + 10.0, y, PANEL[0] - 44.0, 20.0], Hit::Pick(i)));
            out.push(([ox + PANEL[0] - 30.0, y, 20.0, 20.0], Hit::Forget(i)));
        }
        let y = oy + PANEL[1] - 30.0;
        out.push(([ox + 10.0, y, 64.0, 20.0], Hit::Add));
        out.push(([ox + 78.0, y, 64.0, 20.0], Hit::Rename));
        out.push(([ox + PANEL[0] - 74.0, y, 64.0, 20.0], Hit::Close));
        out
    }
    fn distance(p: &Place, x: f64, z: f64) -> f64 {
        ((p.pos[0] - x).powi(2) + (p.pos[2] - z).powi(2)).sqrt()
    }
    /// The places of the player's dimension in the world: a beam each, hidden by what is in
    /// front of it, and its diamond, name and distance seen through everything; set when that
    /// changes (the distances by the metre).
    fn show_in_world(&mut self) {
        let Some(p) = game::get_player() else {
            return;
        };
        let here: Vec<(usize, &Place)> = self
            .places
            .iter()
            .enumerate()
            .filter(|(_, place)| place.dimension == p.dimension)
            .collect();
        let mut icons = Vec::new();
        for (i, place) in &here {
            let [x, y, z] = place.pos;
            let color = COLORS[i % COLORS.len()];
            let metres = Self::distance(place, p.x, p.z);
            let diamond = Icon {
                sprite: "waypoints:diamond".into(),
                color: 0xFF_000000 | color,
                size: 11.0,
                above: format!(
                    "{{\"text\":\"{}\",\"color\":\"#{color:06X}\"}}",
                    place.name.replace(['\\', '"'], "")
                ),
                below: format!("{{\"text\":\"{metres:.0} m\",\"color\":\"#E0E0E0\"}}"),
                // On the beam, at the height of someone standing there.
                at: Point {
                    x,
                    y: y + DIAMOND_HEIGHT,
                    z,
                },
                text_scale: 1.0,
                background: 0x80_000000,
                fixed_size: true,
                through_walls: true,
            };
            if self.chosen == Some(*i) {
                // Ringed in white, the texts outside the ring.
                icons.push(Icon {
                    sprite: "waypoints:diamond_ring".into(),
                    color: 0xFF_FFFFFF,
                    size: 17.0,
                    ..diamond.clone()
                });
                icons.push(Icon {
                    above: String::new(),
                    below: String::new(),
                    ..diamond
                });
            } else {
                icons.push(diamond);
            }
        }
        let shown: String = icons
            .iter()
            .map(|i| {
                format!(
                    "{}|{}|{}|{}|{}\n",
                    i.sprite, i.above, i.below, i.at.x, i.at.z
                )
            })
            .collect();
        if shown == self.shown {
            return;
        }
        self.shown = shown;
        let beams: Vec<Lines> = here
            .iter()
            .map(|(i, place)| {
                let [x, y, z] = place.pos;
                Lines {
                    origin: Point { x, y, z },
                    ends: vec![0.0, 0.0, 0.0, 0.0, BEAM, 0.0],
                    colors: vec![0xC0_000000 | COLORS[i % COLORS.len()]],
                    width: BEAM_WIDTH,
                }
            })
            .collect();
        let _ = world_draw::set_layer(
            "beams",
            &Layer {
                lines: beams,
                boxes: Vec::new(),
                texts: Vec::new(),
                labels: Vec::new(),
                through_walls: false,
            },
        );
        let _ = world_draw::set_icons("marks", &icons);
    }
}

fn inside(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

impl Mod for Waypoints {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new()
            .keybind("open", "Waypoints", "key.keyboard.j")
            .on_hud()
            .on_tick();
        // The keybind's touch button, a compass at the left; and a button on the pause menu.
        widgets::add_touch_button(&TouchButton {
            id: "open".into(),
            label: "Waypoints".into(),
            icon: "item/compass_16".into(),
            shape: Shape::Square,
            x: 0.045,
            y: 0.32,
            size: 0.42,
        })?;
        widgets::add_screen_button(&ScreenButton {
            id: "open".into(),
            screen: "pause".into(),
            anchor: Anchor::TopLeft,
            x: 4.0,
            y: 4.0,
            width: 84.0,
            height: 20.0,
            label: "Waypoints".into(),
            icon: "item/compass_16".into(),
            tooltip: vec!["Your saved places".into()],
            active: true,
        })?;
        Ok((
            Waypoints {
                places: Self::load(),
                chosen: None,
                renaming: None,
                shown: String::new(),
            },
            hooks,
        ))
    }

    fn key(&mut self, id: &str, pressed: bool) {
        if id == "open" && pressed {
            screens::open("list");
        }
    }

    fn tick(&mut self, _tick: u64) {
        self.show_in_world();
    }

    fn screen_render(&mut self, screen: &str, context: RenderContext) {
        if screen != SCREEN {
            return;
        }
        let [ox, oy] = Self::origin(&context);
        draw::panel(ox, oy, PANEL[0], PANEL[1]);
        let title = "Waypoints";
        let w = draw::text_width(title);
        draw::text(ox + (PANEL[0] - w) / 2.0, oy + 8.0, title, 0xFFFFFF, true);
        let (px, pz) = game::get_player().map_or((0.0, 0.0), |p| (p.x, p.z));
        if self.places.is_empty() {
            draw::text(ox + 12.0, oy + 30.0, "No places yet.", 0xA0A0A0, true);
        }
        for (r, hit) in self.buttons(&context) {
            let hovered = inside(r, context.mouse_x, context.mouse_y);
            let label = match hit {
                Hit::Pick(i) => {
                    let p = &self.places[i];
                    let mark = if self.chosen == Some(i) { "> " } else { "" };
                    format!("{mark}{} ({:.0} m)", p.name, Self::distance(p, px, pz))
                }
                Hit::Forget(_) => "x".into(),
                Hit::Add => "Add here".into(),
                Hit::Rename => "Rename".into(),
                Hit::Close => "Close".into(),
            };
            let active = !matches!(hit, Hit::Rename) || self.chosen.is_some();
            draw::button(r[0], r[1], r[2], r[3], &label, hovered && active, active);
        }
        // The name being typed, in a box over the bottom buttons.
        if let Some(name) = &self.renaming {
            let y = oy + PANEL[1] - 54.0;
            draw::fill(ox + 10.0, y, PANEL[0] - 20.0, 18.0, 0xFF_000000);
            draw::fill(ox + 11.0, y + 1.0, PANEL[0] - 22.0, 16.0, 0xFF_202020);
            draw::text(ox + 15.0, y + 5.0, &format!("{name}_"), 0xFFFFFF, true);
        }
    }

    fn screen_mouse(&mut self, screen: &str, event: MouseEvent) -> bool {
        if screen != SCREEN || event.action != MouseAction::Press {
            return screen == SCREEN;
        }
        let context = RenderContext {
            width: game::get_gui().width,
            height: game::get_gui().height,
            mouse_x: event.x,
            mouse_y: event.y,
            delta: 0.0,
        };
        let hit = self
            .buttons(&context)
            .into_iter()
            .find(|(r, _)| inside(*r, event.x, event.y))
            .map(|(_, hit)| hit);
        match hit {
            Some(Hit::Pick(i)) => {
                self.chosen = if self.chosen == Some(i) {
                    None
                } else {
                    Some(i)
                };
            }
            Some(Hit::Forget(i)) => {
                self.places.remove(i);
                self.chosen = None;
                self.save();
            }
            Some(Hit::Add) => {
                if let Some(p) = game::get_player()
                    && self.places.len() < ROWS
                {
                    self.places.push(Place {
                        name: format!("Place {}", self.places.len() + 1),
                        pos: [p.x, p.y, p.z],
                        dimension: p.dimension,
                    });
                    self.save();
                }
            }
            Some(Hit::Rename) => {
                if let Some(i) = self.chosen {
                    self.renaming = Some(self.places[i].name.clone());
                    mobile::keyboard(true);
                }
            }
            Some(Hit::Close) => screens::close(),
            None => {}
        }
        true
    }

    fn screen_key(&mut self, screen: &str, event: &KeyEvent) -> bool {
        let (Some(name), true) = (self.renaming.as_mut(), screen == SCREEN && event.pressed) else {
            return screen == SCREEN;
        };
        match event.key.as_str() {
            "key.keyboard.backspace" => {
                name.pop();
            }
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
                if let Some(i) = self.chosen
                    && !name.trim().is_empty()
                {
                    // `|` and new lines would break the stored line.
                    self.places[i].name = name.trim().replace(['|', '\n'], " ");
                    self.save();
                }
                self.renaming = None;
                mobile::keyboard(false);
            }
            _ => {
                for c in event.text.chars().filter(|c| !c.is_control()) {
                    if name.chars().count() < 24 {
                        name.push(c);
                    }
                }
            }
        }
        true
    }

    fn screen_closed(&mut self, screen: &str) {
        if screen == SCREEN {
            self.renaming = None;
        }
    }

    fn hud_render(&mut self, context: RenderContext) {
        let (Some(i), Some(p)) = (self.chosen, game::get_player()) else {
            return;
        };
        let Some(place) = self.places.get(i) else {
            return;
        };
        if place.dimension != p.dimension {
            return;
        }
        // The place's bearing against the player's facing (yaw 0 is south, +z).
        let (dx, dz) = (place.pos[0] - p.x, place.pos[2] - p.z);
        let bearing = (-dx).atan2(dz).to_degrees() as f32;
        let turn = (bearing - p.yaw + 540.0).rem_euclid(360.0) - 180.0;
        let arrow = match turn {
            t if t.abs() < 20.0 => "^",
            t if t.abs() > 160.0 => "v",
            t if t < 0.0 => "<",
            _ => ">",
        };
        let label = format!("{arrow} {} {:.0} m", place.name, dx.hypot(dz));
        let w = draw::text_width(&label);
        let x = ((context.width as f32 - w) / 2.0).floor();
        draw::fill(x - 3.0, 3.0, w + 6.0, 12.0, 0x80_000000);
        draw::text(x, 5.0, &label, 0xFFFFFF, true);
    }
}

starling_mod_sdk::export_mod!(Waypoints);
