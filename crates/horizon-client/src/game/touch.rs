//! Touch input (horizon_client::touch): on the playing screen the touch controls, drawn over the
//! HUD and turned into the virtual pad, mouse motion and the hotbar; on every other screen the
//! first finger is the mouse, with a close button over the screens a world opens. Android's back
//! key is Escape, and the chat raises the on-screen keyboard.
use super::{Game, Screen};
use horizon_client::{
    gamepad::Button as Pad,
    hud::HudBatch,
    touch::{self, Action, Button, Layout, TouchControls},
    ui_theme::{Face, Ui, color},
};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, KeyCode, NamedKey},
    window::WindowId,
};

/// How long a tap holds the use trigger: vanilla samples input once a tick.
const USE_PULSE: Duration = Duration::from_millis(60);

#[derive(Default)]
pub(super) struct Touch {
    controls: TouchControls,
    /// The finger acting as the mouse off the playing screen.
    mouse: Option<u64>,
    /// A touch was seen: draw the controls (always on Android).
    seen: bool,
    /// The controls were live last frame (the playing screen).
    live: bool,
    use_until: Option<Instant>,
    /// The on-screen keyboard was asked for.
    keyboard: bool,
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
        self.touch.live
    }

    fn touch_layout(&self) -> Layout {
        let size = self.window.as_ref().map(|w| w.inner_size());
        let (w, h) = size.map_or((1, 1), |s| (s.width.max(1), s.height.max(1)));
        // The scale the HUD is drawn at, which the GUI Scale option only caps.
        let scale = HudBatch::new([w, h]).scale;
        Layout::new(w as f32, h as f32, scale)
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
        let p = [t.location.x as f32, t.location.y as f32];
        let layout = self.touch_layout();
        if t.phase == TouchPhase::Started
            && self.touch.mouse.is_none()
            && self.touch_closable()
            && layout.close_at(p)
        {
            let _ = self.touch_key_tap(KeyCode::Escape);
            return true;
        }
        // The mouse finger stays the mouse until it lifts, even if the screen closed under it.
        if !self.touch_playing() || self.touch.mouse == Some(t.id) {
            for mouse in touch::as_mouse(t, &mut self.touch.mouse) {
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

    /// Android's back key: Escape (pause, close a screen).
    pub(super) fn touch_back_key(&mut self, event: &KeyEvent) -> bool {
        if event.logical_key != Key::Named(NamedKey::BrowserBack) {
            return false;
        }
        if event.state == ElementState::Pressed && !event.repeat {
            let _ = self.touch_key_tap(KeyCode::Escape);
        }
        true
    }

    /// Once per loop iteration, before the pads are read.
    pub(super) fn touch_frame(&mut self) {
        let now = Instant::now();
        let live = self.touch_playing();
        if self.touch.live && !live {
            // A screen opened over the world: let go of everything the fingers held.
            let actions = self.touch.controls.reset();
            self.touch_apply(actions);
        }
        self.touch.live = live;
        let actions = self.touch.controls.frame(now);
        self.touch_apply(actions);
        if self.touch.use_until.is_some_and(|t| now >= t) {
            self.touch.use_until = None;
            self.pad.touch_trigger(0, 0.0);
        }
        // The on-screen keyboard while the chat is open (its keys arrive as key events).
        let keyboard =
            (self.touch.seen || cfg!(target_os = "android")) && self.screen == Screen::Chat;
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
                // The right stick's click toggles sneaking.
                Action::Press(Button::Sneak) => self.pad.touch_tap(Pad::RightStick),
                Action::Press(Button::Inventory) => self.pad.touch_tap(Pad::North),
                Action::Press(Button::Chat) => self.pad.touch_tap(Pad::DPadRight),
                Action::Press(Button::Pause) => self.pad.touch_tap(Pad::Start),
                Action::Release(_) => {}
                Action::Use => {
                    self.pad.touch_trigger(0, 1.0);
                    self.touch.use_until = Some(Instant::now() + USE_PULSE);
                }
                Action::AttackStart => self.pad.touch_trigger(1, 1.0),
                Action::AttackStop => self.pad.touch_trigger(1, 0.0),
                Action::Slot(slot) => self.selected = slot,
            }
        }
    }

    /// The controls over the HUD while playing, the close button over screens.
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
        let plate = color::with_alpha(color::WHITE, 0.16);
        let pressed = color::with_alpha(color::WHITE, 0.38);
        let ink = color::with_alpha(color::WHITE, 0.85);
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
        let label = |ui: &mut Ui, c: [f32; 2], r: f32, text: &str| {
            let size = (r / d * 0.5).clamp(9.0, 16.0);
            ui.text_centered(
                Face::Semibold,
                size,
                c[0] / d,
                c[1] / d - size * 0.62,
                ink,
                text,
            );
        };
        if closable {
            let (c, r) = layout.close;
            circle(&mut ui, c, r, color::with_alpha(color::BG, 0.7));
            label(&mut ui, c, r, "X");
        } else {
            let (base, knob) = self
                .touch
                .controls
                .stick
                .unwrap_or((layout.stick_home, layout.stick_home));
            circle(&mut ui, base, layout.stick_radius, plate);
            circle(&mut ui, knob, layout.stick_radius * 0.45, pressed);
            let sneaking = self.player.as_ref().is_some_and(|p| p.shift_key_down);
            for (button, c, r) in &layout.buttons {
                let on =
                    self.touch.controls.held(*button) || (*button == Button::Sneak && sneaking);
                circle(&mut ui, *c, *r, if on { pressed } else { plate });
                label(&mut ui, *c, *r, button.label());
            }
        }
        ui.finish();
    }

    /// Let go of everything the fingers held (the app went to the background).
    pub(super) fn touch_release_all(&mut self) {
        let actions = self.touch.controls.reset();
        self.touch_apply(actions);
        self.touch.mouse = None;
    }

    /// A key press and release from the touch side (the back key, the close button).
    fn touch_key_tap(&mut self, key: KeyCode) -> horizon_client::Result<()> {
        self.control_key_down(key)?;
        self.keys.remove(&key);
        self.physical_keys.remove(&key);
        Ok(())
    }
}
