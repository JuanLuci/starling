//! Example Horizon mod (`docs/mods.md`) for the screen API: J (or its touch button) opens a
//! screen of saved places drawn with vanilla buttons; "Add here" saves where the player stands,
//! a place's button picks it, "x" forgets it and "Rename" names the picked one (bringing up the
//! on-screen keyboard on a phone; Enter keeps the name). The chosen place's distance and
//! direction show at the top of the HUD. Places are kept with `storage`, one per line.
use horizon_mod_sdk::{
    KeyEvent, Mod, MouseAction, MouseEvent, Registration, RenderContext, draw, game, screens,
    storage,
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
}

const SCREEN: &str = "waypoints:list";
/// The panel and its rows, in GUI pixels from the panel's corner.
const PANEL: [f32; 2] = [220.0, 176.0];
const ROW: f32 = 22.0;
const ROWS: usize = 5;

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
}

fn inside(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

impl Mod for Waypoints {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new()
            .keybind("open", "Waypoints", "key.keyboard.j")
            .on_hud();
        Ok((
            Waypoints {
                places: Self::load(),
                chosen: None,
                renaming: None,
            },
            hooks,
        ))
    }

    fn key(&mut self, id: &str, pressed: bool) {
        if id == "open" && pressed {
            screens::open("list");
        }
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
                    screens::keyboard(true);
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
                screens::keyboard(false);
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

horizon_mod_sdk::export_mod!(Waypoints);
