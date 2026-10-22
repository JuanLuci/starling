//! What a platform does for the game that std cannot: show a folder in its file manager, ask the
//! player for a file to import, the system clipboard. horizon-android registers Android's (through
//! the activity); elsewhere nothing is registered and each caller keeps its desktop way.
use std::path::Path;
use std::sync::OnceLock;

pub struct Hooks {
    /// Show the folder in the system's file manager; false when none could.
    pub open_folder: fn(&Path) -> bool,
    /// Ask the player for a file; it arrives in the import inbox (`import::inbox_dir`). False when
    /// no picker could open.
    pub pick_file: fn() -> bool,
    /// The system clipboard's text (None: empty or unreachable).
    pub clipboard_get: fn() -> Option<String>,
    /// Put text on the system clipboard; false when it could not.
    pub clipboard_set: fn(&str) -> bool,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

/// Registers the platform's hooks, once, before the game starts.
pub fn set(hooks: Hooks) {
    let _ = HOOKS.set(hooks);
}

/// Whether a platform registered its hooks (Android).
pub fn registered() -> bool {
    HOOKS.get().is_some()
}

/// The platform's file manager on `dir`: None without one registered (use the desktop's).
pub fn open_folder(dir: &Path) -> Option<bool> {
    HOOKS.get().map(|h| (h.open_folder)(dir))
}

/// The platform's file picker; false without one.
pub fn pick_file() -> bool {
    HOOKS.get().is_some_and(|h| (h.pick_file)())
}

/// The platform's clipboard: None without one registered, Some(None) when it holds no text.
pub fn clipboard_get() -> Option<Option<String>> {
    HOOKS.get().map(|h| (h.clipboard_get)())
}

/// The platform's clipboard: None without one registered.
pub fn clipboard_set(text: &str) -> Option<bool> {
    HOOKS.get().map(|h| (h.clipboard_set)(text))
}
