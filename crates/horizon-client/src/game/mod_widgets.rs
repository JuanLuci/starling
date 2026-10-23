//! The mods' buttons (`widgets`): on the game's screens, drawn as the screen's own buttons are
//! and pressed before the screen sees the click; on the touch controls, where they are placed in
//! the player's layout as the game's own buttons (game/touch.rs); and the looks mods and resource
//! packs give the game's own touch buttons. A press is the mod's keybind of that id
//! ([`Game::mod_bind`]).
use super::{Game, mods::rich_text};
use horizon_client::{
    hud::{HudBatch, rgba},
    screen_draw::{self, Draw, Measure},
    touch::{Button, Place, Shape},
    ui_theme::{self, Kind, Ui},
};
use horizon_mods::{Anchor, ScreenButton, Shape as ModShape};

/// A mod's button on the touch controls, as it is drawn.
#[derive(Clone, Debug)]
pub(super) struct ModFace {
    /// `<mod id>:<button id>`, its name in the layout.
    pub(super) key: String,
    pub(super) label: String,
    /// The atlas sprite drawn on it instead of the label.
    pub(super) icon: Option<String>,
    pub(super) shape: Shape,
}

/// How one of the game's touch buttons looks: a mod's style, else a pack's sprite, else drawn.
pub(super) struct ControlFace {
    pub(super) label: String,
    /// A mod's icon, drawn on the plate.
    pub(super) icon: Option<String>,
    /// A resource pack's whole button: its sprite and its pressed sprite.
    pub(super) skin: Option<(String, Option<String>)>,
    pub(super) shape: Shape,
}

/// A keybind's button: 16 GUI pixels tall, as wide as its label, as vanilla's buttons.
const WIDE_HEIGHT: f32 = 16.0;

/// A button's box on a screen `sw` by `sh` GUI pixels.
fn anchored(b: &ScreenButton, sw: f32, sh: f32) -> [f32; 4] {
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
    [
        ((sw - w) * fx + b.x).floor(),
        ((sh - h) * fy + b.y).floor(),
        w,
        h,
    ]
}

fn inside(r: [f32; 4], (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

impl Game<'_> {
    /// The atlas sprite a mod means by `sprite`: `hud/food_full` (vanilla's GUI sprites),
    /// `item/compass_16` (an item texture), `<namespace>:my_icon` (a content mod's or a pack's
    /// GUI sprite, `assets/<namespace>/textures/gui/sprites/my_icon.png`).
    pub(super) fn mod_sprite(&self, sprite: &str) -> Option<String> {
        let atlas = &self.gui.atlas;
        let (ns, path) = sprite.split_once(':').unwrap_or(("minecraft", sprite));
        [
            format!("gui/sprites/{path}"),
            format!("{ns}:gui/sprites/{path}"),
            sprite.to_owned(),
        ]
        .into_iter()
        .filter(|k| ns == "minecraft" || !k.starts_with("gui/"))
        .find(|k| atlas.sprites.contains_key(k))
    }

    /// The GUI's size now, in GUI pixels.
    fn widget_gui(&self) -> (f32, f32) {
        let batch = HudBatch::new(self.touch_size());
        (batch.width, batch.height)
    }

    /// The mods' buttons on the screen shown now (none under a mod's own screen): mod id, button
    /// and its box in GUI pixels.
    fn mod_screen_buttons(&self) -> Vec<(String, ScreenButton, [f32; 4])> {
        if !self.mods.mods.is_running() || self.mods.screen.is_some() {
            return Vec::new();
        }
        let Some(screen) = self.mod_screen_name() else {
            return Vec::new();
        };
        let (sw, sh) = self.widget_gui();
        self.mods
            .mods
            .widgets()
            .flat_map(|(id, w)| w.screen.iter().map(move |b| (id, b)))
            .filter(|(_, b)| b.screen == screen)
            .map(|(id, b)| (id.to_owned(), b.clone(), anchored(b, sw, sh)))
            .collect()
    }

    /// Over the game's screen: the mods' buttons, as its own (vanilla's, or the Horizon menus'),
    /// the hovered one's tooltip last.
    pub(super) fn draw_mod_screen_buttons(&mut self, batch: &mut HudBatch) {
        let buttons = self.mod_screen_buttons();
        if buttons.is_empty() {
            return;
        }
        let cursor = self.cursor;
        let touch = self.has_touch();
        let pressed = self.mods.pressed_button.clone();
        let mut tooltip = None;
        for (id, b, r) in &buttons {
            let hovered = !touch && inside(*r, cursor);
            let held = pressed.as_ref() == Some(&(id.clone(), b.id.clone()));
            let icon = (!b.icon.is_empty())
                .then(|| self.mod_sprite(&b.icon))
                .flatten();
            // The label after the icon, a square as tall as the button lets it be.
            let side = (r[3] - 4.0).clamp(0.0, 16.0);
            let text_left = if icon.is_some() { side + 4.0 } else { 0.0 };
            if ui_theme::horizon() {
                let atlas = &self.gui.atlas;
                let scale = batch.scale;
                let mut ui = Ui::new(atlas, batch, cursor);
                // GUI pixels to the Horizon menus' dp.
                let k = scale / ui.density;
                let rect = [r[0] * k, r[1] * k, r[2] * k, r[3] * k];
                let label = rich_text(&b.label).plain();
                let padded = if icon.is_some() {
                    format!("    {label}")
                } else {
                    label
                };
                let kind = if held { Kind::Primary } else { Kind::Secondary };
                let tag = b.id.bytes().fold(0x5374_6172_u64, |h, c| {
                    h.wrapping_mul(31).wrapping_add(u64::from(c))
                });
                ui.button(tag, rect, &padded, kind, b.active);
                ui.finish();
            } else {
                let atlas = &self.gui.atlas;
                let m = Measure(&atlas.font);
                let mut label = rich_text(&b.label);
                let room = r[2] as i32 - 4 - text_left as i32;
                if m.width(&label) > room {
                    label = m.head(&label, room);
                }
                let mut draws = Vec::new();
                screen_draw::button(
                    &mut draws,
                    &m,
                    &label,
                    r[0] as i32,
                    r[1] as i32,
                    r[2] as i32,
                    r[3] as i32,
                    b.active,
                    hovered || held,
                );
                // The label in the middle of what the icon leaves.
                for d in &mut draws {
                    if let Draw::Text { x, .. } = d {
                        *x += (text_left / 2.0) as i32;
                    }
                }
                screen_draw::paint(batch, atlas, &draws);
            }
            if let Some(icon) = icon {
                let tint = if b.active {
                    u32::MAX
                } else {
                    rgba(160, 160, 160, 255)
                };
                let (x, y) = (r[0] + 3.0, r[1] + ((r[3] - side) / 2.0).floor());
                horizon_client::ui::sprite_sized(
                    batch,
                    &self.gui.atlas,
                    &icon,
                    x,
                    y,
                    side,
                    side,
                    tint,
                );
            }
            if hovered && !b.tooltip.is_empty() {
                tooltip = Some(b.tooltip.clone());
            }
        }
        if let Some(lines) = tooltip {
            let draws = [Draw::Tooltip {
                lines: lines.iter().map(|l| rich_text(l)).collect(),
                x: cursor.0 as i32,
                y: cursor.1 as i32,
            }];
            screen_draw::paint(batch, &self.gui.atlas, &draws);
        }
    }

    /// A left click on the game's screen: a mod's button under the cursor takes it (its press,
    /// then its release when the button comes up). True when one did.
    pub(super) fn mod_screen_button_click(&mut self, pressed: bool) -> bool {
        if !pressed {
            return match self.mods.pressed_button.take() {
                Some((id, button)) => {
                    self.mod_bind(&id, &button, false);
                    true
                }
                None => false,
            };
        }
        let cursor = self.cursor;
        let hit = self
            .mod_screen_buttons()
            .into_iter()
            .rev()
            .find(|(_, _, r)| inside(*r, cursor));
        let Some((id, b, _)) = hit else {
            return false;
        };
        if b.active {
            self.audio
                .play("minecraft:ui.button.click", None, 0.25, 1.0);
            self.mod_bind(&id, &b.id, true);
            self.mods.pressed_button = Some((id, b.id));
        }
        true
    }

    /// The mods' buttons on the touch controls: each keybind's (wide, its name on it, in a row
    /// at the left under the top buttons) unless the mod gave that id a look of its own, then the
    /// mod's own buttons. Each with the place it has until the player moves it.
    pub(super) fn mod_touch_buttons(&self) -> Vec<(ModFace, Place)> {
        if !self.mods.mods.is_running() {
            return Vec::new();
        }
        let [w, h] = self.touch_size();
        let (w, h) = (w as f32, h as f32);
        let scale = HudBatch::new([w as u32, h as u32]).scale;
        let unit = horizon_client::touch::unit(w, h);
        let atlas = &self.gui.atlas;
        let mut out = Vec::new();
        let place = |x: f32, y: f32, size: f32| Place {
            x,
            y,
            size,
            show: horizon_client::touch::Show::On,
            opacity: 1.0,
        };
        // Wide buttons are as tall as vanilla's 16 GUI pixels: the size is half that.
        let wide_size = WIDE_HEIGHT / 2.0 * scale / unit;
        let wide = |label: &str| {
            let width = HudBatch::text_width(atlas, label) + 10.0;
            Shape::Wide(width / WIDE_HEIGHT)
        };
        let widgets: Vec<_> = self.mods.mods.widgets().collect();
        let own = |id: &str, button: &str| {
            widgets
                .iter()
                .find(|(m, _)| *m == id)
                .and_then(|(_, w)| w.touch.iter().find(|b| b.id == button))
        };
        // The keybinds in a row, 4 GUI pixels apart, from 4 in at 20% of the height.
        let mut x = 4.0 * scale;
        for (id, bind) in self.mods.mods.keybinds() {
            if own(&id, &bind.id).is_some() {
                continue;
            }
            let label: String = bind.name.chars().take(14).collect();
            let shape = wide(&label);
            let half = shape.half(WIDE_HEIGHT / 2.0 * scale);
            let centre = [x + half[0], h * 0.2 + half[1]];
            x += 2.0 * half[0] + 3.0 * scale;
            out.push((
                ModFace {
                    key: format!("{id}:{}", bind.id),
                    label,
                    icon: None,
                    shape,
                },
                place(centre[0] / w, centre[1] / h, wide_size),
            ));
        }
        for (id, w) in &widgets {
            for b in &w.touch {
                let shape = match b.shape {
                    ModShape::Round => Shape::Round,
                    ModShape::Square => Shape::Square,
                    ModShape::Wide => wide(&b.label),
                };
                let icon = (!b.icon.is_empty())
                    .then(|| self.mod_sprite(&b.icon))
                    .flatten();
                let size = if b.shape == ModShape::Wide {
                    wide_size
                } else {
                    b.size
                };
                out.push((
                    ModFace {
                        key: format!("{id}:{}", b.id),
                        label: b.label.clone(),
                        icon,
                        shape,
                    },
                    place(b.x, b.y, size),
                ));
            }
        }
        out
    }

    /// How one of the game's touch buttons looks now.
    pub(super) fn control_face(&self, button: Button) -> ControlFace {
        let name = button.id();
        let style = self
            .mods
            .mods
            .widgets()
            .flat_map(|(_, w)| w.styles.iter())
            .filter(|s| s.control == name)
            .last();
        let skin = self.mod_sprite(&format!("starling:touch/{name}")).map(|s| {
            let pressed = self.mod_sprite(&format!("starling:touch/{name}_pressed"));
            (s, pressed)
        });
        let Some(style) = style else {
            return ControlFace {
                label: button.label().to_owned(),
                icon: None,
                skin,
                shape: Shape::Round,
            };
        };
        ControlFace {
            label: if style.label.is_empty() {
                button.label().to_owned()
            } else {
                style.label.clone()
            },
            icon: (!style.icon.is_empty())
                .then(|| self.mod_sprite(&style.icon))
                .flatten(),
            skin: None,
            shape: match style.shape {
                ModShape::Square => Shape::Square,
                ModShape::Round | ModShape::Wide => Shape::Round,
            },
        }
    }
}
