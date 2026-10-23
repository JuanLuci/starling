//! `horizon-mod.toml`, a mod's manifest, and the `.hmod` file around it.
use serde::Deserialize;
use std::{fmt, io::Read, path::Path};

/// The manifest's file name, beside `mod.wasm`.
pub const MANIFEST: &str = "horizon-mod.toml";
/// The component's file name in a `.hmod` or a mod folder.
pub const WASM: &str = "mod.wasm";
/// A JavaScript mod's source, in place of a component: Starling's shared engine runs it.
pub const JS: &str = "mod.js";
/// The API (WIT package `horizon:mods`) versions this build runs: a mod
/// built for any 0.1.x SDK.
pub const API: &str = "0.1";
/// Starling's own API, the WIT package `starling:mods` (starling-mod-sdk): Horizon's 0.1 and
/// what Starling adds. Its mods say so in their manifest, which Horizon refuses with that line.
pub const STARLING_API: &str = "starling-0.1";
/// The WIT packages and versions a component may link.
const LINKED: [(&str, &str); 2] = [("horizon:mods/", "0.1."), ("starling:mods/", "0.1.")];
/// The manifest `api` lines this build loads. 0.2 is 0.1 plus content:
/// `assets/` and `data/` in the `.hmod`, with blocks and items declared in
/// `data/<id>/horizon/` (docs/modpacks.md). Code still links API 0.1. starling-0.1 links
/// `starling:mods` and may hold content too.
pub const APIS: [&str; 3] = [API, "0.2", STARLING_API];
/// Namespaces no mod may claim: vanilla's, the convention tags' (mods add
/// to those, docs/mods.md) and the game's own.
const RESERVED_NAMESPACES: [&str; 3] = ["minecraft", "c", "horizon"];
/// The biggest `mod.wasm` a mod may ship.
const MAX_WASM: u64 = 32 << 20;
/// The biggest `mod.js`.
const MAX_JS: u64 = 4 << 20;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// `[a-z0-9_]`, 1 to 64 characters: the mod's folder, log and settings key.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// The API line the mod was built for: `0.1`, or `0.2` for content.
    pub api: String,
    #[serde(default)]
    pub side: Side,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Namespaces besides `id` that the mod's `assets/` and `data/` use, for
    /// a content mod whose files sit under other names than its id (Towns
    /// and Towers ships `kaisyn` and `towns_and_towers`). No two content mods
    /// that are on may share one (`content::clash`).
    #[serde(default)]
    pub namespaces: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Runs in the player's game only (HUD, keys, chat commands).
    #[default]
    Client,
    /// Runs where the world is simulated (rules, blocks); not run yet.
    Server,
    /// Both, and the server tells clients which version it expects.
    Both,
}

/// What a mod may do beyond writing its log. Each is a WIT interface the
/// component imports; the player sees them before turning the mod on.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    GameRead,
    WorldRead,
    InventoryRead,
    /// The open menu's slots and clicks in them (Starling's `slots`).
    InventoryClick,
    Chat,
    ChatSend,
    Hud,
    Ui,
    Keys,
    /// Holding sprint and sneak for the player (Starling's `input`).
    Input,
    Sound,
    Net,
    Storage,
    /// New blocks and items from the mod's `data/` (API 0.2): no WIT
    /// interface, the game reads them before the world loads.
    Registries,
}
impl Capability {
    pub const ALL: [Capability; 14] = [
        Capability::GameRead,
        Capability::WorldRead,
        Capability::InventoryRead,
        Capability::InventoryClick,
        Capability::Chat,
        Capability::ChatSend,
        Capability::Hud,
        Capability::Ui,
        Capability::Keys,
        Capability::Input,
        Capability::Sound,
        Capability::Net,
        Capability::Storage,
        Capability::Registries,
    ];
    /// The WIT interfaces it grants, in `horizon:mods`.
    pub fn interfaces(self) -> &'static [&'static str] {
        match self {
            Capability::GameRead => &["game"],
            Capability::WorldRead => &["level"],
            Capability::InventoryRead => &["inventory"],
            Capability::InventoryClick => &["slots"],
            Capability::Chat => &["chat"],
            Capability::ChatSend => &["chat-send"],
            Capability::Hud => &["hud", "draw"],
            Capability::Ui => &["screens", "widgets"],
            Capability::Keys => &["keys"],
            Capability::Input => &["input"],
            Capability::Sound => &["sound"],
            Capability::Net => &["net"],
            Capability::Storage => &["storage"],
            Capability::Registries => &[],
        }
    }
    /// What the player reads before turning the mod on: "May <this>".
    pub fn describe(self) -> &'static str {
        match self {
            Capability::GameRead => "see your position, health, effects and the time",
            Capability::WorldRead => "see the blocks and creatures around you",
            Capability::InventoryRead => "see your items",
            Capability::InventoryClick => "move your items, in your inventory and what you open",
            Capability::Chat => "show you chat messages",
            Capability::ChatSend => "send chat messages and commands as you",
            Capability::Hud => "draw on your screen",
            Capability::Ui => "open its own screens and add buttons to the game's",
            Capability::Keys => "use keys you press",
            Capability::Input => "hold sprint or sneak for you",
            Capability::Sound => "play sounds",
            Capability::Net => "talk to servers on its own channels",
            Capability::Storage => "save its own settings to disk",
            Capability::Registries => "add blocks and items to your worlds",
        }
    }
    /// The capability a component import needs, `Ok(None)` for one every
    /// mod has; `Err` for an import Starling does not offer.
    pub fn of_import(name: &str) -> Result<Option<Capability>, String> {
        let unknown = || format!("imports {name}, which Starling does not offer");
        let (package, rest) = LINKED
            .iter()
            .find_map(|(package, _)| Some((*package, name.strip_prefix(package)?)))
            .ok_or_else(unknown)?;
        let (interface, version) = rest.split_once('@').ok_or_else(unknown)?;
        if !LINKED
            .iter()
            .any(|(p, v)| *p == package && version.starts_with(v))
        {
            let package = package.trim_end_matches('/');
            return Err(format!(
                "needs {package}@{version}; this Starling runs Horizon's API {API} and {STARLING_API}"
            ));
        }
        // Always granted: the log, the shared types and Starling's device interface (which does
        // nothing where it does not apply).
        if interface == "log"
            || interface == "types"
            || (package == "starling:mods/" && interface == "mobile")
        {
            return Ok(None);
        }
        Capability::ALL
            .into_iter()
            .find(|c| c.interfaces().contains(&interface))
            .map(Some)
            .ok_or_else(unknown)
    }
}
impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Capability::GameRead => "game-read",
            Capability::WorldRead => "world-read",
            Capability::InventoryRead => "inventory-read",
            Capability::InventoryClick => "inventory-click",
            Capability::Chat => "chat",
            Capability::ChatSend => "chat-send",
            Capability::Hud => "hud",
            Capability::Ui => "ui",
            Capability::Keys => "keys",
            Capability::Input => "input",
            Capability::Sound => "sound",
            Capability::Net => "net",
            Capability::Storage => "storage",
            Capability::Registries => "registries",
        };
        f.write_str(name)
    }
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        let id_ok = (1..=64).contains(&m.id.len())
            && m.id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !id_ok {
            return Err(format!(
                "{MANIFEST}: id {:?} is not [a-z0-9_]{{1,64}}",
                m.id
            ));
        }
        if !APIS.contains(&m.api.as_str()) {
            return Err(format!(
                "built for API {}; this Starling runs {}",
                m.api,
                APIS.join(" and ")
            ));
        }
        if m.capabilities.contains(&Capability::Registries) && m.api == "0.1" {
            return Err(format!(
                "{MANIFEST}: capability registries needs api = \"0.2\" or later"
            ));
        }
        if !m.namespaces.is_empty() && !m.capabilities.contains(&Capability::Registries) {
            return Err(format!(
                "{MANIFEST}: namespaces are for content, which needs capability registries"
            ));
        }
        for (i, ns) in m.namespaces.iter().enumerate() {
            // Vanilla's namespace characters, at most as long as an id.
            let chars_ok = (1..=64).contains(&ns.len())
                && ns
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
            if !chars_ok {
                return Err(format!(
                    "{MANIFEST}: namespace {ns:?} is not [a-z0-9_.-]{{1,64}}"
                ));
            }
            if RESERVED_NAMESPACES.contains(&ns.as_str()) {
                return Err(format!(
                    "{MANIFEST}: namespace {ns} is not a mod's to claim (only to add to its tags)"
                ));
            }
            if *ns == m.id || m.namespaces[..i].contains(ns) {
                return Err(format!("{MANIFEST}: namespace {ns} is listed twice"));
            }
        }
        Ok(m)
    }
    /// The namespaces the mod's content owns: its id, then `namespaces`.
    pub fn owned_namespaces(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str()).chain(self.namespaces.iter().map(String::as_str))
    }
    /// Checks that the component's imports need only declared capabilities, and that one
    /// linking Starling's API says so in its manifest.
    pub fn check_imports<'a>(
        &self,
        imports: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        for name in imports {
            if name.starts_with("starling:mods/") && self.api != STARLING_API {
                return Err(format!(
                    "links Starling's API: set api = \"{STARLING_API}\" in {MANIFEST}"
                ));
            }
            if let Some(c) = Capability::of_import(name)?
                && !self.capabilities.contains(&c)
            {
                return Err(format!("uses capability {c}, missing from {MANIFEST}"));
            }
        }
        Ok(())
    }
}

/// A mod as found on disk: its manifest and component bytes.
pub struct Package {
    pub manifest: Manifest,
    /// None for a content-only mod (capability `registries`, no code) or a JavaScript one.
    pub wasm: Option<Vec<u8>>,
    /// A JavaScript mod's source (`mod.js`), which Starling's shared engine runs.
    pub js: Option<String>,
}
impl Package {
    /// A `.hmod` (a zip holding `horizon-mod.toml`, `mod.wasm` and, for
    /// content, `assets/` and `data/`) or a folder holding the same.
    pub fn read(path: &Path) -> Result<Self, String> {
        let (toml, wasm, js) = if path.is_dir() {
            let toml = std::fs::read_to_string(path.join(MANIFEST));
            let read = |name: &str| -> Result<Option<Vec<u8>>, String> {
                let file = path.join(name);
                match file.exists() {
                    true => Ok(Some(
                        std::fs::read(file).map_err(|e| format!("{name}: {e}"))?,
                    )),
                    false => Ok(None),
                }
            };
            (
                toml.map_err(|e| format!("{MANIFEST}: {e}"))?,
                read(WASM)?,
                read(JS)?,
            )
        } else {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut entry = |name: &str| -> Result<Option<Vec<u8>>, String> {
                let f = match zip.by_name(name) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::FileNotFound) => return Ok(None),
                    Err(e) => return Err(format!("{name}: {e}")),
                };
                let mut out = Vec::new();
                f.take(MAX_WASM + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| e.to_string())?;
                Ok(Some(out))
            };
            let toml = entry(MANIFEST)?.ok_or_else(|| format!("{MANIFEST}: missing"))?;
            let toml = String::from_utf8(toml).map_err(|_| format!("{MANIFEST}: not UTF-8"))?;
            (toml, entry(WASM)?, entry(JS)?)
        };
        if wasm.as_ref().is_some_and(|w| w.len() as u64 > MAX_WASM) {
            return Err(format!("{WASM} is over {} MiB", MAX_WASM >> 20));
        }
        if js.as_ref().is_some_and(|j| j.len() as u64 > MAX_JS) {
            return Err(format!("{JS} is over {} MiB", MAX_JS >> 20));
        }
        let js = js
            .map(|j| String::from_utf8(j).map_err(|_| format!("{JS}: not UTF-8")))
            .transpose()?;
        let manifest = Manifest::parse(&toml)?;
        if wasm.is_some() && js.is_some() {
            return Err(format!("{WASM} and {JS} both: a mod is one or the other"));
        }
        if js.is_some() && manifest.api != STARLING_API {
            return Err(format!(
                "{JS}: a JavaScript mod runs on Starling's API: set api = \"{STARLING_API}\""
            ));
        }
        if wasm.is_none()
            && js.is_none()
            && !manifest.capabilities.contains(&Capability::Registries)
        {
            return Err(format!(
                "{WASM}: missing (only a content mod, with capability registries, may have no code)"
            ));
        }
        Ok(Package { manifest, wasm, js })
    }
    /// The `.hmod` bytes for `manifest_text` and a component.
    pub fn write(manifest_text: &str, wasm: &[u8]) -> Result<Vec<u8>, String> {
        Self::write_files(manifest_text, Some(wasm), &[])
    }
    /// The `.hmod` bytes for `manifest_text`, a component if the mod has
    /// code, and content `files` (`assets/...`, `data/...`) by path.
    pub fn write_files(
        manifest_text: &str,
        wasm: Option<&[u8]>,
        files: &[(&str, &[u8])],
    ) -> Result<Vec<u8>, String> {
        use std::io::Write;
        Manifest::parse(manifest_text)?;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let err = |e: zip::result::ZipError| e.to_string();
        let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
            zip.start_file(name, options).map_err(err)?;
            zip.write_all(bytes).map_err(|e| e.to_string())
        };
        put(MANIFEST, manifest_text.as_bytes())?;
        if let Some(wasm) = wasm {
            put(WASM, wasm)?;
        }
        for (name, bytes) in files {
            crate::content::check_path(name)?;
            put(name, bytes)?;
        }
        Ok(zip.finish().map_err(err)?.into_inner())
    }
}

/// Turns a `wasm32-unknown-unknown` cdylib built with the SDK into the
/// component the game loads.
#[cfg(feature = "pack")]
pub fn componentize(core: &[u8]) -> Result<Vec<u8>, String> {
    wit_component::ComponentEncoder::default()
        .module(core)
        .and_then(|e| e.validate(true).encode())
        .map_err(|e| format!("{e:#}"))
}

/// The names a component imports: `horizon:mods/game@0.1.0`, ...
#[cfg(feature = "pack")]
pub fn component_imports(component: &[u8]) -> Result<Vec<String>, String> {
    use wit_parser::{WorldItem, WorldKey};
    let wit_component::DecodedWasm::Component(resolve, world) =
        wit_component::decode(component).map_err(|e| format!("{e:#}"))?
    else {
        return Err("a WIT package, not a component".into());
    };
    let names = resolve.worlds[world]
        .imports
        .iter()
        .map(|(key, item)| match (key, item) {
            (WorldKey::Interface(id), _) | (_, WorldItem::Interface { id, .. }) => resolve
                .id_of(*id)
                .unwrap_or_else(|| resolve.name_world_key(key)),
            _ => resolve.name_world_key(key),
        });
    Ok(names.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
id = "coords_hud"
name = "Coordinates HUD"
version = "0.1.0"
api = "0.1"
capabilities = ["game-read", "hud"]
"#;

    #[test]
    fn parses_and_checks_imports() {
        let m = Manifest::parse(EXAMPLE).unwrap();
        assert_eq!(m.side, Side::Client);
        assert_eq!(m.capabilities, [Capability::GameRead, Capability::Hud]);
        let ok = [
            "horizon:mods/log@0.1.0",
            "horizon:mods/game@0.1.0",
            "horizon:mods/hud@0.1.2",
        ];
        assert_eq!(m.check_imports(ok), Ok(()));
        let chat = m.check_imports(["horizon:mods/chat@0.1.0"]);
        assert_eq!(
            chat,
            Err("uses capability chat, missing from horizon-mod.toml".into())
        );
        assert!(m.check_imports(["wasi:filesystem/types@0.2.0"]).is_err());
        assert!(m.check_imports(["horizon:mods/game@0.2.0"]).is_err());
        let storage = m.check_imports(["horizon:mods/storage@0.1.0"]);
        assert_eq!(
            storage,
            Err("uses capability storage, missing from horizon-mod.toml".into())
        );
        let with_storage = EXAMPLE.replace("\"hud\"", "\"hud\", \"storage\"");
        let m = Manifest::parse(&with_storage).unwrap();
        assert_eq!(m.check_imports(["horizon:mods/storage@0.1.0"]), Ok(()));
    }

    #[test]
    fn starling_imports_need_starlings_api_line() {
        // Linking starling:mods needs api = "starling-0.1": Horizon refuses that line itself.
        let horizon = Manifest::parse(EXAMPLE).unwrap();
        assert!(
            horizon
                .check_imports(["starling:mods/game@0.1.0"])
                .unwrap_err()
                .contains("starling-0.1")
        );
        let starling =
            Manifest::parse(&EXAMPLE.replace("api = \"0.1\"", "api = \"starling-0.1\"")).unwrap();
        // The device interface is always granted; the rest are capabilities as in Horizon's.
        let ok = [
            "starling:mods/log@0.1.0",
            "starling:mods/types@0.1.0",
            "starling:mods/mobile@0.1.0",
            "starling:mods/game@0.1.0",
            "starling:mods/draw@0.1.0",
        ];
        assert_eq!(starling.check_imports(ok), Ok(()));
        assert!(
            starling
                .check_imports(["starling:mods/chat@0.1.0"])
                .is_err()
        );
        assert!(
            starling
                .check_imports(["starling:mods/game@0.2.0"])
                .is_err()
        );
        assert!(
            starling
                .check_imports(["horizon:mods/mobile@0.1.0"])
                .is_err()
        );
    }

    #[test]
    fn refuses_bad_ids_and_apis() {
        let bad_id = EXAMPLE.replace("coords_hud", "Coords HUD");
        assert!(Manifest::parse(&bad_id).unwrap_err().contains("is not"));
        let bad_api = EXAMPLE.replace("api = \"0.1\"", "api = \"0.4\"");
        assert!(Manifest::parse(&bad_api).unwrap_err().contains("API 0.4"));
        let unknown = format!("{EXAMPLE}network = true\n");
        assert!(Manifest::parse(&unknown).is_err());
    }

    #[test]
    fn checks_extra_namespaces() {
        let content = "id = \"t_and_t\"\nname = \"T\"\nversion = \"1\"\napi = \"0.2\"\ncapabilities = [\"registries\"]\n";
        let with = |list: &str| Manifest::parse(&format!("{content}namespaces = [{list}]\n"));
        let m = with("\"kaisyn\", \"towns_and_towers\"").unwrap();
        let owned: Vec<&str> = m.owned_namespaces().collect();
        assert_eq!(owned, ["t_and_t", "kaisyn", "towns_and_towers"]);
        for reserved in ["minecraft", "c", "horizon"] {
            let e = with(&format!("\"{reserved}\"")).unwrap_err();
            assert!(e.contains("not a mod's to claim"), "{e}");
        }
        assert!(with("\"t_and_t\"").unwrap_err().contains("twice"));
        assert!(with("\"a\", \"a\"").unwrap_err().contains("twice"));
        assert!(with("\"Kaisyn\"").unwrap_err().contains("is not"));
        assert!(with("\"\"").unwrap_err().contains("is not"));
        let code = format!("{EXAMPLE}namespaces = [\"kaisyn\"]\n");
        assert!(Manifest::parse(&code).unwrap_err().contains("registries"));
    }

    #[test]
    fn hmod_round_trips() {
        let dir = std::env::temp_dir().join(format!("horizon-mods-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("coords_hud.hmod");
        std::fs::write(&path, Package::write(EXAMPLE, b"\0asm").unwrap()).unwrap();
        let p = Package::read(&path).unwrap();
        assert_eq!(p.manifest.id, "coords_hud");
        assert_eq!(p.wasm.as_deref(), Some(&b"\0asm"[..]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
