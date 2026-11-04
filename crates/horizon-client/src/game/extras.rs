//! Horizon's extras in the game (`docs/extras.md`); a native mod's minimap
//! (`native_minimap`: its tiles, image and HUD) and zoom (`native_zoom`) on the camera; clicks
//! and frame times for mods (`native_clicks`, `native_frame_rate`).
//! Each hook does nothing while its extra is off and no mod uses it.
use super::{Dim, Game, Screen, native_hooks::NativeMinimap};
use horizon_client::{
    extras::{Corner, Cps, FpsCounter, fov_target},
    extras_hud::{self, Corners},
    hud::{HudBatch, MINIMAP_SPRITE},
    minimap::{self, Layer, Minimap, View},
    multiplayer,
    reconnect::{Retry, Tracker},
    title::Mode,
};
use std::time::Duration;
use winit::event::MouseButton;

pub(super) struct ExtrasState {
    fps: FpsCounter,
    /// Left and right mouse buttons: held, and their clicks.
    mouse: [bool; 2],
    cps: [Cps; 2],
    minimap: Minimap,
    /// The dimension the minimap's tiles belong to.
    minimap_dim: Option<Dim>,
    /// Auto-reconnect's attempts at the current server.
    reconnect: Tracker,
    /// Volt Boost rolling 48-frame times (ms) for Perf HUD sparkline, Auto-DRS & Thermal Guard.
    pub(super) frame_ms_ring: [f32; 48],
    pub(super) frame_ms_head: usize,
    pub(super) ema_frame_ms: f32,
    pub(super) drs_scale: f32,
    pub(super) last_drs_apply_at: f64,
    pub(super) cached_rss_mib: (f64, u64),
    pub(super) last_mem_trim_at: f64,
    pub(super) drawn_entities: std::sync::atomic::AtomicU32,
}

impl Default for ExtrasState {
    fn default() -> Self {
        Self {
            fps: FpsCounter::default(),
            mouse: [false; 2],
            cps: Default::default(),
            minimap: Minimap::default(),
            minimap_dim: None,
            reconnect: Tracker::default(),
            frame_ms_ring: [0.0; 48],
            frame_ms_head: 0,
            ema_frame_ms: 16.0,
            drs_scale: 1.0,
            last_drs_apply_at: 0.0,
            cached_rss_mib: (-10.0, 0),
            last_mem_trim_at: 0.0,
            drawn_entities: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl ExtrasState {
    pub(super) fn effective_drs_scale(&self) -> f32 {
        self.drs_scale.clamp(0.50, 1.0)
    }
}
/// CPU time the minimap's tiles may take per frame.
const MINIMAP_BUDGET: Duration = Duration::from_micros(300);

impl Game<'_> {
    fn extras_now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
    /// Mouse buttons while playing, for mods' clicks.
    pub(super) fn extras_mouse(&mut self, button: MouseButton, pressed: bool) {
        self.native_mouse_event(button, pressed);
        let i = match button {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            _ => return,
        };
        self.extras.mouse[i] = pressed;
        // For mods (`input.clicks-per-second`, `native_clicks`).
        if pressed {
            let now = self.extras_now();
            self.extras.cps[i].click(now);
        }
    }
    /// Once per frame: the frame's time, and keys let go behind screens.
    pub(super) fn extras_frame(&mut self, elapsed: Duration) {
        // Under agent control the window may lack focus without pausing.
        let focused = self.focused || self.control.is_some();
        let playing = self.screen == Screen::Playing && self.title.is_none() && focused;
        if !playing {
            self.extras.mouse = [false; 2];
        }
        self.update_minimap();
        // For mods (`render.frame-stats`, `native_frame_rate`).
        let now = self.extras_now();
        let ms = (elapsed.as_secs_f32() * 1000.0).clamp(0.1, 250.0);
        self.extras.fps.record(now, ms);
        let head = self.extras.frame_ms_head;
        self.extras.frame_ms_ring[head] = ms;
        self.extras.frame_ms_head = (head + 1) % 48;
        self.extras.ema_frame_ms = self.extras.ema_frame_ms * 0.90 + ms * 0.10;

        if now - self.extras.cached_rss_mib.0 >= 1.0
            && (self.settings.extras.perf_hud != horizon_client::volt_features::PerfHudMode::Off
                || self.settings.extras.memory_saver)
        {
            self.extras.cached_rss_mib = (now, horizon_threads::resident_ram_mib().unwrap_or(0));
        }

        if self.settings.extras.memory_saver && now - self.extras.last_mem_trim_at >= 30.0 {
            self.extras.last_mem_trim_at = now;
            self.particles.trim_memory();
            self.pending_uploads.shrink_to_fit();
            horizon_threads::release_free_memory();
        }
        if self.settings.extras.purge_ram_requested {
            self.settings.extras.purge_ram_requested = false;
            self.particles.trim_memory();
            self.pending_uploads.shrink_to_fit();
            horizon_threads::release_free_memory();
            let rss = horizon_threads::resident_ram_mib().unwrap_or(0);
            self.extras.cached_rss_mib = (now, rss);
            self.titles.set_overlay(
                horizon_client::text::Text::literal(format!(
                    "Volt Boost: Purged unused RAM ({rss} MiB resident)"
                ))
                .with_color(horizon_client::text::GREEN),
                false,
            );
        }

        // Auto-DRS & Mobile Thermal Guard dynamic 3D resolution controller
        let drs_cfg = self.settings.extras.auto_drs.target_and_floor();
        let thermal_on = self.settings.extras.thermal_guard;
        let desired_drs = if playing && (drs_cfg.is_some() || thermal_on) {
            let (target_fps, floor) = drs_cfg.unwrap_or((60.0, 0.72));
            let target_ms = 1000.0 / target_fps;
            let mut s = self.extras.drs_scale;
            if self.extras.ema_frame_ms > target_ms * 1.12 {
                s = (s - 0.04).max(floor);
            } else if thermal_on && self.extras.ema_frame_ms > 24.0 {
                s = (s - 0.05).max(0.70);
            } else if self.extras.ema_frame_ms < target_ms * 0.80 {
                s = (s + 0.02).min(1.0);
            }
            s
        } else {
            1.0
        };
        if (desired_drs - self.extras.drs_scale).abs() >= 0.035
            && now - self.extras.last_drs_apply_at >= 0.45
        {
            self.extras.drs_scale = desired_drs;
            self.extras.last_drs_apply_at = now;
            if let Some(r) = self.renderer.as_mut() {
                let base = self.settings.horizon.resolution_scale as f32 / 100.0;
                r.set_world_post(
                    (base * desired_drs).clamp(0.25, 2.0),
                    self.settings.horizon.fxaa,
                    self.settings.horizon.taa,
                    self.settings.horizon.upscaling.ratio(),
                );
            }
        }
        if thermal_on && playing && self.extras.ema_frame_ms > 28.0 {
            self.particles.max_particles = self.particles.max_particles.min(1024);
        } else {
            self.particles.max_particles = self.settings.extras.particle_budget.max_particles();
        }
        self.native_frame(elapsed);
        self.recipes_received_now();
    }
    /// The frame counter for mods: fps, min, 1% low.
    pub(super) fn frame_reading(&self) -> (u32, u32, u32) {
        let r = self.extras.fps.reading();
        (r.fps, r.min, r.low)
    }
    /// Attack and use clicks in the last second, and whether each is held.
    pub(super) fn mouse_reading(&mut self) -> ((u32, u32), [bool; 2]) {
        let now = self.extras_now();
        let [l, r] = &mut self.extras.cps;
        let cps = (l.per_second(now) as u32, r.per_second(now) as u32);
        (cps, self.extras.mouse)
    }
    /// Auto-reconnect: a connection to `address` as `name` starts.
    pub(super) fn extras_connecting(&mut self, address: &str, name: &str, lan: bool) {
        self.extras.reconnect.connecting(address, name, lan);
    }
    /// The server this game connected to last.
    pub(super) fn server_address(&self) -> Option<&str> {
        self.extras.reconnect.address()
    }
    /// Auto-reconnect: the server let the player in.
    pub(super) fn extras_joined(&mut self) {
        self.extras.reconnect.joined(std::time::Instant::now());
    }
    /// Auto-reconnect: the player left the server on purpose.
    pub(super) fn extras_left(&mut self) {
        self.extras.reconnect.reset();
    }
    /// Auto-reconnect's next attempt after a disconnect, when it is on and
    /// the reason allows one.
    pub(super) fn reconnect_after(
        &mut self,
        reason: &str,
        key: Option<&str>,
        from_server: bool,
    ) -> Option<Retry> {
        if !self.settings.extras.auto_reconnect {
            self.extras.reconnect.reset();
            return None;
        }
        let now = std::time::Instant::now();
        self.extras.reconnect.ended(reason, key, from_server, now)
    }
    /// Show the attempt's countdown on the Disconnected screen.
    pub(super) fn offer_reconnect(&mut self, retry: Option<Retry>) {
        let Some(retry) = retry else {
            return;
        };
        if let Some(title) = self.title.as_mut()
            && let Mode::Multiplayer(multiplayer::Screen::Disconnected { retry: slot, .. }) =
                &mut title.mode
        {
            log::info!(
                "Horizon: reconnecting to {} in {} s",
                horizon_client::net::shown_address(&retry.address),
                retry.seconds_left(std::time::Instant::now())
            );
            *slot = Some(retry);
        }
    }
    /// Auto-reconnect: connect again once the countdown is over. An
    /// online-mode server gets the signed-in account again: `connect_server`
    /// takes it from `auth::current()`, held in memory for the whole run
    /// (the launcher's session file is gone after the first read).
    pub(super) fn extras_retry(&mut self) -> horizon_client::Result<()> {
        if let Some(retry) = self.title.as_mut().and_then(|t| t.due_retry()) {
            self.connect_server_with_options(&retry.address, &retry.name, None, retry.lan)?;
        }
        Ok(())
    }
    /// The minimap a mod shows (`native_minimap`): its corner, its side
    /// in GUI pixels and its view of the player.
    fn minimap_shown(&self) -> Option<(Corner, f32, View)> {
        let p = self.player.as_ref()?;
        if self.title.is_some() || self.ui.hide_gui {
            return None;
        }
        let m = self.minimap_wanted()?;
        let view = View {
            x: p.position.x,
            z: p.position.z,
            yaw: self.yaw,
            rotate: m.rotate,
            blocks_per_pixel: m.blocks_per_pixel,
            round: m.round,
        };
        Some((m.corner, m.size, view))
    }
    /// The minimap shown: a native mod's (`native_minimap`).
    fn minimap_wanted(&self) -> Option<NativeMinimap> {
        self.native_minimap_now()
    }
    /// Refresh the minimap's tiles within the frame budget, recompose its
    /// image when the view or a tile changed, and queue the upload.
    fn update_minimap(&mut self) {
        let Some((_, _, view)) = self.minimap_shown() else {
            if self.minimap_wanted().is_none() || self.player.is_none() {
                self.extras.minimap.clear();
                self.extras.minimap_dim = None;
            }
            return;
        };
        if self.extras.minimap_dim != Some(self.dim) {
            self.extras.minimap.clear();
            self.extras.minimap_dim = Some(self.dim);
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let head = (p.position.y + f64::from(p.dimensions.eye_height)).floor() as i32;
        let (bx, bz) = (view.x.floor() as i32, view.z.floor() as i32);
        // Underground, under a roof or in the Nether: the floor around the player.
        let covered = self.world.light(bx, head, bz) >> 4 == 0;
        let layer = if self.dim == Dim::Nether || covered {
            Layer::Below(head)
        } else {
            Layer::Surface
        };
        // The chunks the image can reach (its corners when it turns).
        let reach = minimap::SIZE as f32 / 2.0 * view.blocks_per_pixel * std::f32::consts::SQRT_2;
        let radius = ((reach / 16.0).ceil() as i32 + 1).min(self.settings.render_distance.max(2));
        self.extras.minimap.update(
            &self.world,
            self.res.facts(),
            (bx >> 4, bz >> 4),
            radius,
            layer,
            MINIMAP_BUDGET,
        );
        if !self.extras.minimap.compose(view) {
            return;
        }
        let Some(rect) = self.gui.atlas.sprites.get(MINIMAP_SPRITE).copied() else {
            return;
        };
        let (w, h) = (self.gui.atlas.width as f32, self.gui.atlas.height as f32);
        self.map_patches.push(horizon_client::font::Patch {
            x: (rect.u0 * w).round() as u32,
            y: (rect.v0 * h).round() as u32,
            width: minimap::SIZE as u32,
            height: minimap::SIZE as u32,
            rgba: self.extras.minimap.image.clone(),
        });
    }
    /// A mod's zoom in effect (`native_zoom`): (factor, smooth, turning
    /// scale); none is 1.
    fn zoom_now(&self) -> (f32, bool, f64) {
        self.native_zoom_now().map_or((1.0, true, 1.0), |z| {
            (z.factor, z.smooth, f64::from(z.turn))
        })
    }
    /// The field-of-view modifier target, divided by a smooth zoom.
    pub(super) fn zoomed_fov_target(&self, vanilla: f32) -> f32 {
        match self.zoom_now() {
            (factor, true, _) => fov_target(vanilla, factor),
            _ => vanilla,
        }
    }
    /// The field of view in degrees, divided by an instant zoom.
    pub(super) fn zoomed_fov_degrees(&self, fov: f32) -> f32 {
        match self.zoom_now() {
            (factor, false, _) => fov / factor,
            _ => fov,
        }
    }
    /// Mouse turning while zoomed.
    pub(super) fn zoom_turn_scale(&self) -> f64 {
        self.zoom_now().2
    }
    /// The minimap and mods' corner panels, in the HUD layer (hidden with F1
    /// by the caller). The top left corner gives way to the debug screen.
    pub(super) fn draw_extras(&mut self, batch: &mut HudBatch) {
        let map = self.minimap_shown();
        let visible = |corner| !(self.debug && corner == Corner::TopLeft);
        let mut corners = Corners::new(batch.width, batch.height, self.effect_rows_height());
        let atlas = &self.gui.atlas;
        if self.settings.extras.perf_hud != horizon_client::volt_features::PerfHudMode::Off
            && visible(self.settings.extras.perf_hud_corner)
        {
            let reading = self.extras.fps.reading();
            let base_pct = self.settings.horizon.resolution_scale as f32;
            let scale_pct = (base_pct * self.extras.effective_drs_scale()).round() as u32;
            let total_ent = (self.mobs.len()
                + self.dropped.len()
                + self.armor_stands.len()
                + self.orbs.len()) as u32;
            let perf_data = extras_hud::VoltPerfHudData {
                full: self.settings.extras.perf_hud
                    == horizon_client::volt_features::PerfHudMode::Full,
                corner: self.settings.extras.perf_hud_corner,
                fps: reading.fps,
                low_1pct: reading.low,
                frame_ms: self.extras.ema_frame_ms,
                ram_mib: self.extras.cached_rss_mib.1,
                scale_pct,
                entities_drawn: self
                    .extras
                    .drawn_entities
                    .load(std::sync::atomic::Ordering::Relaxed),
                entities_total: total_ent,
                particles: self.particles.active_count() as u32,
                chunks: self.world.chunks.len() as u32,
                active_boosts: self.settings.extras.active_volt_optimizations(),
                samples_ms: &self.extras.frame_ms_ring,
                sample_head: self.extras.frame_ms_head,
            };
            extras_hud::draw_volt_perf_hud(batch, atlas, &mut corners, &perf_data);
        }
        if let Some((map_corner, map_size, view)) = map
            && visible(map_corner)
            && !self.extras.minimap.image.is_empty()
            && let Some(image) = atlas.sprites.get(MINIMAP_SPRITE).copied()
        {
            let (arrow, north) = if view.rotate {
                (0.0, Some(view.north()))
            } else {
                (180.0 + view.yaw, None)
            };
            let look = extras_hud::MinimapLook {
                corner: map_corner,
                size: map_size,
                image,
                arrow,
                north,
            };
            extras_hud::draw_minimap(batch, atlas, &mut corners, &look);
        }
        self.native_hud(batch, &mut corners);
    }
    /// The height the status effect icons take at the top right
    /// (`Hud.extractEffects`: beneficial at y 1, harmful at y 26).
    fn effect_rows_height(&self) -> f32 {
        let effects = &self.potions.effects.list;
        if effects.is_empty() || self.screen != Screen::Playing {
            return 0.0;
        }
        let harmful = effects.iter().any(|e| {
            horizon_client::brewing::effect(e.id)
                .is_some_and(|d| d.category != horizon_client::brewing::Category::Beneficial)
        });
        if harmful { 50.0 } else { 25.0 }
    }
}
