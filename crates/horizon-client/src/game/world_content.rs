//! Content mods tied to the world: blocks and items a mod adds join the game's registries once a
//! start, before the resources load (`mods::content_root`), so a world plays with the set it was
//! made with. Each world keeps its set in `horizon-content.json`; opening a world whose set is not
//! the one running starts the game again with that set (`next-start.json` in the private home)
//! and enters the world, instead of opening it without its blocks (which a save would turn into
//! air). A start the player did not ask for that way runs the mods turned on in Options > Mods.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// In a world's folder: the content mods it plays with.
const WORLD_FILE: &str = "horizon-content.json";
/// In the private home: the world the next start enters, and its content mods.
const NEXT_FILE: &str = "next-start.json";

/// What a restart asked of this start.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct NextStart {
    pub(super) world: String,
    pub(super) content: BTreeSet<String>,
}

fn next_path() -> PathBuf {
    horizon_client::logging::private_home().join(NEXT_FILE)
}

/// The restart's request, read (and removed) once a process: a second start reads nothing.
pub(super) fn next_start() -> Option<&'static NextStart> {
    static NEXT: OnceLock<Option<NextStart>> = OnceLock::new();
    NEXT.get_or_init(|| {
        let path = next_path();
        let text = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let world = value["world"].as_str()?.to_owned();
        let content = ids(&value["content"]);
        log::info!("Horizon: started again to enter {world} with its mods {content:?}");
        Some(NextStart { world, content })
    })
    .as_ref()
}

fn ids(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// The mods adding blocks or items this process loaded (`mods::content_root` keeps it).
fn running_cell() -> &'static Mutex<BTreeSet<String>> {
    static RUNNING: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}
pub(super) fn set_running(ids: BTreeSet<String>) {
    *running_cell()
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = ids;
}
pub(super) fn running() -> BTreeSet<String> {
    running_cell()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn world_dir(folder: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("saves")
        .join(folder)
}

/// The content mods world `folder` plays with, when it says.
pub(super) fn recorded(folder: &str) -> Option<BTreeSet<String>> {
    read_set(&world_dir(folder).join(WORLD_FILE))
}

fn read_set(path: &Path) -> Option<BTreeSet<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(ids(&value["mods"]))
}

/// Keeps `mods` as world `folder`'s set; false when the world has no folder yet.
pub(super) fn record(folder: &str, mods: &BTreeSet<String>) -> bool {
    let dir = world_dir(folder);
    if !dir.join("level.dat").is_file() {
        return false;
    }
    let body = serde_json::json!({ "mods": mods });
    match std::fs::write(dir.join(WORLD_FILE), format!("{body:#}\n")) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("Horizon: {folder}'s content mods were not kept: {e}");
            false
        }
    }
}

/// What opening world `folder` needs.
#[derive(Debug, PartialEq)]
pub(super) enum Entry {
    /// It plays with the running set (or has none yet: it takes the running one).
    Open,
    /// Start again with this set, then enter it.
    Restart(BTreeSet<String>),
    /// This start was made for it, and its mods still did not load: entering would lose their
    /// blocks.
    Missing(BTreeSet<String>),
}

pub(super) fn entry(folder: &str) -> Entry {
    let running = running();
    match recorded(folder) {
        Some(set) if set != running => {
            let retried = next_start().is_some_and(|n| n.world == folder && n.content == set);
            if retried {
                Entry::Missing(set.difference(&running).cloned().collect())
            } else {
                Entry::Restart(set)
            }
        }
        Some(_) => Entry::Open,
        None => {
            record(folder, &running);
            Entry::Open
        }
    }
}

/// Writes the next start's request: world `folder` with `content`.
pub(super) fn request_restart(folder: &str, content: &BTreeSet<String>) -> std::io::Result<()> {
    let body = serde_json::json!({ "world": folder, "content": content });
    let path = next_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, body.to_string())
}

/// Starts the game again: Android's launcher activity, or this program with its arguments.
pub(super) fn restart() {
    #[cfg(target_os = "android")]
    crate::restart_app();
    #[cfg(not(target_os = "android"))]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        match std::env::current_exe()
            .and_then(|exe| std::process::Command::new(exe).args(args).spawn())
        {
            Ok(_) => std::process::exit(0),
            Err(e) => log::warn!("Horizon: could not start again: {e}"),
        }
    }
}

/// `mods` as a line names them: "Create, Farmer's Delight", by id.
pub(super) fn names(mods: &BTreeSet<String>) -> String {
    mods.iter().cloned().collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_world_keeps_its_set_and_reads_it_back() {
        let dir =
            std::env::temp_dir().join(format!("horizon-world-content-{}", std::process::id()));
        let world = dir.join("saves").join("w");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), b"").unwrap();
        let file = world.join(WORLD_FILE);
        std::fs::write(&file, serde_json::json!({ "mods": ["create"] }).to_string()).unwrap();
        assert_eq!(read_set(&file), Some(set(&["create"])));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_take_strings_only() {
        let value = serde_json::json!(["create", 3, "farmersdelight"]);
        assert_eq!(ids(&value), set(&["create", "farmersdelight"]));
    }
}
