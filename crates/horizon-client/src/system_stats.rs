//! What the system says about the game's process and the device, for the Stats overlay and
//! `horizon-ctl perf`: the process's memory (/proc/self), the native heap and the thermal
//! throttling level (probes a platform registers: horizon-android's mallinfo and AThermal),
//! and the CPU, GPU and battery temperatures and the fastest core's clock where sysfs lets an
//! app read them (Linux and Android; elsewhere they stay unknown).
//!
//! A thread samples once a second, started by the first `latest`: reading smaps_rollup walks
//! every mapping, too slow for the frame.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Platform probes: each returns None when it cannot tell.
pub struct Probes {
    /// Bytes the native allocator has handed out.
    pub heap_bytes: fn() -> Option<u64>,
    /// The thermal status: 0 none, 1 light, 2 moderate, 3 severe, 4 critical, 5 emergency,
    /// 6 shutdown (Android's AThermalStatus).
    pub thermal_status: fn() -> Option<i32>,
    /// How close the device is to throttling in the next seconds: 1.0 is the throttling point.
    pub thermal_headroom: fn() -> Option<f32>,
}

static PROBES: OnceLock<Probes> = OnceLock::new();

/// Registers the platform's probes (before the first sample).
pub fn set_probes(probes: Probes) {
    let _ = PROBES.set(probes);
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemSample {
    pub rss_bytes: Option<u64>,
    /// Proportional set size: shared pages split among the processes sharing them.
    pub pss_bytes: Option<u64>,
    pub heap_bytes: Option<u64>,
    pub thermal_status: Option<i32>,
    pub thermal_headroom: Option<f32>,
    pub cpu_celsius: Option<f32>,
    pub gpu_celsius: Option<f32>,
    pub battery_celsius: Option<f32>,
    /// The fastest core: its current and its highest clock, in MHz.
    pub cpu_mhz: Option<[u32; 2]>,
}

/// AThermalStatus as a word.
pub fn thermal_name(status: i32) -> &'static str {
    match status {
        0 => "none",
        1 => "light",
        2 => "moderate",
        3 => "severe",
        4 => "critical",
        5 => "emergency",
        6 => "shutdown",
        _ => "unknown",
    }
}

#[derive(Default)]
pub struct Sampler {
    latest: Arc<Mutex<SystemSample>>,
    started: OnceLock<()>,
}

impl Sampler {
    /// The last sample (all unknown until the first second has passed).
    pub fn latest(&self) -> SystemSample {
        self.started.get_or_init(|| {
            let latest = self.latest.clone();
            let _ = std::thread::Builder::new()
                .name("horizon-stats".into())
                .spawn(move || {
                    let zones = Zones::find(Path::new("/sys/class/thermal"));
                    let cpus = fastest_core(Path::new("/sys/devices/system/cpu"));
                    loop {
                        let sample = sample(&zones, cpus.as_deref());
                        if let Ok(mut l) = latest.lock() {
                            *l = sample;
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                });
        });
        self.latest.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

fn sample(zones: &Zones, cpu: Option<&Path>) -> SystemSample {
    let probes = PROBES.get();
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let rollup = fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let khz = |name: &str| {
        cpu.and_then(|c| fs::read_to_string(c.join(name)).ok())
            .and_then(|t| t.trim().parse::<u32>().ok())
    };
    SystemSample {
        rss_bytes: kib_field(&status, "VmRSS:").map(|k| k * 1024),
        pss_bytes: kib_field(&rollup, "Pss:").map(|k| k * 1024),
        heap_bytes: probes.and_then(|p| (p.heap_bytes)()),
        thermal_status: probes.and_then(|p| (p.thermal_status)()),
        thermal_headroom: probes.and_then(|p| (p.thermal_headroom)()),
        cpu_celsius: zones.hottest(&zones.cpu),
        gpu_celsius: zones.hottest(&zones.gpu),
        battery_celsius: zones.hottest(&zones.battery),
        cpu_mhz: khz("scaling_cur_freq")
            .zip(khz("cpuinfo_max_freq"))
            .map(|(cur, max)| [cur / 1000, max / 1000]),
    }
}

/// A `Name:   1234 kB` line's number.
fn kib_field(text: &str, name: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix(name))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// The thermal zones that read as the CPU, the GPU and the battery, by their type names
/// (Qualcomm's `cpu-1-0`, `cpuss-0`, `gpuss-0`, `battery`; others' `cpu_therm`, `gpu`...).
#[derive(Default)]
struct Zones {
    cpu: Vec<PathBuf>,
    gpu: Vec<PathBuf>,
    battery: Vec<PathBuf>,
}

impl Zones {
    fn find(root: &Path) -> Self {
        let mut zones = Zones::default();
        let Ok(entries) = fs::read_dir(root) else {
            return zones;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = fs::read_to_string(path.join("type")) else {
                continue;
            };
            // Only zones this process may read count.
            if fs::read_to_string(path.join("temp")).is_err() {
                continue;
            }
            let kind = kind.trim().to_ascii_lowercase();
            let temp = path.join("temp");
            if kind.starts_with("cpu") {
                zones.cpu.push(temp);
            } else if kind.starts_with("gpu") {
                zones.gpu.push(temp);
            } else if kind == "battery" || kind.starts_with("batt") {
                zones.battery.push(temp);
            }
        }
        zones
    }
    /// The hottest zone in °C (sysfs gives millidegrees; unplugged sensors read -273).
    fn hottest(&self, temps: &[PathBuf]) -> Option<f32> {
        temps
            .iter()
            .filter_map(|t| fs::read_to_string(t).ok()?.trim().parse::<i64>().ok())
            .filter(|&m| m > 0)
            .max()
            .map(|m| m as f32 / 1000.0)
    }
}

/// The cpufreq folder of the core with the highest maximum clock.
fn fastest_core(root: &Path) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("cpu")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|e| e.path().join("cpufreq"))
        .filter_map(|p| {
            let max = fs::read_to_string(p.join("cpuinfo_max_freq")).ok()?;
            Some((max.trim().parse::<u64>().ok()?, p))
        })
        .max_by_key(|(max, _)| *max)
        .map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_fields_are_read_in_kib() {
        let status = "Name:\thorizon\nVmHWM:\t    3840 kB\nVmRSS:\t  828000 kB\n";
        assert_eq!(kib_field(status, "VmRSS:"), Some(828_000));
        assert_eq!(
            kib_field("Rss: 5012 kB\nPss: 1092 kB\n", "Pss:"),
            Some(1092)
        );
        assert_eq!(kib_field(status, "Pss:"), None);
    }

    #[test]
    fn zones_are_sorted_by_their_type_and_the_hottest_wins() {
        let root = std::env::temp_dir().join(format!("horizon-zones-{}", std::process::id()));
        for (i, (kind, temp)) in [
            ("cpu-1-0", "54700"),
            ("cpuss-0", "56100"),
            ("gpuss-0", "49800"),
            ("battery", "36400"),
            ("pa", "44000"),
            ("cpu-9-9", "-273000"),
        ]
        .iter()
        .enumerate()
        {
            let zone = root.join(format!("thermal_zone{i}"));
            fs::create_dir_all(&zone).unwrap();
            fs::write(zone.join("type"), format!("{kind}\n")).unwrap();
            fs::write(zone.join("temp"), format!("{temp}\n")).unwrap();
        }
        let zones = Zones::find(&root);
        assert_eq!(zones.cpu.len(), 3);
        assert_eq!(zones.hottest(&zones.cpu), Some(56.1));
        assert_eq!(zones.hottest(&zones.gpu), Some(49.8));
        assert_eq!(zones.hottest(&zones.battery), Some(36.4));
        fs::remove_dir_all(root).unwrap();
    }
}
