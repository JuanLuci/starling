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

/// The on-screen keyboard's height in window pixels, from HorizonActivity's insets listener.
static IME_HEIGHT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// `HorizonActivity.onImeHeight` (packaging/android/java), called on the UI thread.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_horizon_android_HorizonActivity_onImeHeight(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    pixels: i32,
) {
    IME_HEIGHT.store(pixels.max(0) as u32, std::sync::atomic::Ordering::Relaxed);
}

/// How far the on-screen keyboard covers the window from the bottom, in pixels.
pub(crate) fn ime_height() -> u32 {
    IME_HEIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

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

/// bionic's `struct mallinfo`: every field a `size_t`.
#[repr(C)]
struct Mallinfo {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    /// Bytes allocated and not freed.
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

unsafe extern "C" {
    fn mallinfo() -> Mallinfo;
    fn dlopen(filename: *const std::ffi::c_char, flags: std::ffi::c_int) -> *mut std::ffi::c_void;
    fn dlsym(
        handle: *mut std::ffi::c_void,
        symbol: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
}

/// The native heap in use (the Stats overlay's `heap`).
fn heap_bytes() -> Option<u64> {
    // SAFETY: mallinfo takes nothing and returns a plain struct.
    Some(unsafe { mallinfo() }.uordblks as u64)
}

/// The NDK's thermal API (AThermal, Android 11; the headroom Android 12), looked up at run time
/// so the library still loads on Android 8.
struct Thermal {
    manager: usize,
    status: Option<unsafe extern "C" fn(usize) -> std::ffi::c_int>,
    headroom: Option<unsafe extern "C" fn(usize, std::ffi::c_int) -> f32>,
}

fn thermal() -> Option<&'static Thermal> {
    static THERMAL: OnceLock<Option<Thermal>> = OnceLock::new();
    THERMAL
        .get_or_init(|| {
            // SAFETY: dlopen and dlsym get NUL-terminated names; each symbol found is cast to the
            // signature the NDK headers give it (thermal.h), and the manager it acquires lives
            // for the process.
            unsafe {
                let lib = dlopen(c"libandroid.so".as_ptr(), 2 /* RTLD_NOW */);
                if lib.is_null() {
                    return None;
                }
                let acquire = dlsym(lib, c"AThermal_acquireManager".as_ptr());
                if acquire.is_null() {
                    return None;
                }
                let acquire: unsafe extern "C" fn() -> usize = std::mem::transmute(acquire);
                let manager = acquire();
                if manager == 0 {
                    return None;
                }
                let symbol = |name: &std::ffi::CStr| {
                    let s = dlsym(lib, name.as_ptr());
                    (!s.is_null()).then_some(s)
                };
                Some(Thermal {
                    manager,
                    status: symbol(c"AThermal_getCurrentThermalStatus")
                        .map(|s| std::mem::transmute(s)),
                    headroom: symbol(c"AThermal_getThermalHeadroom")
                        .map(|s| std::mem::transmute(s)),
                })
            }
        })
        .as_ref()
}

fn thermal_status() -> Option<i32> {
    let t = thermal()?;
    // SAFETY: the function and the manager came from the NDK (`thermal`).
    t.status.map(|f| unsafe { f(t.manager) })
}

fn thermal_headroom() -> Option<f32> {
    let t = thermal()?;
    // SAFETY: as `thermal_status`. The forecast is for 10 s ahead; NaN when asked too often.
    t.headroom
        .map(|f| unsafe { f(t.manager, 10) })
        .filter(|h| h.is_finite())
}

/// Functions of libandroid.so newer than Android 8, looked up at run time like `thermal`.
struct FrameHints {
    /// ADPF's performance hint (Android 13): the manager's session for the render thread.
    create: Option<unsafe extern "C" fn(usize, *const i32, usize, i64) -> usize>,
    manager: usize,
    update: Option<unsafe extern "C" fn(usize, i64) -> std::ffi::c_int>,
    report: Option<unsafe extern "C" fn(usize, i64) -> std::ffi::c_int>,
    /// ANativeWindow_setFrameRate (Android 11).
    set_frame_rate: Option<unsafe extern "C" fn(*mut std::ffi::c_void, f32, i8) -> i32>,
}

fn frame_hints() -> &'static FrameHints {
    static HINTS: OnceLock<FrameHints> = OnceLock::new();
    HINTS.get_or_init(|| {
        // SAFETY: as `thermal`: NUL-terminated names, each symbol cast to its signature in the
        // NDK headers (performance_hint.h, native_window.h); the manager lives for the process.
        unsafe {
            let lib = dlopen(c"libandroid.so".as_ptr(), 2 /* RTLD_NOW */);
            let symbol = |name: &std::ffi::CStr| {
                let s = if lib.is_null() {
                    std::ptr::null_mut()
                } else {
                    dlsym(lib, name.as_ptr())
                };
                (!s.is_null()).then_some(s)
            };
            let manager = symbol(c"APerformanceHint_getManager")
                .map(|f| std::mem::transmute::<_, unsafe extern "C" fn() -> usize>(f)())
                .unwrap_or(0);
            FrameHints {
                create: symbol(c"APerformanceHint_createSession").map(|s| std::mem::transmute(s)),
                manager,
                update: symbol(c"APerformanceHint_updateTargetWorkDuration")
                    .map(|s| std::mem::transmute(s)),
                report: symbol(c"APerformanceHint_reportActualWorkDuration")
                    .map(|s| std::mem::transmute(s)),
                set_frame_rate: symbol(c"ANativeWindow_setFrameRate")
                    .map(|s| std::mem::transmute(s)),
            }
        }
    })
}

unsafe extern "C" {
    fn gettid() -> i32;
}

/// The render thread's hint session, its target, and the window and rate last voted for.
#[derive(Default)]
struct HintState {
    session: usize,
    target_ns: i64,
    window: usize,
    voted: f32,
}

thread_local! {
    static HINT_STATE: std::cell::RefCell<HintState> = std::cell::RefCell::default();
}

/// Tell Android about a frame the render thread finished: its CPU time against the time the
/// frame cap allows (ADPF), so the scheduler raises the clock or moves the thread to a bigger
/// core when a tick makes a frame late, rather than following a load average; and the frame
/// cap as the display's refresh rate, so a 60 FPS cap gets a 60 Hz screen (every frame shown
/// as long) instead of a 120 Hz one alternating one and two refreshes. `fps_limit` 260 and up
/// is no cap.
pub(crate) fn frame_hint(cpu: std::time::Duration, fps_limit: i32) {
    let hints = frame_hints();
    let cap = (fps_limit < 260).then(|| fps_limit.clamp(10, 240));
    let target_ns = 1_000_000_000 / i64::from(cap.unwrap_or(120));
    HINT_STATE.with_borrow_mut(|state| {
        if state.session == 0
            && hints.manager != 0
            && let Some(create) = hints.create
        {
            // SAFETY: the manager came from APerformanceHint_getManager; one thread id, this one.
            state.session = unsafe {
                let tid = gettid();
                create(hints.manager, &tid, 1, target_ns)
            };
            state.target_ns = target_ns;
        }
        if state.session != 0 {
            // SAFETY: a session this thread created, never closed.
            unsafe {
                if state.target_ns != target_ns
                    && let Some(update) = hints.update
                {
                    update(state.session, target_ns);
                    state.target_ns = target_ns;
                }
                if let Some(report) = hints.report {
                    report(state.session, (cpu.as_nanos() as i64).max(1));
                }
            }
        }
        let Some(set_frame_rate) = hints.set_frame_rate else {
            return;
        };
        let Some(window) = android_app().native_window() else {
            return;
        };
        let rate = cap.map_or(0.0, |c| c as f32);
        let ptr = window.ptr().as_ptr() as usize;
        if (state.window, state.voted) != (ptr, rate) {
            // SAFETY: the window is held for the call; 0 is
            // ANATIVEWINDOW_FRAME_RATE_COMPATIBILITY_DEFAULT, and a rate of 0 is no preference.
            unsafe { set_frame_rate(ptr as *mut std::ffi::c_void, rate, 0) };
            state.window = ptr;
            state.voted = rate;
        }
    });
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
    horizon_client::system_stats::set_probes(horizon_client::system_stats::Probes {
        heap_bytes,
        thermal_status,
        thermal_headroom,
    });
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
