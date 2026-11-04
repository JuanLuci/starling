//! Volt Boost: performance and optimization settings shared by the Extras
//! settings and the engine hooks that honour them. Every value defaults to
//! vanilla behaviour, so a fresh install is unchanged.

use serde::{Deserialize, Serialize};

// ============================================================================
// 4. Volt Boost — Tier 1, Tier 2 & Tier 3 Performance & Optimization Types
// ============================================================================

/// Master one-tap profile for **Volt Boost**. `Off` preserves 100% vanilla behaviour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoltBoostPreset {
    #[default]
    Off,
    Balanced,
    Competitive,
    UltraFps,
    BatterySaver,
    LowRam,
}

impl VoltBoostPreset {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Balanced,
        Self::Competitive,
        Self::UltraFps,
        Self::BatterySaver,
        Self::LowRam,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Balanced => "balanced",
            Self::Competitive => "competitive",
            Self::UltraFps => "ultra_fps",
            Self::BatterySaver => "battery_saver",
            Self::LowRam => "low_ram",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "balanced" => Some(Self::Balanced),
            "competitive" => Some(Self::Competitive),
            "ultra_fps" => Some(Self::UltraFps),
            "battery_saver" => Some(Self::BatterySaver),
            "low_ram" => Some(Self::LowRam),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "options.off",
            Self::Balanced => "volt.boost.preset.balanced",
            Self::Competitive => "volt.boost.preset.competitive",
            Self::UltraFps => "volt.boost.preset.ultra_fps",
            Self::BatterySaver => "volt.boost.preset.battery_saver",
            Self::LowRam => "volt.boost.preset.low_ram",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Balanced => "Balanced",
            Self::Competitive => "Competitive PvP",
            Self::UltraFps => "Ultra FPS",
            Self::BatterySaver => "Battery Saver",
            Self::LowRam => "Low-RAM Rescue",
        }
    }
}

/// Live performance pill overlay (`Off`, `Compact` FPS/1%-low/RAM, `Full` telemetry + graph).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfHudMode {
    #[default]
    Off,
    Compact,
    Full,
}

impl PerfHudMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Compact, Self::Full];

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Compact,
            Self::Compact => Self::Full,
            Self::Full => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Compact => "compact",
            Self::Full => "full",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" | "false" => Some(Self::Off),
            "compact" | "true" => Some(Self::Compact),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "options.off",
            Self::Compact => "volt.boost.perf_hud.compact",
            Self::Full => "volt.boost.perf_hud.full",
        }
    }
}

/// Per-frame chunk GPU upload limiter to prevent camera-turn frame spikes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkUploadCap {
    #[default]
    Auto,
    Smooth4,
    Smooth8,
    Balanced16,
    Burst32,
}

impl ChunkUploadCap {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Smooth4,
        Self::Smooth8,
        Self::Balanced16,
        Self::Burst32,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn max_uploads(self) -> Option<usize> {
        match self {
            Self::Auto => None,
            Self::Smooth4 => Some(4),
            Self::Smooth8 => Some(8),
            Self::Balanced16 => Some(16),
            Self::Burst32 => Some(32),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Smooth4 => "4",
            Self::Smooth8 => "8",
            Self::Balanced16 => "16",
            Self::Burst32 => "32",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "4" | "smooth4" => Some(Self::Smooth4),
            "8" | "smooth8" => Some(Self::Smooth8),
            "16" | "balanced16" => Some(Self::Balanced16),
            "32" | "burst32" => Some(Self::Burst32),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Smooth4 => "4 / frame",
            Self::Smooth8 => "8 / frame",
            Self::Balanced16 => "16 / frame",
            Self::Burst32 => "32 / frame",
        }
    }
}

/// Maximum rendered entities (nearest-first priority so nearby combat targets never vanish).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCap {
    #[default]
    Unlimited,
    Cap200,
    Cap128,
    Cap96,
    Cap64,
    Cap32,
}

impl EntityCap {
    pub const ALL: [Self; 6] = [
        Self::Unlimited,
        Self::Cap200,
        Self::Cap128,
        Self::Cap96,
        Self::Cap64,
        Self::Cap32,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn limit(self) -> Option<usize> {
        match self {
            Self::Unlimited => None,
            Self::Cap200 => Some(200),
            Self::Cap128 => Some(128),
            Self::Cap96 => Some(96),
            Self::Cap64 => Some(64),
            Self::Cap32 => Some(32),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlimited => "unlimited",
            Self::Cap200 => "200",
            Self::Cap128 => "128",
            Self::Cap96 => "96",
            Self::Cap64 => "64",
            Self::Cap32 => "32",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unlimited" | "off" => Some(Self::Unlimited),
            "200" => Some(Self::Cap200),
            "128" => Some(Self::Cap128),
            "96" => Some(Self::Cap96),
            "64" => Some(Self::Cap64),
            "32" => Some(Self::Cap32),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unlimited => "Unlimited",
            Self::Cap200 => "200 Max",
            Self::Cap128 => "128 Max",
            Self::Cap96 => "96 Max",
            Self::Cap64 => "64 Max",
            Self::Cap32 => "32 Max",
        }
    }
}

/// Dropped ground-item rendering cap & distance cutoffs for farm/explosion drops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemRenderCap {
    #[default]
    Unlimited,
    Cap128,
    Cap64,
    Cap32,
}

impl ItemRenderCap {
    pub const ALL: [Self; 4] = [Self::Unlimited, Self::Cap128, Self::Cap64, Self::Cap32];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn limit_and_dist_sq(self) -> Option<(usize, f64)> {
        match self {
            Self::Unlimited => None,
            Self::Cap128 => Some((128, 40.0 * 40.0)),
            Self::Cap64 => Some((64, 28.0 * 28.0)),
            Self::Cap32 => Some((32, 20.0 * 20.0)),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlimited => "unlimited",
            Self::Cap128 => "128",
            Self::Cap64 => "64",
            Self::Cap32 => "32",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unlimited" | "off" => Some(Self::Unlimited),
            "128" => Some(Self::Cap128),
            "64" => Some(Self::Cap64),
            "32" => Some(Self::Cap32),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unlimited => "Unlimited",
            Self::Cap128 => "128 (40m)",
            Self::Cap64 => "64 (28m)",
            Self::Cap32 => "32 (20m)",
        }
    }
}

/// Chest / sign / banner / skull / shulker block-entity render cap + wall occlusion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockEntityCap {
    #[default]
    Unlimited,
    Cap64,
    Cap32,
    Cap16,
}

impl BlockEntityCap {
    pub const ALL: [Self; 4] = [Self::Unlimited, Self::Cap64, Self::Cap32, Self::Cap16];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn limit_and_dist_sq(self) -> Option<(usize, f64)> {
        match self {
            Self::Unlimited => None,
            Self::Cap64 => Some((64, 48.0 * 48.0)),
            Self::Cap32 => Some((32, 32.0 * 32.0)),
            Self::Cap16 => Some((16, 24.0 * 24.0)),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlimited => "unlimited",
            Self::Cap64 => "64",
            Self::Cap32 => "32",
            Self::Cap16 => "16",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unlimited" | "off" => Some(Self::Unlimited),
            "64" => Some(Self::Cap64),
            "32" => Some(Self::Cap32),
            "16" => Some(Self::Cap16),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unlimited => "Unlimited",
            Self::Cap64 => "64 + Occlude",
            Self::Cap32 => "32 + Occlude",
            Self::Cap16 => "16 + Occlude",
        }
    }
}

/// Entity contact-shadow simplification mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FastShadowMode {
    #[default]
    Vanilla,
    Nearby16,
    Off,
}

impl FastShadowMode {
    pub const ALL: [Self; 3] = [Self::Vanilla, Self::Nearby16, Self::Off];

    pub fn next(self) -> Self {
        match self {
            Self::Vanilla => Self::Nearby16,
            Self::Nearby16 => Self::Off,
            Self::Off => Self::Vanilla,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Nearby16 => "nearby_16",
            Self::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "vanilla" => Some(Self::Vanilla),
            "nearby_16" | "nearby" => Some(Self::Nearby16),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Vanilla => "volt.boost.shadows.vanilla",
            Self::Nearby16 => "volt.boost.shadows.nearby",
            Self::Off => "options.off",
        }
    }
}

/// Active particle engine cap & distance cutoff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleBudget {
    #[default]
    Vanilla16k,
    Cap4096,
    Cap2048,
    Cap512,
    Off,
}

impl ParticleBudget {
    pub const ALL: [Self; 5] = [
        Self::Vanilla16k,
        Self::Cap4096,
        Self::Cap2048,
        Self::Cap512,
        Self::Off,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn max_particles(self) -> usize {
        match self {
            Self::Vanilla16k => 16_384,
            Self::Cap4096 => 4_096,
            Self::Cap2048 => 2_048,
            Self::Cap512 => 512,
            Self::Off => 0,
        }
    }

    pub fn max_distance_sq(self) -> Option<f64> {
        match self {
            Self::Vanilla16k => None,
            Self::Cap4096 => Some(36.0 * 36.0),
            Self::Cap2048 => Some(24.0 * 24.0),
            Self::Cap512 => Some(16.0 * 16.0),
            Self::Off => Some(0.0),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla16k => "16384",
            Self::Cap4096 => "4096",
            Self::Cap2048 => "2048",
            Self::Cap512 => "512",
            Self::Off => "0",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "16384" | "vanilla" => Some(Self::Vanilla16k),
            "4096" => Some(Self::Cap4096),
            "2048" => Some(Self::Cap2048),
            "512" => Some(Self::Cap512),
            "0" | "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla16k => "16K (Vanilla)",
            Self::Cap4096 => "4,096 (36m)",
            Self::Cap2048 => "2,048 (24m)",
            Self::Cap512 => "512 (16m)",
            Self::Off => "OFF",
        }
    }
}

/// Enchantment glint & armor-trim shader simplification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlintTrimMode {
    #[default]
    Full,
    NearbyOnly,
    Off,
}

impl GlintTrimMode {
    pub const ALL: [Self; 3] = [Self::Full, Self::NearbyOnly, Self::Off];

    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::NearbyOnly,
            Self::NearbyOnly => Self::Off,
            Self::Off => Self::Full,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NearbyOnly => "nearby",
            Self::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "nearby" => Some(Self::NearbyOnly),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Full => "volt.boost.glint.full",
            Self::NearbyOnly => "volt.boost.glint.nearby",
            Self::Off => "options.off",
        }
    }
}

/// Automatic Dynamic Resolution Scaling (scales 3D scene while keeping HUD 100% native crisp).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoDrsMode {
    #[default]
    Off,
    Target60,
    Target120,
    Aggressive50,
}

impl AutoDrsMode {
    pub const ALL: [Self; 4] = [
        Self::Off,
        Self::Target60,
        Self::Target120,
        Self::Aggressive50,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn target_and_floor(self) -> Option<(f32, f32)> {
        match self {
            Self::Off => None,
            Self::Target60 => Some((60.0, 0.65)),
            Self::Target120 => Some((120.0, 0.60)),
            Self::Aggressive50 => Some((60.0, 0.50)),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Target60 => "target_60",
            Self::Target120 => "target_120",
            Self::Aggressive50 => "aggressive_50",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "target_60" => Some(Self::Target60),
            "target_120" => Some(Self::Target120),
            "aggressive_50" => Some(Self::Aggressive50),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Target60 => "60 FPS (65-100%)",
            Self::Target120 => "120 FPS (60-100%)",
            Self::Aggressive50 => "Ultra (50-100%)",
        }
    }
}

/// Dynamic FPS cap when paused, in menus, or AFK-idle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DynamicIdleFps {
    #[default]
    Off,
    Fps30,
    Fps15,
}

impl DynamicIdleFps {
    pub const ALL: [Self; 3] = [Self::Off, Self::Fps30, Self::Fps15];

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Fps30,
            Self::Fps30 => Self::Fps15,
            Self::Fps15 => Self::Off,
        }
    }

    pub fn cap_fps(self) -> Option<u32> {
        match self {
            Self::Off => None,
            Self::Fps30 => Some(30),
            Self::Fps15 => Some(15),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fps30 => "30",
            Self::Fps15 => "15",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "30" => Some(Self::Fps30),
            "15" => Some(Self::Fps15),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Fps30 => "30 FPS Idle",
            Self::Fps15 => "15 FPS Eco",
        }
    }
}

/// Pause / menu background blur saver (skips multi-pass Kawase blur on mobile tile GPUs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuBgMode {
    #[default]
    PanoramaBlur,
    FastDark,
}

impl MenuBgMode {
    pub const ALL: [Self; 2] = [Self::PanoramaBlur, Self::FastDark];

    pub fn next(self) -> Self {
        match self {
            Self::PanoramaBlur => Self::FastDark,
            Self::FastDark => Self::PanoramaBlur,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PanoramaBlur => "blur",
            Self::FastDark => "fast_dark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "blur" | "panorama" => Some(Self::PanoramaBlur),
            "fast_dark" | "fast" => Some(Self::FastDark),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::PanoramaBlur => "volt.boost.menu_bg.blur",
            Self::FastDark => "volt.boost.menu_bg.fast_dark",
        }
    }
}

/// Active simultaneous audio mixer voice cap to avoid CPU mixer spikes in mob farms/TNT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioVoiceCap {
    #[default]
    Vanilla,
    Cap32,
    Cap16,
}

impl AudioVoiceCap {
    pub const ALL: [Self; 3] = [Self::Vanilla, Self::Cap32, Self::Cap16];

    pub fn next(self) -> Self {
        match self {
            Self::Vanilla => Self::Cap32,
            Self::Cap32 => Self::Cap16,
            Self::Cap16 => Self::Vanilla,
        }
    }

    pub fn max_per_tick(self) -> Option<usize> {
        match self {
            Self::Vanilla => None,
            Self::Cap32 => Some(8),
            Self::Cap16 => Some(4),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Cap32 => "32",
            Self::Cap16 => "16",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "vanilla" | "off" => Some(Self::Vanilla),
            "32" => Some(Self::Cap32),
            "16" => Some(Self::Cap16),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Cap32 => "32 Voices",
            Self::Cap16 => "16 Voices",
        }
    }
}

