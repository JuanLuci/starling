//! Touch input (horizon_client::touch): on the playing screen the touch controls, drawn over the
//! HUD and turned into the virtual pad, mouse motion and the hotbar, laid out as the player's
//! touch-layout.json says (edited in place: a long press on pause); on every other screen the
//! first finger is the pointer, with a close button over the screens a world opens. Android's
//! back key is Escape, and the chat raises the on-screen keyboard and moves above it. On the
//! screens with slots a finger works them as Bedrock's touch inventory does (slots.rs). The mods'
//! buttons are in the layout as the game's own (game/mod_widgets.rs).
mod slots;
mod text_mirror;

use super::{Game, Screen, mod_widgets::ModFace};
use horizon_client::gamepad::ServerStick;
use horizon_client::{
    Result,
    gamepad::Button as Pad,
    hud::HudBatch,
    touch::{
        self, Action, Button, Chip, EditorAction, Layout, LayoutConfig, LayoutEditor, Shape, Show,
        Target, TouchControls,
    },
    ui_theme::{Face, Ui, color},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, KeyCode, NamedKey},
    window::WindowId,
};

pub(super) const HELP: &[&str] = &[
    "touch-layout [edit|done|reset]  the touch controls' layout (touch-layout.json): show it, edit it in place, save, or go back to the default",
];

/// How long a tap holds the use trigger: vanilla samples input once a tick.
const USE_PULSE: Duration = Duration::from_millis(60);
/// The layout's file, beside options.txt.
pub(super) const LAYOUT_FILE: &str = "touch-layout.json";

#[derive(Default)]
pub(super) struct Touch {
    controls: TouchControls,
    /// The finger acting as the mouse off the playing screen.
    pointer: touch::TouchPointer,
    /// The finger on a slot and the stack it picked (slots.rs).
    slots: slots::SlotTouch,
    /// A touch was seen: draw the controls (always on Android).
    seen: bool,
    /// The controls were live last frame (the playing screen).
    live: bool,
    use_until: Option<Instant>,
    /// The on-screen keyboard was asked for.
    keyboard: bool,
    /// The keyboard is wanted over the text field shown now: a typing screen opened, or a tap
    /// landed on the field; a tap beside it, the back key or the field going away end it.
    wanted: bool,
    /// The text field the keyboard was last decided for (`text_context`).
    context: Option<&'static str>,
    /// The keyboard has shown since it was asked for: its height falling to 0 then means the
    /// player put it away (the back key), which the game is not told otherwise.
    ime_seen: bool,
    /// The keyboard's editor on Android (text_mirror.rs).
    mirror: text_mirror::Mirror,
    /// The player's layout, read from touch-layout.json on the first frame.
    config: Option<LayoutConfig>,
    /// The layout editor, while it is open.
    editor: Option<LayoutEditor>,
    /// The chat bar's button last tapped and when (it lights up a moment).
    chat_key: Option<(usize, Instant)>,
    /// The finger on the chat bar: its moves and lift are the bar's.
    chat_finger: Option<u64>,
    /// Fingers on the mods' buttons: (finger, mod id, button id).
    mod_keys: Vec<(u64, String, String)>,
    /// The mods' buttons as last offered to the layout.
    faces: Vec<ModFace>,
}

/// The chat bar's keys, left to right: history up and down, completion, the cursor; then Paste.
const CHAT_KEYS: [KeyCode; 5] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::Tab,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
];

/// A pixel arrow centred on (cx, cy): a head 7 wide and a stem 3 wide. `dir`: 0 up, 1 down,
/// 2 left, 3 right.
fn arrow(
    batch: &mut HudBatch,
    atlas: &horizon_client::hud::HudAtlas,
    c: [f32; 2],
    dir: u8,
    ink: u32,
) {
    let [cx, cy] = c;
    for k in 0..4 {
        let (k, w) = (k as f32, 2.0 * k as f32 + 1.0);
        let [x, y, w, h] = match dir {
            0 => [cx - k, cy - 3.0 + k, w, 1.0],
            1 => [cx - k, cy + 3.0 - k, w, 1.0],
            2 => [cx - 3.0 + k, cy - k, 1.0, w],
            _ => [cx + 3.0 - k, cy - k, 1.0, w],
        };
        batch.fill(atlas, x, y, w, h, ink);
    }
    let [x, y] = match dir {
        0 => [cx - 1.0, cy + 1.0],
        1 => [cx - 1.0, cy - 3.0],
        2 => [cx + 1.0, cy - 1.0],
        _ => [cx - 3.0, cy - 1.0],
    };
    batch.fill(atlas, x, y, 3.0, 3.0, ink);
}

/// A pixel clipboard centred on (cx, cy), for Paste: a board 9 by 11, its clip and two lines.
fn clipboard(batch: &mut HudBatch, atlas: &horizon_client::hud::HudAtlas, c: [f32; 2], ink: u32) {
    let [cx, cy] = c;
    let (left, top) = (cx - 4.0, cy - 4.0);
    batch.fill(atlas, left, top, 1.0, 10.0, ink);
    batch.fill(atlas, left + 8.0, top, 1.0, 10.0, ink);
    batch.fill(atlas, left, top + 9.0, 9.0, 1.0, ink);
    batch.fill(atlas, left, top, 2.0, 1.0, ink);
    batch.fill(atlas, left + 7.0, top, 2.0, 1.0, ink);
    batch.fill(atlas, cx - 2.0, top - 1.0, 5.0, 2.0, ink);
    batch.fill(atlas, cx - 2.0, cy, 5.0, 1.0, ink);
    batch.fill(atlas, cx - 2.0, cy + 2.0, 5.0, 1.0, ink);
}

// The controls' looks, in window pixels: a plate, a pressed plate and a label at a control's
// opacity.
fn plate(a: f32) -> u32 {
    color::with_alpha(color::WHITE, 0.16 * a)
}
fn pressed(a: f32) -> u32 {
    color::with_alpha(color::WHITE, 0.38 * a)
}
fn ink(a: f32) -> u32 {
    color::with_alpha(color::WHITE, 0.85 * a)
}
fn circle(ui: &mut Ui, c: [f32; 2], r: f32, fill: u32) {
    let d = ui.density;
    ui.rounded(
        (c[0] - r) / d,
        (c[1] - r) / d,
        2.0 * r / d,
        2.0 * r / d,
        r / d,
        fill,
    );
}
fn label(ui: &mut Ui, c: [f32; 2], r: f32, text: &str, tint: u32) {
    let d = ui.density;
    let size = (r / d * 0.5).clamp(9.0, 16.0);
    ui.text_centered(
        Face::Semibold,
        size,
        c[0] / d,
        c[1] / d - size * 0.62,
        tint,
        text,
    );
}
/// A plate and an outline of any shape (circles as above).
fn plate_of(ui: &mut Ui, shape: Shape, c: [f32; 2], r: f32, fill: u32) {
    let d = ui.density;
    let [hw, hh] = shape.half(r);
    let corner = match shape {
        Shape::Square => r * 0.3,
        Shape::Round | Shape::Wide(_) => r,
    };
    ui.rounded(
        (c[0] - hw) / d,
        (c[1] - hh) / d,
        2.0 * hw / d,
        2.0 * hh / d,
        corner / d,
        fill,
    );
}
fn ring_of(ui: &mut Ui, shape: Shape, c: [f32; 2], r: f32, line: u32) {
    let d = ui.density;
    let [hw, hh] = shape.half(r);
    let corner = match shape {
        Shape::Square => r * 0.3,
        Shape::Round | Shape::Wide(_) => r,
    };
    ui.border(
        (c[0] - hw) / d,
        (c[1] - hh) / d,
        2.0 * hw / d,
        2.0 * hh / d,
        corner / d,
        line,
    );
}
/// A sprite over a button, `share` of its height, at an opacity.
fn icon(ui: &mut Ui, sprite: &str, c: [f32; 2], r: f32, share: f32, a: f32) {
    let gui = ui.batch.scale;
    let side = (2.0 * r * share / gui).round();
    let (x, y) = (
        (c[0] / gui - side / 2.0).round(),
        (c[1] / gui - side / 2.0).round(),
    );
    let tint = horizon_client::hud::rgba(255, 255, 255, (a.clamp(0.0, 1.0) * 255.0) as u8);
    horizon_client::ui::sprite_sized(ui.batch, ui.atlas, sprite, x, y, side, side, tint);
}
/// A label's size for a button's radius: a wide button's is its height's, not its width's.
fn text_r(shape: Shape, r: f32) -> f32 {
    match shape {
        Shape::Wide(_) => r * 1.5,
        Shape::Round | Shape::Square => r,
    }
}

/// The layout editor over whatever is behind it (the world, the title, the setup's settings):
/// every control where it will be, the selected one's chips, the opacity slider, and Done, Reset
/// and the movement control. `face` names a button or a mod's button (and gives the latter its
/// icon); `shape` is a button's.
pub(super) fn draw_editor(
    ui: &mut Ui,
    layout: &Layout,
    config: &LayoutConfig,
    editor: &LayoutEditor,
    face: &dyn Fn(Target) -> (String, Option<String>),
    shape: &dyn Fn(Button) -> Shape,
) {
    let d = ui.density;
    let (w, h) = (layout.width, layout.height);
    // The movement control as it moves the player now.
    let dpad = layout.dpad;
    ui.fill(0.0, 0.0, w / d, h / d, color::with_alpha(color::BG, 0.45));
    for (target, place, c, r) in LayoutEditor::targets(config, w, h) {
        // As it will look (at least faintly, to be found again), outlined if a ghost.
        let a = (place.opacity * config.opacity).max(0.35);
        let shape = match target {
            Target::Extra(i) => config.extra[i].shape,
            Target::Button(b) => shape(b),
            Target::Stick if dpad => Shape::Square,
            Target::Stick => Shape::Round,
        };
        match place.show {
            Show::On => plate_of(ui, shape, c, r, pressed(a)),
            Show::Ghost => ring_of(ui, shape, c, r, ink(0.5)),
            Show::Off => plate_of(ui, shape, c, r, plate(0.5)),
        }
        if editor.selected == Some(target) {
            ring_of(ui, shape, c, r, color::ACCENT);
        }
        let (name, sprite) = match target {
            Target::Stick if dpad => ("D-pad".to_owned(), None),
            Target::Stick => ("Move".to_owned(), None),
            _ => face(target),
        };
        let tint = if place.show == Show::Off {
            ink(0.4)
        } else {
            ink(1.0)
        };
        // A mod's button with an icon shows it, as while playing (its name is on the opacity
        // slider once picked).
        let drawn = match target {
            Target::Extra(_) if !matches!(shape, Shape::Wide(_)) => sprite,
            _ => None,
        };
        match drawn {
            Some(sprite) => {
                let a = if place.show == Show::Off { 0.4 } else { 1.0 };
                icon(ui, &sprite, c, r, 0.6, a);
            }
            None => label(ui, c, text_r(shape, r), &name, tint),
        }
    }
    let shown = editor.selected.and_then(|t| {
        LayoutEditor::targets(config, w, h)
            .into_iter()
            .find(|(x, ..)| *x == t)
            .map(|(_, p, ..)| p.show)
    });
    for (chip, c, r) in editor.chips(config, w, h) {
        circle(ui, c, r, color::with_alpha(color::BG, 0.85));
        let text = match chip {
            Chip::Smaller => "-",
            Chip::Bigger => "+",
            Chip::Visibility => match shown {
                Some(Show::Ghost) => "Ghost",
                Some(Show::Off) => "Off",
                _ => "On",
            },
        };
        label(ui, c, r, text, ink(1.0));
    }
    // The opacity slider: the selected control's, or everyone's.
    let ([x0, y], [x1, _]) = LayoutEditor::opacity_track(w, h);
    let value = editor.opacity(config);
    let u = layout.unit;
    ui.rounded(
        x0 / d,
        (y - 0.08 * u) / d,
        (x1 - x0) / d,
        0.16 * u / d,
        0.08 * u / d,
        color::with_alpha(color::BG, 0.85),
    );
    ui.rounded(
        x0 / d,
        (y - 0.08 * u) / d,
        (x1 - x0) * value / d,
        0.16 * u / d,
        0.08 * u / d,
        color::ACCENT,
    );
    circle(ui, [x0 + (x1 - x0) * value, y], 0.28 * u, ink(1.0));
    let whose = match editor.selected {
        Some(Target::Stick) => "Move".to_owned(),
        Some(Target::Button(b)) => b.label().to_owned(),
        Some(target @ Target::Extra(_)) => face(target).0,
        None => "all".to_owned(),
    };
    ui.text_centered(
        Face::Semibold,
        12.0,
        (x0 + x1) / 2.0 / d,
        (y - 0.75 * u) / d,
        ink(1.0),
        &format!("Opacity ({whose}): {:.0}%", value * 100.0),
    );
    for (action, c, r) in LayoutEditor::bar(w, h) {
        let fill = match action {
            EditorAction::Done => color::with_alpha(color::ACCENT, 0.9),
            EditorAction::Reset | EditorAction::Movement => color::with_alpha(color::BG, 0.85),
        };
        circle(ui, c, r, fill);
        let text = match action {
            EditorAction::Done => "Done",
            EditorAction::Reset => "Reset",
            // The movement control picked (a press switches it): auto is the D-pad where the
            // stick moves as the keys.
            EditorAction::Movement => match config.dpad {
                Some(true) => "D-pad",
                Some(false) => "Stick",
                None => "Auto",
            },
        };
        label(ui, c, r, text, ink(1.0));
    }
}

impl Game<'_> {
    /// The player plays with a touch screen: always on Android, elsewhere once a touch was seen.
    pub(super) fn has_touch(&self) -> bool {
        self.touch.seen
    }
    fn touch_playing(&self) -> bool {
        self.screen == Screen::Playing
            && self.title.is_none()
            && self.player.is_some()
            && !self.over_world_screen()
    }

    /// A screen drawn over the world without leaving the playing screen: the sign editor, a
    /// book, a command block, a dialog. Fingers are a pointer there, as on any other screen.
    fn over_world_screen(&self) -> bool {
        self.sign_edit.is_some()
            || self.book.is_some()
            || self.command_block_screen.is_some()
            || self.mods.screen.is_some()
            || self.server_packs.prompting()
            || self.dialog_open()
    }

    /// A screen opened in a world that the back key or the close button leaves.
    fn touch_closable(&self) -> bool {
        self.title.is_none()
            && (!matches!(self.screen, Screen::Playing | Screen::Dead) || self.over_world_screen())
    }

    /// The touch controls turn the view as a grabbed pointer does (mouse.rs).
    pub(super) fn touch_turns(&self) -> bool {
        self.touch.live && self.touch.editor.is_none()
    }

    pub(super) fn touch_size(&self) -> [u32; 2] {
        self.window.as_ref().map_or([1, 1], |w| {
            let s = w.inner_size();
            [s.width.max(1), s.height.max(1)]
        })
    }

    fn touch_layout(&self) -> Layout {
        let [w, h] = self.touch_size();
        // The scale the HUD is drawn at, which the GUI Scale option only caps.
        let scale = HudBatch::new([w, h]).scale;
        let mut layout = match &self.touch.config {
            Some(config) => Layout::from_config(config, w as f32, h as f32, scale),
            None => Layout::new(w as f32, h as f32, scale),
        };
        layout.dpad = self.touch_dpad();
        layout
    }

    /// Whether the D-pad moves the player: as the layout picked, else when the stick moves as the
    /// keys on this server (their eight directions on screen).
    fn touch_dpad(&self) -> bool {
        let picked = self.touch.config.as_ref().and_then(|c| c.dpad);
        picked.unwrap_or(self.remote_stick_mode() == Some(ServerStick::Keys))
    }

    fn touch_layout_path() -> Result<std::path::PathBuf> {
        Ok(std::env::current_dir()?.join(LAYOUT_FILE))
    }

    fn save_touch_layout(&self) {
        let Some(config) = &self.touch.config else {
            return;
        };
        let saved =
            Self::touch_layout_path().and_then(|path| Ok(std::fs::write(path, config.to_json())?));
        if let Err(e) = saved {
            log::warn!("Horizon: the touch layout was not saved: {e}");
        }
    }

    /// A touch event; true when it was taken (the caller passes the rest on).
    pub(super) fn touch_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        let WindowEvent::Touch(t) = event else {
            return false;
        };
        if !self.touch.seen {
            self.touch.seen = true;
            // Mods read the touch screen from this first finger on (`mobile.has-touch`).
            self.refresh_mod_snapshot();
        }
        self.last_input = Instant::now();
        // The offer of the Starling folder, over the title, takes the touches while it shows.
        #[cfg(target_os = "android")]
        if self.files_offer_touch(t) || self.update_offer_touch(t) {
            return true;
        }
        // A touch in the chat brings back a keyboard the back key put away.
        if t.phase == TouchPhase::Started && self.screen == Screen::Chat {
            self.touch.wanted = true;
        }
        let p = [t.location.x as f32, t.location.y as f32];
        // The chat bar: a tap is its key, and the finger stays the bar's until it lifts.
        if t.phase == TouchPhase::Started
            && let Some(i) = self.chat_bar_at(p)
        {
            self.touch.chat_key = Some((i, Instant::now()));
            self.touch.chat_finger = Some(t.id);
            match CHAT_KEYS.get(i) {
                Some(&key) => {
                    let _ = self.touch_key_tap(key);
                }
                None => self.chat_paste(),
            }
            return true;
        }
        if self.touch.chat_finger == Some(t.id) {
            if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.touch.chat_finger = None;
            }
            return true;
        }
        // The mods' buttons: held while a finger is on them.
        if t.phase == TouchPhase::Started
            && let Some((id, bind)) = self.mod_button_at(p)
        {
            self.touch.mod_keys.push((t.id, id.clone(), bind.clone()));
            self.mod_bind(&id, &bind, true);
            return true;
        }
        if let Some(i) = self.touch.mod_keys.iter().position(|(f, ..)| *f == t.id) {
            if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                let (_, id, bind) = self.touch.mod_keys.remove(i);
                self.mod_bind(&id, &bind, false);
            }
            return true;
        }
        if self.touch.editor.is_some() && (self.touch_playing() || self.title.is_some()) {
            let [w, h] = self.touch_size();
            let config = self.touch.config.get_or_insert_with(LayoutConfig::default);
            let editor = self.touch.editor.as_mut().unwrap();
            if editor.event(config, w as f32, h as f32, t.id, t.phase, p)
                == Some(EditorAction::Done)
            {
                self.touch.editor = None;
                self.save_touch_layout();
            }
            return true;
        }
        let layout = self.touch_layout();
        if t.phase == TouchPhase::Started
            && !self.touch.pointer.down()
            && self.touch_closable()
            && layout.close_at(p)
        {
            let _ = self.touch_key_tap(KeyCode::Escape);
            return true;
        }
        if !self.touch_playing()
            && !self.touch.pointer.owns(t.id)
            && self.slot_touch_event(event_loop, id, t)
        {
            return true;
        }
        // The mouse finger stays the mouse until it lifts, even if the screen closed under it.
        if !self.touch_playing() || self.touch.pointer.owns(t.id) {
            let slop = touch::slop(layout.width, layout.height);
            // A finger on a scrollbar drags it, as the mouse would.
            if t.phase == TouchPhase::Started && self.on_scrollbar(p) {
                self.touch.pointer.prefer_drag();
            }
            // Lists follow the finger one to one at this GUI scale.
            let gui_scale = HudBatch::new(self.touch_size()).scale;
            self.touch
                .pointer
                .set_gui_scale(gui_scale, self.settings.mouse_wheel_sensitivity);
            for mouse in self.touch.pointer.event(t, slop, Instant::now()) {
                self.window_event(event_loop, id, mouse);
            }
            // A tap on the text field brings up the on-screen keyboard, a tap beside it puts the
            // keyboard away (the chat keeps it).
            if t.phase == TouchPhase::Ended
                && let Some(context) = self.text_context()
                && context != "chat"
            {
                self.touch.wanted = self.on_text_field().unwrap_or(true);
            }
            return true;
        }
        let actions = self
            .touch
            .controls
            .event(&layout, t.id, t.phase, p, Instant::now());
        self.touch_apply(actions);
        true
    }

    /// Android's back key: Escape (pause, close a screen), or Done in the layout editor.
    pub(super) fn touch_back_key(&mut self, event: &KeyEvent) -> bool {
        if event.logical_key != Key::Named(NamedKey::BrowserBack) {
            return false;
        }
        if event.state == ElementState::Pressed && !event.repeat {
            if self.touch.editor.take().is_some() {
                self.save_touch_layout();
            } else {
                let _ = self.touch_key_tap(KeyCode::Escape);
            }
        }
        true
    }

    /// Once per loop iteration, before the pads are read.
    pub(super) fn touch_frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.touch.config.is_none() {
            let text = Self::touch_layout_path()
                .ok()
                .and_then(|path| std::fs::read_to_string(path).ok());
            self.touch.config =
                Some(text.map_or_else(LayoutConfig::default, |t| LayoutConfig::from_json(&t)));
        }
        // The mods' buttons, where the player put them (or their mods do).
        if self.touch.seen || cfg!(target_os = "android") {
            let (faces, offered): (Vec<_>, Vec<_>) = self
                .mod_touch_buttons()
                .into_iter()
                .map(|(face, place)| {
                    let offer = (face.key.clone(), place, face.shape);
                    (face, offer)
                })
                .unzip();
            if let Some(config) = &mut self.touch.config
                && (!offered.is_empty() || config.extra.iter().any(|e| e.live))
            {
                config.offer(&offered);
            }
            self.touch.faces = faces;
        }
        if let Some(id) = self.window.as_ref().map(|w| w.id()) {
            for mouse in self.touch.pointer.frame_events(now) {
                self.window_event(event_loop, id, mouse);
            }
            self.slot_touch_frame(event_loop, id);
        }
        let live = self.touch_playing();
        if self.touch.live && !live {
            // A screen opened over the world: let go of everything the fingers held.
            let actions = self.touch.controls.reset();
            self.touch_apply(actions);
            if self.touch.editor.take().is_some() {
                self.save_touch_layout();
            }
        }
        self.touch.live = live;
        // Fingers resting on the controls send no events: still input for the idle frame cap
        // (Inactivity FPS Limit: AFK), as a held key would be.
        if self.touch.controls.touching() || self.touch.pointer.down() {
            self.last_input = now;
        }
        let actions = self.touch.controls.frame(now);
        self.touch_apply(actions);
        if self.touch.use_until.is_some_and(|t| now >= t) {
            self.touch.use_until = None;
            self.pad.touch_trigger(0, 0.0);
        }
        // The on-screen keyboard (its keys arrive as key events; on Android an editor behind it
        // holds the field, text_mirror.rs). The screens made for typing
        // (chat, sign, book, command block) open it with them; the other text fields on a tap on
        // them. Android shows it only to a focused window: back from the background, it is asked
        // for again once the focus is back.
        let context = self.text_context();
        if context != self.touch.context {
            self.touch.context = context;
            self.touch.wanted =
                matches!(context, Some("chat" | "sign" | "book" | "command" | "mod"));
        }
        #[cfg(target_os = "android")]
        if self.touch.keyboard {
            if crate::ime_height() > 0 {
                self.touch.ime_seen = true;
            } else if self.touch.ime_seen {
                // Put away with the back key: a tap on the field brings it back.
                self.touch.wanted = false;
                self.touch.ime_seen = false;
            }
        }
        let keyboard =
            (self.touch.seen || cfg!(target_os = "android")) && self.touch.wanted && self.focused;
        // On Android the keyboard's editor shows it (text_mirror.rs).
        #[cfg(target_os = "android")]
        if self.window.is_some() {
            self.android_keyboard(keyboard);
        }
        #[cfg(not(target_os = "android"))]
        if keyboard != self.touch.keyboard
            && let Some(window) = &self.window
        {
            window.set_ime_allowed(keyboard);
        }
        if keyboard != self.touch.keyboard && self.window.is_some() {
            self.touch.keyboard = keyboard;
            self.touch.ime_seen = false;
        }
    }

    /// The text field shown now, by name: the chat, a sign, a book, a command block, the anvil's
    /// name, the creative and recipe searches, the title's forms, the options' searches.
    fn text_context(&mut self) -> Option<&'static str> {
        if self.mods.screen.is_some() {
            // A mod's screen is over everything: its text field, when it asked for the keyboard.
            self.mods.keyboard.then_some("mod")
        } else if self.screen == Screen::Chat {
            Some("chat")
        } else if self.sign_edit.is_some() {
            Some("sign")
        } else if matches!(self.book, Some(super::books::BookScreen::Edit { .. })) {
            Some("book")
        } else if self.command_block_screen.is_some() {
            Some("command")
        } else if self.station_typing() {
            Some("anvil")
        } else if self.screen == Screen::Creative && self.creative_screen.tab().search {
            Some("creative")
        } else if self.recipe_search_focused() {
            Some("recipe")
        } else if self.title.as_mut().is_some_and(|t| t.text_focused()) {
            Some("title")
        } else if self.screen == Screen::Options && self.ui.options.text_focused() {
            Some("options")
        } else {
            None
        }
    }

    /// Whether the cursor (where a tap just landed) is on the text field shown now; None where
    /// the field's place is not known.
    fn on_text_field(&mut self) -> Option<bool> {
        let (size, cursor) = (self.gui_size(), self.cursor);
        let inside = |r: [f32; 4]| {
            cursor.0 >= r[0] && cursor.0 < r[0] + r[2] && cursor.1 >= r[1] && cursor.1 < r[1] + r[3]
        };
        match self.text_context()? {
            "title" => self.title.as_mut().map(|t| t.text_field_at(cursor, size)),
            "options" => Some(self.ui.options.text_field_at(cursor.0, cursor.1)),
            context => self.text_field_rect(context).map(inside),
        }
    }

    /// The text field's box in GUI pixels, a little larger for a finger, on the screens that
    /// draw one at a known place (vanilla's layouts).
    fn text_field_rect(&self, context: &str) -> Option<[f32; 4]> {
        let batch = HudBatch::new(self.touch_size());
        let w = batch.width;
        match context {
            // SignEditScreen and HangingSignEditScreen's board, around (w / 2, 90..125).
            "sign" => Some([w / 2.0 - 50.0, 60.0, 100.0, 110.0]),
            // BookEditScreen's page text, inside the 192x192 book at (w / 2 - 96, 2).
            "book" => Some([w / 2.0 - 66.0, 26.0, 132.0, 140.0]),
            // CommandBlockEditScreen's command box.
            "command" => Some([w / 2.0 - 150.0, 50.0, 300.0, 20.0]),
            // AnvilScreen's name box (62, 24), 103 wide.
            "anvil" => {
                let (ox, oy) = self.inventory_origin(&batch);
                Some([ox + 60.0, oy + 21.0, 107.0, 16.0])
            }
            // CreativeModeInventoryScreen's search box (82, 6), 80 wide.
            "creative" => {
                let (ox, oy) = Self::creative_origin(&batch);
                Some([ox + 80.0, oy + 3.0, 86.0, 14.0])
            }
            _ => None,
        }
    }

    /// Whether a finger landing at window pixel `p` is on a scrollbar's track, which it should
    /// drag like a held mouse instead of scrolling the list.
    fn on_scrollbar(&self, p: [f32; 2]) -> bool {
        let batch = HudBatch::new(self.touch_size());
        let (x, y) = (p[0] / batch.scale, p[1] / batch.scale);
        // A mod's screen: where it said a finger drags (`mobile.drag-areas`).
        if self.mods.screen.is_some() {
            return self
                .mods
                .drag_areas
                .iter()
                .any(|a| x >= a[0] && x < a[0] + a[2] && y >= a[1] && y < a[1] + a[3]);
        }
        if self.screen == Screen::Creative {
            let (ox, oy) = Self::creative_origin(&batch);
            return self.creative_screen.inside_scrollbar(x - ox, y - oy);
        }
        false
    }

    fn touch_apply(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Stick(stick) => self.pad.touch_stick(stick),
                // Held as the left stick's click (key.sprint) while the stick sprints.
                Action::Sprint(on) => self.pad.touch_hold(Pad::LeftStick, on),
                // Relative motion as a grabbed mouse makes it (MouseHandler.onMove).
                Action::Look(dx, dy) => self.mouse.add_motion(dx as f64, dy as f64),
                Action::Press(Button::Jump) => self.pad.touch_hold(Pad::South, true),
                Action::Release(Button::Jump) => self.pad.touch_hold(Pad::South, false),
                Action::Press(Button::Attack) | Action::AttackStart => {
                    self.pad.touch_trigger(1, 1.0)
                }
                Action::Release(Button::Attack) | Action::AttackStop => {
                    self.pad.touch_trigger(1, 0.0)
                }
                Action::Press(Button::Use) => {
                    self.touch.use_until = None;
                    self.pad.touch_trigger(0, 1.0);
                }
                Action::Release(Button::Use) => self.pad.touch_trigger(0, 0.0),
                // The pad's buttons for the rest (docs/controllers.md); the right stick's click
                // toggles sneaking.
                Action::Press(Button::Sneak) => self.pad.touch_tap(Pad::RightStick),
                Action::Press(Button::Inventory) => self.pad.touch_tap(Pad::North),
                Action::Press(Button::Chat) => self.pad.touch_tap(Pad::DPadRight),
                Action::Press(Button::Pause) => self.pad.touch_tap(Pad::Start),
                Action::Press(Button::Drop) => self.pad.touch_tap(Pad::East),
                Action::Press(Button::Offhand) => self.pad.touch_tap(Pad::West),
                Action::Press(Button::Camera) => self.pad.touch_tap(Pad::DPadUp),
                Action::Press(Button::Command) => {
                    // key.command: the chat opened with a slash.
                    let _ = self.touch_key_tap(KeyCode::Slash);
                }
                Action::Release(_) => {}
                Action::Use => {
                    self.pad.touch_trigger(0, 1.0);
                    self.touch.use_until = Some(Instant::now() + USE_PULSE);
                }
                Action::Slot(slot) => self.selected = slot,
                Action::ToggleStats => self.toggle_stats_overlay(),
                Action::EditLayout => self.open_touch_editor(),
            }
        }
    }

    pub(super) fn open_touch_editor(&mut self) {
        let actions = self.touch.controls.reset();
        self.touch_apply(actions);
        self.touch.editor = Some(LayoutEditor::default());
    }

    /// The controls over the HUD while playing (or the layout editor), the close button over
    /// screens.
    pub(super) fn draw_touch(&self, batch: &mut HudBatch) {
        if !(self.touch.seen || cfg!(target_os = "android")) {
            return;
        }
        let (playing, closable) = (self.touch_playing(), self.touch_closable());
        // The editor also over the title, before a world.
        if !playing && !closable && self.touch.editor.is_none() {
            return;
        }
        let layout = self.touch_layout();
        let mut ui = Ui::new(&self.gui.atlas, batch, (-1.0, -1.0));
        // A mod's button: its plate, its icon or its label.
        let face_of = |key: &str| self.touch.faces.iter().find(|f| f.key == key);
        if closable {
            let (c, r) = layout.close;
            circle(&mut ui, c, r, color::with_alpha(color::BG, 0.7));
            label(&mut ui, c, r, "X", ink(1.0));
        } else if let (Some(editor), Some(config)) = (&self.touch.editor, &self.touch.config) {
            let face = |target: Target| match target {
                Target::Button(b) => (self.control_face(b).label, None),
                Target::Extra(i) => {
                    let key = &config.extra[i].key;
                    let face = face_of(key);
                    (
                        face.map_or_else(|| key.clone(), |f| f.label.clone()),
                        face.and_then(|f| f.icon.clone()),
                    )
                }
                Target::Stick => (String::new(), None),
            };
            let shape = |b: Button| self.control_face(b).shape;
            draw_editor(&mut ui, &layout, config, editor, &face, &shape);
        } else {
            let (base, knob) = self
                .touch
                .controls
                .stick
                .unwrap_or((layout.stick_home, layout.stick_home));
            // A ghost stick shows only while a finger holds it.
            let a = if self.touch.controls.stick.is_some() {
                layout.stick_alpha.max(0.6)
            } else {
                layout.stick_alpha
            };
            if layout.dpad {
                // Bedrock's D-pad: four arrows, the one held lit; the middle holds nothing yet.
                let a = layout.stick_alpha.max(0.35);
                let held = self.touch.controls.dpad;
                for (cell, c, half) in layout.dpad_buttons() {
                    let on = held == Some(cell);
                    plate_of(
                        &mut ui,
                        Shape::Square,
                        c,
                        half,
                        if on { pressed(a) } else { plate(a) },
                    );
                    let text = match cell {
                        [0, -1] => "↑",
                        [-1, 0] => "←",
                        [1, 0] => "→",
                        _ => "↓",
                    };
                    label(&mut ui, c, half, text, ink(a));
                }
            } else if a > 0.0 {
                circle(&mut ui, base, layout.stick_radius, plate(a));
                circle(&mut ui, knob, layout.stick_radius * 0.45, pressed(a));
            }
            let sneaking = self.player.as_ref().is_some_and(|p| p.shift_key_down);
            for (button, c, r, a) in &layout.buttons {
                if *a <= 0.0 {
                    continue;
                }
                let on =
                    self.touch.controls.held(*button) || (*button == Button::Sneak && sneaking);
                let face = self.control_face(*button);
                // A pack's button is drawn whole; a mod's look is a plate with its icon or label.
                if let Some((skin, lit)) = &face.skin {
                    let sprite = lit.as_ref().filter(|_| on).unwrap_or(skin);
                    icon(&mut ui, sprite, *c, *r, 1.0, *a);
                    continue;
                }
                let fill = if on { pressed(*a) } else { plate(*a) };
                plate_of(&mut ui, face.shape, *c, *r, fill);
                match &face.icon {
                    Some(sprite) => icon(&mut ui, sprite, *c, *r, 0.55, *a),
                    None => label(&mut ui, *c, *r, &face.label, ink(*a)),
                }
            }
            // The mods' buttons, lit while held.
            for (i, c, r, a, shape) in &layout.extra {
                let extra = self.touch.config.as_ref().and_then(|c| c.extra.get(*i));
                let (Some(extra), true) = (extra.map(|e| e.key.clone()), *a > 0.0) else {
                    continue;
                };
                let Some(face) = face_of(&extra) else {
                    continue;
                };
                let on = self.mods.keys_down.contains(&extra);
                let fill = if on { pressed(*a) } else { plate(*a) };
                plate_of(&mut ui, *shape, *c, *r, fill);
                match (&face.icon, shape) {
                    (Some(sprite), Shape::Round | Shape::Square) => {
                        icon(&mut ui, sprite, *c, *r, 0.6, *a)
                    }
                    _ => label(&mut ui, *c, text_r(*shape, *r), &face.label, ink(*a)),
                }
            }
        }
        ui.finish();
    }

    /// Let go of everything the fingers held (the app went to the background).
    pub(super) fn touch_release_all(&mut self) {
        let actions = self.touch.controls.reset();
        self.touch_apply(actions);
        // No window events here: the screen gets them back when the app returns.
        let _ = self.touch.pointer.cancel();
        // The system put the keyboard away: ask for it again when the chat is back.
        self.touch.keyboard = false;
    }

    /// The mod's button under window pixel `p` while playing (not in the layout editor): mod id,
    /// button id.
    fn mod_button_at(&self, p: [f32; 2]) -> Option<(String, String)> {
        if !self.touch_playing() || self.touch.editor.is_some() {
            return None;
        }
        let config = self.touch.config.as_ref()?;
        let i = self.touch_layout().extra_at(p)?;
        let (id, button) = config.extra.get(i)?.key.split_once(':')?;
        Some((id.to_owned(), button.to_owned()))
    }
    /// Whether the chat bar shows: the chat open on a touch screen.
    fn chat_bar_shown(&self) -> bool {
        self.screen == Screen::Chat && (self.touch.seen || cfg!(target_os = "android"))
    }
    /// The chat bar's buttons in GUI pixels (x, y, w, h): at the right, just above the text
    /// field (lifted with it over the keyboard).
    fn chat_bar(batch: &HudBatch) -> [[f32; 4]; 6] {
        let (w, h, gap) = (24.0, 16.0, 2.0);
        let y = batch.height - 14.0 - h - 2.0;
        let left = batch.width - 2.0 - 6.0 * w - 5.0 * gap;
        std::array::from_fn(|i| [left + i as f32 * (w + gap), y, w, h])
    }
    /// The chat bar's button under window pixel `p`.
    fn chat_bar_at(&self, p: [f32; 2]) -> Option<usize> {
        if !self.chat_bar_shown() {
            return None;
        }
        let batch = HudBatch::new(self.touch_size());
        let lift = self.keyboard_height() as f32 / batch.scale;
        let (x, y) = (p[0] / batch.scale, p[1] / batch.scale + lift);
        Self::chat_bar(&batch)
            .iter()
            .position(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
    }
    /// The chat bar: vanilla buttons for ↑ ↓ Tab ← → and Paste, the one just tapped lit.
    pub(super) fn draw_chat_bar(&self, batch: &mut HudBatch) {
        if !self.chat_bar_shown() {
            return;
        }
        let atlas = &self.gui.atlas;
        let lit = self
            .touch
            .chat_key
            .filter(|(_, at)| at.elapsed() < Duration::from_millis(150))
            .map(|(i, _)| i);
        let ink = horizon_client::hud::rgba(255, 255, 255, 255);
        for (i, r) in Self::chat_bar(batch).iter().enumerate() {
            let sprite = if lit == Some(i) {
                "gui/sprites/widget/button_highlighted"
            } else {
                "gui/sprites/widget/button"
            };
            horizon_client::ui::nine_slice(batch, atlas, sprite, r[0], r[1], r[2], r[3], u32::MAX);
            let c = [(r[0] + r[2] / 2.0).floor(), (r[1] + r[3] / 2.0).floor()];
            match i {
                2 => {
                    let tw = HudBatch::text_width(atlas, "Tab");
                    batch.text(
                        atlas,
                        "Tab",
                        (c[0] - tw / 2.0).floor() + 1.0,
                        c[1] - 4.0,
                        ink,
                        true,
                    );
                }
                0 => arrow(batch, atlas, c, 0, ink),
                1 => arrow(batch, atlas, c, 1, ink),
                3 => arrow(batch, atlas, c, 2, ink),
                4 => arrow(batch, atlas, c, 3, ink),
                _ => clipboard(batch, atlas, c, ink),
            }
        }
    }
    /// How far the on-screen keyboard covers the window, in pixels (Android).
    fn keyboard_height(&self) -> u32 {
        #[cfg(target_os = "android")]
        if self.touch.keyboard {
            return crate::ime_height();
        }
        0
    }

    /// With the chat open over the on-screen keyboard, the HUD moves up above it.
    pub(super) fn lift_over_keyboard(&self, batch: &mut HudBatch) {
        let height = self.keyboard_height();
        if self.screen != Screen::Chat || height == 0 {
            return;
        }
        let lift = height as f32 / batch.scale;
        for vertex in &mut batch.vertices {
            vertex.pos[1] -= lift;
        }
        for (_, blend) in &mut batch.blends {
            if let horizon_client::hud::Blend::Depth(rect) = blend {
                rect[1] = rect[1].saturating_sub(height);
            }
        }
    }

    /// `touch-layout`: the layout, or open, close (saving) or reset its editor.
    /// `touch down|move|up|tap X Y [FINGER]`: a finger at a GUI pixel, as the touch screen's.
    pub(super) fn touch_input_command(
        &mut self,
        words: &[&str],
        event_loop: &ActiveEventLoop,
    ) -> Result<Option<Value>> {
        if words.first() != Some(&"touch") {
            return Ok(None);
        }
        let number = |i: usize| -> Result<f64> {
            let word = words.get(i).ok_or("touch down|move|up|tap X Y [FINGER]")?;
            Ok(word
                .parse::<f64>()
                .map_err(|_| format!("{word} is not a number"))?)
        };
        let phases: &[TouchPhase] = match words.get(1).copied() {
            Some("down") => &[TouchPhase::Started],
            Some("move") => &[TouchPhase::Moved],
            Some("up") => &[TouchPhase::Ended],
            Some("tap") => &[TouchPhase::Started, TouchPhase::Ended],
            _ => return Err("touch down|move|up|tap X Y [FINGER]".into()),
        };
        let scale = HudBatch::new(self.touch_size()).scale;
        let location = winit::dpi::PhysicalPosition::new(
            number(2)? * f64::from(scale),
            number(3)? * f64::from(scale),
        );
        let finger = if words.len() > 4 {
            number(4)? as u64
        } else {
            0
        };
        let id = self
            .window
            .as_ref()
            .map(|w| w.id())
            .ok_or("no window yet")?;
        for &phase in phases {
            let touch = winit::event::Touch {
                device_id: winit::event::DeviceId::dummy(),
                phase,
                location,
                force: None,
                id: finger,
            };
            self.window_event(event_loop, id, WindowEvent::Touch(touch));
        }
        Ok(Some(json!({ "ok": true, "screen": self.screen_label() })))
    }

    pub(super) fn touch_command(&mut self, words: &[&str]) -> Result<Option<Value>> {
        if words.first() != Some(&"touch-layout") {
            return Ok(None);
        }
        match words.get(1).copied() {
            None => {}
            Some("edit") => self.open_touch_editor(),
            Some("done") => {
                if self.touch.editor.take().is_some() {
                    self.save_touch_layout();
                }
            }
            Some("reset") => {
                self.touch.config = Some(LayoutConfig::default());
                self.save_touch_layout();
            }
            Some(other) => {
                return Err(format!("touch-layout takes edit, done or reset, not {other}").into());
            }
        }
        let config = self.touch.config.clone().unwrap_or_default();
        let layout: Value = serde_json::from_str(&config.to_json()).unwrap_or(Value::Null);
        Ok(Some(json!({
            "ok": true,
            "editing": self.touch.editor.is_some(),
            "selected": self.touch.editor.as_ref().and_then(|e| e.selected).map(|t| match t {
                Target::Stick => "stick".to_owned(),
                Target::Button(b) => b.id().to_owned(),
                Target::Extra(i) => config.extra.get(i).map_or_else(String::new, |e| e.key.clone()),
            }),
            "layout": layout,
        })))
    }

    /// A key press and release from the touch side (the back key, the close button).
    fn touch_key_tap(&mut self, key: KeyCode) -> Result<()> {
        self.control_key_down(key)?;
        self.keys.remove(&key);
        self.physical_keys.remove(&key);
        Ok(())
    }
}
