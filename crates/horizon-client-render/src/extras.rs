//! Horizon's optional extras (minimap, lighting, inventory tools): their
//! settings, kept in `horizon-extras.txt` next to `options.txt` so that file
//! stays exactly vanilla's, and the logic behind them. Every extra is off by
//! default; see `docs/extras.md`. Horizon's own switches outside the Extras page
//! (No AI, Agent Control, Discord Rich Presence) are kept here too.
use crate::volt_features::{
    AudioVoiceCap, AutoDrsMode, BlockEntityCap, ChunkUploadCap, DynamicIdleFps, EntityCap,
    FastShadowMode, GlintTrimMode, ItemRenderCap, MenuBgMode, ParticleBudget, PerfHudMode,
    VoltBoostPreset,
};
use std::{collections::VecDeque, fs, path::Path};

pub const FILE: &str = "horizon-extras.txt";
/// The name the options screen uses while the sort key waits for a key.
pub const SORT_KEY: &str = "horizon.sort";

/// The minimap's sizes in GUI pixels, and its zooms in blocks per pixel.
pub const MINIMAP_SIZES: [(u8, &str); 3] = [(64, "Small"), (96, "Medium"), (128, "Large")];
pub const MINIMAP_ZOOMS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

/// A screen corner for an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Corner::TopLeft => "top_left",
            Corner::TopRight => "top_right",
            Corner::BottomLeft => "bottom_left",
            Corner::BottomRight => "bottom_right",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
        }
    }
    fn parse(s: &str, default: Corner) -> Corner {
        Self::ALL
            .into_iter()
            .find(|c| c.name() == s)
            .unwrap_or(default)
    }
    pub fn top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }
    pub fn left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    pub fullbright: bool,
    /// Held, dropped and burning light sources light their surroundings (`dynamic_light`).
    pub dynamic_lights: bool,
    pub zoom: bool,
    /// A vanilla key name (`key.keyboard.z`).
    pub zoom_key: String,
    /// The zoom the key starts at.
    pub zoom_level: u8,
    /// Ease through vanilla's FOV easing (as the spyglass does).
    pub zoom_smooth: bool,
    /// Divide mouse turning by the zoom.
    pub zoom_sensitivity: bool,
    /// The FPS counter and keystrokes extras are official mods now: their
    /// old settings stay here, kept as they were.
    pub fps: bool,
    pub fps_corner: Corner,
    /// Also show the minimum and 1% low.
    pub fps_lows: bool,
    pub keystrokes: bool,
    pub keystrokes_corner: Corner,
    /// Clicks per second under the mouse buttons.
    pub keystrokes_cps: bool,
    /// The official mods once installed for the extras they replace, by mod
    /// id (`movedToMods`).
    pub moved: Vec<String>,
    pub minimap: bool,
    pub minimap_corner: Corner,
    /// Side in GUI pixels (`MINIMAP_SIZES`).
    pub minimap_size: u8,
    pub minimap_round: bool,
    /// Blocks per pixel (`MINIMAP_ZOOMS`).
    pub minimap_zoom: f32,
    /// Turn with the player (forward up) instead of north up.
    pub minimap_rotate: bool,
    /// Retry lost multiplayer connections (`reconnect`).
    pub auto_reconnect: bool,
    /// Positional sounds leave a few static sound channels free for UI sounds
    /// and others played to the listener (`sound_engine::HEARD_RESERVE`).
    pub ui_sound_reserve: bool,
    /// The player's own name tag over its model in third person.
    pub own_name_tag: bool,
    /// No particles from breaking or mining a block (other particles stay).
    pub hide_block_particles: bool,
    /// Mouse Tweaks-style drags in the inventory and storage containers.
    pub mouse_tweaks: bool,
    /// With them, the wheel over a slot moves its items one at a time.
    pub mouse_tweaks_wheel: bool,
    /// Sort buttons on the inventory and chest-like containers.
    pub sort: bool,
    /// A vanilla key name: sorts the part under the cursor.
    pub sort_key: String,
    /// A new chat line slides in from the left while it fades in (`Chat::layout`).
    pub chat_animation: bool,
    /// The indicator bar at the left of a chat line (on by default, as vanilla).
    pub chat_indicator: bool,
    /// No AI: every AI feature hidden and off (`horizon_core::no_ai`; the launcher writes it too).
    pub no_ai: bool,
    /// Agents may control the game the player starts: it opens its control socket for
    /// horizon-ctl and MCP clients (off by default; an agent's own launch always opens it).
    pub agent_control: bool,
    /// F3+R and F3+E start/stop a repro recording and write its bundle (off by
    /// default: a recording copies the world).
    pub repro_keys: bool,
    /// "Playing Horizon" on the player's Discord profile (`discord_presence`; on by default).
    pub discord_presence: bool,
    /// Another controller can join P1's world (`splitscreen`, off by default).
    pub splitscreen: bool,
    /// Two players in one window: 0 auto (side by side when wider than tall), 1 side by side,
    /// 2 top and bottom (`splitscreenTwo`).
    pub splitscreen_two: u8,
    /// Three players on a screen: 0 one wide on top, 1 one tall on the left, 2 side by side
    /// (`splitscreenThree`).
    pub splitscreen_three: u8,
    /// Splitscreen: each player's name in the corner of their view (`splitscreenNames`).
    pub splitscreen_names: bool,
    /// The wheel over a cycle button in the settings changes it (`CycleButton.mouseScrolled`,
    /// on by default as vanilla); off, the wheel only scrolls the list.
    pub wheel_cycles_settings: bool,
    /// The built-in music tracks the Music page turned off (sound ids): they
    /// leave every pool they play in (`musicOff`, empty by default: vanilla's
    /// random selection).
    pub music_off: Vec<String>,
    /// The Music page: the Music... button on Music & Sounds, playing a
    /// built-in track and leaving the off ones out (`musicPage`, off by
    /// default: vanilla plays exactly as before).
    pub music_page: bool,
    /// Cosmetics (`horizon_cosmetics`, off unless enabled): the player's own, and the other
    /// players' (`cosmeticsMine`, `cosmeticsOthers`; both on by default).
    pub cosmetics_mine: bool,
    pub cosmetics_others: bool,
    // --- Volt Boost (Tiers 1, 2 & 3 — all OFF / Vanilla by default) ---
    pub volt_preset: VoltBoostPreset,
    pub perf_hud: PerfHudMode,
    pub perf_hud_corner: Corner,
    pub cull_leaves: bool,
    pub frustum_priority_mesh: bool,
    pub cave_sky_cull: bool,
    pub chunk_upload_cap: ChunkUploadCap,
    pub fast_weather: bool,
    pub distant_lod_boost: bool,
    pub entity_cap: EntityCap,
    pub item_render_cap: ItemRenderCap,
    pub block_entity_cap: BlockEntityCap,
    pub cull_sign_text: bool,
    pub fast_shadows: FastShadowMode,
    pub distant_mob_throttle: bool,
    pub hub_lag_shield: bool,
    pub particle_budget: ParticleBudget,
    pub chaos_particle_shield: bool,
    pub anim_throttle: bool,
    pub glint_mode: GlintTrimMode,
    pub auto_drs: AutoDrsMode,
    pub dynamic_fps: DynamicIdleFps,
    pub menu_bg_mode: MenuBgMode,
    pub thermal_guard: bool,
    pub volt_reflex: bool,
    pub thread_boost: bool,
    pub prewarm_pipelines: bool,
    pub audio_voice_cap: AudioVoiceCap,
    pub memory_saver: bool,
    pub keep_screen_awake: bool,
    pub active_anim_cull: bool,
    pub lookahead_stream: bool,
    pub fast_translucency: bool,
    pub particle_frustum_cull: bool,
    /// Transient one-shot flag triggered when the user taps "Purge Unused RAM Now".
    pub purge_ram_requested: bool,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            fullbright: false,
            dynamic_lights: false,
            zoom: false,
            zoom_key: "key.keyboard.z".into(),
            zoom_level: 4,
            zoom_smooth: true,
            zoom_sensitivity: true,
            fps: false,
            fps_corner: Corner::TopLeft,
            fps_lows: false,
            keystrokes: false,
            keystrokes_corner: Corner::TopLeft,
            keystrokes_cps: true,
            moved: Vec::new(),
            minimap: false,
            minimap_corner: Corner::TopRight,
            minimap_size: 96,
            minimap_round: false,
            minimap_zoom: 1.0,
            minimap_rotate: false,
            auto_reconnect: false,
            ui_sound_reserve: false,
            own_name_tag: false,
            hide_block_particles: false,
            mouse_tweaks: false,
            mouse_tweaks_wheel: true,
            sort: false,
            sort_key: "key.keyboard.unknown".into(),
            chat_animation: false,
            chat_indicator: true,
            no_ai: false,
            agent_control: false,
            repro_keys: false,
            discord_presence: true,
            splitscreen: false,
            splitscreen_two: 0,
            splitscreen_three: 0,
            splitscreen_names: true,
            wheel_cycles_settings: true,
            music_off: Vec::new(),
            music_page: false,
            cosmetics_mine: true,
            cosmetics_others: true,
            volt_preset: VoltBoostPreset::Off,
            perf_hud: PerfHudMode::Off,
            perf_hud_corner: Corner::TopLeft,
            cull_leaves: false,
            frustum_priority_mesh: false,
            cave_sky_cull: false,
            chunk_upload_cap: ChunkUploadCap::Auto,
            fast_weather: false,
            distant_lod_boost: false,
            entity_cap: EntityCap::Unlimited,
            item_render_cap: ItemRenderCap::Unlimited,
            block_entity_cap: BlockEntityCap::Unlimited,
            cull_sign_text: false,
            fast_shadows: FastShadowMode::Vanilla,
            distant_mob_throttle: false,
            hub_lag_shield: false,
            particle_budget: ParticleBudget::Vanilla16k,
            chaos_particle_shield: false,
            anim_throttle: false,
            glint_mode: GlintTrimMode::Full,
            auto_drs: AutoDrsMode::Off,
            dynamic_fps: DynamicIdleFps::Off,
            menu_bg_mode: MenuBgMode::PanoramaBlur,
            thermal_guard: false,
            volt_reflex: false,
            thread_boost: false,
            prewarm_pipelines: false,
            audio_voice_cap: AudioVoiceCap::Vanilla,
            memory_saver: false,
            keep_screen_awake: false,
            active_anim_cull: false,
            lookahead_stream: false,
            fast_translucency: false,
            particle_frustum_cull: false,
            purge_ram_requested: false,
        }
    }
}
fn bool_text(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}
impl Extras {
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    pub fn parse(text: &str) -> Self {
        let mut e = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            let flag = |d: bool| match value {
                "true" => true,
                "false" => false,
                _ => d,
            };
            match key.trim() {
                "fullbright" => e.fullbright = flag(e.fullbright),
                "dynamicLights" => e.dynamic_lights = flag(e.dynamic_lights),
                "zoom" => e.zoom = flag(e.zoom),
                "zoomKey" if value.starts_with("key.") => e.zoom_key = value.to_owned(),
                "zoomLevel" => {
                    if let Ok(v) = value.parse::<u8>() {
                        e.zoom_level = v.clamp(2, 10);
                    }
                }
                "zoomSmooth" => e.zoom_smooth = flag(e.zoom_smooth),
                "zoomSensitivity" => e.zoom_sensitivity = flag(e.zoom_sensitivity),
                "fps" => e.fps = flag(e.fps),
                "fpsCorner" => e.fps_corner = Corner::parse(value, e.fps_corner),
                "fpsLows" => e.fps_lows = flag(e.fps_lows),
                "keystrokes" => e.keystrokes = flag(e.keystrokes),
                "keystrokesCorner" => {
                    e.keystrokes_corner = Corner::parse(value, e.keystrokes_corner)
                }
                "keystrokesCps" => e.keystrokes_cps = flag(e.keystrokes_cps),
                "movedToMods" => {
                    e.moved = value
                        .split(',')
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned)
                        .collect()
                }
                "minimap" => e.minimap = flag(e.minimap),
                "minimapCorner" => e.minimap_corner = Corner::parse(value, e.minimap_corner),
                "minimapSize" => {
                    if let Some(&(v, _)) = MINIMAP_SIZES.iter().find(|s| s.0.to_string() == value) {
                        e.minimap_size = v;
                    }
                }
                "minimapShape" => e.minimap_round = value == "round",
                "minimapZoom" => {
                    if let Some(&z) = MINIMAP_ZOOMS.iter().find(|z| z.to_string() == value) {
                        e.minimap_zoom = z;
                    }
                }
                "minimapRotate" => e.minimap_rotate = flag(e.minimap_rotate),
                "autoReconnect" => e.auto_reconnect = flag(e.auto_reconnect),
                "uiSoundReserve" => e.ui_sound_reserve = flag(e.ui_sound_reserve),
                "ownNameTag" => e.own_name_tag = flag(e.own_name_tag),
                "hideBlockParticles" => e.hide_block_particles = flag(e.hide_block_particles),
                "mouseTweaks" => e.mouse_tweaks = flag(e.mouse_tweaks),
                "mouseTweaksWheel" => e.mouse_tweaks_wheel = flag(e.mouse_tweaks_wheel),
                "sort" => e.sort = flag(e.sort),
                "sortKey" if value.starts_with("key.") => e.sort_key = value.to_owned(),
                "chatAnimation" => e.chat_animation = flag(e.chat_animation),
                "chatIndicator" => e.chat_indicator = flag(e.chat_indicator),
                horizon_core::no_ai::KEY => e.no_ai = flag(e.no_ai),
                "agentControl" => e.agent_control = flag(e.agent_control),
                "reproKeys" => e.repro_keys = flag(e.repro_keys),
                "discordPresence" => e.discord_presence = flag(e.discord_presence),
                "splitscreen" => e.splitscreen = flag(e.splitscreen),
                "splitscreenTwo" => e.splitscreen_two = value.parse().unwrap_or(0).min(2),
                "splitscreenThree" => e.splitscreen_three = value.parse().unwrap_or(0).min(2),
                "splitscreenNames" => e.splitscreen_names = flag(e.splitscreen_names),
                "wheelCyclesSettings" => e.wheel_cycles_settings = flag(e.wheel_cycles_settings),
                "musicOff" => {
                    e.music_off = value
                        .split(',')
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned)
                        .collect()
                }
                "musicPage" => e.music_page = flag(e.music_page),
                "cosmeticsMine" => e.cosmetics_mine = flag(e.cosmetics_mine),
                "cosmeticsOthers" => e.cosmetics_others = flag(e.cosmetics_others),
                // Volt Boost:
                "voltPreset" => {
                    if let Some(v) = VoltBoostPreset::parse(value) {
                        e.volt_preset = v;
                    }
                }
                "voltPerfHud" => {
                    if let Some(v) = PerfHudMode::parse(value) {
                        e.perf_hud = v;
                    }
                }
                "voltPerfHudCorner" => e.perf_hud_corner = Corner::parse(value, e.perf_hud_corner),
                "voltCullLeaves" => e.cull_leaves = flag(e.cull_leaves),
                "voltFrustumMesh" => e.frustum_priority_mesh = flag(e.frustum_priority_mesh),
                "voltCaveSkyCull" => e.cave_sky_cull = flag(e.cave_sky_cull),
                "voltChunkUploadCap" => {
                    if let Some(v) = ChunkUploadCap::parse(value) {
                        e.chunk_upload_cap = v;
                    }
                }
                "voltFastWeather" => e.fast_weather = flag(e.fast_weather),
                "voltDistantLodManual" => e.distant_lod_boost = flag(e.distant_lod_boost),
                "voltEntityCap" => {
                    if let Some(v) = EntityCap::parse(value) {
                        e.entity_cap = v;
                    }
                }
                "voltItemCap" => {
                    if let Some(v) = ItemRenderCap::parse(value) {
                        e.item_render_cap = v;
                    }
                }
                "voltBlockEntityCap" => {
                    if let Some(v) = BlockEntityCap::parse(value) {
                        e.block_entity_cap = v;
                    }
                }
                "voltCullSignText" => e.cull_sign_text = flag(e.cull_sign_text),
                "voltFastShadows" => {
                    if let Some(v) = FastShadowMode::parse(value) {
                        e.fast_shadows = v;
                    }
                }
                "voltDistantMobThrottle" => e.distant_mob_throttle = flag(e.distant_mob_throttle),
                "voltHubLagShield" => e.hub_lag_shield = flag(e.hub_lag_shield),
                "voltParticleBudget" => {
                    if let Some(v) = ParticleBudget::parse(value) {
                        e.particle_budget = v;
                    }
                }
                "voltChaosShield" => e.chaos_particle_shield = flag(e.chaos_particle_shield),
                "voltAnimThrottle" => e.anim_throttle = flag(e.anim_throttle),
                "voltGlintMode" => {
                    if let Some(v) = GlintTrimMode::parse(value) {
                        e.glint_mode = v;
                    }
                }
                "voltAutoDrs" => {
                    if let Some(v) = AutoDrsMode::parse(value) {
                        e.auto_drs = v;
                    }
                }
                "voltDynamicFps" => {
                    if let Some(v) = DynamicIdleFps::parse(value) {
                        e.dynamic_fps = v;
                    }
                }
                "voltMenuBg" => {
                    if let Some(v) = MenuBgMode::parse(value) {
                        e.menu_bg_mode = v;
                    }
                }
                "voltThermalGuard" => e.thermal_guard = flag(e.thermal_guard),
                "voltReflex" => e.volt_reflex = flag(e.volt_reflex),
                "voltThreadBoost" => e.thread_boost = flag(e.thread_boost),
                "voltPrewarm" => e.prewarm_pipelines = flag(e.prewarm_pipelines),
                "voltAudioCap" => {
                    if let Some(v) = AudioVoiceCap::parse(value) {
                        e.audio_voice_cap = v;
                    }
                }
                "voltMemorySaver" => e.memory_saver = flag(e.memory_saver),
                "voltKeepAwake" => e.keep_screen_awake = flag(e.keep_screen_awake),
                "voltActiveAnimCull" => e.active_anim_cull = flag(e.active_anim_cull),
                "voltLookaheadStream" => e.lookahead_stream = flag(e.lookahead_stream),
                "voltFastTranslucency" => e.fast_translucency = flag(e.fast_translucency),
                "voltParticleFrustum" => e.particle_frustum_cull = flag(e.particle_frustum_cull),
                _ => {}
            }
        }
        e
    }
    pub fn serialize(&self) -> String {
        let mut s = format!(
            "fullbright:{}\ndynamicLights:{}\nzoom:{}\nzoomKey:{}\nzoomLevel:{}\nzoomSmooth:{}\nzoomSensitivity:{}\nfps:{}\nfpsCorner:{}\nfpsLows:{}\nkeystrokes:{}\nkeystrokesCorner:{}\nkeystrokesCps:{}\nmovedToMods:{}\nminimap:{}\nminimapCorner:{}\nminimapSize:{}\nminimapShape:{}\nminimapZoom:{}\nminimapRotate:{}\nautoReconnect:{}\nuiSoundReserve:{}\nownNameTag:{}\nhideBlockParticles:{}\nmouseTweaks:{}\nmouseTweaksWheel:{}\nsort:{}\nsortKey:{}\nchatAnimation:{}\nchatIndicator:{}\n{}:{}\nagentControl:{}\nreproKeys:{}\ndiscordPresence:{}\nsplitscreen:{}\nsplitscreenTwo:{}\nsplitscreenThree:{}\nsplitscreenNames:{}\nwheelCyclesSettings:{}\nmusicOff:{}\nmusicPage:{}\ncosmeticsMine:{}\ncosmeticsOthers:{}\n",
            bool_text(self.fullbright),
            bool_text(self.dynamic_lights),
            bool_text(self.zoom),
            self.zoom_key,
            self.zoom_level,
            bool_text(self.zoom_smooth),
            bool_text(self.zoom_sensitivity),
            bool_text(self.fps),
            self.fps_corner.name(),
            bool_text(self.fps_lows),
            bool_text(self.keystrokes),
            self.keystrokes_corner.name(),
            bool_text(self.keystrokes_cps),
            self.moved.join(","),
            bool_text(self.minimap),
            self.minimap_corner.name(),
            self.minimap_size,
            if self.minimap_round {
                "round"
            } else {
                "square"
            },
            self.minimap_zoom,
            bool_text(self.minimap_rotate),
            bool_text(self.auto_reconnect),
            bool_text(self.ui_sound_reserve),
            bool_text(self.own_name_tag),
            bool_text(self.hide_block_particles),
            bool_text(self.mouse_tweaks),
            bool_text(self.mouse_tweaks_wheel),
            bool_text(self.sort),
            self.sort_key,
            bool_text(self.chat_animation),
            bool_text(self.chat_indicator),
            horizon_core::no_ai::KEY,
            bool_text(self.no_ai),
            bool_text(self.agent_control),
            bool_text(self.repro_keys),
            bool_text(self.discord_presence),
            bool_text(self.splitscreen),
            self.splitscreen_two,
            self.splitscreen_three,
            bool_text(self.splitscreen_names),
            bool_text(self.wheel_cycles_settings),
            self.music_off.join(","),
            bool_text(self.music_page),
            bool_text(self.cosmetics_mine),
            bool_text(self.cosmetics_others),
        );
        s.push_str(&format!(
            "voltPreset:{}\nvoltPerfHud:{}\nvoltPerfHudCorner:{}\nvoltCullLeaves:{}\nvoltFrustumMesh:{}\nvoltCaveSkyCull:{}\nvoltChunkUploadCap:{}\nvoltFastWeather:{}\nvoltDistantLodManual:{}\nvoltEntityCap:{}\nvoltItemCap:{}\nvoltBlockEntityCap:{}\nvoltCullSignText:{}\nvoltFastShadows:{}\nvoltDistantMobThrottle:{}\nvoltHubLagShield:{}\nvoltParticleBudget:{}\nvoltChaosShield:{}\nvoltAnimThrottle:{}\nvoltGlintMode:{}\nvoltAutoDrs:{}\nvoltDynamicFps:{}\nvoltMenuBg:{}\nvoltThermalGuard:{}\nvoltReflex:{}\nvoltThreadBoost:{}\nvoltPrewarm:{}\nvoltAudioCap:{}\nvoltMemorySaver:{}\nvoltKeepAwake:{}\nvoltActiveAnimCull:{}\nvoltLookaheadStream:{}\nvoltFastTranslucency:{}\nvoltParticleFrustum:{}\n",
            self.volt_preset.as_str(),
            self.perf_hud.as_str(),
            self.perf_hud_corner.name(),
            bool_text(self.cull_leaves),
            bool_text(self.frustum_priority_mesh),
            bool_text(self.cave_sky_cull),
            self.chunk_upload_cap.as_str(),
            bool_text(self.fast_weather),
            bool_text(self.distant_lod_boost),
            self.entity_cap.as_str(),
            self.item_render_cap.as_str(),
            self.block_entity_cap.as_str(),
            bool_text(self.cull_sign_text),
            self.fast_shadows.as_str(),
            bool_text(self.distant_mob_throttle),
            bool_text(self.hub_lag_shield),
            self.particle_budget.as_str(),
            bool_text(self.chaos_particle_shield),
            bool_text(self.anim_throttle),
            self.glint_mode.as_str(),
            self.auto_drs.as_str(),
            self.dynamic_fps.as_str(),
            self.menu_bg_mode.as_str(),
            bool_text(self.thermal_guard),
            bool_text(self.volt_reflex),
            bool_text(self.thread_boost),
            bool_text(self.prewarm_pipelines),
            self.audio_voice_cap.as_str(),
            bool_text(self.memory_saver),
            bool_text(self.keep_screen_awake),
            bool_text(self.active_anim_cull),
            bool_text(self.lookahead_stream),
            bool_text(self.fast_translucency),
            bool_text(self.particle_frustum_cull),
        ));
        s
    }
    /// Binds the extra key `name` (`SORT_KEY`); false for
    /// another name (a vanilla mapping).
    pub fn set_key(&mut self, name: &str, value: String) -> bool {
        match name {
            SORT_KEY => self.sort_key = value,
            _ => return false,
        }
        true
    }
    /// Written only once something differs from the defaults, so a vanilla
    /// setup gains no file.
    pub fn save(&self, dir: &Path) {
        let path = dir.join(FILE);
        if *self == Self::default() && !path.exists() {
            return;
        }
        let _ = fs::write(path, self.serialize());
    }
    /// Count of active Volt Boost optimizations currently enabled.
    pub fn active_volt_optimizations(&self) -> usize {
        let mut n = 0usize;
        for b in [
            self.cull_leaves,
            self.frustum_priority_mesh,
            self.cave_sky_cull,
            self.chunk_upload_cap != ChunkUploadCap::Auto,
            self.fast_weather,
            self.distant_lod_boost,
            self.entity_cap != EntityCap::Unlimited,
            self.item_render_cap != ItemRenderCap::Unlimited,
            self.block_entity_cap != BlockEntityCap::Unlimited,
            self.cull_sign_text,
            self.fast_shadows != FastShadowMode::Vanilla,
            self.distant_mob_throttle,
            self.hub_lag_shield,
            self.particle_budget != ParticleBudget::Vanilla16k,
            self.chaos_particle_shield,
            self.anim_throttle,
            self.glint_mode != GlintTrimMode::Full,
            self.auto_drs != AutoDrsMode::Off,
            self.dynamic_fps != DynamicIdleFps::Off,
            self.menu_bg_mode != MenuBgMode::PanoramaBlur,
            self.thermal_guard,
            self.volt_reflex,
            self.thread_boost,
            self.prewarm_pipelines,
            self.audio_voice_cap != AudioVoiceCap::Vanilla,
            self.memory_saver,
            self.active_anim_cull,
            self.lookahead_stream,
            self.fast_translucency,
            self.particle_frustum_cull,
        ] {
            if b {
                n += 1;
            }
        }
        n
    }

    /// Applies a full **Volt Boost** profile across Tier 1, Tier 2, and Tier 3 settings.
    pub fn apply_volt_preset(&mut self, preset: VoltBoostPreset) {
        self.volt_preset = preset;
        match preset {
            VoltBoostPreset::Off => {
                self.cull_leaves = false;
                self.frustum_priority_mesh = false;
                self.cave_sky_cull = false;
                self.chunk_upload_cap = ChunkUploadCap::Auto;
                self.fast_weather = false;
                self.distant_lod_boost = false;
                self.entity_cap = EntityCap::Unlimited;
                self.item_render_cap = ItemRenderCap::Unlimited;
                self.block_entity_cap = BlockEntityCap::Unlimited;
                self.cull_sign_text = false;
                self.fast_shadows = FastShadowMode::Vanilla;
                self.distant_mob_throttle = false;
                self.hub_lag_shield = false;
                self.particle_budget = ParticleBudget::Vanilla16k;
                self.chaos_particle_shield = false;
                self.anim_throttle = false;
                self.glint_mode = GlintTrimMode::Full;
                self.auto_drs = AutoDrsMode::Off;
                self.dynamic_fps = DynamicIdleFps::Off;
                self.menu_bg_mode = MenuBgMode::PanoramaBlur;
                self.thermal_guard = false;
                self.volt_reflex = false;
                self.thread_boost = false;
                self.prewarm_pipelines = false;
                self.audio_voice_cap = AudioVoiceCap::Vanilla;
                self.memory_saver = false;
                self.active_anim_cull = false;
                self.lookahead_stream = false;
                self.fast_translucency = false;
                self.particle_frustum_cull = false;
            }
            VoltBoostPreset::Balanced => {
                self.cull_leaves = true;
                self.frustum_priority_mesh = true;
                self.cave_sky_cull = true;
                self.chunk_upload_cap = ChunkUploadCap::Balanced16;
                self.fast_weather = false;
                self.distant_lod_boost = false;
                self.entity_cap = EntityCap::Cap128;
                self.item_render_cap = ItemRenderCap::Cap128;
                self.block_entity_cap = BlockEntityCap::Cap64;
                self.cull_sign_text = true;
                self.fast_shadows = FastShadowMode::Nearby16;
                self.distant_mob_throttle = true;
                self.hub_lag_shield = false;
                self.particle_budget = ParticleBudget::Cap4096;
                self.chaos_particle_shield = true;
                self.anim_throttle = false;
                self.glint_mode = GlintTrimMode::Full;
                self.auto_drs = AutoDrsMode::Off;
                self.dynamic_fps = DynamicIdleFps::Fps30;
                self.menu_bg_mode = MenuBgMode::FastDark;
                self.thermal_guard = true;
                self.volt_reflex = true;
                self.thread_boost = true;
                self.prewarm_pipelines = true;
                self.audio_voice_cap = AudioVoiceCap::Cap32;
                self.memory_saver = false;
                self.active_anim_cull = true;
                self.lookahead_stream = true;
                self.fast_translucency = true;
                self.particle_frustum_cull = true;
            }
            VoltBoostPreset::Competitive => {
                self.cull_leaves = true;
                self.frustum_priority_mesh = true;
                self.cave_sky_cull = true;
                self.chunk_upload_cap = ChunkUploadCap::Balanced16;
                self.fast_weather = true;
                self.distant_lod_boost = false;
                self.entity_cap = EntityCap::Cap96;
                self.item_render_cap = ItemRenderCap::Cap64;
                self.block_entity_cap = BlockEntityCap::Cap32;
                self.cull_sign_text = true;
                self.fast_shadows = FastShadowMode::Nearby16;
                self.distant_mob_throttle = true;
                self.hub_lag_shield = true;
                self.particle_budget = ParticleBudget::Cap2048;
                self.chaos_particle_shield = true;
                self.anim_throttle = false;
                self.glint_mode = GlintTrimMode::NearbyOnly;
                self.auto_drs = AutoDrsMode::Target120;
                self.dynamic_fps = DynamicIdleFps::Fps30;
                self.menu_bg_mode = MenuBgMode::FastDark;
                self.thermal_guard = true;
                self.volt_reflex = true;
                self.thread_boost = true;
                self.prewarm_pipelines = true;
                self.audio_voice_cap = AudioVoiceCap::Cap32;
                self.memory_saver = false;
                self.active_anim_cull = true;
                self.lookahead_stream = true;
                self.fast_translucency = true;
                self.particle_frustum_cull = true;
            }
            VoltBoostPreset::UltraFps => {
                self.cull_leaves = true;
                self.frustum_priority_mesh = true;
                self.cave_sky_cull = true;
                self.chunk_upload_cap = ChunkUploadCap::Smooth8;
                self.fast_weather = true;
                self.distant_lod_boost = false;
                self.entity_cap = EntityCap::Cap64;
                self.item_render_cap = ItemRenderCap::Cap32;
                self.block_entity_cap = BlockEntityCap::Cap16;
                self.cull_sign_text = true;
                self.fast_shadows = FastShadowMode::Off;
                self.distant_mob_throttle = true;
                self.hub_lag_shield = true;
                self.particle_budget = ParticleBudget::Cap512;
                self.chaos_particle_shield = true;
                self.anim_throttle = true;
                self.glint_mode = GlintTrimMode::Off;
                self.auto_drs = AutoDrsMode::Aggressive50;
                self.dynamic_fps = DynamicIdleFps::Fps15;
                self.menu_bg_mode = MenuBgMode::FastDark;
                self.thermal_guard = true;
                self.volt_reflex = true;
                self.thread_boost = true;
                self.prewarm_pipelines = true;
                self.audio_voice_cap = AudioVoiceCap::Cap16;
                self.memory_saver = true;
                self.active_anim_cull = true;
                self.lookahead_stream = true;
                self.fast_translucency = true;
                self.particle_frustum_cull = true;
            }
            VoltBoostPreset::BatterySaver => {
                self.cull_leaves = true;
                self.frustum_priority_mesh = true;
                self.cave_sky_cull = true;
                self.chunk_upload_cap = ChunkUploadCap::Smooth8;
                self.fast_weather = true;
                self.distant_lod_boost = false;
                self.entity_cap = EntityCap::Cap96;
                self.item_render_cap = ItemRenderCap::Cap64;
                self.block_entity_cap = BlockEntityCap::Cap32;
                self.cull_sign_text = true;
                self.fast_shadows = FastShadowMode::Off;
                self.distant_mob_throttle = true;
                self.hub_lag_shield = true;
                self.particle_budget = ParticleBudget::Cap2048;
                self.chaos_particle_shield = true;
                self.anim_throttle = true;
                self.glint_mode = GlintTrimMode::NearbyOnly;
                self.auto_drs = AutoDrsMode::Target60;
                self.dynamic_fps = DynamicIdleFps::Fps15;
                self.menu_bg_mode = MenuBgMode::FastDark;
                self.thermal_guard = true;
                self.volt_reflex = false;
                self.thread_boost = false;
                self.prewarm_pipelines = true;
                self.audio_voice_cap = AudioVoiceCap::Cap16;
                self.memory_saver = true;
                self.active_anim_cull = true;
                self.lookahead_stream = true;
                self.fast_translucency = true;
                self.particle_frustum_cull = true;
            }
            VoltBoostPreset::LowRam => {
                self.cull_leaves = true;
                self.frustum_priority_mesh = true;
                self.cave_sky_cull = true;
                self.chunk_upload_cap = ChunkUploadCap::Smooth4;
                self.fast_weather = true;
                self.distant_lod_boost = false;
                self.entity_cap = EntityCap::Cap64;
                self.item_render_cap = ItemRenderCap::Cap32;
                self.block_entity_cap = BlockEntityCap::Cap16;
                self.cull_sign_text = true;
                self.fast_shadows = FastShadowMode::Off;
                self.distant_mob_throttle = true;
                self.hub_lag_shield = true;
                self.particle_budget = ParticleBudget::Cap512;
                self.chaos_particle_shield = true;
                self.anim_throttle = true;
                self.glint_mode = GlintTrimMode::Off;
                self.auto_drs = AutoDrsMode::Target60;
                self.dynamic_fps = DynamicIdleFps::Fps15;
                self.menu_bg_mode = MenuBgMode::FastDark;
                self.thermal_guard = true;
                self.volt_reflex = true;
                self.thread_boost = true;
                self.prewarm_pipelines = false;
                self.audio_voice_cap = AudioVoiceCap::Cap16;
                self.memory_saver = true;
                self.active_anim_cull = true;
                self.lookahead_stream = false;
                self.fast_translucency = true;
                self.particle_frustum_cull = true;
            }
        }
    }

    /// Inspects hardware cores, pointer width, and `/proc/meminfo` total RAM to apply the best
    /// automatic Volt Boost profile for this device.
    pub fn auto_optimize_device(&mut self) -> VoltBoostPreset {
        let total_ram_mib = detect_total_ram_mib();
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let chosen =
            if cfg!(target_pointer_width = "32") || total_ram_mib.is_some_and(|m| m <= 4200) {
                VoltBoostPreset::LowRam
            } else if total_ram_mib.is_some_and(|m| m <= 6400) || cores <= 6 {
                VoltBoostPreset::UltraFps
            } else {
                VoltBoostPreset::Balanced
            };
        self.apply_volt_preset(chosen);
        if self.perf_hud == PerfHudMode::Off {
            self.perf_hud = PerfHudMode::Compact;
        }
        chosen
    }
}

pub fn detect_total_ram_mib() -> Option<u64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib / 1024);
        }
    }
    None
}

/// The field-of-view modifier target `Camera.tickFov` eases toward: vanilla's
/// (`getFieldOfViewModifier`) divided by a mod's zoom (`native_zoom`). A
/// zoom of 1 leaves it as is.
pub fn fov_target(vanilla: f32, zoom: f32) -> f32 {
    if zoom == 1.0 { vanilla } else { vanilla / zoom }
}
/// Frame times over the last few seconds: frames per second, the minimum
/// and the 1% low, recomputed twice a second.
pub struct FpsCounter {
    /// (end time in seconds, frame milliseconds), oldest first.
    frames: VecDeque<(f64, f32)>,
    shown: FpsReading,
    computed_at: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FpsReading {
    pub fps: u32,
    pub min: u32,
    pub low: u32,
}
/// Seconds the minimum and 1% low look back.
const LOW_WINDOW: f64 = 5.0;
impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            frames: VecDeque::new(),
            shown: FpsReading::default(),
            computed_at: f64::NEG_INFINITY,
        }
    }
}
impl FpsCounter {
    /// A frame that ended at `now` (seconds) and took `ms`.
    pub fn record(&mut self, now: f64, ms: f32) {
        self.frames.push_back((now, ms));
        while self
            .frames
            .front()
            .is_some_and(|&(t, _)| now - t > LOW_WINDOW)
        {
            self.frames.pop_front();
        }
        if now - self.computed_at >= 0.5 {
            self.computed_at = now;
            self.shown = self.compute(now);
        }
    }
    fn compute(&self, now: f64) -> FpsReading {
        let fps = self.frames.iter().filter(|&&(t, _)| now - t < 1.0).count() as u32;
        let mut times: Vec<f32> = self.frames.iter().map(|f| f.1).collect();
        if times.is_empty() {
            return FpsReading::default();
        }
        times.sort_by(|a, b| b.total_cmp(a));
        let to_fps = |ms: f32| (1000.0 / ms.max(0.001)).round() as u32;
        // The 1% low: the frame rate of the 99th percentile frame time.
        let low = times[(times.len() - 1) / 100];
        FpsReading {
            fps,
            min: to_fps(times[0]),
            low: to_fps(low),
        }
    }
    pub fn reading(&self) -> FpsReading {
        self.shown
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Clicks of one button in the last second.
#[derive(Default)]
pub struct Cps {
    clicks: VecDeque<f64>,
}
impl Cps {
    pub fn click(&mut self, now: f64) {
        self.clicks.push_back(now);
        self.forget(now);
    }
    fn forget(&mut self, now: f64) {
        while self.clicks.front().is_some_and(|&t| now - t >= 1.0) {
            self.clicks.pop_front();
        }
    }
    pub fn per_second(&mut self, now: f64) -> usize {
        self.forget(now);
        self.clicks.len()
    }
}
/// Two players sharing a window (`splitscreen_two`): halves this way.
pub const SPLIT_TWO: [&str; 3] = ["Auto", "Side by side", "Top and bottom"];
/// Three players sharing a window (`splitscreen_three`).
pub const SPLIT_THREE: [&str; 3] = ["One wide on top", "One tall on the left", "Side by side"];

/// The settings the Extras page shows, one per row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Fullbright,
    DynamicLights,
    OwnNameTag,
    HideBlockParticles,
    AutoReconnect,
    UiSoundReserve,
    /// The Music... page on Music & Sounds (`Music page`).
    MusicPage,
    MouseTweaks,
    MouseTweaksWheel,
    Sort,
    SortKey,
    /// A new chat line slides in and fades in (`Chat::layout`).
    ChatAnimation,
    /// The coloured bar at the left of a chat line.
    ChatIndicator,
    /// Not on the Extras page: Horizon Settings shows No AI, its AI Agent page Agent Control.
    NoAi,
    AgentControl,
    /// On the Horizon Settings page.
    DiscordPresence,
    /// On the AI Agent page, next to Agent Control: the F3 recording shortcuts.
    ReproKeys,
    /// On the Splitscreen settings page.
    Splitscreen,
    WheelCyclesSettings,
    /// Shown only while cosmetics are enabled (`horizon_cosmetics::enabled`).
    CosmeticsMine,
    CosmeticsOthers,
    /// The Volt Boost performance page (Tiers 1, 2 & 3).
    VoltPreset,
    VoltAutoOptimize,
    VoltPurgeRam,
    VoltPerfHud,
    VoltPerfHudCorner,
    VoltCullLeaves,
    VoltFrustumMesh,
    VoltCaveSkyCull,
    VoltChunkUploadCap,
    VoltFastWeather,
    VoltDistantLod,
    VoltEntityCap,
    VoltItemCap,
    VoltBlockEntityCap,
    VoltCullSignText,
    VoltFastShadows,
    VoltDistantMobThrottle,
    VoltHubLagShield,
    VoltParticleBudget,
    VoltChaosShield,
    VoltAnimThrottle,
    VoltGlintMode,
    VoltAutoDrs,
    VoltDynamicFps,
    VoltMenuBg,
    VoltThermalGuard,
    VoltReflex,
    VoltThreadBoost,
    VoltPrewarm,
    VoltAudioCap,
    VoltMemorySaver,
    VoltKeepAwake,
    VoltActiveAnimCull,
    VoltLookaheadStream,
    VoltFastTranslucency,
    VoltParticleFrustum,
}
/// The Extras page: headers (language keys) and items.
pub fn rows() -> Vec<Result<Item, &'static str>> {
    use Item::*;
    let mut rows = vec![
        Err("horizon.extras.header.lighting"),
        Ok(Fullbright),
        Ok(DynamicLights),
        Err("horizon.extras.header.third_person"),
        Ok(OwnNameTag),
        Err("horizon.extras.header.particles"),
        Ok(HideBlockParticles),
        Err("horizon.extras.header.inventory"),
        Ok(MouseTweaks),
        Ok(MouseTweaksWheel),
        Ok(Sort),
        Ok(SortKey),
        Err("horizon.extras.header.chat"),
        Ok(ChatAnimation),
        Ok(ChatIndicator),
        Err("horizon.extras.header.multiplayer"),
        Ok(AutoReconnect),
        Err("horizon.extras.header.sound"),
        Ok(UiSoundReserve),
        Ok(MusicPage),
        Err("horizon.extras.header.menus"),
        Ok(WheelCyclesSettings),
    ];
    if horizon_cosmetics::enabled() {
        rows.extend([
            Err("horizon.extras.header.cosmetics"),
            Ok(CosmeticsMine),
            Ok(CosmeticsOthers),
        ]);
    }
    rows
}
/// The Volt Boost performance and optimization suite rows (Tiers 1, 2 & 3).
pub fn volt_boost_rows() -> Vec<Result<Item, &'static str>> {
    use Item::*;
    vec![
        Err("horizon.extras.header.volt_boost_profiles"),
        Ok(VoltPreset),
        Ok(VoltAutoOptimize),
        Ok(VoltPerfHud),
        Ok(VoltPerfHudCorner),
        Ok(VoltPurgeRam),
        Err("horizon.extras.header.volt_boost_terrain"),
        Ok(VoltCullLeaves),
        Ok(VoltFrustumMesh),
        Ok(VoltLookaheadStream),
        Ok(VoltFastTranslucency),
        Ok(VoltCaveSkyCull),
        Ok(VoltChunkUploadCap),
        Ok(VoltFastWeather),
        Ok(VoltDistantLod),
        Err("horizon.extras.header.volt_boost_entities"),
        Ok(VoltEntityCap),
        Ok(VoltItemCap),
        Ok(VoltBlockEntityCap),
        Ok(VoltCullSignText),
        Ok(VoltFastShadows),
        Ok(VoltDistantMobThrottle),
        Ok(VoltHubLagShield),
        Err("horizon.extras.header.volt_boost_particles"),
        Ok(VoltParticleBudget),
        Ok(VoltParticleFrustum),
        Ok(VoltChaosShield),
        Ok(VoltAnimThrottle),
        Ok(VoltActiveAnimCull),
        Ok(VoltGlintMode),
        Err("horizon.extras.header.volt_boost_system"),
        Ok(VoltAutoDrs),
        Ok(VoltDynamicFps),
        Ok(VoltMenuBg),
        Ok(VoltThermalGuard),
        Ok(VoltReflex),
        Ok(VoltThreadBoost),
        Ok(VoltPrewarm),
        Ok(VoltAudioCap),
        Ok(VoltMemorySaver),
        Ok(VoltKeepAwake),
    ]
}
impl Item {
    /// The language key of the caption (`horizon.*`, assets/horizon/lang).
    pub fn caption_key(self) -> &'static str {
        match self {
            Item::Fullbright => "horizon.extras.fullbright",
            Item::DynamicLights => "horizon.extras.dynamic_lights",
            Item::OwnNameTag => "horizon.extras.own_name_tag",
            Item::HideBlockParticles => "horizon.extras.hide_block_particles",
            Item::AutoReconnect => "horizon.extras.auto_reconnect",
            Item::UiSoundReserve => "horizon.extras.ui_sound_reserve",
            Item::MusicPage => "horizon.extras.music_page",
            Item::MouseTweaks => "horizon.extras.mouse_tweaks",
            Item::MouseTweaksWheel => "horizon.extras.mouse_tweaks_wheel",
            Item::Sort => "horizon.extras.sort",
            Item::SortKey => "horizon.extras.sort_key",
            Item::NoAi => "horizon.extras.no_ai",
            Item::AgentControl => "horizon.extras.agent_control",
            Item::ReproKeys => "horizon.extras.repro_keys",
            Item::DiscordPresence => "horizon.extras.discord_presence",
            Item::Splitscreen => "horizon.extras.splitscreen",
            Item::ChatAnimation => "horizon.extras.chat_animation",
            Item::ChatIndicator => "horizon.extras.chat_indicator",
            Item::WheelCyclesSettings => "horizon.extras.wheel_cycles_settings",
            Item::CosmeticsMine => "horizon.extras.cosmetics_mine",
            Item::CosmeticsOthers => "horizon.extras.cosmetics_others",
            Item::VoltPreset => "horizon.extras.volt_preset",
            Item::VoltAutoOptimize => "horizon.extras.volt_auto_optimize",
            Item::VoltPurgeRam => "horizon.extras.volt_purge_ram",
            Item::VoltPerfHud => "horizon.extras.volt_perf_hud",
            Item::VoltPerfHudCorner => "horizon.extras.volt_perf_hud_corner",
            Item::VoltCullLeaves => "horizon.extras.volt_cull_leaves",
            Item::VoltFrustumMesh => "horizon.extras.volt_frustum_mesh",
            Item::VoltCaveSkyCull => "horizon.extras.volt_cave_sky_cull",
            Item::VoltChunkUploadCap => "horizon.extras.volt_chunk_upload_cap",
            Item::VoltFastWeather => "horizon.extras.volt_fast_weather",
            Item::VoltDistantLod => "horizon.extras.volt_distant_lod",
            Item::VoltEntityCap => "horizon.extras.volt_entity_cap",
            Item::VoltItemCap => "horizon.extras.volt_item_cap",
            Item::VoltBlockEntityCap => "horizon.extras.volt_block_entity_cap",
            Item::VoltCullSignText => "horizon.extras.volt_cull_sign_text",
            Item::VoltFastShadows => "horizon.extras.volt_fast_shadows",
            Item::VoltDistantMobThrottle => "horizon.extras.volt_distant_mob_throttle",
            Item::VoltHubLagShield => "horizon.extras.volt_hub_lag_shield",
            Item::VoltParticleBudget => "horizon.extras.volt_particle_budget",
            Item::VoltChaosShield => "horizon.extras.volt_chaos_shield",
            Item::VoltAnimThrottle => "horizon.extras.volt_anim_throttle",
            Item::VoltGlintMode => "horizon.extras.volt_glint_mode",
            Item::VoltAutoDrs => "horizon.extras.volt_auto_drs",
            Item::VoltDynamicFps => "horizon.extras.volt_dynamic_fps",
            Item::VoltMenuBg => "horizon.extras.volt_menu_bg",
            Item::VoltThermalGuard => "horizon.extras.volt_thermal_guard",
            Item::VoltReflex => "horizon.extras.volt_reflex",
            Item::VoltThreadBoost => "horizon.extras.volt_thread_boost",
            Item::VoltPrewarm => "horizon.extras.volt_prewarm",
            Item::VoltAudioCap => "horizon.extras.volt_audio_cap",
            Item::VoltMemorySaver => "horizon.extras.volt_memory_saver",
            Item::VoltKeepAwake => "horizon.extras.volt_keep_awake",
            Item::VoltActiveAnimCull => "horizon.extras.volt_active_anim_cull",
            Item::VoltLookaheadStream => "horizon.extras.volt_lookahead_stream",
            Item::VoltFastTranslucency => "horizon.extras.volt_fast_translucency",
            Item::VoltParticleFrustum => "horizon.extras.volt_particle_frustum",
        }
    }
    /// The language key of the description shown as the row's tooltip.
    pub fn description_key(self) -> &'static str {
        match self {
            Item::Fullbright => "horizon.extras.fullbright.tooltip",
            Item::DynamicLights => "horizon.extras.dynamic_lights.tooltip",
            Item::OwnNameTag => "horizon.extras.own_name_tag.tooltip",
            Item::HideBlockParticles => "horizon.extras.hide_block_particles.tooltip",
            Item::AutoReconnect => "horizon.extras.auto_reconnect.tooltip",
            Item::UiSoundReserve => "horizon.extras.ui_sound_reserve.tooltip",
            Item::MusicPage => "horizon.extras.music_page.tooltip",
            Item::MouseTweaks => "horizon.extras.mouse_tweaks.tooltip",
            Item::MouseTweaksWheel => "horizon.extras.mouse_tweaks_wheel.tooltip",
            Item::Sort => "horizon.extras.sort.tooltip",
            Item::SortKey => "horizon.extras.sort_key.tooltip",
            Item::NoAi => "horizon.extras.no_ai.tooltip",
            Item::AgentControl => "horizon.extras.agent_control.tooltip",
            Item::ReproKeys => "horizon.extras.repro_keys.tooltip",
            Item::DiscordPresence => "horizon.extras.discord_presence.tooltip",
            Item::Splitscreen => "horizon.extras.splitscreen.tooltip",
            Item::ChatAnimation => "horizon.extras.chat_animation.tooltip",
            Item::ChatIndicator => "horizon.extras.chat_indicator.tooltip",
            Item::WheelCyclesSettings => "horizon.extras.wheel_cycles_settings.tooltip",
            Item::CosmeticsMine => "horizon.extras.cosmetics_mine.tooltip",
            Item::CosmeticsOthers => "horizon.extras.cosmetics_others.tooltip",
            Item::VoltPreset => "horizon.extras.volt_preset.tooltip",
            Item::VoltAutoOptimize => "horizon.extras.volt_auto_optimize.tooltip",
            Item::VoltPurgeRam => "horizon.extras.volt_purge_ram.tooltip",
            Item::VoltPerfHud => "horizon.extras.volt_perf_hud.tooltip",
            Item::VoltPerfHudCorner => "horizon.extras.volt_perf_hud_corner.tooltip",
            Item::VoltCullLeaves => "horizon.extras.volt_cull_leaves.tooltip",
            Item::VoltFrustumMesh => "horizon.extras.volt_frustum_mesh.tooltip",
            Item::VoltCaveSkyCull => "horizon.extras.volt_cave_sky_cull.tooltip",
            Item::VoltChunkUploadCap => "horizon.extras.volt_chunk_upload_cap.tooltip",
            Item::VoltFastWeather => "horizon.extras.volt_fast_weather.tooltip",
            Item::VoltDistantLod => "horizon.extras.volt_distant_lod.tooltip",
            Item::VoltEntityCap => "horizon.extras.volt_entity_cap.tooltip",
            Item::VoltItemCap => "horizon.extras.volt_item_cap.tooltip",
            Item::VoltBlockEntityCap => "horizon.extras.volt_block_entity_cap.tooltip",
            Item::VoltCullSignText => "horizon.extras.volt_cull_sign_text.tooltip",
            Item::VoltFastShadows => "horizon.extras.volt_fast_shadows.tooltip",
            Item::VoltDistantMobThrottle => "horizon.extras.volt_distant_mob_throttle.tooltip",
            Item::VoltHubLagShield => "horizon.extras.volt_hub_lag_shield.tooltip",
            Item::VoltParticleBudget => "horizon.extras.volt_particle_budget.tooltip",
            Item::VoltChaosShield => "horizon.extras.volt_chaos_shield.tooltip",
            Item::VoltAnimThrottle => "horizon.extras.volt_anim_throttle.tooltip",
            Item::VoltGlintMode => "horizon.extras.volt_glint_mode.tooltip",
            Item::VoltAutoDrs => "horizon.extras.volt_auto_drs.tooltip",
            Item::VoltDynamicFps => "horizon.extras.volt_dynamic_fps.tooltip",
            Item::VoltMenuBg => "horizon.extras.volt_menu_bg.tooltip",
            Item::VoltThermalGuard => "horizon.extras.volt_thermal_guard.tooltip",
            Item::VoltReflex => "horizon.extras.volt_reflex.tooltip",
            Item::VoltThreadBoost => "horizon.extras.volt_thread_boost.tooltip",
            Item::VoltPrewarm => "horizon.extras.volt_prewarm.tooltip",
            Item::VoltAudioCap => "horizon.extras.volt_audio_cap.tooltip",
            Item::VoltMemorySaver => "horizon.extras.volt_memory_saver.tooltip",
            Item::VoltKeepAwake => "horizon.extras.volt_keep_awake.tooltip",
            Item::VoltActiveAnimCull => "horizon.extras.volt_active_anim_cull.tooltip",
            Item::VoltLookaheadStream => "horizon.extras.volt_lookahead_stream.tooltip",
            Item::VoltFastTranslucency => "horizon.extras.volt_fast_translucency.tooltip",
            Item::VoltParticleFrustum => "horizon.extras.volt_particle_frustum.tooltip",
        }
    }
    /// The caption in the current language (English when the key is missing).
    pub fn caption_text(self) -> String {
        crate::lang::get_or(self.caption_key(), self.caption())
    }
    /// The description in the current language (English when the key is missing).
    pub fn description_text(self) -> String {
        crate::lang::get_or(self.description_key(), self.description())
    }
    /// The English caption: the fallback of [`Item::caption_text`], and the text of
    /// `horizon.extras.*` in en_us.json.
    pub fn caption(self) -> &'static str {
        match self {
            Item::Fullbright => "Fullbright",
            Item::DynamicLights => "Dynamic lights",
            Item::OwnNameTag => "Own name tag",
            Item::HideBlockParticles => "Hide block-break particles",
            Item::AutoReconnect => "Auto-reconnect",
            Item::UiSoundReserve => "Keep channels for UI sounds",
            Item::MusicPage => "Music page",
            Item::MouseTweaks => "Mouse Tweaks controls",
            Item::MouseTweaksWheel => "Scroll wheel moves items",
            Item::Sort => "Sort buttons",
            Item::SortKey => "Sort key",
            Item::ChatAnimation => "Smooth chat",
            Item::ChatIndicator => "Message indicator",
            Item::NoAi => "No AI",
            Item::AgentControl => "Agent Control",
            Item::ReproKeys => "Recording shortcuts",
            Item::DiscordPresence => "Discord Rich Presence",
            Item::Splitscreen => "Splitscreen",
            Item::WheelCyclesSettings => "Scroll wheel changes settings",
            Item::CosmeticsMine => "My cosmetics",
            Item::CosmeticsOthers => "Other players' cosmetics",
            Item::VoltPreset => "Boost Profile",
            Item::VoltAutoOptimize => "Auto-Tune Device",
            Item::VoltPurgeRam => "Purge Unused RAM",
            Item::VoltPerfHud => "Live Perf HUD",
            Item::VoltPerfHudCorner => "Perf HUD Corner",
            Item::VoltCullLeaves => "Smart Leaf Cull",
            Item::VoltFrustumMesh => "Frustum-First Mesh",
            Item::VoltCaveSkyCull => "Cave Sky Cull",
            Item::VoltChunkUploadCap => "Chunk Upload Cap",
            Item::VoltFastWeather => "Fast Weather",
            Item::VoltDistantLod => "Distant LOD Boost",
            Item::VoltEntityCap => "Max Entities",
            Item::VoltItemCap => "Dropped Item Cap",
            Item::VoltBlockEntityCap => "Block-Entity Cap",
            Item::VoltCullSignText => "Sign Text Cull",
            Item::VoltFastShadows => "Entity Shadows",
            Item::VoltDistantMobThrottle => "Mob Pose Throttle",
            Item::VoltHubLagShield => "Hub Lag Shield",
            Item::VoltParticleBudget => "Particle Budget",
            Item::VoltChaosShield => "Burst Lag Shield",
            Item::VoltAnimThrottle => "Smart 10Hz Atlas",
            Item::VoltGlintMode => "Glint & Trim Mode",
            Item::VoltAutoDrs => "Auto-DRS Scale",
            Item::VoltDynamicFps => "Dynamic Menu FPS",
            Item::VoltMenuBg => "Menu Backdrop",
            Item::VoltThermalGuard => "Thermal Guard",
            Item::VoltReflex => "Volt Reflex",
            Item::VoltThreadBoost => "big.LITTLE Boost",
            Item::VoltPrewarm => "Shader Pre-Warm",
            Item::VoltAudioCap => "Audio Voice Cap",
            Item::VoltMemorySaver => "RAM Compactor",
            Item::VoltKeepAwake => "Keep Screen Awake",
            Item::VoltActiveAnimCull => "Active-Only Sprite Anim",
            Item::VoltLookaheadStream => "Velocity Lookahead Stream",
            Item::VoltFastTranslucency => "Adaptive Water Sort",
            Item::VoltParticleFrustum => "Frustum Particle Cull",
        }
    }
    /// The switch this setting belongs to: it does nothing while that is off.
    pub fn parent(self) -> Option<Item> {
        use Item::*;
        Some(match self {
            UiSoundReserve => return None,
            MouseTweaksWheel => MouseTweaks,
            VoltPerfHudCorner => VoltPerfHud,
            SortKey => Sort,
            ChatIndicator => ChatAnimation,
            Fullbright
            | DynamicLights
            | OwnNameTag
            | HideBlockParticles
            | MouseTweaks
            | Sort
            | AutoReconnect
            | NoAi
            | AgentControl
            | ReproKeys
            | DiscordPresence
            | Splitscreen
            | ChatAnimation
            | WheelCyclesSettings
            | MusicPage
            | CosmeticsMine
            | CosmeticsOthers
            | VoltPreset
            | VoltAutoOptimize
            | VoltPurgeRam
            | VoltPerfHud
            | VoltCullLeaves
            | VoltFrustumMesh
            | VoltCaveSkyCull
            | VoltChunkUploadCap
            | VoltFastWeather
            | VoltDistantLod
            | VoltEntityCap
            | VoltItemCap
            | VoltBlockEntityCap
            | VoltCullSignText
            | VoltFastShadows
            | VoltDistantMobThrottle
            | VoltHubLagShield
            | VoltParticleBudget
            | VoltChaosShield
            | VoltAnimThrottle
            | VoltGlintMode
            | VoltAutoDrs
            | VoltDynamicFps
            | VoltMenuBg
            | VoltThermalGuard
            | VoltReflex
            | VoltThreadBoost
            | VoltPrewarm
            | VoltAudioCap
            | VoltMemorySaver
            | VoltKeepAwake
            | VoltActiveAnimCull
            | VoltLookaheadStream
            | VoltFastTranslucency
            | VoltParticleFrustum => {
                return None;
            }
        })
    }
    /// The English description: the fallback of [`Item::description_text`].
    pub fn description(self) -> &'static str {
        match self {
            Item::Fullbright => "Show blocks and entities at full light, even in dark areas.",
            Item::DynamicLights => {
                "Torches and other light sources held, dropped or on fire light the blocks around them."
            }
            Item::OwnNameTag => {
                "Shows your name above your head in third person, as other players see it. Hidden when the server hides your team's names or you're invisible."
            }
            Item::HideBlockParticles => {
                "Hides the debris particles that burst from a broken block and crumble off the one you mine. Other particles stay."
            }
            Item::AutoReconnect => {
                "Rejoins a server after a lost connection or a restart, never after a kick or ban."
            }
            Item::UiSoundReserve => {
                "Sounds in the world leave 16 of the 247 sound channels free, so a flood of them (a big explosion) can't silence menu clicks."
            }
            Item::MusicPage => {
                "Music & Sounds gets a Music... page: play any built-in track now, or turn one off so the game stops picking it. Nothing changes until you turn a track off."
            }
            Item::MouseTweaks => {
                "Right-drag places one per slot, left-drag gathers the held item, Shift + left-drag moves every slot."
            }
            Item::MouseTweaksWheel => {
                "Down sends one item of the slot to the other side, up brings one back."
            }
            Item::Sort => {
                "Sorts the inventory, chests, barrels, ender chests and shulker boxes in creative tab order, merging stacks."
            }
            Item::SortKey => {
                "Sorts the part under the cursor. Click, then press a key; Escape unbinds it."
            }
            Item::ChatAnimation => {
                "A new chat line slides in from the left while it fades in, instead of appearing whole."
            }
            Item::ChatIndicator => "The coloured bar at the left of a chat line.",
            Item::NoAi => {
                "Hides and turns off every AI feature: agent control, horizon-ctl and the MCP servers. The launcher's switch too."
            }
            Item::DiscordPresence => {
                "Shows Playing Horizon on your Discord profile while Discord runs: menus, single player or multiplayer, the dimension and the time played. Never a world's name or a server's address."
            }
            Item::Splitscreen => {
                "In a world, another controller can join as a new player: press A when it asks, then pick a profile and where to play."
            }
            Item::AgentControl => {
                "Your own AI agent can play this game through horizon-ctl or an MCP client, on this computer only."
            }
            Item::ReproKeys => {
                "F3+R starts or stops a repro recording and F3+E writes its bundle. Off by default: a recording copies the world."
            }
            Item::CosmeticsMine => {
                "Shows the cosmetics you wear on your own player, in third person and the inventory."
            }
            Item::CosmeticsOthers => {
                "Shows the cosmetics other Horizon players wear. Off, the game never asks what they wear."
            }
            Item::VoltPreset => {
                "Applies a curated one-tap Optimization profile (OFF, Balanced, Competitive PvP, Ultra FPS, Battery Saver or Low-RAM Rescue)."
            }
            Item::VoltAutoOptimize => {
                "Inspects your device's CPU core count, 32/64-bit ABI and system RAM to automatically configure the optimal Optimization profile."
            }
            Item::VoltPurgeRam => {
                "Immediately compacts transient caches, particle buffers and unused OS heap pages to reclaim resident RAM."
            }
            Item::VoltPerfHud => {
                "Shows a sleek crimson telemetry overlay with live FPS, 1% lows, frame pacing, active entities, chunks and resident RAM."
            }
            Item::VoltPerfHudCorner => {
                "Screen corner where the live Optimization performance HUD docks."
            }
            Item::VoltCullLeaves => {
                "Skips hidden interior leaf-to-leaf faces inside dense jungles and forests while keeping outer tree canopies crisp."
            }
            Item::VoltFrustumMesh => {
                "Prioritizes chunk GPU uploads and meshing inside your camera view cone first so turning never reveals blank terrain."
            }
            Item::VoltCaveSkyCull => {
                "Skips sky dome, celestial bodies and cloud rendering passes when enclosed deep underground with zero sky light."
            }
            Item::VoltChunkUploadCap => {
                "Caps chunk mesh GPU uploads per frame (Auto, 4, 8, 16 or 32) to eliminate camera-turn frame spikes on mobile GPUs."
            }
            Item::VoltFastWeather => {
                "Reduces rain and snow column overdraw radius so heavy storms stay smooth on tile-based mobile GPUs."
            }
            Item::VoltDistantLod => {
                "Shifts outer render-distance rings into Horizon's low-cost distant LOD band so wide horizons use a fraction of RAM and triangles."
            }
            Item::VoltEntityCap => {
                "Renders nearest entities first and caps distant crowd rendering so mob farms and busy hubs never tank FPS."
            }
            Item::VoltItemCap => {
                "Caps rendered ground item stacks and trims distant item drops after big explosions or farm harvests."
            }
            Item::VoltBlockEntityCap => {
                "Occludes chests, signs, skulls, banners and shulkers behind solid walls and caps simultaneous block-entity draws."
            }
            Item::VoltCullSignText => {
                "Skips expensive glyph layout on signs and floating holograms beyond 16 blocks while keeping the sign model visible."
            }
            Item::VoltFastShadows => {
                "Limits entity contact shadows to nearby 16m targets or turns them off completely."
            }
            Item::VoltDistantMobThrottle => {
                "Simplifies distant entity feature overlays and pose work beyond 24 blocks."
            }
            Item::VoltHubLagShield => {
                "Automatically trims distant crowd armor stands, pets and players beyond 16m whenever more than 24 entities pack into one area."
            }
            Item::VoltParticleBudget => {
                "Caps active particles (16K Vanilla, 4,096, 2,048, 512 or OFF) and culls tiny distant particles."
            }
            Item::VoltChaosShield => {
                "Clamps TNT/crystal explosion debris, rapid Efficiency V block-break spray and crowded firework bursts."
            }
            Item::VoltAnimThrottle => {
                "Steps animated lava, water, fire and portal atlas uploads at 10 Hz instead of 20 Hz to halve GPU texture-copy bandwidth."
            }
            Item::VoltGlintMode => {
                "Simplifies enchantment glint shaders to held/nearby items only or disables glint overdraw completely."
            }
            Item::VoltAutoDrs => {
                "Dynamically adjusts 3D render scale to hold 60 or 120 FPS during heavy scenes while keeping UI and crosshair 100% native sharp."
            }
            Item::VoltDynamicFps => {
                "Automatically drops frame rate to 30 or 15 FPS while paused, in menus or AFK to save battery and cool down the SoC."
            }
            Item::VoltMenuBg => {
                "Replaces multi-pass menu blur with an instant zero-GPU crimson-dark backdrop."
            }
            Item::VoltThermalGuard => {
                "Detects sustained mobile thermal frame-time spikes and smoothly steps down render scale and particle load before stutter hits."
            }
            Item::VoltReflex => {
                "Minimizes render-ahead queue latency and paces touch camera sampling right before frame submission."
            }
            Item::VoltThreadBoost => {
                "Boosts render/audio threads onto prime big.LITTLE cores with 50us timer slack and coalesces background chunk workers."
            }
            Item::VoltPrewarm => {
                "Pre-warms gameplay caches and graphics pipelines during startup so entering a world has zero first-frame shader hitches."
            }
            Item::VoltAudioCap => {
                "Caps simultaneous sound mixer bursts per tick so mob farms, redstone machines and TNT chains never crackle or stall."
            }
            Item::VoltMemorySaver => {
                "Aggressively shrinks transient buffers, recycles unloaded chunk capacity and returns freed pages to Android immediately."
            }
            Item::VoltKeepAwake => {
                "Keeps the mobile display awake during asset downloads, world generation and active sessions."
            }
            Item::VoltActiveAnimCull => {
                "Skips GPU compute and mip-blending for animated block sprites that are not present in any resident chunk."
            }
            Item::VoltLookaheadStream => {
                "Prioritizes chunk generation, lighting and meshing ahead of your movement and camera direction."
            }
            Item::VoltFastTranslucency => {
                "Uses distance-adaptive hysteresis when re-sorting distant translucent water and glass sections."
            }
            Item::VoltParticleFrustum => {
                "Rejects off-screen and behind-camera particles before sprite lookup, lighting and billboard generation."
            }
            Item::WheelCyclesSettings => {
                "The scroll wheel over a button in the settings changes its value, as in vanilla. Off, the wheel only scrolls the list."
            }
        }
    }
    /// The frame-time cost the main switch of an extra shows.
    pub fn cost(self) -> Option<crate::graphics_style::Cost> {
        use crate::graphics_style::Cost;
        match self {
            Item::Fullbright => Some(Cost::None),
            Item::UiSoundReserve => Some(Cost::None),
            Item::MusicPage => Some(Cost::None),
            Item::DynamicLights => Some(Cost::Low),
            Item::ChatAnimation => Some(Cost::None),
            Item::OwnNameTag
            | Item::HideBlockParticles
            | Item::AutoReconnect
            | Item::MouseTweaks
            | Item::Sort
            | Item::Splitscreen
            | Item::WheelCyclesSettings => Some(Cost::None),
            Item::CosmeticsMine | Item::CosmeticsOthers => Some(Cost::Low),
            _ => None,
        }
    }
    /// On/off items and their value.
    pub fn switch(self, e: &Extras) -> Option<bool> {
        Some(match self {
            Item::Fullbright => e.fullbright,
            Item::DynamicLights => e.dynamic_lights,
            Item::OwnNameTag => e.own_name_tag,
            Item::HideBlockParticles => e.hide_block_particles,
            Item::AutoReconnect => e.auto_reconnect,
            Item::UiSoundReserve => e.ui_sound_reserve,
            Item::MusicPage => e.music_page,
            Item::MouseTweaks => e.mouse_tweaks,
            Item::MouseTweaksWheel => e.mouse_tweaks_wheel,
            Item::Sort => e.sort,
            Item::ChatAnimation => e.chat_animation,
            Item::ChatIndicator => e.chat_indicator,
            Item::NoAi => e.no_ai,
            Item::AgentControl => e.agent_control,
            Item::ReproKeys => e.repro_keys,
            Item::DiscordPresence => e.discord_presence,
            Item::Splitscreen => e.splitscreen,
            Item::WheelCyclesSettings => e.wheel_cycles_settings,
            Item::CosmeticsMine => e.cosmetics_mine,
            Item::CosmeticsOthers => e.cosmetics_others,
            Item::VoltCullLeaves => e.cull_leaves,
            Item::VoltFrustumMesh => e.frustum_priority_mesh,
            Item::VoltCaveSkyCull => e.cave_sky_cull,
            Item::VoltFastWeather => e.fast_weather,
            Item::VoltDistantLod => e.distant_lod_boost,
            Item::VoltCullSignText => e.cull_sign_text,
            Item::VoltDistantMobThrottle => e.distant_mob_throttle,
            Item::VoltHubLagShield => e.hub_lag_shield,
            Item::VoltChaosShield => e.chaos_particle_shield,
            Item::VoltAnimThrottle => e.anim_throttle,
            Item::VoltThermalGuard => e.thermal_guard,
            Item::VoltReflex => e.volt_reflex,
            Item::VoltThreadBoost => e.thread_boost,
            Item::VoltPrewarm => e.prewarm_pipelines,
            Item::VoltMemorySaver => e.memory_saver,
            Item::VoltKeepAwake => e.keep_screen_awake,
            Item::VoltActiveAnimCull => e.active_anim_cull,
            Item::VoltLookaheadStream => e.lookahead_stream,
            Item::VoltFastTranslucency => e.fast_translucency,
            Item::VoltParticleFrustum => e.particle_frustum_cull,
            _ => return None,
        })
    }
    /// The value of the other items, as their button shows it.
    pub fn value(self, e: &Extras) -> String {
        match self {
            Item::SortKey => crate::keybinds::display_name(&e.sort_key).plain(),
            Item::VoltPreset => e.volt_preset.label().into(),
            Item::VoltAutoOptimize => {
                let n = e.active_volt_optimizations();
                if n == 0 {
                    "Tap to Tune".into()
                } else {
                    format!("{n} Active")
                }
            }
            Item::VoltPurgeRam => "Purge Now".into(),
            Item::VoltPerfHud => match e.perf_hud {
                PerfHudMode::Off => "Off".into(),
                PerfHudMode::Compact => "Compact".into(),
                PerfHudMode::Full => "Full Graph".into(),
            },
            Item::VoltPerfHudCorner => e.perf_hud_corner.label().into(),
            Item::VoltChunkUploadCap => e.chunk_upload_cap.label().into(),
            Item::VoltEntityCap => e.entity_cap.label().into(),
            Item::VoltItemCap => e.item_render_cap.label().into(),
            Item::VoltBlockEntityCap => e.block_entity_cap.label().into(),
            Item::VoltFastShadows => match e.fast_shadows {
                FastShadowMode::Vanilla => "Vanilla",
                FastShadowMode::Nearby16 => "Nearby 16m",
                FastShadowMode::Off => "Off",
            }
            .into(),
            Item::VoltParticleBudget => e.particle_budget.label().into(),
            Item::VoltGlintMode => match e.glint_mode {
                GlintTrimMode::Full => "Full",
                GlintTrimMode::NearbyOnly => "Nearby Only",
                GlintTrimMode::Off => "Off",
            }
            .into(),
            Item::VoltAutoDrs => e.auto_drs.label().into(),
            Item::VoltDynamicFps => e.dynamic_fps.label().into(),
            Item::VoltMenuBg => match e.menu_bg_mode {
                MenuBgMode::PanoramaBlur => "Blur",
                MenuBgMode::FastDark => "Fast Dark",
            }
            .into(),
            Item::VoltAudioCap => e.audio_voice_cap.label().into(),
            other => match other.switch(e) {
                Some(true) => crate::lang::get_or("On", "On"),
                _ => crate::lang::get_or("Off", "Off"),
            },
        }
    }
    /// A click (`_back` for the right button: every item left is a switch,
    /// so both flip it). The sort key binds instead.
    pub fn cycle(self, e: &mut Extras, back: bool) {
        fn step<T: Copy + PartialEq>(slice: &[T], cur: T, back: bool) -> T {
            let idx = slice.iter().position(|&v| v == cur).unwrap_or(0);
            slice[if back {
                (idx + slice.len() - 1) % slice.len()
            } else {
                (idx + 1) % slice.len()
            }]
        }
        match self {
            Item::Fullbright => e.fullbright = !e.fullbright,
            Item::DynamicLights => e.dynamic_lights = !e.dynamic_lights,
            Item::OwnNameTag => e.own_name_tag = !e.own_name_tag,
            Item::HideBlockParticles => e.hide_block_particles = !e.hide_block_particles,
            Item::AutoReconnect => e.auto_reconnect = !e.auto_reconnect,
            Item::UiSoundReserve => e.ui_sound_reserve = !e.ui_sound_reserve,
            Item::MusicPage => e.music_page = !e.music_page,
            Item::MouseTweaks => e.mouse_tweaks = !e.mouse_tweaks,
            Item::MouseTweaksWheel => e.mouse_tweaks_wheel = !e.mouse_tweaks_wheel,
            Item::Sort => e.sort = !e.sort,
            Item::ChatAnimation => e.chat_animation = !e.chat_animation,
            Item::ChatIndicator => e.chat_indicator = !e.chat_indicator,
            Item::NoAi => e.no_ai = !e.no_ai,
            Item::AgentControl => e.agent_control = !e.agent_control,
            Item::ReproKeys => e.repro_keys = !e.repro_keys,
            Item::DiscordPresence => e.discord_presence = !e.discord_presence,
            Item::Splitscreen => e.splitscreen = !e.splitscreen,
            Item::WheelCyclesSettings => e.wheel_cycles_settings = !e.wheel_cycles_settings,
            Item::CosmeticsMine => e.cosmetics_mine = !e.cosmetics_mine,
            Item::CosmeticsOthers => e.cosmetics_others = !e.cosmetics_others,
            Item::VoltPreset => {
                let next = step(&VoltBoostPreset::ALL, e.volt_preset, back);
                e.apply_volt_preset(next);
            }
            Item::VoltAutoOptimize => {
                e.auto_optimize_device();
            }
            Item::VoltPurgeRam => {
                e.purge_ram_requested = true;
                horizon_threads::release_free_memory();
            }
            Item::VoltPerfHud => e.perf_hud = step(&PerfHudMode::ALL, e.perf_hud, back),
            Item::VoltPerfHudCorner => {
                e.perf_hud_corner = step(&Corner::ALL, e.perf_hud_corner, back)
            }
            Item::VoltCullLeaves => e.cull_leaves = !e.cull_leaves,
            Item::VoltFrustumMesh => e.frustum_priority_mesh = !e.frustum_priority_mesh,
            Item::VoltCaveSkyCull => e.cave_sky_cull = !e.cave_sky_cull,
            Item::VoltChunkUploadCap => {
                e.chunk_upload_cap = step(&ChunkUploadCap::ALL, e.chunk_upload_cap, back)
            }
            Item::VoltFastWeather => e.fast_weather = !e.fast_weather,
            Item::VoltDistantLod => e.distant_lod_boost = !e.distant_lod_boost,
            Item::VoltEntityCap => e.entity_cap = step(&EntityCap::ALL, e.entity_cap, back),
            Item::VoltItemCap => {
                e.item_render_cap = step(&ItemRenderCap::ALL, e.item_render_cap, back)
            }
            Item::VoltBlockEntityCap => {
                e.block_entity_cap = step(&BlockEntityCap::ALL, e.block_entity_cap, back)
            }
            Item::VoltCullSignText => e.cull_sign_text = !e.cull_sign_text,
            Item::VoltFastShadows => {
                e.fast_shadows = step(&FastShadowMode::ALL, e.fast_shadows, back)
            }
            Item::VoltDistantMobThrottle => e.distant_mob_throttle = !e.distant_mob_throttle,
            Item::VoltHubLagShield => e.hub_lag_shield = !e.hub_lag_shield,
            Item::VoltParticleBudget => {
                e.particle_budget = step(&ParticleBudget::ALL, e.particle_budget, back)
            }
            Item::VoltChaosShield => e.chaos_particle_shield = !e.chaos_particle_shield,
            Item::VoltAnimThrottle => e.anim_throttle = !e.anim_throttle,
            Item::VoltGlintMode => e.glint_mode = step(&GlintTrimMode::ALL, e.glint_mode, back),
            Item::VoltAutoDrs => e.auto_drs = step(&AutoDrsMode::ALL, e.auto_drs, back),
            Item::VoltDynamicFps => e.dynamic_fps = step(&DynamicIdleFps::ALL, e.dynamic_fps, back),
            Item::VoltMenuBg => e.menu_bg_mode = step(&MenuBgMode::ALL, e.menu_bg_mode, back),
            Item::VoltThermalGuard => e.thermal_guard = !e.thermal_guard,
            Item::VoltReflex => e.volt_reflex = !e.volt_reflex,
            Item::VoltThreadBoost => e.thread_boost = !e.thread_boost,
            Item::VoltPrewarm => e.prewarm_pipelines = !e.prewarm_pipelines,
            Item::VoltAudioCap => {
                e.audio_voice_cap = step(&AudioVoiceCap::ALL, e.audio_voice_cap, back)
            }
            Item::VoltMemorySaver => e.memory_saver = !e.memory_saver,
            Item::VoltKeepAwake => e.keep_screen_awake = !e.keep_screen_awake,
            Item::VoltActiveAnimCull => e.active_anim_cull = !e.active_anim_cull,
            Item::VoltLookaheadStream => e.lookahead_stream = !e.lookahead_stream,
            Item::VoltFastTranslucency => e.fast_translucency = !e.fast_translucency,
            Item::VoltParticleFrustum => e.particle_frustum_cull = !e.particle_frustum_cull,
            Item::SortKey => {}
        }
    }
    /// The key name the options screen waits on for a key item.
    pub fn key_name(self) -> Option<&'static str> {
        match self {
            Item::SortKey => Some(SORT_KEY),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_off_by_default_and_round_trips() {
        let d = Extras::default();
        assert!(!d.fullbright && !d.dynamic_lights);
        assert!(!d.zoom && !d.fps && !d.keystrokes && !d.minimap && !d.auto_reconnect);
        assert!(!d.mouse_tweaks && !d.sort && !d.own_name_tag && !d.ui_sound_reserve);
        assert!(!d.hide_block_particles);
        assert!(!d.music_page && d.music_off.is_empty());
        assert!(d.discord_presence && d.wheel_cycles_settings);
        assert_eq!(Extras::parse(""), d);
        assert_eq!(Extras::parse(&d.serialize()), d);
        let mut e = d.clone();
        Item::Fullbright.cycle(&mut e, false);
        assert_eq!(Item::Fullbright.switch(&e), Some(true));
        Item::DynamicLights.cycle(&mut e, false);
        assert_eq!(Item::DynamicLights.switch(&e), Some(true));
        assert_eq!(Extras::parse(&e.serialize()), e);
        e.zoom = true;
        e.zoom_key = "key.keyboard.c".into();
        e.zoom_level = 6;
        e.fps_corner = Corner::BottomRight;
        e.keystrokes = true;
        e.keystrokes_cps = false;
        e.moved = vec!["fps_overlay".into(), "keystrokes".into()];
        e.minimap = true;
        e.minimap_size = 128;
        e.minimap_round = true;
        e.minimap_zoom = 0.5;
        e.minimap_rotate = true;
        e.minimap_corner = Corner::BottomLeft;
        e.auto_reconnect = true;
        Item::OwnNameTag.cycle(&mut e, false);
        assert_eq!(Item::OwnNameTag.switch(&e), Some(true));
        assert_eq!(Item::HideBlockParticles.switch(&e), Some(false));
        Item::HideBlockParticles.cycle(&mut e, false);
        assert!(e.hide_block_particles);
        assert_eq!(Item::HideBlockParticles.switch(&e), Some(true));
        let only = Extras::parse("hideBlockParticles:true\n");
        assert!(only.hide_block_particles && !only.own_name_tag);
        e.mouse_tweaks = true;
        e.mouse_tweaks_wheel = false;
        e.sort = true;
        e.sort_key = "key.keyboard.r".into();
        e.chat_animation = true;
        e.chat_indicator = false;
        e.discord_presence = false;
        assert!(!e.splitscreen);
        Item::Splitscreen.cycle(&mut e, false);
        assert_eq!(Item::Splitscreen.switch(&e), Some(true));
        Item::WheelCyclesSettings.cycle(&mut e, false);
        assert_eq!(Item::WheelCyclesSettings.switch(&e), Some(false));
        e.music_off = vec![
            "minecraft:music/game/calm1".into(),
            "minecraft:music.menu".into(),
        ];
        assert!(!e.music_page);
        Item::MusicPage.cycle(&mut e, false);
        assert_eq!(Item::MusicPage.switch(&e), Some(true));
        assert_eq!(Extras::parse(&e.serialize()), e);
    }

    #[test]
    fn bad_lines_keep_defaults() {
        let e = Extras::parse("zoom:maybe\nzoomLevel:99\nfpsCorner:middle\nzoomKey:z\nnoise");
        assert!(!e.zoom);
        assert_eq!(e.zoom_level, 10);
        assert_eq!(e.fps_corner, Corner::TopLeft);
        assert_eq!(e.zoom_key, "key.keyboard.z");
    }

    #[test]
    fn a_mods_zoom_eases_like_the_spyglass() {
        // Camera.tickFov halves the distance to the target each tick.
        let mut camera = crate::camera::Camera::default();
        camera.tick(None, 1.0);
        for _ in 0..30 {
            camera.tick(None, 1.0);
        }
        let target = fov_target(1.0, 4.0);
        camera.tick(None, target);
        let first = camera.calculate_fov(1.0, 70, 1.0, None, Default::default(), false);
        assert!((first - 70.0 * 0.625).abs() < 1e-3, "{first}");
        for _ in 0..30 {
            camera.tick(None, target);
        }
        let settled = camera.calculate_fov(1.0, 70, 1.0, None, Default::default(), false);
        assert!((settled - 70.0 / 4.0).abs() < 1e-3, "{settled}");
        // Deeper than the 0.1 floor stops there, as vanilla clamps.
        for _ in 0..30 {
            camera.tick(None, fov_target(1.0, 20.0));
        }
        let floor = camera.calculate_fov(1.0, 70, 1.0, None, Default::default(), false);
        assert!((floor - 7.0).abs() < 1e-3, "{floor}");
    }

    #[test]
    fn fps_counter_reports_rate_minimum_and_low() {
        let mut c = FpsCounter::default();
        let mut t = 0.0;
        // Two seconds at 100 fps with one 50 ms hitch.
        for i in 0..200 {
            let ms = if i == 150 { 50.0 } else { 10.0 };
            t += f64::from(ms) / 1000.0;
            c.record(t, ms);
        }
        c.computed_at = f64::NEG_INFINITY;
        c.record(t + 0.01, 10.0);
        let r = c.reading();
        assert!((95..=101).contains(&r.fps), "{r:?}");
        assert_eq!(r.min, 20);
        assert_eq!(r.low, 100, "one frame in 200 is under the 1% mark");
        // Hitches beyond 1% of frames pull the 1% low down.
        let mut c = FpsCounter::default();
        let mut t = 0.0;
        for i in 0..100 {
            let ms = if i % 25 == 0 { 40.0 } else { 10.0 };
            t += f64::from(ms) / 1000.0;
            c.record(t, ms);
        }
        c.computed_at = f64::NEG_INFINITY;
        c.record(t, 10.0);
        assert_eq!(c.reading().low, 25);
    }

    #[test]
    fn old_frames_leave_the_window() {
        let mut c = FpsCounter::default();
        c.record(0.0, 100.0);
        for i in 1..=700 {
            c.record(f64::from(i) / 100.0, 10.0);
        }
        assert_eq!(c.reading().min, 100, "the 100 ms frame is 7 s old");
    }

    #[test]
    fn cps_counts_the_last_second() {
        let mut cps = Cps::default();
        for i in 0..10 {
            cps.click(f64::from(i) * 0.1);
        }
        assert_eq!(cps.per_second(0.95), 10);
        assert_eq!(cps.per_second(1.25), 7);
        assert_eq!(cps.per_second(3.0), 0);
    }

    #[test]
    fn items_cycle_through_their_values() {
        let mut e = Extras::default();
        Item::Sort.cycle(&mut e, true);
        assert_eq!(Item::Sort.value(&e), "On");
        // The minimap's old keys still parse, for the move to its mod.
        assert_eq!(
            Extras::parse("minimapSize:7\nminimapZoom:3").minimap_zoom,
            1.0
        );
        for item in rows().into_iter().flatten() {
            assert!(!item.caption().is_empty() && !item.description().is_empty());
        }
    }
}
