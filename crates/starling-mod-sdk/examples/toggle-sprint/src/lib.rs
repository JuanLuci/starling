//! Example Starling mod in the spirit of ToggleSprint: V (or its touch button) toggles sprinting
//! and G sneaking, held for the player (`input.hold`) until pressed again and kept between
//! sessions (`storage`). The top left of the HUD says what is on, as the original's did: toggled,
//! held with the key, or the game's own sprint, and flying.
use starling_mod_sdk::{
    Mod, Registration, game,
    hud::{self, Corner, Line},
    input, keys, storage,
};

struct ToggleSprint {
    sprint: bool,
    sneak: bool,
    /// The HUD's lines as last set, to set them again only when they change.
    shown: Vec<String>,
}

/// The storage key: a byte for sprint, one for sneak.
const SAVED: &str = "toggles";

impl ToggleSprint {
    /// Holds what is on, and keeps it for next time.
    fn apply(&self) {
        input::hold("key.sprint", self.sprint);
        input::hold("key.sneak", self.sneak);
        let _ = storage::set(SAVED, &[u8::from(self.sprint), u8::from(self.sneak)]);
    }

    fn lines(&self) -> Vec<String> {
        let player = game::get_player();
        let mut lines = Vec::new();
        if player.as_ref().is_some_and(|p| p.flying) {
            lines.push("[Flying]".to_owned());
        }
        if self.sneak {
            lines.push("[Sneaking (Toggled)]".to_owned());
        } else if keys::is_control_down("key.sneak") {
            lines.push("[Sneaking (Key Held)]".to_owned());
        }
        if self.sprint {
            lines.push("[Sprinting (Toggled)]".to_owned());
        } else if player.as_ref().is_some_and(|p| p.sprinting) {
            lines.push("[Sprinting (Vanilla)]".to_owned());
        }
        lines
    }
}

impl Mod for ToggleSprint {
    fn init() -> Result<(Self, Registration), String> {
        let saved = storage::get(SAVED).unwrap_or_default();
        let on = |i: usize| saved.get(i) == Some(&1);
        let toggles = ToggleSprint {
            sprint: on(0),
            sneak: on(1),
            shown: Vec::new(),
        };
        toggles.apply();
        let hooks = Registration::new()
            .keybind("sprint", "Toggle Sprint", "key.keyboard.v")
            .keybind("sneak", "Toggle Sneak", "key.keyboard.g")
            .on_tick();
        Ok((toggles, hooks))
    }

    fn key(&mut self, id: &str, pressed: bool) {
        match (id, pressed) {
            ("sprint", true) => self.sprint = !self.sprint,
            ("sneak", true) => self.sneak = !self.sneak,
            _ => return,
        }
        self.apply();
    }

    fn tick(&mut self, _tick: u64) {
        let lines = self.lines();
        if lines == self.shown {
            return;
        }
        if lines.is_empty() {
            hud::clear();
        } else {
            let panel: Vec<Line> = lines
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::TopLeft, &panel);
        }
        self.shown = lines;
    }
}

starling_mod_sdk::export_mod!(ToggleSprint);
