//! What mods draw (`draw` calls from their HUD and screen hooks), their HUD elements before,
//! after or instead of the vanilla HUD's parts, their own screens, and the game's screens'
//! events for them (docs/mods.md).
use super::{Game, Screen, mods::rich_text};
use horizon_client::{
    hud::{HudBatch, rgba},
    hud_layout::{self, GuiDraw},
    items::ItemStack,
    screen_draw::{self, Draw, Measure},
};
use horizon_mods::{DrawOp, HudPart, KeyEvent, MouseAction, MouseEvent, Placement, RenderContext};
use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

/// A mod's colour (0xAARRGGBB, alpha 0 opaque) as the HUD batch's.
fn color(c: u32) -> u32 {
    let a = match (c >> 24) as u8 {
        0 => 255,
        a => a,
    };
    rgba((c >> 16) as u8, (c >> 8) as u8, c as u8, a)
}
/// Whether a draw call's numbers are usable: finite, sizes positive and at most a few screens.
fn sane(op: &DrawOp) -> bool {
    let ok = |v: &[f32]| v.iter().all(|v| v.is_finite() && v.abs() < 100_000.0);
    match op {
        DrawOp::Fill { x, y, w, h, .. } | DrawOp::Gradient { x, y, w, h, .. } => {
            ok(&[*x, *y, *w, *h]) && *w >= 0.0 && *h >= 0.0
        }
        // Nine-slices tile their middle: a bounded size keeps the quads bounded.
        DrawOp::Sprite { x, y, w, h, .. } | DrawOp::Panel { x, y, w, h } => {
            ok(&[*x, *y]) && (1.0..=4096.0).contains(w) && (1.0..=4096.0).contains(h)
        }
        DrawOp::Button { x, y, w, h, .. } => {
            ok(&[*x, *y]) && (8.0..=4096.0).contains(w) && (4.0..=4096.0).contains(h)
        }
        DrawOp::Text { x, y, .. } | DrawOp::Item { x, y, .. } | DrawOp::ItemId { x, y, .. } => {
            ok(&[*x, *y])
        }
        DrawOp::Tooltip { x, y, lines } => ok(&[*x, *y]) && !lines.is_empty(),
        DrawOp::Push { x, y, scale } => ok(&[*x, *y, *scale]),
        DrawOp::Pop => true,
        DrawOp::Player {
            x,
            y,
            w,
            h,
            size,
            turn,
        } => {
            ok(&[*x, *y, *turn])
                && (1.0..=4096.0).contains(w)
                && (1.0..=4096.0).contains(h)
                && (1.0..=1024.0).contains(size)
        }
        DrawOp::ItemTooltip { x, y, .. } => ok(&[*x, *y]),
    }
}

/// Which HUD part a vanilla health-bar sprite belongs to.
fn health_part(d: &GuiDraw) -> HudPart {
    let GuiDraw::Sprite { sprite, .. } = d else {
        return HudPart::Health;
    };
    if sprite.starts_with("hud/heart/vehicle") {
        HudPart::MountHealth
    } else if sprite.starts_with("hud/armor") {
        HudPart::Armor
    } else if sprite.starts_with("hud/food") {
        HudPart::Food
    } else if sprite.starts_with("hud/air") {
        HudPart::Air
    } else {
        HudPart::Health
    }
}

impl Game<'_> {
    fn mod_context(&self, batch: &HudBatch) -> RenderContext {
        RenderContext {
            width: batch.width as u32,
            height: batch.height as u32,
            mouse_x: self.cursor.0,
            mouse_y: self.cursor.1,
            delta: self.frame_delta(),
        }
    }
    /// Seconds the last frame took.
    fn frame_delta(&self) -> f32 {
        if self.fps > 0.0 { 1.0 / self.fps } else { 0.0 }
    }
    /// Paints what a mod's render hook drew.
    pub(super) fn paint_mod_ops(&mut self, batch: &mut HudBatch, ops: &[DrawOp]) {
        if ops.is_empty() {
            return;
        }
        // Push / pop: (translate x, translate y, scale).
        let mut poses: Vec<(f32, f32, f32)> = vec![(0.0, 0.0, 1.0)];
        for op in ops {
            let (tx, ty, s) = *poses.last().expect("the base pose stays");
            // A mod's numbers are not trusted: nothing it draws may stop the game.
            if !sane(op) {
                continue;
            }
            let start = batch.vertices.len();
            match op {
                DrawOp::Fill {
                    x,
                    y,
                    w,
                    h,
                    color: c,
                } => {
                    batch.fill(&self.gui.atlas, *x, *y, *w, *h, color(*c));
                }
                DrawOp::Gradient {
                    x,
                    y,
                    w,
                    h,
                    top,
                    bottom,
                } => {
                    batch.fill_gradient(
                        &self.gui.atlas,
                        (*x, *y, *w, *h),
                        color(*top),
                        color(*bottom),
                    );
                }
                DrawOp::Text {
                    x,
                    y,
                    text,
                    color: c,
                    shadow,
                } => {
                    batch.text_styled(
                        &self.gui.atlas,
                        &rich_text(text),
                        *x,
                        *y,
                        color(*c),
                        *shadow,
                    );
                }
                DrawOp::Item { x, y, stack } => {
                    if let Some(stack) = self.game_stack(stack) {
                        self.draw_item(batch, &stack, *x, *y);
                    }
                }
                DrawOp::ItemId { x, y, id } => {
                    if self.res.items().items.contains_key(id.as_str()) {
                        let stack = ItemStack::new(id, 1);
                        self.draw_item_model(batch, &stack, *x, *y);
                    }
                }
                DrawOp::Sprite { x, y, w, h, sprite } => {
                    if let Some(key) = self.mod_sprite(sprite) {
                        horizon_client::ui::sprite_sized(
                            batch,
                            &self.gui.atlas,
                            &key,
                            *x,
                            *y,
                            *w,
                            *h,
                            u32::MAX,
                        );
                    }
                }
                DrawOp::Tooltip { x, y, lines } => {
                    let draws = [Draw::Tooltip {
                        lines: lines.iter().map(|l| rich_text(l)).collect(),
                        x: *x as i32,
                        y: *y as i32,
                    }];
                    screen_draw::paint(batch, &self.gui.atlas, &draws);
                }
                DrawOp::Button {
                    x,
                    y,
                    w,
                    h,
                    label,
                    hovered,
                    active,
                } => {
                    let atlas = &self.gui.atlas;
                    let m = Measure(&atlas.font);
                    // Button lays out a label that fits (vanilla's never overflow): cut it.
                    let mut label = rich_text(label);
                    if m.width(&label) > *w as i32 - 4 {
                        label = m.head(&label, *w as i32 - 4);
                    }
                    let mut draws = Vec::new();
                    screen_draw::button(
                        &mut draws, &m, &label, *x as i32, *y as i32, *w as i32, *h as i32,
                        *active, *hovered,
                    );
                    screen_draw::paint(batch, atlas, &draws);
                }
                DrawOp::Panel { x, y, w, h } => {
                    let atlas = &self.gui.atlas;
                    horizon_client::ui::nine_slice(
                        batch,
                        atlas,
                        "gui/sprites/popup/background",
                        *x,
                        *y,
                        *w,
                        *h,
                        u32::MAX,
                    );
                }
                // Drawn in 3D in its box: placed by the pose itself.
                DrawOp::Player {
                    x,
                    y,
                    w,
                    h,
                    size,
                    turn,
                } => {
                    let (x0, y0) = (tx + x * s, ty + y * s);
                    let (x1, y1) = (x0 + w * s, y0 + h * s);
                    self.draw_player_preview_turned(batch, [x0, y0, x1, y1], size * s, *turn);
                    continue;
                }
                // Placed on the screen as the game places tooltips (inside it), so not posed.
                DrawOp::ItemTooltip { x, y, stack } => {
                    if let Some(stack) = self.game_stack(stack) {
                        let lines = self.stack_tooltip(&stack);
                        self.draw_tooltip_lines(batch, &lines, tx + x * s, ty + y * s);
                    }
                    continue;
                }
                DrawOp::Push { x, y, scale } => {
                    poses.push((tx + x * s, ty + y * s, s * scale));
                    continue;
                }
                DrawOp::Pop => {
                    if poses.len() > 1 {
                        poses.pop();
                    }
                    continue;
                }
            }
            if (tx, ty, s) != (0.0, 0.0, 1.0) {
                for v in &mut batch.vertices[start..] {
                    v.pos = [tx + v.pos[0] * s, ty + v.pos[1] * s];
                }
            }
        }
    }
    /// A vanilla HUD part, with the mods' elements before it, after it or instead of it.
    pub(super) fn mod_hud_part(
        &mut self,
        batch: &mut HudBatch,
        part: HudPart,
        vanilla: impl FnOnce(&mut Self, &mut HudBatch),
    ) {
        if !self.mods.mods.is_running() {
            vanilla(self, batch);
            return;
        }
        let context = self.mod_context(batch);
        let has = |game: &Self, placement| game.mods.mods.has_hud_element(part, placement);
        if has(self, Placement::Before) {
            let ops = self
                .mods
                .mods
                .hud_elements(part, Placement::Before, context);
            self.paint_mod_ops(batch, &ops);
        }
        if has(self, Placement::Replace) {
            let ops = self
                .mods
                .mods
                .hud_elements(part, Placement::Replace, context);
            self.paint_mod_ops(batch, &ops);
        } else {
            vanilla(self, batch);
        }
        if has(self, Placement::After) {
            let ops = self.mods.mods.hud_elements(part, Placement::After, context);
            self.paint_mod_ops(batch, &ops);
        }
        self.show_mod_output();
    }
    /// The health bars' draws (hearts, armour, food, air, a mount's hearts), each part with
    /// the mods' elements.
    pub(super) fn paint_health_parts(&mut self, batch: &mut HudBatch, draws: Vec<GuiDraw>) {
        if !self.mods.mods.is_running() {
            hud_layout::paint(batch, &self.gui.atlas, &draws);
            return;
        }
        for part in [
            HudPart::Health,
            HudPart::MountHealth,
            HudPart::Armor,
            HudPart::Food,
            HudPart::Air,
        ] {
            let own: Vec<GuiDraw> = draws
                .iter()
                .filter(|d| health_part(d) == part)
                .cloned()
                .collect();
            // A bar that does not show (food while riding, air out of water) gets no elements.
            if own.is_empty() {
                continue;
            }
            self.mod_hud_part(batch, part, |game, batch| {
                hud_layout::paint(batch, &game.gui.atlas, &own);
            });
        }
    }
    /// Over the HUD: the mods' `hud-render` drawing.
    pub(super) fn draw_mod_hud(&mut self, batch: &mut HudBatch) {
        if !self.mods.mods.is_running() || !self.mods.mods.wants(|h| h.hud) {
            return;
        }
        let context = self.mod_context(batch);
        let ops = self.mods.mods.hud_render(context);
        self.paint_mod_ops(batch, &ops);
        self.show_mod_output();
    }
    /// Over the game's screen: what the mods draw on it, then a mod's own screen.
    pub(super) fn draw_mod_screens(&mut self, batch: &mut HudBatch) {
        if !self.mods.mods.is_running() {
            return;
        }
        let Some(name) = self.mod_screen_name() else {
            return;
        };
        let own = self.mods.screen.is_some();
        if !own && !self.mods.mods.wants(|h| h.screens) {
            return;
        }
        if own {
            // Screen.renderBackground over a world: the dark gradient.
            let (w, h) = (batch.width, batch.height);
            batch.fill_gradient(
                &self.gui.atlas,
                (0.0, 0.0, w, h),
                rgba(16, 16, 16, 192),
                rgba(16, 16, 16, 208),
            );
        }
        let context = self.mod_context(batch);
        let ops = self.mods.mods.screen_render(&name, context);
        self.paint_mod_ops(batch, &ops);
        self.show_mod_output();
    }
    /// The screen shown now, as mods name it; None while playing.
    pub(super) fn mod_screen_name(&self) -> Option<String> {
        if let Some(screen) = &self.mods.screen {
            return Some(screen.clone());
        }
        if let Some(title) = &self.title {
            use horizon_client::title::Mode;
            let name = match &title.mode {
                Mode::Select { .. } => "worlds",
                Mode::Create { .. } => "create_world",
                Mode::Multiplayer(_) => "multiplayer",
                _ => "title",
            };
            return Some(name.into());
        }
        if self.screen == Screen::Inventory
            && self.remote.as_ref().is_some_and(|r| r.menu.is_some())
        {
            return Some("container".into());
        }
        match self.screen_label() {
            "playing" => None,
            "paused" => Some("pause".into()),
            "dead" => Some("death".into()),
            "station" => Some("container".into()),
            other => Some(other.replace('-', "_")),
        }
    }
    /// Once a frame: `screen-closed` and `screen-opened` when the screen changed.
    pub(super) fn mods_screen_events(&mut self) {
        let now = self.mod_screen_name();
        if now == self.mods.shown {
            return;
        }
        if let Some(old) = self.mods.shown.take() {
            self.mods.mods.screen_closed(&old);
        }
        if let Some(new) = &now {
            self.mods.mods.screen_opened(new);
        }
        self.mods.shown = now;
        self.mods.held = None;
    }
    /// `screens.open`: the mod's screen over everything, the mouse freed.
    pub(super) fn open_mod_screen(&mut self, screen: String) {
        self.begin_creative_stand_in(&screen);
        self.mods.screen = Some(screen);
        self.mods.keyboard = false;
        self.mods.drag_areas.clear();
        self.capture(false);
    }
    pub(super) fn close_mod_screen(&mut self) {
        self.end_creative_stand_in(false);
        self.mods.screen = None;
        self.mods.held = None;
        self.mods.keyboard = false;
        self.mods.drag_areas.clear();
        if self.screen == Screen::Playing
            && self.title.is_none()
            && self.player.is_some()
            && !self.over_world_screen_open()
        {
            self.capture(true);
        }
    }
    /// A sign, book or command block editor over the world.
    fn over_world_screen_open(&self) -> bool {
        self.sign_edit.is_some() || self.book.is_some() || self.command_block_screen.is_some()
    }
    /// The mods' turn at a window event: a mod screen takes all input, the game's screens'
    /// events go to the mods asking for them first, and keys reach the mods' keybinds while
    /// playing. True when the game must not see the event.
    pub(super) fn mods_window_event(&mut self, event: &WindowEvent) -> bool {
        if !self.mods.mods.is_running() {
            return false;
        }
        let own = self.mods.screen.is_some();
        let screen = self.mod_screen_name();
        let screen_events = own || (screen.is_some() && self.mods.mods.wants(|h| h.screens));
        let playing = self.screen == Screen::Playing
            && self.title.is_none()
            && screen.is_none()
            && self.player.is_some();
        match event {
            WindowEvent::KeyboardInput { event: k, .. } => {
                let PhysicalKey::Code(code) = k.physical_key else {
                    return own;
                };
                let pressed = k.state == ElementState::Pressed;
                let key = horizon_client::keybinds::key_name(code)
                    .map(str::to_owned)
                    .unwrap_or_default();
                if own && pressed && code == KeyCode::Escape {
                    self.close_mod_screen();
                    return true;
                }
                if screen_events && let Some(screen) = &screen {
                    let event = KeyEvent {
                        key: key.clone(),
                        pressed,
                        text: k.text.as_ref().map(|t| t.to_string()).unwrap_or_default(),
                        shift: self.modifiers.shift_key(),
                        control: self.modifiers.control_key(),
                        alt: self.modifiers.alt_key(),
                    };
                    let used = self.mods.mods.screen_key(screen, &event);
                    self.show_mod_output();
                    // The inventory key closes the creative screen's stand-in, as it closes the
                    // creative screen, when the mod's screen did not use it (to type, say).
                    if !used
                        && pressed
                        && !k.repeat
                        && self.mods.standing_in()
                        && self.settings.keys.translate(code) == Some(Ok(KeyCode::KeyE))
                    {
                        self.close_mod_screen();
                        return true;
                    }
                    if used || own {
                        return true;
                    }
                }
                if playing && !k.repeat && !key.is_empty() {
                    self.mods_key(&key, pressed);
                }
                false
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (b, name) = match button {
                    MouseButton::Left => (0, "key.mouse.left"),
                    MouseButton::Right => (1, "key.mouse.right"),
                    MouseButton::Middle => (2, "key.mouse.middle"),
                    _ => return own,
                };
                let pressed = *state == ElementState::Pressed;
                // The mods' buttons on the game's screen, before the screen and the mods' hooks;
                // a button's release is its own wherever it lands (its press may have opened a
                // mod's screen over it).
                let release = !pressed && self.mods.pressed_button.is_some();
                if b == 0
                    && (release || (!own && screen.is_some()))
                    && self.mod_screen_button_click(pressed)
                {
                    return true;
                }
                if screen_events && let Some(screen) = &screen {
                    self.mods.held = pressed.then_some(b);
                    let event = MouseEvent {
                        action: if pressed {
                            MouseAction::Press
                        } else {
                            MouseAction::Release
                        },
                        x: self.cursor.0,
                        y: self.cursor.1,
                        button: b,
                        scroll: 0.0,
                    };
                    let used = self.mods.mods.screen_mouse(screen, event);
                    self.show_mod_output();
                    if used || own {
                        return true;
                    }
                }
                if playing {
                    self.mods_key(name, pressed);
                }
                false
            }
            WindowEvent::MouseWheel { delta, .. } if screen_events => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 30.0) as f32,
                };
                let Some(screen) = &screen else {
                    return own;
                };
                let event = MouseEvent {
                    action: MouseAction::Scroll,
                    x: self.cursor.0,
                    y: self.cursor.1,
                    button: 0,
                    scroll: lines,
                };
                let used = self.mods.mods.screen_mouse(screen, event);
                self.show_mod_output();
                used || own
            }
            WindowEvent::CursorMoved { position, .. } if screen_events => {
                if let (Some(b), Some(screen), Some(r)) = (self.mods.held, &screen, &self.renderer)
                {
                    let scale = HudBatch::new([r.config.width, r.config.height]).scale;
                    let event = MouseEvent {
                        action: MouseAction::Drag,
                        x: position.x as f32 / scale,
                        y: position.y as f32 / scale,
                        button: b,
                        scroll: 0.0,
                    };
                    self.mods.mods.screen_mouse(screen, event);
                    self.show_mod_output();
                }
                // The game still moves its cursor.
                false
            }
            _ => false,
        }
    }
}
