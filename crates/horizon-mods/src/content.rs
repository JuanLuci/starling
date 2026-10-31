//! A content mod's `assets/` and `data/` (API 0.2, docs/modpacks.md): the
//! files it adds to the game's resource tree, and the blocks and items it
//! declares in `data/<id>/horizon/block/` and `data/<id>/horizon/item/`.
//!
//! Content is data, not code: nothing here runs in the sandbox. What a mod
//! may add is fenced instead. Its own namespaces (its id and the ones its
//! manifest lists), plus additions to vanilla tags and to the convention
//! tags (`c`, which NeoForge and Fabric mods share). Paths are plain, files
//! and totals are capped, vanilla ids cannot be redefined, and no two mods
//! share a namespace ([`clash`]).
use crate::manifest::{Capability, MANIFEST, Manifest};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

/// Content files per mod, bytes per file and bytes per mod.
pub const MAX_FILES: usize = 65_536;
pub const MAX_FILE: u64 = 16 << 20;
pub const MAX_TOTAL: u64 = 512 << 20;

/// Checks a content path: `assets/` or `data/`, then `/`-separated parts of
/// `[a-z0-9_.-]`, none of them `.` or `..` (vanilla's resource-location
/// characters, so the path means the same on every OS).
pub fn check_path(name: &str) -> Result<(), String> {
    let bad = || format!("{name:?} is not a plain assets/ or data/ path");
    let mut parts = name.split('/');
    if !matches!(parts.next(), Some("assets" | "data")) {
        return Err(bad());
    }
    let mut count = 0;
    for part in parts {
        count += 1;
        let chars_ok = part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
        if part.is_empty() || part == "." || part == ".." || !chars_ok {
            return Err(bad());
        }
    }
    // assets/<namespace>/<file> at least.
    if count < 2 {
        return Err(bad());
    }
    Ok(())
}

/// One of mod `id`'s own GUI sprites, `assets/<id>/textures/gui/sprites/<path>.png`: the files a
/// mod without capability registries may ship, the icons of its buttons.
pub fn gui_sprite(name: &str, id: &str) -> bool {
    name.strip_prefix("assets/")
        .and_then(|n| n.strip_prefix(id))
        .and_then(|n| n.strip_prefix("/textures/gui/sprites/"))
        .is_some_and(|n| n.ends_with(".png") && !n.is_empty())
}

/// A vanilla identifier, `minecraft:path`.
fn vanilla_id(id: &str) -> bool {
    id.strip_prefix("minecraft:").is_some_and(|path| {
        !path.is_empty()
            && path
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_./-".contains(&b))
    })
}

/// What a declared block changes from its template; unset keeps the
/// template's value.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BlockProperties {
    /// Destroy time in seconds of hand mining (-1: unbreakable).
    pub hardness: Option<f32>,
    pub explosion_resistance: Option<f32>,
    /// Light emitted, 0 to 15.
    pub light: Option<u8>,
    pub requires_tool: Option<bool>,
    pub friction: Option<f32>,
    /// A `MapColor` id, 0 to 61.
    pub map_color: Option<u8>,
    /// A vanilla block whose sounds this block uses.
    pub sounds: Option<String>,
    /// `MODEL` (the terrain draws its model), `INVISIBLE`, or `ENTITYBLOCK_ANIMATED` (the game
    /// draws it: a kinetic part's turning model).
    pub render_shape: Option<String>,
    /// Starling: the block's own shape, for a block shaped unlike its template: boxes
    /// `[x1, y1, z1, x2, y2, z2]` in sixteenths (as model elements, -16 to 32: a block out at
    /// most), made for a state facing up (or along y) and turned to each state's `facing` (else
    /// `axis`), as Create's shapers are made. Its outline, collision and support are these
    /// boxes, and it hides none of its neighbours' faces nor their light.
    pub shape: Option<Vec<[f32; 6]>>,
}

/// The render shapes a block may declare.
pub const RENDER_SHAPES: [&str; 3] = ["MODEL", "INVISIBLE", "ENTITYBLOCK_ANIMATED"];

/// What a block is in a kinetic network (rotation passed from block to block, as Create's):
/// its part, a source's speed, and the models the game turns with it. Its axis or facing is
/// its state's `axis` or `facing` property.
#[derive(Clone, Debug, Deserialize, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Kinetic {
    pub part: KineticPart,
    /// A source's speed in RPM (a motor's always, a crank's while it is turned), at most 256
    /// either way.
    #[serde(default)]
    pub rpm: f32,
    /// The block's own model turns about its axis (the game draws it: its render shape is
    /// `ENTITYBLOCK_ANIMATED`).
    #[serde(default)]
    pub spins: bool,
    /// A model (`<ns>:block/<path>`) drawn turning at each face the part connects through,
    /// made pointing south: a gearbox's and a motor's shaft ends.
    #[serde(default)]
    pub half: Option<String>,
    /// Another model turning with the block, turned as its blockstate turns its own (a crank's
    /// handle).
    #[serde(default)]
    pub extra: Option<String>,
    /// The way `extra` is made pointing (`north`...), when it is not made as the block's own
    /// model: it is turned from there to the state's `facing` (Create's crank handle points
    /// north, its block up).
    #[serde(default)]
    pub extra_facing: Option<String>,
    /// A model the machine moves down as it works, turned as the blockstate turns its own: a
    /// press's head (Create's `mechanical_press/head`), a mixer's head, which also turns.
    #[serde(default)]
    pub head: Option<String>,
    /// A model moving down with `head` without turning: a mixer's pole.
    #[serde(default)]
    pub pole: Option<String>,
    /// More models the game draws for its part, in the order the part names them (a belt's:
    /// start, middle, end, their bottoms, its diagonals, its pulley).
    #[serde(default)]
    pub models: Vec<String>,
    /// Create's stress: what it takes from its network, in stress units per RPM it turns at
    /// (Create's `impact`: a drill's 4, a press's 8).
    #[serde(default)]
    pub stress: f32,
    /// A source's stress capacity, in stress units per RPM it turns its network at (a water
    /// wheel's 32, a crank's 8). A network taking more than its sources give stands still.
    #[serde(default)]
    pub capacity: f32,
}

/// The directions a kinetic model may be made pointing.
pub const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// `north`... as a unit vector.
pub fn direction(name: &str) -> Option<[i32; 3]> {
    Some(match name {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "down" => [0, -1, 0],
        "up" => [0, 1, 0],
        _ => return None,
    })
}

/// The quarter turns about x and y (a blockstate variant's) that point a model made pointing
/// south at face `f`: a kinetic part's `half`.
pub fn turns_to(f: [i32; 3]) -> [u8; 3] {
    turns_from([0, 0, 1], f)
}

/// The quarter turns about x and y (a blockstate variant's) that point a model made pointing
/// `made` at `f`: the first of x then y, as a variant turns its model (x first, each quarter
/// taking south up and south west).
pub fn turns_from(made: [i32; 3], f: [i32; 3]) -> [u8; 3] {
    let x = |[a, b, c]: [i32; 3]| [a, c, -b];
    let y = |[a, b, c]: [i32; 3]| [-c, b, a];
    for tx in 0..4u8 {
        let mut v = made;
        for _ in 0..tx {
            v = x(v);
        }
        for ty in 0..4u8 {
            if v == f {
                return [tx, ty, 0];
            }
            v = y(v);
        }
    }
    [0; 3]
}

/// A kinetic part: what it joins and how.
#[derive(Clone, Copy, Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KineticPart {
    /// Along its axis, both ends.
    Shaft,
    /// A shaft that also meshes with the cogwheels beside it (radius half a block).
    Cogwheel,
    /// The same, a block in radius: meshes diagonally with small ones and at right angles with
    /// large ones.
    LargeCogwheel,
    /// Joins the four faces around its axis.
    Gearbox,
    /// A source turning its facing side at `rpm`.
    Motor,
    /// A source turning its back while a player turns it (sneaking: the other way).
    Crank,
    /// Turned through its back, it breaks the block it faces (Create's mechanical drill: a
    /// step a tick past its hardness over a hundredth of its speed, ten steps a block).
    Drill,
    /// A shaft along its axis; two a block apart turning opposite ways crush the items that
    /// fall between them (Create's crushing wheels and their crushing and milling recipes).
    CrushingWheel,
    /// A small cogwheel about y that also joins the block below; turned, it mills what is put
    /// in it (Create's millstone and its milling recipes).
    Millstone,
    /// An axle along its facing's axis; turned, it presses what is below it (Create's
    /// mechanical press: an item on a depot or on the ground, a basin's items compacted).
    Press,
    /// A small cogwheel about y joining nothing else; turned above a basin, it mixes what is in
    /// it (Create's mechanical mixer).
    Mixer,
    /// Turned through its back, it blows (or pulls) air out of its front: the items in the air
    /// are pushed, and washed, blasted, smoked or haunted by what the air passes (Create's
    /// encased fan).
    Fan,
    /// An axle along its axis (or its facing's) that water flowing past its rim turns, 8 RPM
    /// the way it flows (Create's water wheel).
    WaterWheel,
    /// An axle along its axis that a redstone signal (its state's `powered`) breaks (Create's
    /// clutch).
    Clutch,
    /// An axle along its axis that a redstone signal turns the other way past it (Create's
    /// gearshift).
    Gearshift,
    /// A mechanical belt's segment (Create's belt): its pulleys (at its ends) join the shafts at
    /// their sides and turn together, and it carries items along.
    Belt,
    /// Create's mechanical saw: facing sideways, turned through its back, it cuts the block it
    /// faces as a drill does (only wood, leaves and plants) and the whole tree above it; facing
    /// up or down, an axle across it (its state's `axis_along_first`), and facing up it cuts the
    /// items put on it (cutting and stonecutting recipes).
    Saw,
}

/// What a block with work of its own keeps and does (Starling: Create's machines,
/// `game/kinetic_machines.rs`), besides any kinetic part: its items live in a block entity the
/// world saves (its id the block's), and nearby players see them.
#[derive(Clone, Copy, Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Machine {
    /// Holds a stack on top for a press or a fan to work on (Create's depot).
    Depot,
    /// Holds the items a press compacts or a mixer mixes, and their results (Create's basin).
    Basin,
    /// A belt segment (Create's belt; with the kinetic part `belt`): the belt's items are its
    /// start's.
    Belt,
    /// Burns fuel and heats the basin above it, its state's `blaze` saying how much (Create's
    /// blaze burner).
    Burner,
    /// Faces away from the inventory behind it: puts what comes in at its open side into it, or
    /// (its state's `extracting`) takes items out and drops them; `powered` pauses it (Create's
    /// funnels; a `brass` one takes up to a stack at a time).
    Funnel,
    /// Items fall through it, from the inventory above (or landing on it) to the one below, the
    /// chute it leads into (its state's `facing`: down, or a diagonal one's way), or dropped
    /// below (Create's chute).
    Chute,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockFile {
    template: String,
    #[serde(default)]
    properties: BlockProperties,
    #[serde(default = "yes")]
    item: bool,
    #[serde(default)]
    kinetic: Option<Kinetic>,
    #[serde(default)]
    machine: Option<Machine>,
    #[serde(default)]
    states: Option<std::collections::BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    fuel: Option<u32>,
    #[serde(default)]
    models: Vec<String>,
}
fn yes() -> bool {
    true
}

/// A block the mod adds: a vanilla template's behaviour, states and shapes
/// under a new id, with `properties` applied.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// `<mod id>:<path>`.
    pub id: String,
    /// The vanilla block it copies, `minecraft:<path>`.
    pub template: String,
    pub properties: BlockProperties,
    /// Also add a block item with the same id.
    pub item: bool,
    /// Its part in a kinetic network, if any.
    pub kinetic: Option<Kinetic>,
    /// The machine it is, if it keeps items of its own.
    pub machine: Option<Machine>,
    /// Starling: properties of its own, when no vanilla block has its (Create's belt, funnels,
    /// saw): each property's values, in its states' order, the first the default's. Its states
    /// take the template's behaviour and facts, each from the template's state sharing most of
    /// its properties.
    pub states: Option<std::collections::BTreeMap<String, Vec<String>>>,
    /// How long its item burns in a furnace, in ticks (0: not at all, whatever its template's
    /// does).
    pub fuel: Option<u32>,
    /// Starling: models (`<ns>:block/<path>`) the game draws for it besides its own, in the
    /// order its machine names them (a blaze burner's blaze, its rods and its flame).
    pub models: Vec<String>,
}

/// The most states a block may declare of its own.
pub const MAX_OWN_STATES: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemFile {
    template: String,
    max_stack_size: Option<u8>,
    rarity: Option<Rarity>,
    fuel: Option<u32>,
}

/// An item the mod adds: a vanilla template item's components under a new
/// id, with these changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    /// The vanilla item it copies, `minecraft:<path>`.
    pub template: String,
    pub max_stack_size: Option<u8>,
    pub rarity: Option<Rarity>,
    /// How long it burns in a furnace, in ticks (0: not at all, whatever its template does).
    pub fuel: Option<u32>,
}

#[derive(Debug)]
enum Source {
    Dir(PathBuf),
    Zip(PathBuf),
}

/// A mod's content: its files and declarations.
#[derive(Debug)]
pub struct Content {
    source: Source,
    /// Its fetched assets (Starling, [`crate::fetch`]): their folder, and the files there the
    /// mod has none of its own for.
    fetched: Option<(PathBuf, BTreeSet<String>)>,
    /// Every content file (`assets/...`, `data/...`), sorted.
    pub files: Vec<String>,
    pub blocks: Vec<Block>,
    pub items: Vec<Item>,
    /// The namespaces it owns: the mod id, then the manifest's `namespaces`.
    pub namespaces: Vec<String>,
}

/// Reads a mod's content files ([`Content::reader`]).
pub struct FileReader<'a> {
    own: OwnFiles<'a>,
    fetched: Option<(&'a Path, &'a BTreeSet<String>)>,
}
enum OwnFiles<'a> {
    Dir(&'a Path),
    Zip(Box<zip::ZipArchive<std::fs::File>>),
}
impl FileReader<'_> {
    /// One content file's bytes.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>, String> {
        check_path(name)?;
        let fetched = self
            .fetched
            .filter(|(_, names)| names.contains(name))
            .map(|(dir, _)| dir);
        let open = |root: &Path| std::fs::File::open(root.join(name));
        let reader: Box<dyn Read + '_> = match (&mut self.own, fetched) {
            (_, Some(root)) => Box::new(open(root).map_err(|e| format!("{name}: {e}"))?),
            (OwnFiles::Dir(root), None) => {
                Box::new(open(root).map_err(|e| format!("{name}: {e}"))?)
            }
            (OwnFiles::Zip(zip), None) => {
                Box::new(zip.by_name(name).map_err(|e| format!("{name}: {e}"))?)
            }
        };
        let mut out = Vec::new();
        reader
            .take(MAX_FILE + 1)
            .read_to_end(&mut out)
            .map_err(|e| format!("{name}: {e}"))?;
        if out.len() as u64 > MAX_FILE {
            return Err(format!("{name} is over {} MiB", MAX_FILE >> 20));
        }
        Ok(out)
    }
}

fn reader<'a>(
    source: &'a Source,
    fetched: &'a Option<(PathBuf, BTreeSet<String>)>,
) -> Result<FileReader<'a>, String> {
    Ok(FileReader {
        own: match source {
            Source::Dir(root) => OwnFiles::Dir(root),
            Source::Zip(path) => OwnFiles::Zip(Box::new(open_zip(path)?)),
        },
        fetched: fetched.as_ref().map(|(dir, names)| (dir.as_path(), names)),
    })
}

/// JSON text without the comments Gson's lenient reader skips: `//` and
/// `#` to the end of the line, and `/* ... */`, outside strings.
pub fn without_comments(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut string = false;
    while i < bytes.len() {
        let b = bytes[i];
        if string {
            out.push(b);
            if b == b'\\' {
                out.extend(bytes.get(i + 1));
                i += 1;
            } else if b == b'"' {
                string = false;
            }
            i += 1;
            continue;
        }
        match (b, bytes.get(i + 1)) {
            (b'"', _) => {
                string = true;
                out.push(b);
                i += 1;
            }
            (b'/', Some(b'/')) | (b'#', _) => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            (b'/', Some(b'*')) => {
                i += 2;
                while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                    i += 1;
                }
                i += 2;
                out.push(b' ');
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// Why two content mods cannot be on together: a namespace both own (one's
/// id or listed namespace is the other's), which would mix their files.
/// `a` and `b` are (mod id, owned namespaces).
pub fn clash(a: (&str, &[String]), b: (&str, &[String])) -> Option<String> {
    let shared = a.1.iter().find(|ns| b.1.contains(ns))?;
    Some(format!(
        "mods {} and {} both use the namespace {shared}; turn one of them off",
        a.0, b.0
    ))
}

impl Content {
    /// The content of the mod at `path` (a `.hmod` or a mod folder).
    pub fn read(path: &Path, manifest: &Manifest) -> Result<Content, String> {
        Self::read_with(path, manifest, None)
    }

    /// [`Content::read`], with the assets its manifest fetches when they are unpacked in
    /// `fetched` ([`crate::fetch::dir`]): a file the mod has of its own is its own.
    pub fn read_with(
        path: &Path,
        manifest: &Manifest,
        fetched: Option<&Path>,
    ) -> Result<Content, String> {
        let (source, mut sized) = if path.is_dir() {
            let mut out = Vec::new();
            for top in ["assets", "data"] {
                walk(path, &path.join(top), &mut out)?;
            }
            (Source::Dir(path.to_owned()), out)
        } else {
            let mut zip = open_zip(path)?;
            let mut out = Vec::new();
            for i in 0..zip.len() {
                let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
                let name = entry.name();
                if (name.starts_with("assets/") || name.starts_with("data/")) && !entry.is_dir() {
                    out.push((name.to_owned(), entry.size()));
                }
            }
            (Source::Zip(path.to_owned()), out)
        };
        let fetch = manifest.assets.as_ref().map(|a| &a.fetch);
        let fetched = match (fetched, fetch) {
            (Some(dir), Some(fetch)) if crate::fetch::ready(dir, fetch) => {
                let own: BTreeSet<&str> = sized.iter().map(|(n, _)| n.as_str()).collect();
                let mut found = Vec::new();
                walk(dir, &dir.join("assets"), &mut found)?;
                // Only the folders it takes, whatever else is there.
                found.retain(|(n, _)| {
                    !own.contains(n.as_str()) && fetch.take.iter().any(|t| n.starts_with(t))
                });
                let names = found.iter().map(|(n, _)| n.clone()).collect();
                sized.extend(found);
                Some((dir.to_owned(), names))
            }
            _ => None,
        };
        let mut files = Vec::with_capacity(sized.len());
        let mut total = 0u64;
        for (name, size) in sized {
            check_path(&name)?;
            if size > MAX_FILE {
                return Err(format!("{name} is over {} MiB", MAX_FILE >> 20));
            }
            total += size;
            files.push(name);
        }
        if files.len() > MAX_FILES {
            return Err(format!("more than {MAX_FILES} content files"));
        }
        if total > MAX_TOTAL {
            return Err(format!("content is over {} MiB", MAX_TOTAL >> 20));
        }
        files.sort();
        files.dedup();
        let mut content = Content {
            source,
            fetched,
            files,
            blocks: Vec::new(),
            items: Vec::new(),
            namespaces: manifest.owned_namespaces().map(str::to_owned).collect(),
        };
        content.check(manifest)?;
        Ok(content)
    }

    /// Where the files are read from: the mod's folder or its `.hmod`.
    pub fn source(&self) -> &Path {
        match &self.source {
            Source::Dir(path) | Source::Zip(path) => path,
        }
    }

    /// The folder its fetched assets are read from, when they are.
    pub fn fetched(&self) -> Option<&Path> {
        self.fetched.as_ref().map(|(dir, _)| dir.as_path())
    }

    /// No files: a mod with code only.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// One content file's bytes.
    pub fn file(&self, name: &str) -> Result<Vec<u8>, String> {
        self.reader()?.read(name)
    }

    /// A reader of its content files. A packaged mod's archive is opened
    /// once for all of them (opening it reads its whole index).
    pub fn reader(&self) -> Result<FileReader<'_>, String> {
        reader(&self.source, &self.fetched)
    }

    /// Where content may go, and the declarations.
    fn check(&mut self, manifest: &Manifest) -> Result<(), String> {
        let id = manifest.id.as_str();
        // Its own GUI sprites (its buttons' icons) any mod may have; the rest is content.
        if let Some(name) = self.files.iter().find(|f| !gui_sprite(f, id))
            && !manifest.capabilities.contains(&Capability::Registries)
        {
            return Err(format!(
                "{name}: a mod without capability registries in {MANIFEST} has its own GUI \
                 sprites only (assets/{id}/textures/gui/sprites/*.png)"
            ));
        }
        let mut blocks = Vec::new();
        let mut items = Vec::new();
        for name in &self.files {
            let mut parts = name.splitn(3, '/');
            let (top, namespace, rest) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let own = namespace == id;
            let owned = own || manifest.namespaces.iter().any(|n| n == namespace);
            // Vanilla's own files stay vanilla's; a mod may only add to its
            // tags, and to the convention tags mods share.
            let shared = matches!(namespace, "minecraft" | "c");
            let allowed = owned || (top == "data" && shared && rest.starts_with("tags/"));
            if !allowed {
                return Err(format!(
                    "{name}: a mod adds files in its own namespaces ({top}/{id}/ and those listed in namespaces) or to vanilla and convention (c) tags"
                ));
            }
            if !own && top == "data" && rest.starts_with("horizon/") {
                return Err(format!(
                    "{name}: blocks and items are declared in data/{id}/horizon/"
                ));
            }
            let Some(decl) = (own && top == "data")
                .then(|| rest.strip_prefix("horizon/"))
                .flatten()
            else {
                continue;
            };
            let parse = |kind: &str| -> Option<String> {
                let path = decl
                    .strip_prefix(kind)?
                    .strip_prefix('/')?
                    .strip_suffix(".json")?;
                Some(format!("{id}:{path}"))
            };
            if let Some(block) = parse("block") {
                blocks.push((block, name.clone()));
            } else if let Some(item) = parse("item") {
                items.push((item, name.clone()));
            } else {
                return Err(format!(
                    "{name}: data/{id}/horizon/ holds block/*.json and item/*.json only"
                ));
            }
        }
        let mut ids = BTreeSet::new();
        // One reader for every declaration: a packaged mod's archive opened per file read its
        // whole index each time (Create's 900 declarations in 10,000 files: half a minute on a
        // phone, at every start).
        let mut reader = reader(&self.source, &self.fetched)?;
        for (block, name) in blocks {
            let file: BlockFile =
                serde_json::from_slice(&reader.read(&name)?).map_err(|e| format!("{name}: {e}"))?;
            check_block(&file).map_err(|e| format!("{name}: {e}"))?;
            if file.item {
                ids.insert(block.clone());
            }
            self.blocks.push(Block {
                id: block,
                template: file.template,
                properties: file.properties,
                item: file.item,
                kinetic: file.kinetic,
                machine: file.machine,
                states: file.states,
                fuel: file.fuel,
                models: file.models,
            });
        }
        for (item, name) in items {
            let file: ItemFile =
                serde_json::from_slice(&reader.read(&name)?).map_err(|e| format!("{name}: {e}"))?;
            if !vanilla_id(&file.template) {
                return Err(format!(
                    "{name}: template {:?} is not a vanilla item (minecraft:...)",
                    file.template
                ));
            }
            if file.max_stack_size.is_some_and(|n| !(1..=99).contains(&n)) {
                return Err(format!("{name}: max_stack_size is 1 to 99"));
            }
            if !ids.insert(item.clone()) {
                return Err(format!("{name}: {item} is also a block's item"));
            }
            self.items.push(Item {
                id: item,
                template: file.template,
                max_stack_size: file.max_stack_size,
                rarity: file.rarity,
                fuel: file.fuel,
            });
        }
        Ok(())
    }
}

fn check_block(file: &BlockFile) -> Result<(), String> {
    if !vanilla_id(&file.template) {
        return Err(format!(
            "template {:?} is not a vanilla block (minecraft:...)",
            file.template
        ));
    }
    if let Some(states) = &file.states {
        let plain = |s: &str| {
            (1..=32).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        let mut count = 1usize;
        for (name, values) in states {
            if !plain(name) {
                return Err(format!("state property {name:?} is not [a-z0-9_]{{1,32}}"));
            }
            if values.is_empty() || values.iter().any(|v| !plain(v)) {
                return Err(format!(
                    "state property {name}: its values are one or more of [a-z0-9_]{{1,32}}"
                ));
            }
            let unique: BTreeSet<&String> = values.iter().collect();
            if unique.len() != values.len() {
                return Err(format!("state property {name} lists a value twice"));
            }
            count = count.saturating_mul(values.len());
        }
        if states.is_empty() || states.len() > 16 || count > MAX_OWN_STATES {
            return Err(format!(
                "states: 1 to 16 properties, at most {MAX_OWN_STATES} states in all"
            ));
        }
    }
    let p = &file.properties;
    let finite = |v: Option<f32>| v.is_none_or(f32::is_finite);
    if !finite(p.hardness) || !finite(p.explosion_resistance) || !finite(p.friction) {
        return Err("hardness, explosion_resistance and friction are numbers".into());
    }
    if p.hardness.is_some_and(|h| h < 0.0 && h != -1.0) {
        return Err("hardness is 0 or more, or -1 (unbreakable)".into());
    }
    if p.explosion_resistance.is_some_and(|r| r < 0.0) {
        return Err("explosion_resistance is 0 or more".into());
    }
    if p.friction.is_some_and(|f| !(0.0..=2.0).contains(&f)) {
        return Err("friction is 0 to 2".into());
    }
    if p.light.is_some_and(|l| l > 15) {
        return Err("light is 0 to 15".into());
    }
    if p.map_color.is_some_and(|c| c > 61) {
        return Err("map_color is a MapColor id, 0 to 61".into());
    }
    if p.sounds.as_deref().is_some_and(|s| !vanilla_id(s)) {
        return Err("sounds names a vanilla block (minecraft:...)".into());
    }
    if p.render_shape
        .as_deref()
        .is_some_and(|s| !RENDER_SHAPES.contains(&s))
    {
        return Err("render_shape is MODEL, INVISIBLE or ENTITYBLOCK_ANIMATED".into());
    }
    if p.shape.as_ref().is_some_and(|boxes| {
        boxes.is_empty()
            || boxes.iter().any(|b| {
                b.iter().any(|v| !(-16.0..=32.0).contains(v)) || (0..3).any(|i| b[i] >= b[i + 3])
            })
    }) {
        return Err(
            "shape is boxes [x1, y1, z1, x2, y2, z2] in sixteenths, -16 to 32, each \
                    from its least corner to its greatest"
                .into(),
        );
    }
    if let Some(k) = &file.kinetic {
        if !k.rpm.is_finite() || k.rpm.abs() > 256.0 {
            return Err("kinetic rpm is a number from -256 to 256".into());
        }
        let model = |m: &Option<String>| {
            m.as_deref().is_none_or(|m| {
                m.split_once(':')
                    .is_some_and(|(ns, p)| !ns.is_empty() && !p.is_empty())
            })
        };
        if !model(&k.half) || !model(&k.extra) || !model(&k.head) || !model(&k.pole) {
            return Err(
                "kinetic half, extra, head and pole name models as <namespace>:<path>".into(),
            );
        }
        if k.extra_facing
            .as_deref()
            .is_some_and(|f| !DIRECTIONS.contains(&f))
        {
            return Err("kinetic extra_facing is down, up, north, south, west or east".into());
        }
    }
    Ok(())
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(file).map_err(|e| e.to_string())
}

/// Files under `dir`, relative to `root` with `/` separators. Symbolic links
/// are not followed: a mod folder holds its own files.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) -> Result<(), String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in read.flatten() {
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            walk(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let parts: Option<Vec<&str>> = rel.iter().map(|p| p.to_str()).collect();
            let name = parts.ok_or_else(|| format!("{} is not UTF-8", rel.display()))?;
            out.push((name.join("/"), meta.len()));
        }
        if out.len() > MAX_FILES {
            return Err(format!("more than {MAX_FILES} content files"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Package;

    const MANIFEST_TEXT: &str = r#"
id = "ruby"
name = "Ruby"
version = "0.1.0"
api = "0.2"
side = "both"
capabilities = ["registries"]
"#;
    const BLOCK: &[u8] = br#"{"template": "minecraft:iron_block",
        "properties": {"hardness": 4.0, "light": 3, "sounds": "minecraft:amethyst_block"}}"#;
    const ITEM: &[u8] = br#"{"template": "minecraft:diamond", "rarity": "uncommon"}"#;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("horizon-content-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn hmod(name: &str, manifest: &str, files: &[(&str, &[u8])]) -> Result<Content, String> {
        let dir = scratch(name);
        let path = dir.join("m.hmod");
        std::fs::write(&path, Package::write_files(manifest, None, files)?).unwrap();
        let package = Package::read(&path);
        let content = package.and_then(|p| Content::read(&path, &p.manifest));
        std::fs::remove_dir_all(&dir).unwrap();
        content
    }

    #[test]
    fn content_only_mod_round_trips() {
        let files: &[(&str, &[u8])] = &[
            ("data/ruby/horizon/block/ruby_block.json", BLOCK),
            ("data/ruby/horizon/item/ruby.json", ITEM),
            ("data/ruby/recipe/ruby_block.json", b"{}"),
            ("data/minecraft/tags/block/mineable/pickaxe.json", b"{}"),
            ("data/c/tags/item/gems/ruby.json", b"{}"),
            ("assets/ruby/textures/block/ruby_block.png", b"png"),
        ];
        let content = hmod("ok", MANIFEST_TEXT, files).unwrap();
        assert_eq!(content.files.len(), 6);
        assert_eq!(
            content.blocks,
            [Block {
                id: "ruby:ruby_block".into(),
                template: "minecraft:iron_block".into(),
                properties: BlockProperties {
                    hardness: Some(4.0),
                    light: Some(3),
                    sounds: Some("minecraft:amethyst_block".into()),
                    ..BlockProperties::default()
                },
                item: true,
                kinetic: None,
                machine: None,
                states: None,
                fuel: None,
                models: Vec::new(),
            }]
        );
        assert_eq!(content.items[0].id, "ruby:ruby");
        assert_eq!(content.items[0].rarity, Some(Rarity::Uncommon));
    }

    #[test]
    fn a_kinetic_block_names_its_part_its_models_and_a_render_shape() {
        let file: BlockFile = serde_json::from_str(
            r#"{"template":"minecraft:iron_chain",
                "properties":{"render_shape":"ENTITYBLOCK_ANIMATED"},
                "kinetic":{"part":"large_cogwheel","spins":true}}"#,
        )
        .unwrap();
        assert!(check_block(&file).is_ok());
        let kinetic = file.kinetic.unwrap();
        assert_eq!(kinetic.part, KineticPart::LargeCogwheel);
        assert!(kinetic.spins && kinetic.half.is_none() && kinetic.rpm == 0.0);
        let refused =
            |json: &str| check_block(&serde_json::from_str::<BlockFile>(json).unwrap()).is_err();
        assert!(refused(
            r#"{"template":"minecraft:iron_chain","properties":{"render_shape":"SPIN"}}"#
        ));
        assert!(refused(
            r#"{"template":"minecraft:iron_chain","kinetic":{"part":"motor","rpm":300}}"#
        ));
        assert!(refused(
            r#"{"template":"minecraft:oak_log","kinetic":{"part":"gearbox","half":"shaft_half"}}"#
        ));
        assert!(refused(
            r#"{"template":"minecraft:lightning_rod","kinetic":{"part":"crank","extra_facing":"in"}}"#
        ));
        // A shape of its own: boxes in sixteenths, a block out at most, least corner first.
        let shaped: BlockFile = serde_json::from_str(
            r#"{"template":"minecraft:oak_log","properties":{"shape":[[0,6,0,16,10,16],[-8,0,6,24,16,10]]}}"#,
        )
        .unwrap();
        assert!(check_block(&shaped).is_ok());
        assert!(refused(
            r#"{"template":"minecraft:oak_log","properties":{"shape":[[0,0,0,16,40,16]]}}"#
        ));
        assert!(refused(
            r#"{"template":"minecraft:oak_log","properties":{"shape":[[8,0,0,4,16,16]]}}"#
        ));
        assert!(refused(
            r#"{"template":"minecraft:oak_log","properties":{"shape":[]}}"#
        ));
        let crank: BlockFile = serde_json::from_str(
            r#"{"template":"minecraft:lightning_rod","kinetic":{"part":"crank",
                "extra":"create:block/hand_crank/handle","extra_facing":"north"}}"#,
        )
        .unwrap();
        assert!(check_block(&crank).is_ok());
        assert_eq!(
            crank.kinetic.unwrap().extra_facing.as_deref(),
            Some("north")
        );
        // A part the game does not know is not read at all.
        let unknown = r#"{"template":"minecraft:iron_chain","kinetic":{"part":"flywheel"}}"#;
        assert!(serde_json::from_str::<BlockFile>(unknown).is_err());
    }

    #[test]
    fn folder_mod_reads_the_same() {
        let dir = scratch("folder");
        std::fs::write(dir.join(MANIFEST), MANIFEST_TEXT).unwrap();
        let block = dir.join("data/ruby/horizon/block");
        std::fs::create_dir_all(&block).unwrap();
        std::fs::write(block.join("ruby_block.json"), BLOCK).unwrap();
        let package = Package::read(&dir).unwrap();
        assert!(package.wasm.is_none());
        let content = Content::read(&dir, &package.manifest).unwrap();
        assert_eq!(content.files, ["data/ruby/horizon/block/ruby_block.json"]);
        assert_eq!(content.file(&content.files[0]).unwrap(), BLOCK);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refuses_content_outside_its_fence() {
        let refused = |name: &str, files: &[(&str, &[u8])]| {
            let e = hmod(name, MANIFEST_TEXT, files).unwrap_err();
            assert!(!e.is_empty());
            e
        };
        let e = refused("other-ns", &[("data/other/horizon/block/a.json", BLOCK)]);
        assert!(e.contains("own namespaces"), "{e}");
        refused(
            "vanilla-asset",
            &[("assets/minecraft/textures/block/stone.png", b"x")],
        );
        refused(
            "vanilla-recipe",
            &[("data/minecraft/recipe/stone.json", b"{}")],
        );
        refused("convention-recipe", &[("data/c/recipe/stone.json", b"{}")]);
        refused("convention-asset", &[("assets/c/lang/en_us.json", b"{}")]);
        let e = refused(
            "modded-template",
            &[(
                "data/ruby/horizon/block/a.json",
                br#"{"template": "ruby:other"}"#,
            )],
        );
        assert!(e.contains("not a vanilla block"), "{e}");
        refused(
            "light",
            &[(
                "data/ruby/horizon/block/a.json",
                br#"{"template": "minecraft:stone", "properties": {"light": 16}}"#,
            )],
        );
        refused(
            "unknown-key",
            &[(
                "data/ruby/horizon/block/a.json",
                br#"{"template": "minecraft:stone", "glow": true}"#,
            )],
        );
        let e = refused(
            "clash",
            &[
                ("data/ruby/horizon/block/a.json", BLOCK),
                ("data/ruby/horizon/item/a.json", ITEM),
            ],
        );
        assert!(e.contains("also a block's item"), "{e}");
        refused("other-kind", &[("data/ruby/horizon/entity/a.json", b"{}")]);
        refused(
            "stack",
            &[(
                "data/ruby/horizon/item/a.json",
                br#"{"template": "minecraft:stick", "max_stack_size": 100}"#,
            )],
        );
    }

    #[test]
    fn listed_namespaces_hold_content() {
        let manifest = format!("{MANIFEST_TEXT}namespaces = [\"kaisyn\"]\n");
        let files: &[(&str, &[u8])] = &[
            ("data/kaisyn/worldgen/template_pool/a.json", b"{}"),
            ("data/kaisyn/structure/a.nbt", b"nbt"),
            ("assets/kaisyn/lang/en_us.json", b"{}"),
        ];
        let content = hmod("listed", &manifest, files).unwrap();
        assert_eq!(content.files.len(), 3);
        assert_eq!(content.namespaces, ["ruby", "kaisyn"]);
        // Declarations stay in the id's namespace.
        let decl: &[(&str, &[u8])] = &[("data/kaisyn/horizon/block/a.json", BLOCK)];
        let e = hmod("listed-decl", &manifest, decl).unwrap_err();
        assert!(e.contains("declared in data/ruby/horizon/"), "{e}");
        // A namespace it does not list is still outside.
        let other: &[(&str, &[u8])] = &[("data/other/structure/a.nbt", b"nbt")];
        assert!(hmod("listed-other", &manifest, other).is_err());
    }

    #[test]
    fn mods_sharing_a_namespace_clash() {
        let t_and_t = ["t_and_t".to_owned(), "kaisyn".to_owned()];
        let kaisyn = ["kaisyn".to_owned()];
        let ruby = ["ruby".to_owned(), "gems".to_owned()];
        assert_eq!(
            clash(("t_and_t", &t_and_t), ("kaisyn", &kaisyn)).as_deref(),
            Some("mods t_and_t and kaisyn both use the namespace kaisyn; turn one of them off")
        );
        assert_eq!(clash(("t_and_t", &t_and_t), ("ruby", &ruby)), None);
    }

    #[test]
    fn content_needs_the_registries_capability_but_gui_sprites() {
        let code_only = MANIFEST_TEXT
            .replace("api = \"0.2\"", "api = \"0.1\"")
            .replace("capabilities = [\"registries\"]", "capabilities = []");
        // No code and no registries: nothing to load.
        let e = hmod("empty", &code_only, &[]).unwrap_err();
        assert!(e.contains("mod.wasm: missing"), "{e}");
        // Registries on API 0.1.
        let old = MANIFEST_TEXT.replace("api = \"0.2\"", "api = \"0.1\"");
        assert!(Manifest::parse(&old).unwrap_err().contains("needs api"));
        // Content without the capability, in a mod with code.
        let dir = scratch("no-cap");
        let path = dir.join("m.hmod");
        let files: &[(&str, &[u8])] = &[("assets/ruby/lang/en_us.json", b"{}")];
        std::fs::write(
            &path,
            Package::write_files(&code_only, Some(b"\0asm"), files).unwrap(),
        )
        .unwrap();
        let package = Package::read(&path).unwrap();
        let e = Content::read(&path, &package.manifest).err().unwrap();
        assert!(e.contains("without capability registries"), "{e}");
        // Its own GUI sprites (its buttons' icons) a mod has without it; another namespace's not.
        let sprites: &[(&str, &[u8])] = &[("assets/ruby/textures/gui/sprites/sort.png", b"png")];
        std::fs::write(
            &path,
            Package::write_files(&code_only, Some(b"\0asm"), sprites).unwrap(),
        )
        .unwrap();
        let content = Content::read(&path, &package.manifest).unwrap();
        assert_eq!(content.files, ["assets/ruby/textures/gui/sprites/sort.png"]);
        let theirs: &[(&str, &[u8])] = &[("assets/opal/textures/gui/sprites/sort.png", b"png")];
        std::fs::write(
            &path,
            Package::write_files(&code_only, Some(b"\0asm"), theirs).unwrap(),
        )
        .unwrap();
        assert!(Content::read(&path, &package.manifest).is_err());
        assert!(gui_sprite(
            "assets/ruby/textures/gui/sprites/a/b.png",
            "ruby"
        ));
        assert!(!gui_sprite(
            "assets/ruby/textures/gui/sprites/a.json",
            "ruby"
        ));
        assert!(!gui_sprite("assets/ruby/textures/block/a.png", "ruby"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paths_are_plain() {
        assert!(check_path("assets/ruby/textures/block/ruby_block.png").is_ok());
        assert!(check_path("data/ruby/loot_table/blocks/a.json").is_ok());
        for bad in [
            "assets/ruby",
            "assets/../x/y",
            "assets/ruby//a",
            "assets/Ruby/a",
            "assets/ruby/a b",
            "/assets/ruby/a",
            "other/ruby/a",
            "assets\\ruby\\a",
        ] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
    }
}
