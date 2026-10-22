//! Touch input (horizon_client::touch): on the playing screen the touch controls, drawn over the
//! HUD and turned into the virtual pad, mouse motion and the hotbar, laid out as the player's
//! touch-layout.json says (edited in place: a long press on pause); on every other screen the
//! first finger is the pointer, with a close button over the screens a world opens. Android's
//! back key is Escape, and the chat raises the on-screen keyboard and moves above it.
use super::{Game, Screen};
use horizon_client::{
    Result,
    gamepad::Button as Pad,
    hud::HudBatch,
    touch::{
        self, Action, Button, Chip, EditorAction, Layout, LayoutConfig, LayoutEditor, Show, Target,
        TouchControls,
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
const LAYOUT_FILE: &str = "touch-layout.json";

#[derive(Default)]
pub(super) struct Touch {
    controls: TouchControls,
    /// The finger acting as the mouse off the playing screen.
    pointer: touch::TouchPointer,
    /// A touch was seen: draw the controls (always on Android).
    seen: bool,
    /// The controls were live last frame (the playing screen).
    live: bool,
    use_until: Option<Instant>,
    /// The on-screen keyboard was asked for.
    keyboard: bool,
    /// The player's layout, read from touch-layout.json on the first frame.
    config: Option<LayoutConfig>,
    /// The layout editor, while it is open.
    editor: Option<LayoutEditor>,
}

impl Game<'_> {
    fn touch_playing(&self) -> bool {
        self.screen == Screen::Playing && self.title.is_none() && self.player.is_some()
    }

    /// A screen opened in a world that the back key or the close button leaves.
    fn touch_closable(&self) -> bool {
        self.title.is_none() && !matches!(self.screen, Screen::Playing | Screen::Dead)
    }

    /// The touch controls turn the view as a grabbed pointer does (mouse.rs).
    pub(super) fn touch_turns(&self) -> bool {
        self.touch.live && self.touch.editor.is_none()
    }

    fn touch_size(&self) -> [u32; 2] {
        self.window.as_ref().map_or([1, 1], |w| {
            let s = w.inner_size();
            [s.width.max(1), s.height.max(1)]
        })
    }

    fn touch_layout(&self) -> Layout {
        let [w, h] = self.touch_size();
        // The scale the HUD is drawn at, which the GUI Scale option only caps.
        let scale = HudBatch::new([w, h]).scale;
        match &self.touch.config {
            Some(config) => Layout::from_config(config, w as f32, h as f32, scale),
            None => Layout::new(w as f32, h as f32, scale),
        }
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
        self.touch.seen = true;
        self.last_input = Instant::now();
        // A touch in the chat brings back a keyboard the back key put away.
        if t.phase == TouchPhase::Started
            && self.screen == Screen::Chat
            && let Some(window) = &self.window
        {
            window.set_ime_allowed(true);
        }
        let p = [t.location.x as f32, t.location.y as f32];
        if self.touch.editor.is_some() && self.touch_playing() {
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
        // The mouse finger stays the mouse until it lifts, even if the screen closed under it.
        if !self.touch_playing() || self.touch.pointer.owns(t.id) {
            let slop = touch::slop(layout.width, layout.height);
            for mouse in self.touch.pointer.event(t, slop, Instant::now()) {
                self.window_event(event_loop, id, mouse);
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
        if let Some(id) = self.window.as_ref().map(|w| w.id()) {
            for mouse in self.touch.pointer.frame_events(now) {
                self.window_event(event_loop, id, mouse);
            }
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
        // The on-screen keyboard while the chat is open (its keys arrive as key events). Android
        // shows it only to a focused window: back from the background, it is asked for again once
        // the focus has come back.
        let keyboard = (self.touch.seen || cfg!(target_os = "android"))
            && self.screen == Screen::Chat
            && self.focused;
        if keyboard != self.touch.keyboard
            && let Some(window) = &self.window
        {
            window.set_ime_allowed(keyboard);
            self.touch.keyboard = keyboard;
        }
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
        if !playing && !closable {
            return;
        }
        let layout = self.touch_layout();
        let mut ui = Ui::new(&self.gui.atlas, batch, (-1.0, -1.0));
        let d = ui.density;
        // A plate, a pressed plate and a label, at a control's opacity.
        let plate = |a: f32| color::with_alpha(color::WHITE, 0.16 * a);
        let pressed = |a: f32| color::with_alpha(color::WHITE, 0.38 * a);
        let ink = |a: f32| color::with_alpha(color::WHITE, 0.85 * a);
        let circle = |ui: &mut Ui, c: [f32; 2], r: f32, fill: u32| {
            ui.rounded(
                (c[0] - r) / d,
                (c[1] - r) / d,
                2.0 * r / d,
                2.0 * r / d,
                r / d,
                fill,
            );
        };
        let ring = |ui: &mut Ui, c: [f32; 2], r: f32, line: u32| {
            ui.border(
                (c[0] - r) / d,
                (c[1] - r) / d,
                2.0 * r / d,
                2.0 * r / d,
                r / d,
                line,
            );
        };
        let label = |ui: &mut Ui, c: [f32; 2], r: f32, text: &str, tint: u32| {
            let size = (r / d * 0.5).clamp(9.0, 16.0);
            ui.text_centered(
                Face::Semibold,
                size,
                c[0] / d,
                c[1] / d - size * 0.62,
                tint,
                text,
            );
        };
        if closable {
            let (c, r) = layout.close;
            circle(&mut ui, c, r, color::with_alpha(color::BG, 0.7));
            label(&mut ui, c, r, "X", ink(1.0));
        } else if let (Some(editor), Some(config)) = (&self.touch.editor, &self.touch.config) {
            let (w, h) = (layout.width, layout.height);
            ui.fill(0.0, 0.0, w / d, h / d, color::with_alpha(color::BG, 0.45));
            for (target, place, c, r) in LayoutEditor::targets(config, w, h) {
                // As it will look (at least faintly, to be found again), outlined if a ghost.
                let a = (place.opacity * config.opacity).max(0.35);
                match place.show {
                    Show::On => circle(&mut ui, c, r, pressed(a)),
                    Show::Ghost => ring(&mut ui, c, r, ink(0.5)),
                    Show::Off => circle(&mut ui, c, r, plate(0.5)),
                }
                if editor.selected == Some(target) {
                    ring(&mut ui, c, r, color::ACCENT);
                }
                let text = match target {
                    Target::Stick => "Move",
                    Target::Button(b) => b.label(),
                };
                let tint = if place.show == Show::Off {
                    ink(0.4)
                } else {
                    ink(1.0)
                };
                label(&mut ui, c, r, text, tint);
            }
            let shown = editor.selected.and_then(|t| {
                LayoutEditor::targets(config, w, h)
                    .into_iter()
                    .find(|(x, ..)| *x == t)
                    .map(|(_, p, ..)| p.show)
            });
            for (chip, c, r) in editor.chips(config, w, h) {
                circle(&mut ui, c, r, color::with_alpha(color::BG, 0.85));
                let text = match chip {
                    Chip::Smaller => "-",
                    Chip::Bigger => "+",
                    Chip::Visibility => match shown {
                        Some(Show::Ghost) => "Ghost",
                        Some(Show::Off) => "Off",
                        _ => "On",
                    },
                };
                label(&mut ui, c, r, text, ink(1.0));
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
            circle(&mut ui, [x0 + (x1 - x0) * value, y], 0.28 * u, ink(1.0));
            let whose = match editor.selected {
                Some(Target::Stick) => "Move",
                Some(Target::Button(b)) => b.label(),
                None => "all",
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
                    EditorAction::Reset => color::with_alpha(color::BG, 0.85),
                };
                circle(&mut ui, c, r, fill);
                let text = match action {
                    EditorAction::Done => "Done",
                    EditorAction::Reset => "Reset",
                };
                label(&mut ui, c, r, text, ink(1.0));
            }
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
            if a > 0.0 {
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
                circle(&mut ui, *c, *r, if on { pressed(*a) } else { plate(*a) });
                label(&mut ui, *c, *r, button.label(), ink(*a));
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
                Target::Stick => "stick",
                Target::Button(b) => b.id(),
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
