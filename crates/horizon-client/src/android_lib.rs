//! Horizon on Android: the desktop client's game module, run from a NativeActivity.
//!
//! The crate root of horizon-android (crates/horizon-android/Cargo.toml), not a module of
//! horizon-client: it sits beside main.rs so that `mod game` is the binary's own module, its
//! files and paths (`crate::game`) unchanged, compiled a second time with another entry point.
//! `android_main` gives the process what the desktop binary gets from its launcher (the game
//! folder as the current directory and `HORIZON_HOME`, the command line) and then runs
//! `game::run` as `main` does.
#![cfg(target_os = "android")]

mod game;

use horizon_client::{crash, logging};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use winit::platform::android::activity::AndroidApp;

static APP: OnceLock<AndroidApp> = OnceLock::new();
static ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// The activity, for the event loop (`EventLoopBuilderExtAndroid::with_android_app`).
pub(crate) fn android_app() -> AndroidApp {
    APP.get().expect("android_main sets the app first").clone()
}

/// The command line the desktop binary would get, without the program name.
pub(crate) fn args() -> Vec<String> {
    ARGS.get().cloned().unwrap_or_default()
}

/// The game's command line and environment: `--resources` in the game folder, then
/// `horizon-args.txt` there (written over adb for tests), which may override it: one argument per
/// line, or `NAME=value` for an environment variable (`HORIZON_GPU_CULL=0`, `WGPU_BACKEND=...`).
fn command_line(home: &Path) -> (Vec<String>, Vec<(String, String)>) {
    let mut args = vec![
        "--resources".to_owned(),
        home.join("resources").display().to_string(),
    ];
    let mut env = Vec::new();
    let text = std::fs::read_to_string(home.join("horizon-args.txt")).unwrap_or_default();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.split_once('=') {
            Some((name, value))
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') =>
            {
                env.push((name.to_owned(), value.to_owned()))
            }
            _ => args.push(line.to_owned()),
        }
    }
    (args, env)
}

/// Horizon's console lines in logcat, under the tag `horizon`.
fn logcat(level: log::Level, message: &str) {
    use std::ffi::CString;
    let priority = match level {
        log::Level::Error => 6,
        log::Level::Warn => 5,
        log::Level::Info => 4,
        log::Level::Debug => 3,
        log::Level::Trace => 2,
    };
    let tag = c"horizon";
    for line in message.lines() {
        let Ok(text) = CString::new(line.replace('\0', " ")) else {
            continue;
        };
        // SAFETY: both pointers are NUL-terminated strings that outlive the call.
        unsafe { __android_log_write(priority, tag.as_ptr(), text.as_ptr()) };
    }
}

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        priority: std::ffi::c_int,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    horizon_client::telemetry::mark_start();
    let home: PathBuf = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/horizon"));
    let _ = std::fs::create_dir_all(&home);
    let (args, env) = command_line(&home);
    // SAFETY: no other thread of ours runs yet; the game reads these from its threads afterwards.
    unsafe {
        std::env::set_var("HORIZON_HOME", &home);
        for (name, value) in &env {
            std::env::set_var(name, value);
        }
    }
    if let Err(e) = std::env::set_current_dir(&home) {
        logcat(
            log::Level::Error,
            &format!("Horizon: cannot enter {}: {e}", home.display()),
        );
    }
    logging::set_console(logcat);
    logging::init(&home, "Render thread");
    crash::install(home.clone(), "client");
    log::info!(
        "Horizon: Android, game folder {}, arguments {args:?}, environment {env:?}",
        home.display()
    );
    let _ = ARGS.set(args);
    let _ = APP.set(app);
    if let Err(error) = game::run() {
        log::error!("Horizon: {error}");
        crash::report_error(&home, "client", &*error);
    }
    logging::flush();
    // The activity is gone. Android may keep the process and start a new activity in it, but
    // winit's event loop cannot be made twice in one process: end it, so the next launch is a
    // fresh one.
    std::process::exit(0);
}
