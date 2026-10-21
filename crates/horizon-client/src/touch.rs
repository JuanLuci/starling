//! Touch controls for phones (horizon-android): a floating stick that walks, dragging anywhere
//! else that turns the view (a tap there uses the held item, a still press breaks), buttons for
//! jump, sneak, inventory, chat and pause, and the hotbar's slots to tap. Each finger is
//! claimed by one control when it lands and keeps it until it lifts.
//!
//! This is the state machine only: positions are window pixels, and the game turns the
//! [`Action`]s into its virtual pad, mouse motion and hotbar (game/touch.rs). Off the playing
//! screen touches are the mouse instead ([`as_mouse`]).
use std::time::{Duration, Instant};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, Touch, TouchPhase, WindowEvent},
};

/// A press that stays put this long breaks the block in view (Bedrock's touch default).
pub const HOLD: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Jump,
    Sneak,
    Inventory,
    Chat,
    Pause,
}

impl Button {
    pub const ALL: [Button; 5] = [
        Button::Jump,
        Button::Sneak,
        Button::Inventory,
        Button::Chat,
        Button::Pause,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Button::Jump => "Jump",
            Button::Sneak => "Sneak",
            Button::Inventory => "Bag",
            Button::Chat => "Chat",
            Button::Pause => "II",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// The walking stick, -1..1, x right and y up.
    Stick([f32; 2]),
    /// Sprint on or off: the stick pushed forward past its rim starts it, and it lasts while
    /// the stick still points forward.
    Sprint(bool),
    /// Turn the view by these mouse counts.
    Look(f32, f32),
    Press(Button),
    Release(Button),
    /// A quick tap on the view: use the held item once.
    Use,
    AttackStart,
    AttackStop,
    /// Select hotbar slot 0..8.
    Slot(usize),
    /// Three fingers landing together: show or hide the Stats overlay.
    ToggleStats,
}

/// Fingers landing within this of each other count as one three-finger tap.
const CHORD: Duration = Duration::from_millis(350);

/// Where the controls are on a window, in its pixels.
#[derive(Clone, Debug)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    /// The size unit: a ninth of the shorter side.
    pub unit: f32,
    /// The stick takes fingers landing left of this and below `stick_top`.
    pub stick_right: f32,
    pub stick_top: f32,
    /// Where the stick rests before a finger moves it, and its reach.
    pub stick_home: [f32; 2],
    pub stick_radius: f32,
    pub buttons: Vec<(Button, [f32; 2], f32)>,
    /// The hotbar: its left edge, top, slot width (vanilla's 182x22 at the GUI scale).
    pub hotbar: [f32; 3],
    /// The close button over screens opened in a world (they have none for touch).
    pub close: ([f32; 2], f32),
}

impl Layout {
    /// `gui_scale` is the scale the HUD is drawn at (`HudBatch::scale`), not the option.
    pub fn new(width: f32, height: f32, gui_scale: f32) -> Self {
        let u = width.min(height) / 9.0;
        let (w, h) = (width, height);
        let s = gui_scale.max(1.0);
        // Gui.renderHotbar: guiWidth / 2 - 91, guiHeight - 22, in whole GUI pixels.
        let (gui_w, gui_h) = ((w / s).ceil(), (h / s).ceil());
        let hotbar_left = ((gui_w as i32) / 2 - 91) as f32 * s;
        Layout {
            width,
            height,
            unit: u,
            stick_right: w * 0.4,
            stick_top: h * 0.3,
            stick_home: [2.4 * u, h - 2.4 * u],
            stick_radius: 1.3 * u,
            buttons: vec![
                (Button::Jump, [w - 1.9 * u, h - 2.2 * u], 0.8 * u),
                (Button::Sneak, [w - 3.9 * u, h - 1.3 * u], 0.6 * u),
                (Button::Inventory, [w - 1.0 * u, 1.0 * u], 0.55 * u),
                (Button::Pause, [w / 2.0, 0.75 * u], 0.45 * u),
                (Button::Chat, [w - 2.4 * u, 1.0 * u], 0.55 * u),
            ],
            hotbar: [hotbar_left, (gui_h - 22.0) * s, 20.0 * s],
            close: ([w - 0.8 * u, 0.8 * u], 0.5 * u),
        }
    }
    /// Whether `p` presses the close button.
    pub fn close_at(&self, p: [f32; 2]) -> bool {
        dist(p, self.close.0) <= self.close.1 * 1.2
    }
    fn button_at(&self, p: [f32; 2]) -> Option<Button> {
        self.buttons
            .iter()
            .find(|(_, c, r)| dist(p, *c) <= r * 1.15)
            .map(|(b, ..)| *b)
    }
    fn slot_at(&self, p: [f32; 2]) -> Option<usize> {
        let [left, top, slot] = self.hotbar;
        let i = ((p[0] - left - slot * 0.05) / slot).floor();
        (p[1] >= top && (0.0..9.0).contains(&i)).then_some(i as usize)
    }
    /// Mouse counts per pixel dragged: the window's height turns the view about 180° at the
    /// default sensitivity (0.15° a count).
    fn look_scale(&self) -> f32 {
        1200.0 / self.height
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[derive(Clone, Copy, Debug)]
enum Role {
    Stick {
        origin: [f32; 2],
        sprinting: bool,
    },
    Look {
        start: [f32; 2],
        last: [f32; 2],
        down: Instant,
        dragged: bool,
        attacking: bool,
    },
    Button(Button),
    /// A finger on the hotbar, or one that landed on nothing that takes it.
    Spent,
}

#[derive(Default)]
pub struct TouchControls {
    fingers: Vec<(u64, Role)>,
    /// When the fingers down now landed, the latest last (for the three-finger tap).
    landed: Vec<Instant>,
    /// Where the stick is drawn and how far it is pushed.
    pub stick: Option<([f32; 2], [f32; 2])>,
}

impl TouchControls {
    pub fn event(
        &mut self,
        layout: &Layout,
        id: u64,
        phase: TouchPhase,
        p: [f32; 2],
        now: Instant,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        match phase {
            TouchPhase::Started => {
                let role = if let Some(button) = layout.button_at(p) {
                    out.push(Action::Press(button));
                    Role::Button(button)
                } else if let Some(slot) = layout.slot_at(p) {
                    out.push(Action::Slot(slot));
                    Role::Spent
                } else if p[0] < layout.stick_right
                    && p[1] > layout.stick_top
                    && !self
                        .fingers
                        .iter()
                        .any(|(_, r)| matches!(r, Role::Stick { .. }))
                {
                    self.stick = Some((p, p));
                    Role::Stick {
                        origin: p,
                        sprinting: false,
                    }
                } else {
                    Role::Look {
                        start: p,
                        last: p,
                        down: now,
                        dragged: false,
                        attacking: false,
                    }
                };
                self.fingers.retain(|(f, _)| *f != id);
                self.fingers.push((id, role));
                self.landed.retain(|t| now.duration_since(*t) < CHORD);
                self.landed.push(now);
                if self.landed.len() >= 3 && self.fingers.len() >= 3 {
                    // The three fingers stop whatever they began.
                    self.landed.clear();
                    let start = self.fingers.len() - 3;
                    for i in start..self.fingers.len() {
                        let role = std::mem::replace(&mut self.fingers[i].1, Role::Spent);
                        out.retain(|a| !matches!(a, Action::Press(_) | Action::Slot(_)));
                        out.extend(self.lift(role, false, now));
                    }
                    out.retain(|a| !matches!(a, Action::Use));
                    out.push(Action::ToggleStats);
                }
            }
            TouchPhase::Moved => {
                let Some((_, role)) = self.fingers.iter_mut().find(|(f, _)| *f == id) else {
                    return out;
                };
                match role {
                    Role::Stick { origin, sprinting } => {
                        let (dx, dy) = (p[0] - origin[0], p[1] - origin[1]);
                        let r = layout.stick_radius;
                        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                        let k = len.min(r) / len;
                        let knob = [origin[0] + dx * k, origin[1] + dy * k];
                        self.stick = Some((*origin, knob));
                        let stick = [dx * k / r, -dy * k / r];
                        out.push(Action::Stick(stick));
                        let forward = stick[1] > stick[0].abs();
                        let sprint = if *sprinting {
                            forward && stick[1] > 0.5
                        } else {
                            forward && len > r * 1.15
                        };
                        if sprint != *sprinting {
                            *sprinting = sprint;
                            out.push(Action::Sprint(sprint));
                        }
                    }
                    Role::Look {
                        start,
                        last,
                        dragged,
                        ..
                    } => {
                        if !*dragged && dist(p, *start) > layout.unit * 0.15 {
                            *dragged = true;
                        }
                        if *dragged {
                            let s = layout.look_scale();
                            out.push(Action::Look((p[0] - last[0]) * s, (p[1] - last[1]) * s));
                        }
                        *last = p;
                    }
                    Role::Button(_) | Role::Spent => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(i) = self.fingers.iter().position(|(f, _)| *f == id) else {
                    return out;
                };
                let (_, role) = self.fingers.remove(i);
                out.extend(self.lift(role, phase == TouchPhase::Ended, now));
            }
        }
        out
    }

    fn lift(&mut self, role: Role, ended: bool, now: Instant) -> Vec<Action> {
        match role {
            Role::Stick { sprinting, .. } => {
                self.stick = None;
                let mut out = vec![Action::Stick([0.0, 0.0])];
                if sprinting {
                    out.push(Action::Sprint(false));
                }
                out
            }
            Role::Look {
                down,
                dragged,
                attacking,
                ..
            } => {
                if attacking {
                    vec![Action::AttackStop]
                } else if ended && !dragged && now - down < HOLD {
                    vec![Action::Use]
                } else {
                    Vec::new()
                }
            }
            Role::Button(button) => vec![Action::Release(button)],
            Role::Spent => Vec::new(),
        }
    }

    /// Each frame: a view press held still past [`HOLD`] starts breaking.
    pub fn frame(&mut self, now: Instant) -> Vec<Action> {
        let mut out = Vec::new();
        for (_, role) in &mut self.fingers {
            if let Role::Look {
                down,
                dragged: false,
                attacking,
                ..
            } = role
                && !*attacking
                && now - *down >= HOLD
            {
                *attacking = true;
                out.push(Action::AttackStart);
            }
        }
        out
    }

    /// Lift every finger (a menu opened, the app went to the background).
    pub fn reset(&mut self) -> Vec<Action> {
        let now = Instant::now();
        std::mem::take(&mut self.fingers)
            .into_iter()
            .flat_map(|(_, role)| self.lift(role, false, now))
            .collect()
    }

    /// Whether a control holds `button` down.
    pub fn held(&self, button: Button) -> bool {
        self.fingers
            .iter()
            .any(|(_, r)| matches!(r, Role::Button(b) if *b == button))
    }
}

/// A touch off the playing screen as the mouse: the first finger moves the pointer and holds the
/// left button; other fingers are ignored.
pub fn as_mouse(touch: &Touch, primary: &mut Option<u64>) -> Vec<WindowEvent> {
    let (device_id, position) = (touch.device_id, touch.location);
    let moved = || WindowEvent::CursorMoved {
        device_id,
        position: PhysicalPosition::new(position.x, position.y),
    };
    let button = |state| WindowEvent::MouseInput {
        device_id,
        state,
        button: MouseButton::Left,
    };
    match touch.phase {
        TouchPhase::Started if primary.is_none() => {
            *primary = Some(touch.id);
            vec![moved(), button(ElementState::Pressed)]
        }
        TouchPhase::Moved if *primary == Some(touch.id) => vec![moved()],
        TouchPhase::Ended | TouchPhase::Cancelled if *primary == Some(touch.id) => {
            *primary = None;
            vec![moved(), button(ElementState::Released)]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout::new(3200.0, 1440.0, 4.0)
    }

    #[test]
    fn the_stick_walks_and_centres_when_lifted() {
        let (l, now) = (layout(), Instant::now());
        let mut t = TouchControls::default();
        assert!(
            t.event(&l, 1, TouchPhase::Started, [400.0, 1100.0], now)
                .is_empty()
        );
        let push = t.event(
            &l,
            1,
            TouchPhase::Moved,
            [400.0, 1100.0 - 10.0 * l.stick_radius],
            now,
        );
        // Pushed far past the rim: full forward, and running.
        assert!(
            matches!(push[..], [Action::Stick([x, y]), Action::Sprint(true)]
            if x == 0.0 && (y - 1.0).abs() < 1e-5)
        );
        assert_eq!(
            t.event(&l, 1, TouchPhase::Ended, [400.0, 900.0], now),
            vec![Action::Stick([0.0, 0.0]), Action::Sprint(false)]
        );
        assert!(t.stick.is_none());
    }

    #[test]
    fn pushing_past_the_rim_sprints_until_the_stick_turns_back() {
        let (l, now) = (layout(), Instant::now());
        let r = l.stick_radius;
        let mut t = TouchControls::default();
        t.event(&l, 1, TouchPhase::Started, [400.0, 1100.0], now);
        let walk = t.event(&l, 1, TouchPhase::Moved, [400.0, 1100.0 - r], now);
        assert!(!walk.contains(&Action::Sprint(true)));
        let run = t.event(&l, 1, TouchPhase::Moved, [400.0, 1100.0 - r * 1.3], now);
        assert!(run.contains(&Action::Sprint(true)));
        let still = t.event(&l, 1, TouchPhase::Moved, [400.0, 1100.0 - r * 0.8], now);
        assert!(!still.contains(&Action::Sprint(false)));
        let back = t.event(&l, 1, TouchPhase::Moved, [400.0, 1100.0 + r * 0.5], now);
        assert!(back.contains(&Action::Sprint(false)));
    }

    #[test]
    fn three_fingers_together_toggle_the_stats_and_take_nothing_else() {
        let (l, now) = (layout(), Instant::now());
        let mut t = TouchControls::default();
        t.event(&l, 1, TouchPhase::Started, [2000.0, 600.0], now);
        t.event(&l, 2, TouchPhase::Started, [2200.0, 600.0], now + CHORD / 4);
        let third = t.event(&l, 3, TouchPhase::Started, [2400.0, 600.0], now + CHORD / 2);
        assert_eq!(third, vec![Action::ToggleStats]);
        // Lifting them neither uses nor breaks.
        assert!(t.frame(now + HOLD * 2).is_empty());
        for id in 1..=3 {
            assert!(
                t.event(&l, id, TouchPhase::Ended, [0.0, 0.0], now + HOLD * 2)
                    .is_empty()
            );
        }
        // Fingers far apart in time are just fingers.
        t.event(&l, 4, TouchPhase::Started, [2000.0, 600.0], now);
        t.event(&l, 5, TouchPhase::Started, [2200.0, 600.0], now + CHORD * 2);
        let late = t.event(&l, 6, TouchPhase::Started, [2400.0, 600.0], now + CHORD * 4);
        assert!(late.is_empty());
    }

    #[test]
    fn the_hotbar_sits_where_vanilla_draws_it() {
        // 3200x1440 at GUI scale 6: 534x240 GUI pixels, the hotbar from x 176 to 358, y 218.
        let l = Layout::new(3200.0, 1440.0, 6.0);
        assert_eq!(l.hotbar, [176.0 * 6.0, 218.0 * 6.0, 120.0]);
        assert_eq!(l.slot_at([176.0 * 6.0 + 186.0, 1400.0]), Some(1));
        assert_eq!(l.slot_at([176.0 * 6.0 + 186.0, 1200.0]), None);
    }

    #[test]
    fn the_view_turns_on_drag_uses_on_tap_and_breaks_on_hold() {
        let (l, now) = (layout(), Instant::now());
        let mut t = TouchControls::default();
        t.event(&l, 2, TouchPhase::Started, [2000.0, 600.0], now);
        let look = t.event(&l, 2, TouchPhase::Moved, [2100.0, 600.0], now);
        assert!(matches!(look[..], [Action::Look(dx, dy)] if dx > 0.0 && dy == 0.0));
        assert!(
            t.event(&l, 2, TouchPhase::Ended, [2100.0, 600.0], now)
                .is_empty()
        );

        t.event(&l, 3, TouchPhase::Started, [2000.0, 600.0], now);
        assert_eq!(
            t.event(&l, 3, TouchPhase::Ended, [2000.0, 600.0], now + HOLD / 3),
            vec![Action::Use]
        );

        t.event(&l, 4, TouchPhase::Started, [2000.0, 600.0], now);
        assert_eq!(t.frame(now + HOLD), vec![Action::AttackStart]);
        assert!(t.frame(now + HOLD * 2).is_empty());
        assert_eq!(
            t.event(&l, 4, TouchPhase::Ended, [2000.0, 600.0], now + HOLD * 3),
            vec![Action::AttackStop]
        );
    }

    #[test]
    fn buttons_and_slots_take_their_fingers() {
        let (l, now) = (layout(), Instant::now());
        let mut t = TouchControls::default();
        let (_, jump, _) = l.buttons[0];
        assert_eq!(
            t.event(&l, 5, TouchPhase::Started, jump, now),
            vec![Action::Press(Button::Jump)]
        );
        assert!(t.held(Button::Jump));
        // A second finger on the left half while jump is held still walks.
        assert!(
            t.event(&l, 6, TouchPhase::Started, [300.0, 1200.0], now)
                .is_empty()
        );
        assert!(t.stick.is_some());
        assert_eq!(
            t.reset(),
            vec![Action::Release(Button::Jump), Action::Stick([0.0, 0.0])]
        );
        let [left, top, slot] = l.hotbar;
        assert_eq!(
            t.event(
                &l,
                7,
                TouchPhase::Started,
                [left + slot * 4.5, top + 10.0],
                now
            ),
            vec![Action::Slot(4)]
        );
    }
}
