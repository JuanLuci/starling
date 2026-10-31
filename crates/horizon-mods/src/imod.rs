//! Inox mods (`.imod`, docs/inox-mods.md): blocks and items declared in one
//! `inox.toml`, built into the game by the content-mod loader.
//!
//! An `.imod` is a zip (or a folder) holding `inox.toml`, the mod's textures
//! under `assets/<id>/textures/`, and optionally any `assets/` and `data/`
//! file a content mod may hold. From the declarations this module writes, in
//! memory, the files a content mod would carry by hand: the block and item
//! declarations in `data/<id>/horizon/`, blockstates, block and item models,
//! item definitions, the English names, loot tables and tool tags. The mod's
//! own files win over generated ones at the same path, except for the
//! language file and tags, which are merged. Everything then goes through the
//! same checks and loader as a `.hmod` (`content.rs`, `mod_content.rs`).
use crate::manifest::{MANIFEST, Manifest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// The declarations' file name in an `.imod` or an Inox mod folder.
pub const IMOD_MANIFEST: &str = "inox.toml";
/// An Inox mod file's extension.
pub const EXTENSION: &str = "imod";

/// Whether `path` is an Inox mod: an `.imod` file or a folder holding
/// `inox.toml`.
pub fn is_imod(path: &Path) -> bool {
    if path.is_dir() {
        path.join(IMOD_MANIFEST).is_file()
    } else {
        path.extension().is_some_and(|e| e == EXTENSION)
    }
}

/// `inox.toml`'s text, from an `.imod` or a mod folder.
pub fn read_toml(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let bytes = if path.is_dir() {
        std::fs::read(path.join(IMOD_MANIFEST)).map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?
    } else {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let entry = zip
            .by_name(IMOD_MANIFEST)
            .map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?;
        let mut out = Vec::new();
        // Declarations are text: 1 MiB is far more than any needs.
        entry
            .take(1 << 20)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        out
    };
    String::from_utf8(bytes).map_err(|_| format!("{IMOD_MANIFEST}: not UTF-8"))
}

/// The `.imod` bytes for `inox.toml`'s text and the mod's own `files`
/// (`assets/...`, `data/...`) by path. Checks the declarations parse.
pub fn write(toml_text: &str, files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    parse(toml_text)?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    put(IMOD_MANIFEST, toml_text.as_bytes())?;
    for (name, bytes) in files {
        crate::content::check_path(name)?;
        put(name, bytes)?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// `inox.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InoxFile {
    #[serde(rename = "mod")]
    pub meta: ModMeta,
    /// Blocks by name: `[block.ruby_block]` is `<id>:ruby_block`.
    #[serde(default)]
    pub block: BTreeMap<String, BlockDecl>,
    /// Items by name: `[item.ruby]` is `<id>:ruby`.
    #[serde(default)]
    pub item: BTreeMap<String, ItemDecl>,
    /// Ore veins by name: `[ore.ruby_ore]` is the configured and placed
    /// feature `<id>:ruby_ore`, added to the biomes it names.
    #[serde(default)]
    pub ore: BTreeMap<String, OreDecl>,
    /// Armour sets by name: `[armor.ruby]` makes `<id>:ruby_helmet`,
    /// `_chestplate`, `_leggings` and `_boots`, and the equipment asset `<id>:ruby`.
    #[serde(default)]
    pub armor: BTreeMap<String, ArmorDecl>,
    /// Tool sets by name: `[tools.ruby]` makes `<id>:ruby_sword`, `_pickaxe`,
    /// `_axe`, `_shovel` and `_hoe`.
    #[serde(default)]
    pub tools: BTreeMap<String, ToolsDecl>,
    /// Tags by registry, then name: `[tag.item.gems]` is the item tag
    /// `<id>:gems`, `[tag.block."minecraft:beacon_base_blocks"]` adds to vanilla's.
    #[serde(default)]
    pub tag: BTreeMap<String, BTreeMap<String, TagDecl>>,
    /// Creative inventory tabs by name: `[tab.ruby]` is the tab `<id>:ruby`.
    /// Without any, the mod gets one tab holding all its items.
    #[serde(default)]
    pub tab: BTreeMap<String, TabDecl>,
}

/// `[mod]`: who the mod is, as Options > Mods lists it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModMeta {
    /// `[a-z0-9_]`, 1 to 64 characters: its namespace, settings key and log name.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// More namespaces its raw `assets/` and `data/` files use.
    #[serde(default)]
    pub namespaces: Vec<String>,
}

/// A block: a vanilla template's behaviour with its own look, name and numbers.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BlockDecl {
    /// The vanilla block it behaves as (`minecraft:iron_block`).
    pub template: String,
    /// Its English name; the name in title case when left out.
    pub name: Option<String>,
    /// `assets/<id>/textures/block/<texture>.png`, or a full texture id with
    /// `:` (`minecraft:block/stone`). The block's name when left out.
    pub texture: Option<String>,
    /// How the block model is made: `cube_all` (one texture on every face,
    /// the default) or `none` (the mod ships its own blockstate and model).
    pub model: Option<String>,
    pub hardness: Option<f32>,
    pub explosion_resistance: Option<f32>,
    pub light: Option<u8>,
    pub requires_tool: Option<bool>,
    pub friction: Option<f32>,
    pub map_color: Option<u8>,
    /// The vanilla block whose sounds it plays.
    pub sounds: Option<String>,
    /// Whether it has an item (placeable, in the creative inventory). Default true.
    #[serde(default = "yes")]
    pub item: bool,
    /// What breaking it drops: `self` (default), `none`, or an item id.
    pub drops: Option<String>,
    /// The tool that mines it fast: `pickaxe`, `axe`, `shovel` or `hoe`.
    pub tool: Option<String>,
    /// The tool tier it needs: `stone`, `iron` or `diamond`.
    pub tier: Option<String>,
    /// Block tags it joins (`minecraft:beacon_base_blocks`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Other creative tabs its item is also in (`minecraft:building_blocks`).
    #[serde(default)]
    pub creative_tabs: Vec<String>,
}

/// An item: a vanilla template's behaviour with its own look, name and numbers.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ItemDecl {
    /// The vanilla item it behaves as (`minecraft:diamond`).
    pub template: String,
    pub name: Option<String>,
    /// `assets/<id>/textures/item/<texture>.png`, or a full texture id with `:`.
    pub texture: Option<String>,
    /// `generated` (flat, the default), `handheld` (held like a tool), or
    /// `none` (the mod ships its own item model and definition).
    pub model: Option<String>,
    pub max_stack_size: Option<u8>,
    /// `common`, `uncommon`, `rare` or `epic`.
    pub rarity: Option<String>,
    /// Uses before it breaks (`minecraft:max_damage`).
    pub durability: Option<u32>,
    /// Attack damage and attack speed as the tooltip shows them (an iron
    /// sword's are 6 and 1.6): the main hand's `minecraft:attribute_modifiers`.
    /// Both or neither, since they replace the template's modifiers.
    pub attack_damage: Option<f64>,
    pub attack_speed: Option<f64>,
    /// What repairs it in an anvil: an item id or a `#` item tag (`minecraft:repairable`).
    pub repair: Option<String>,
    /// How well it enchants (`minecraft:enchantable`; iron tools 14, iron armour 9).
    pub enchantability: Option<u32>,
    /// Makes it a food (`minecraft:food` and `minecraft:consumable`).
    pub food: Option<FoodDecl>,
    /// Item tags it joins (`minecraft:swords`, `c:gems`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Other creative tabs it is also in (`minecraft:combat`).
    #[serde(default)]
    pub creative_tabs: Vec<String>,
    /// Data components set (`minecraft:max_damage`) or removed (`!minecraft:repairable`).
    #[serde(default)]
    pub components: BTreeMap<String, Value>,
}

/// `food` of an item: what eating it gives.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FoodDecl {
    /// Hunger points (half drumsticks) it fills.
    pub nutrition: u32,
    /// Saturation points it gives (a golden apple's are 9.6, bread's 6.0).
    pub saturation: f32,
    /// Edible with a full hunger bar.
    #[serde(default)]
    pub always_eat: bool,
    /// Seconds to eat it (1.6 by default).
    pub eat_seconds: Option<f32>,
    /// Effects it gives.
    #[serde(default)]
    pub effects: Vec<EffectDecl>,
}

/// An effect a food gives.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EffectDecl {
    /// A vanilla effect (`minecraft:speed`).
    pub id: String,
    pub seconds: f32,
    /// 1 is level I (the default).
    pub level: Option<u8>,
    /// 0 to 1, the chance it is given (1 by default).
    pub chance: Option<f32>,
}

/// An ore vein: where and how often a block replaces stone underground.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OreDecl {
    /// The block it places in stone: a block of this mod by name, or an id.
    pub block: String,
    /// The block it places in deepslate, tuff and the like; none by default
    /// (the vein then skips deepslate, as emerald ore's does).
    pub deepslate_block: Option<String>,
    /// Blocks in a vein, 1 to 64 (iron's is 9, diamond's 4 to 12).
    pub size: u32,
    /// Veins tried per chunk.
    pub count: u32,
    /// The lowest and highest Y a vein starts at.
    pub min_y: i32,
    pub max_y: i32,
    /// How heights spread: `uniform` (default) or `triangle` (most veins
    /// in the middle of the range, as iron's and diamond's).
    pub shape: Option<String>,
    /// 0 to 1: the chance a block of the vein touching air is skipped
    /// (0 by default; diamond's 0.5 hides it from caves).
    pub discard_on_air: Option<f32>,
    /// The biomes it generates in: an id, a `#` biome tag or a list of them.
    /// `#minecraft:is_overworld` by default.
    pub biomes: Option<Value>,
}

/// An armour set: four pieces copying iron armour with their own numbers,
/// textures and equipment asset.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArmorDecl {
    /// The set's name before each piece's (`Ruby` makes `Ruby Helmet`);
    /// the set's name in title case by default.
    pub name: Option<String>,
    /// The durability multiplier, as vanilla's armour materials have it
    /// (iron 15, diamond 33, netherite 37): a helmet lasts 11 times it, a
    /// chestplate 16, leggings 15, boots 13.
    pub durability: u32,
    /// Armour points of the helmet, chestplate, leggings and boots (iron's
    /// are `[2, 6, 5, 2]`, diamond's `[3, 8, 6, 3]`).
    pub defense: [u32; 4],
    /// Armour toughness of each piece (diamond 2, netherite 3).
    pub toughness: Option<f64>,
    /// Knockback resistance of each piece, 0 to 1 (netherite 0.1).
    pub knockback_resistance: Option<f64>,
    /// What repairs it: an item id or a `#` item tag.
    pub repair: Option<String>,
    pub enchantability: Option<u32>,
    /// The sound of putting a piece on (`minecraft:item.armor.equip_iron` by default).
    pub equip_sound: Option<String>,
    pub rarity: Option<String>,
    /// The pieces to make, all four by default.
    pub pieces: Option<Vec<String>>,
    /// Other creative tabs the pieces are also in (`minecraft:combat`).
    #[serde(default)]
    pub creative_tabs: Vec<String>,
}

/// A tool set: a sword, pickaxe, axe, shovel and hoe copying a vanilla tier's,
/// with their own numbers.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolsDecl {
    /// The set's name before each tool's (`Ruby` makes `Ruby Pickaxe`).
    pub name: Option<String>,
    /// The vanilla tier whose blocks they can mine and whose tools they copy:
    /// `wood`, `stone`, `iron`, `gold`, `diamond` or `netherite`.
    pub tier: String,
    /// Uses before a tool breaks (iron 250, diamond 1561, netherite 2031).
    pub durability: u32,
    /// Mining speed on the blocks a tool is for (iron 6, diamond 8, netherite 9, gold 12).
    pub speed: f32,
    /// The tier's attack damage bonus (wood and gold 0, stone 1, iron 2,
    /// diamond 3, netherite 4): an iron sword deals 4 + 2.
    pub damage: f64,
    /// What repairs them: an item id or a `#` item tag (the tier's by default).
    pub repair: Option<String>,
    pub enchantability: Option<u32>,
    pub rarity: Option<String>,
    /// The tools to make, all five by default.
    pub pieces: Option<Vec<String>>,
    /// Other creative tabs the tools are also in (`minecraft:tools_and_utilities`).
    #[serde(default)]
    pub creative_tabs: Vec<String>,
}

/// A tag: its values (ids, names of this mod's things, `#` tags), added to
/// what others put in it, or replacing it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TagDecl {
    pub values: Vec<String>,
    /// Drops what vanilla and the mods before put in it.
    #[serde(default)]
    pub replace: bool,
}

/// A creative inventory tab of the mod's own, on the pages after vanilla's.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TabDecl {
    /// Its title; the mod's name by default.
    pub name: Option<String>,
    /// The item on its button; its first item by default.
    pub icon: Option<String>,
    /// What it holds, in order; every item of the mod by default.
    pub items: Option<Vec<String>>,
}

/// A tool: its name, the blocks it is fast on (none for the sword), its
/// item tag, and the attack damage over the tier's bonus and the attack
/// speed as the tooltip shows them.
const TOOLS: [(&str, Option<&str>, &str, f64, f64); 5] = [
    ("sword", None, "swords", 4.0, 1.6),
    ("pickaxe", Some("mineable/pickaxe"), "pickaxes", 2.0, 1.2),
    ("axe", Some("mineable/axe"), "axes", 7.0, 0.9),
    ("shovel", Some("mineable/shovel"), "shovels", 2.5, 1.0),
    ("hoe", Some("mineable/hoe"), "hoes", 1.0, 1.0),
];
/// A tier: its name in `inox.toml`, and in vanilla's item and tag ids.
const TIERS: [(&str, &str, &str); 6] = [
    ("wood", "wooden", "wooden"),
    ("stone", "stone", "stone"),
    ("iron", "iron", "iron"),
    ("gold", "golden", "gold"),
    ("diamond", "diamond", "diamond"),
    ("netherite", "netherite", "netherite"),
];

/// An armour piece: its name suffix, the iron piece it copies, its slot,
/// vanilla's durability factor and its armour tag.
const PIECES: [(&str, &str, u32, &str); 4] = [
    ("helmet", "head", 11, "head_armor"),
    ("chestplate", "chest", 16, "chest_armor"),
    ("leggings", "legs", 15, "leg_armor"),
    ("boots", "feet", 13, "foot_armor"),
];

fn yes() -> bool {
    true
}

/// Parses `inox.toml`: the manifest the content loader checks a mod with
/// (a content mod, `api = "0.2"`, capability `registries`), and the
/// declarations.
pub fn parse(text: &str) -> Result<(Manifest, InoxFile), String> {
    let file: InoxFile = toml::from_str(text).map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?;
    let m = &file.meta;
    // Build the target manifest through its own parser: this branch validates manifests in
    // Manifest::parse (there is no Manifest::validated method on this revision).
    let mut fields = toml::map::Map::new();
    fields.insert("id".into(), toml::Value::String(m.id.clone()));
    fields.insert("name".into(), toml::Value::String(m.name.clone()));
    fields.insert("version".into(), toml::Value::String(m.version.clone()));
    fields.insert(
        "description".into(),
        toml::Value::String(m.description.clone()),
    );
    fields.insert(
        "authors".into(),
        toml::Value::Array(m.authors.iter().cloned().map(toml::Value::String).collect()),
    );
    fields.insert("api".into(), toml::Value::String("0.2".into()));
    fields.insert("side".into(), toml::Value::String("both".into()));
    fields.insert(
        "capabilities".into(),
        toml::Value::Array(vec![toml::Value::String("registries".into())]),
    );
    fields.insert(
        "namespaces".into(),
        toml::Value::Array(
            m.namespaces
                .iter()
                .cloned()
                .map(toml::Value::String)
                .collect(),
        ),
    );
    let text = toml::to_string(&toml::Value::Table(fields))
        .map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?;
    let manifest = Manifest::parse(&text).map_err(|e| e.replace(MANIFEST, IMOD_MANIFEST))?;
    let names = file.block.keys().chain(file.item.keys());
    let sets = file.armor.keys().chain(file.tools.keys());
    for name in names
        .chain(file.ore.keys())
        .chain(sets)
        .chain(file.tab.keys())
    {
        check_name(name)?;
    }
    if let Some(n) = file.block.keys().find(|n| file.item.contains_key(*n)) {
        return Err(format!(
            "{IMOD_MANIFEST}: {n} is both a block and an item (a block's item has its name)"
        ));
    }
    for (set, armor) in &file.armor {
        for piece in armor_pieces(set, armor)? {
            let n = format!("{set}_{piece}");
            if file.block.contains_key(&n) || file.item.contains_key(&n) {
                return Err(format!(
                    "{IMOD_MANIFEST}: armor {set} makes {n}, which is also declared"
                ));
            }
        }
    }
    for (set, tools) in &file.tools {
        for piece in tool_pieces(set, tools)? {
            let n = format!("{set}_{piece}");
            let armor = file.armor.iter().any(|(a, d)| {
                armor_pieces(a, d).is_ok_and(|p| p.iter().any(|p| format!("{a}_{p}") == n))
            });
            if file.block.contains_key(&n) || file.item.contains_key(&n) || armor {
                return Err(format!(
                    "{IMOD_MANIFEST}: tools {set} makes {n}, which is also declared"
                ));
            }
        }
    }
    Ok((manifest, file))
}

/// A block or item name: `[a-z0-9_]`, 1 to 64 characters.
fn check_name(name: &str) -> Result<(), String> {
    let ok = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{IMOD_MANIFEST}: name {name:?} is not [a-z0-9_]{{1,64}}"
        ))
    }
}

/// `ruby_block` -> `Ruby Block`.
fn title(name: &str) -> String {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A texture id, and the file it needs in the mod when it is the mod's own.
fn texture(id: &str, kind: &str, name: &str, given: Option<&str>) -> (String, Option<String>) {
    let t = given.unwrap_or(name);
    if t.contains(':') {
        (t.to_owned(), None)
    } else {
        (
            format!("{id}:{kind}/{t}"),
            Some(format!("assets/{id}/textures/{kind}/{t}.png")),
        )
    }
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut out = serde_json::to_vec_pretty(v).unwrap_or_default();
    out.push(b'\n');
    out
}

/// The pieces an armour set makes, checked.
fn armor_pieces<'a>(set: &str, armor: &'a ArmorDecl) -> Result<Vec<&'a str>, String> {
    let all = PIECES.map(|p| p.0);
    let pieces: Vec<&str> = match &armor.pieces {
        Some(list) => list.iter().map(String::as_str).collect(),
        None => all.to_vec(),
    };
    match pieces.iter().find(|p| !all.contains(p)) {
        Some(p) => Err(format!(
            "{IMOD_MANIFEST}: armor {set}: piece {p:?} is helmet, chestplate, leggings or boots"
        )),
        None if pieces.is_empty() => Err(format!("{IMOD_MANIFEST}: armor {set}: no pieces")),
        None => Ok(pieces),
    }
}

/// A block or item reference in `inox.toml`: a name of this mod's, or an id.
fn full_id(id: &str, name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("{id}:{name}")
    }
}

/// A tag id's file: `minecraft:swords` is `data/minecraft/tags/<kind>/swords.json`.
fn tag_file(kind: &str, tag: &str, what: &str) -> Result<String, String> {
    let (ns, path) = tag
        .strip_prefix('#')
        .unwrap_or(tag)
        .split_once(':')
        .ok_or_else(|| format!("{what}: tag {tag:?} is <namespace>:<path>"))?;
    Ok(format!("data/{ns}/tags/{kind}/{path}.json"))
}

/// The components an item's keys make (`durability`, `attack_damage`...),
/// with the ones it sets itself in `components`. A component set both ways is
/// refused.
fn item_components(id: &str, i: &ItemDecl, what: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut made: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(d) = i.durability {
        if d == 0 {
            return Err(format!("{what}: durability is 1 or more"));
        }
        made.insert("minecraft:max_damage".into(), d.into());
    }
    match (i.attack_damage, i.attack_speed) {
        (Some(damage), Some(speed)) => {
            if !damage.is_finite() || !speed.is_finite() || damage < 0.0 || speed <= 0.0 {
                return Err(format!(
                    "{what}: attack_damage is 0 or more and attack_speed more than 0"
                ));
            }
            // The player's own 1 damage and 4 attacks a second come first.
            made.insert(
                "minecraft:attribute_modifiers".into(),
                json!([
                    { "type": "minecraft:attack_damage", "id": "minecraft:base_attack_damage",
                      "amount": damage - 1.0, "operation": "add_value", "slot": "mainhand" },
                    { "type": "minecraft:attack_speed", "id": "minecraft:base_attack_speed",
                      "amount": speed - 4.0, "operation": "add_value", "slot": "mainhand" }
                ]),
            );
        }
        (None, None) => {}
        _ => {
            return Err(format!(
                "{what}: set attack_damage and attack_speed together"
            ));
        }
    }
    if let Some(r) = &i.repair {
        made.insert(
            "minecraft:repairable".into(),
            json!({ "items": full_ref(id, r) }),
        );
    }
    if let Some(e) = i.enchantability {
        made.insert("minecraft:enchantable".into(), json!({ "value": e }));
    }
    if let Some(f) = &i.food {
        if !f.saturation.is_finite() || f.saturation < 0.0 {
            return Err(format!("{what}: food saturation is 0 or more"));
        }
        let seconds = f.eat_seconds.unwrap_or(1.6);
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(format!("{what}: food eat_seconds is 0 or more"));
        }
        made.insert(
            "minecraft:food".into(),
            json!({ "nutrition": f.nutrition, "saturation": f.saturation,
                    "can_always_eat": f.always_eat }),
        );
        let mut effects = Vec::new();
        for e in &f.effects {
            let chance = e.chance.unwrap_or(1.0);
            if !(0.0..=1.0).contains(&chance) || !e.seconds.is_finite() || e.seconds <= 0.0 {
                return Err(format!(
                    "{what}: effect {}: seconds is more than 0 and chance 0 to 1",
                    e.id
                ));
            }
            let level = e.level.unwrap_or(1);
            if level == 0 {
                return Err(format!("{what}: effect {}: level is 1 or more", e.id));
            }
            effects.push(json!({
                "type": "minecraft:apply_effects",
                "effects": [{ "id": e.id, "duration": (e.seconds * 20.0).round() as u32,
                              "amplifier": level - 1, "show_icon": true }],
                "probability": chance
            }));
        }
        made.insert(
            "minecraft:consumable".into(),
            json!({ "consume_seconds": seconds, "animation": "eat",
                    "sound": "minecraft:entity.generic.eat", "has_consume_particles": true,
                    "on_consume_effects": effects }),
        );
    }
    for (key, value) in &i.components {
        let kind = key.strip_prefix('!').unwrap_or(key);
        if made.contains_key(kind) {
            return Err(format!(
                "{what}: component {key} is also set by one of its keys (durability, attack_damage, repair, enchantability, food)"
            ));
        }
        made.insert(key.clone(), value.clone());
    }
    Ok(made)
}

/// The tools a set makes, checked.
fn tool_pieces<'a>(set: &str, tools: &'a ToolsDecl) -> Result<Vec<&'a str>, String> {
    let all = TOOLS.map(|t| t.0);
    let pieces: Vec<&str> = match &tools.pieces {
        Some(list) => list.iter().map(String::as_str).collect(),
        None => all.to_vec(),
    };
    match pieces.iter().find(|p| !all.contains(p)) {
        Some(p) => Err(format!(
            "{IMOD_MANIFEST}: tools {set}: piece {p:?} is sword, pickaxe, axe, shovel or hoe"
        )),
        None if pieces.is_empty() => Err(format!("{IMOD_MANIFEST}: tools {set}: no pieces")),
        None => Ok(pieces),
    }
}

/// A tag value: a `#` tag, an id, or a name of this mod's.
fn full_ref(id: &str, r: &str) -> String {
    match r.strip_prefix('#') {
        Some(tag) => format!("#{}", full_id(id, tag)),
        None => full_id(id, r),
    }
}

/// A tool set's tools as item declarations.
fn tool_items(id: &str, set: &str, t: &ToolsDecl) -> Result<Vec<(String, ItemDecl)>, String> {
    let what = format!("tools {id}:{set}");
    let Some(&(_, prefix, tag_tier)) = TIERS.iter().find(|x| x.0 == t.tier) else {
        return Err(format!(
            "{what}: tier {:?} is wood, stone, iron, gold, diamond or netherite",
            t.tier
        ));
    };
    if t.durability == 0 || !t.speed.is_finite() || t.speed <= 0.0 {
        return Err(format!(
            "{what}: durability is 1 or more and speed more than 0"
        ));
    }
    if !t.damage.is_finite() || t.damage < 0.0 {
        return Err(format!("{what}: damage is 0 or more"));
    }
    let title_name = t.name.clone().unwrap_or_else(|| title(set));
    let pieces = tool_pieces(set, t)?;
    let mut items = Vec::new();
    for &(piece, mineable, tag, damage, speed) in &TOOLS {
        if !pieces.contains(&piece) {
            continue;
        }
        // The hoe deals no tier bonus but swings faster with it, as vanilla's.
        let (attack, swing) = match piece {
            "hoe" => (damage, (speed + t.damage).min(4.0)),
            _ => (damage + t.damage, speed),
        };
        let mut components = BTreeMap::new();
        if let Some(blocks) = mineable {
            components.insert(
                "minecraft:tool".to_owned(),
                json!({ "rules": [
                    { "blocks": format!("#minecraft:incorrect_for_{tag_tier}_tool"),
                      "correct_for_drops": false },
                    { "blocks": format!("#minecraft:{blocks}"), "correct_for_drops": true,
                      "speed": t.speed }
                ], "damage_per_block": 1 }),
            );
        }
        items.push((
            format!("{set}_{piece}"),
            ItemDecl {
                template: format!("minecraft:{prefix}_{piece}"),
                name: Some(format!("{title_name} {}", title(piece))),
                model: Some("handheld".into()),
                rarity: t.rarity.clone(),
                durability: Some(t.durability),
                attack_damage: Some(attack),
                attack_speed: Some(swing),
                repair: t.repair.clone(),
                enchantability: t.enchantability,
                tags: vec![format!("minecraft:{tag}")],
                creative_tabs: t.creative_tabs.clone(),
                components,
                ..ItemDecl::default()
            },
        ));
    }
    Ok(items)
}

/// An armour set's pieces as item declarations, and its equipment asset.
fn armor_items(
    id: &str,
    set: &str,
    a: &ArmorDecl,
) -> Result<(Vec<(String, ItemDecl)>, Value), String> {
    let what = format!("armor {id}:{set}");
    if a.durability == 0 {
        return Err(format!("{what}: durability is 1 or more"));
    }
    let toughness = a.toughness.unwrap_or(0.0);
    let knockback = a.knockback_resistance.unwrap_or(0.0);
    if !toughness.is_finite() || toughness < 0.0 || !(0.0..=1.0).contains(&knockback) {
        return Err(format!(
            "{what}: toughness is 0 or more and knockback_resistance 0 to 1"
        ));
    }
    let title_name = a.name.clone().unwrap_or_else(|| title(set));
    let pieces = armor_pieces(set, a)?;
    let mut items = Vec::new();
    let mut leggings = false;
    let mut others = false;
    for (i, &(piece, slot, factor, tag)) in PIECES.iter().enumerate() {
        if !pieces.contains(&piece) {
            continue;
        }
        if piece == "leggings" {
            leggings = true;
        } else {
            others = true;
        }
        let modifier = |kind: &str, amount: f64| {
            json!({ "type": kind, "id": format!("minecraft:armor.{piece}"), "amount": amount,
                    "operation": "add_value", "slot": slot })
        };
        let mut modifiers = vec![modifier("minecraft:armor", f64::from(a.defense[i]))];
        if toughness > 0.0 {
            modifiers.push(modifier("minecraft:armor_toughness", toughness));
        }
        if knockback > 0.0 {
            modifiers.push(modifier("minecraft:knockback_resistance", knockback));
        }
        let mut components = BTreeMap::new();
        components.insert(
            "minecraft:attribute_modifiers".to_owned(),
            Value::from(modifiers),
        );
        components.insert(
            "minecraft:equippable".to_owned(),
            json!({ "slot": slot, "asset_id": format!("{id}:{set}"),
                    "equip_sound": a.equip_sound.as_deref()
                        .unwrap_or("minecraft:item.armor.equip_iron") }),
        );
        let decl = ItemDecl {
            template: format!("minecraft:iron_{piece}"),
            name: Some(format!("{title_name} {}", title(piece))),
            rarity: a.rarity.clone(),
            durability: Some(factor * a.durability),
            repair: a.repair.clone(),
            creative_tabs: a.creative_tabs.clone(),
            enchantability: a.enchantability,
            tags: vec![
                format!("minecraft:{tag}"),
                "minecraft:trimmable_armor".into(),
            ],
            components,
            ..ItemDecl::default()
        };
        items.push((format!("{set}_{piece}"), decl));
    }
    let layer = json!([{ "texture": format!("{id}:{set}") }]);
    let mut layers = serde_json::Map::new();
    if others {
        layers.insert("humanoid".into(), layer.clone());
    }
    if leggings {
        layers.insert("humanoid_leggings".into(), layer);
    }
    Ok((items, json!({ "layers": layers })))
}

/// An ore's configured feature, placed feature and biome modifier.
fn ore_files(file: &InoxFile, name: &str, o: &OreDecl) -> Result<Vec<(String, Value)>, String> {
    let id = file.meta.id.as_str();
    let what = format!("ore {id}:{name}");
    let block = |b: &str| -> Result<String, String> {
        if !b.contains(':') && !file.block.contains_key(b) {
            return Err(format!("{what}: {b} is not a block of this mod"));
        }
        Ok(full_id(id, b))
    };
    if !(1..=64).contains(&o.size) {
        return Err(format!("{what}: size is 1 to 64"));
    }
    if o.count == 0 || o.min_y > o.max_y {
        return Err(format!(
            "{what}: count is 1 or more and min_y at most max_y"
        ));
    }
    let discard = o.discard_on_air.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&discard) {
        return Err(format!("{what}: discard_on_air is 0 to 1"));
    }
    // 26.3's ore: the config's fields at the top, each target's state as a string.
    let target = |tag: &str, b: String| {
        json!({ "target": { "predicate_type": "minecraft:tag_match", "tag": tag },
                "state": b })
    };
    let mut targets = vec![target("minecraft:stone_ore_replaceables", block(&o.block)?)];
    if let Some(d) = &o.deepslate_block {
        targets.push(target("minecraft:deepslate_ore_replaceables", block(d)?));
    }
    let (min, max) = (
        json!({ "absolute": o.min_y }),
        json!({ "absolute": o.max_y }),
    );
    let height = match o.shape.as_deref().unwrap_or("uniform") {
        "uniform" => {
            json!({ "type": "minecraft:uniform", "min_inclusive": min, "max_inclusive": max })
        }
        "triangle" => {
            json!({ "type": "minecraft:trapezoid", "min_inclusive": min, "max_inclusive": max })
        }
        s => return Err(format!("{what}: shape {s:?} is uniform or triangle")),
    };
    let biomes = o
        .biomes
        .clone()
        .unwrap_or_else(|| "#minecraft:is_overworld".into());
    let biomes_ok = match &biomes {
        Value::String(_) => true,
        Value::Array(a) => !a.is_empty() && a.iter().all(Value::is_string),
        _ => false,
    };
    if !biomes_ok {
        return Err(format!("{what}: biomes is an id, a #tag or a list of them"));
    }
    let feature = format!("{id}:{name}");
    Ok(vec![
        (
            format!("data/{id}/worldgen/configured_feature/{name}.json"),
            json!({ "type": "minecraft:ore", "size": o.size,
                    "discard_chance_on_air_exposure": discard, "targets": targets }),
        ),
        (
            format!("data/{id}/worldgen/placed_feature/{name}.json"),
            json!({ "feature": feature,
                    "placement": [
                        { "type": "minecraft:count", "count": o.count },
                        { "type": "minecraft:in_square" },
                        { "type": "minecraft:height_range", "height": height },
                        { "type": "minecraft:biome" }
                    ] }),
        ),
        (
            format!("data/{id}/neoforge/biome_modifier/{name}.json"),
            json!({ "type": "neoforge:add_features", "biomes": biomes,
                    "features": feature, "step": "underground_ores" }),
        ),
    ])
}

/// What `generate` writes, before merging with the mod's own files.
#[derive(Default)]
struct Out {
    files: BTreeMap<String, Value>,
    /// English names, `block.<id>.<name>` / `item.<id>.<name>`.
    lang: BTreeMap<String, String>,
    /// Tag file -> values.
    tags: BTreeMap<String, Vec<String>>,
    /// Tag files that replace what came before (`replace = true`).
    replace: BTreeSet<String>,
}

/// The files `file`'s declarations make, merged with the mod's own `raw`
/// files (`read` gives one's bytes): the mod's files win at a path, except
/// that `assets/<id>/lang/en_us.json` and tags are merged (the mod's names
/// and values first). Refuses a missing texture or an unknown option.
pub fn generate(
    file: &InoxFile,
    raw: &BTreeSet<String>,
    mut read: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let id = file.meta.id.as_str();
    let mut out = Out::default();
    let need = |path: Option<String>, what: &str| -> Result<(), String> {
        match path {
            Some(p) if !raw.contains(&p) => Err(format!("{what}: texture {p} is missing")),
            _ => Ok(()),
        }
    };
    for (name, b) in &file.block {
        let what = format!("block {id}:{name}");
        let mut properties = serde_json::Map::new();
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                properties.insert(k.into(), v);
            }
        };
        put("hardness", b.hardness.map(Value::from));
        put(
            "explosion_resistance",
            b.explosion_resistance.map(Value::from),
        );
        put("light", b.light.map(Value::from));
        put("requires_tool", b.requires_tool.map(Value::from));
        put("friction", b.friction.map(Value::from));
        put("map_color", b.map_color.map(Value::from));
        put("sounds", b.sounds.clone().map(Value::from));
        out.files.insert(
            format!("data/{id}/horizon/block/{name}.json"),
            json!({ "template": b.template, "properties": properties, "item": b.item }),
        );
        match b.model.as_deref().unwrap_or("cube_all") {
            "cube_all" => {
                let (tex, file) = texture(id, "block", name, b.texture.as_deref());
                need(file, &what)?;
                out.files.insert(
                    format!("assets/{id}/blockstates/{name}.json"),
                    json!({ "variants": { "": { "model": format!("{id}:block/{name}") } } }),
                );
                out.files.insert(
                    format!("assets/{id}/models/block/{name}.json"),
                    json!({ "parent": "minecraft:block/cube_all", "textures": { "all": tex } }),
                );
            }
            "none" => {}
            m => return Err(format!("{what}: model {m:?} is cube_all or none")),
        }
        if b.item {
            out.files.insert(
                format!("assets/{id}/items/{name}.json"),
                json!({ "model": { "type": "minecraft:model", "model": format!("{id}:block/{name}") } }),
            );
        }
        out.lang.insert(
            format!("block.{id}.{name}"),
            b.name.clone().unwrap_or_else(|| title(name)),
        );
        let drop = match b.drops.as_deref().unwrap_or("self") {
            "none" => None,
            "self" if !b.item => {
                return Err(format!(
                    "{what}: drops itself but has no item (item = false): set drops"
                ));
            }
            "self" => Some(format!("{id}:{name}")),
            other if other.contains(':') => Some(other.to_owned()),
            other => {
                return Err(format!(
                    "{what}: drops {other:?} is self, none or an item id"
                ));
            }
        };
        if let Some(drop) = drop {
            out.files.insert(
                format!("data/{id}/loot_table/blocks/{name}.json"),
                json!({
                    "type": "minecraft:block",
                    "pools": [{
                        "rolls": 1,
                        "entries": [{ "type": "minecraft:item", "name": drop }],
                        "conditions": [{ "condition": "minecraft:survives_explosion" }]
                    }],
                    "random_sequence": format!("{id}:blocks/{name}")
                }),
            );
        }
        if let Some(tool) = &b.tool {
            if !matches!(tool.as_str(), "pickaxe" | "axe" | "shovel" | "hoe") {
                return Err(format!(
                    "{what}: tool {tool:?} is pickaxe, axe, shovel or hoe"
                ));
            }
            let tag = format!("data/minecraft/tags/block/mineable/{tool}.json");
            out.tags
                .entry(tag)
                .or_default()
                .push(format!("{id}:{name}"));
        }
        if let Some(tier) = &b.tier {
            if !matches!(tier.as_str(), "stone" | "iron" | "diamond") {
                return Err(format!("{what}: tier {tier:?} is stone, iron or diamond"));
            }
            let tag = format!("data/minecraft/tags/block/needs_{tier}_tool.json");
            out.tags
                .entry(tag)
                .or_default()
                .push(format!("{id}:{name}"));
        }
        for tag in &b.tags {
            let tag = tag_file("block", tag, &what)?;
            out.tags
                .entry(tag)
                .or_default()
                .push(format!("{id}:{name}"));
        }
    }
    for (name, o) in &file.ore {
        out.files.extend(ore_files(file, name, o)?);
    }
    let mut items: Vec<(String, ItemDecl)> = file
        .item
        .iter()
        .map(|(n, i)| (n.clone(), i.clone()))
        .collect();
    for (set, a) in &file.armor {
        let (pieces, asset) = armor_items(id, set, a)?;
        out.files
            .insert(format!("assets/{id}/equipment/{set}.json"), asset.clone());
        for kind in ["humanoid", "humanoid_leggings"] {
            if asset["layers"].get(kind).is_some() {
                let texture = format!("assets/{id}/textures/entity/equipment/{kind}/{set}.png");
                need(Some(texture), &format!("armor {id}:{set}"))?;
            }
        }
        items.extend(pieces);
    }
    for (set, t) in &file.tools {
        items.extend(tool_items(id, set, t)?);
    }
    for (kind, tags) in &file.tag {
        let kind_ok = !kind.is_empty()
            && kind
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_/".contains(&b));
        if !kind_ok {
            return Err(format!(
                "{IMOD_MANIFEST}: tag registry {kind:?} is a registry name (block, item, worldgen/biome...)"
            ));
        }
        for (name, t) in tags {
            let what = format!("tag {kind} {name}");
            let path = tag_file(kind, &full_id(id, name), &what)?;
            if t.replace {
                out.replace.insert(path.clone());
            }
            let values = t.values.iter().map(|v| full_ref(id, v));
            out.tags.entry(path).or_default().extend(values);
        }
    }
    // Creative tabs: the mod's own (one with everything when it declares none),
    // and its things' places in other tabs.
    let mut all_items: Vec<String> = file
        .block
        .iter()
        .filter(|(_, b)| b.item)
        .map(|(n, _)| format!("{id}:{n}"))
        .collect();
    all_items.extend(items.iter().map(|(n, _)| format!("{id}:{n}")));
    let mut tabs = file.tab.clone();
    if tabs.is_empty() {
        tabs.insert(id.to_owned(), TabDecl::default());
    }
    for (name, t) in &tabs {
        let listed = t
            .items
            .as_ref()
            .map(|list| list.iter().map(|i| full_id(id, i)).collect::<Vec<_>>());
        let mut tab = json!({
            "title": t.name.clone().unwrap_or_else(|| file.meta.name.clone()),
            "items": listed.unwrap_or_else(|| all_items.clone()),
        });
        if let Some(icon) = &t.icon {
            tab["icon"] = full_id(id, icon).into();
        }
        out.files
            .insert(format!("data/{id}/horizon/creative_tab/{name}.json"), tab);
    }
    let mut added: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let placed = file
        .block
        .iter()
        .map(|(n, b)| (n, &b.creative_tabs))
        .chain(items.iter().map(|(n, i)| (n, &i.creative_tabs)));
    for (name, targets) in placed {
        for t in targets {
            added
                .entry(full_id(id, t))
                .or_default()
                .push(format!("{id}:{name}"));
        }
    }
    for (target, held) in added {
        let file_name = format!("add_to_{}", target.replace([':', '/'], "_"));
        if tabs.contains_key(&file_name) {
            return Err(format!(
                "{IMOD_MANIFEST}: tab {file_name} is a name Inox uses"
            ));
        }
        out.files.insert(
            format!("data/{id}/horizon/creative_tab/{file_name}.json"),
            json!({ "horizon:add_to": target, "items": held }),
        );
    }
    for (name, i) in &items {
        let name = name.as_str();
        let what = format!("item {id}:{name}");
        let mut decl = serde_json::Map::new();
        decl.insert("template".into(), i.template.clone().into());
        if let Some(n) = i.max_stack_size {
            decl.insert("max_stack_size".into(), n.into());
        }
        if let Some(r) = &i.rarity {
            decl.insert("rarity".into(), r.clone().into());
        }
        let components = item_components(id, i, &what)?;
        if !components.is_empty() {
            decl.insert(
                "components".into(),
                Value::Object(components.into_iter().collect()),
            );
        }
        for tag in &i.tags {
            let tag = tag_file("item", tag, &what)?;
            out.tags
                .entry(tag)
                .or_default()
                .push(format!("{id}:{name}"));
        }
        out.files.insert(
            format!("data/{id}/horizon/item/{name}.json"),
            Value::Object(decl),
        );
        let parent = match i.model.as_deref().unwrap_or("generated") {
            "generated" => Some("minecraft:item/generated"),
            "handheld" => Some("minecraft:item/handheld"),
            "none" => None,
            m => {
                return Err(format!(
                    "{what}: model {m:?} is generated, handheld or none"
                ));
            }
        };
        if let Some(parent) = parent {
            let (tex, file) = texture(id, "item", name, i.texture.as_deref());
            need(file, &what)?;
            out.files.insert(
                format!("assets/{id}/models/item/{name}.json"),
                json!({ "parent": parent, "textures": { "layer0": tex } }),
            );
            out.files.insert(
                format!("assets/{id}/items/{name}.json"),
                json!({ "model": { "type": "minecraft:model", "model": format!("{id}:item/{name}") } }),
            );
        }
        out.lang.insert(
            format!("item.{id}.{name}"),
            i.name.clone().unwrap_or_else(|| title(name)),
        );
    }
    let mut files = BTreeMap::new();
    for (path, value) in out.files {
        if !raw.contains(&path) {
            files.insert(path, pretty(&value));
        }
    }
    if !out.lang.is_empty() {
        let path = format!("assets/{id}/lang/en_us.json");
        let mut names = serde_json::Map::new();
        if raw.contains(&path) {
            let own: Value =
                serde_json::from_slice(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            let Value::Object(own) = own else {
                return Err(format!("{path}: not a JSON object"));
            };
            names = own;
        }
        for (k, v) in out.lang {
            names.entry(k).or_insert(v.into());
        }
        files.insert(path, pretty(&Value::Object(names)));
    }
    for (path, values) in out.tags {
        let mut tag = json!({ "values": [] });
        if raw.contains(&path) {
            tag = serde_json::from_slice(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        }
        let Some(list) = tag.get_mut("values").and_then(Value::as_array_mut) else {
            return Err(format!("{path}: a tag holds a values list"));
        };
        list.extend(values.into_iter().map(Value::from));
        if out.replace.contains(&path) {
            tag["replace"] = true.into();
        }
        files.insert(path, pretty(&tag));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use crate::Capability;
    use super::*;

    const TOML: &str = r#"
[mod]
id = "ruby"
name = "Ruby"
version = "0.1.0"

[block.ruby_block]
template = "minecraft:iron_block"
hardness = 4.0
light = 3
tool = "pickaxe"
tier = "iron"

[block.ruby_ore]
template = "minecraft:iron_ore"
name = "Ruby Ore"
texture = "minecraft:block/stone"
drops = "ruby:ruby"
tool = "pickaxe"

[item.ruby]
template = "minecraft:diamond"
rarity = "uncommon"

[item.ruby_sword]
template = "minecraft:iron_sword"
model = "handheld"
components = { "minecraft:max_damage" = 900 }
"#;

    fn raw(files: &[&str]) -> BTreeSet<String> {
        files.iter().map(|s| (*s).to_owned()).collect()
    }
    fn json_of(files: &BTreeMap<String, Vec<u8>>, path: &str) -> Value {
        serde_json::from_slice(&files[path]).unwrap()
    }
    const TEXTURES: [&str; 3] = [
        "assets/ruby/textures/block/ruby_block.png",
        "assets/ruby/textures/item/ruby.png",
        "assets/ruby/textures/item/ruby_sword.png",
    ];

    #[test]
    fn declarations_make_a_content_mod() {
        let (manifest, file) = parse(TOML).unwrap();
        assert_eq!(manifest.api, "0.2");
        assert_eq!(manifest.capabilities, [Capability::Registries]);
        let files = generate(&file, &raw(&TEXTURES), |_| unreachable!()).unwrap();
        assert_eq!(
            json_of(&files, "data/ruby/horizon/block/ruby_block.json"),
            json!({ "template": "minecraft:iron_block", "item": true,
                    "properties": { "hardness": 4.0, "light": 3 } })
        );
        assert_eq!(
            json_of(&files, "assets/ruby/models/block/ruby_ore.json")["textures"]["all"],
            "minecraft:block/stone"
        );
        assert_eq!(
            json_of(&files, "data/ruby/loot_table/blocks/ruby_ore.json")["pools"][0]["entries"][0]
                ["name"],
            "ruby:ruby"
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/mineable/pickaxe.json"),
            json!({ "values": ["ruby:ruby_block", "ruby:ruby_ore"] })
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/needs_iron_tool.json"),
            json!({ "values": ["ruby:ruby_block"] })
        );
        assert_eq!(
            json_of(&files, "data/ruby/horizon/item/ruby_sword.json"),
            json!({ "template": "minecraft:iron_sword",
                    "components": { "minecraft:max_damage": 900 } })
        );
        assert_eq!(
            json_of(&files, "assets/ruby/models/item/ruby_sword.json")["parent"],
            "minecraft:item/handheld"
        );
        let lang = json_of(&files, "assets/ruby/lang/en_us.json");
        assert_eq!(lang["block.ruby.ruby_block"], "Ruby Block");
        assert_eq!(lang["block.ruby.ruby_ore"], "Ruby Ore");
        assert_eq!(lang["item.ruby.ruby_sword"], "Ruby Sword");
    }

    #[test]
    fn the_mods_own_files_win_and_lang_and_tags_merge() {
        let (_, file) = parse(TOML).unwrap();
        let mut own = raw(&TEXTURES);
        own.extend(raw(&[
            "assets/ruby/models/block/ruby_block.json",
            "assets/ruby/lang/en_us.json",
            "data/minecraft/tags/block/mineable/pickaxe.json",
        ]));
        let files = generate(&file, &own, |p| {
            Ok(match p {
                "assets/ruby/lang/en_us.json" => br#"{"item.ruby.ruby": "Red Gem"}"#.to_vec(),
                _ => br#"{"values": ["ruby:other"]}"#.to_vec(),
            })
        })
        .unwrap();
        assert!(!files.contains_key("assets/ruby/models/block/ruby_block.json"));
        assert_eq!(
            json_of(&files, "assets/ruby/lang/en_us.json")["item.ruby.ruby"],
            "Red Gem"
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/mineable/pickaxe.json")["values"][0],
            "ruby:other"
        );
    }

    #[test]
    fn mistakes_are_refused() {
        let (_, file) = parse(TOML).unwrap();
        let e = generate(&file, &raw(&TEXTURES[1..]), |_| unreachable!()).unwrap_err();
        assert!(
            e.contains("assets/ruby/textures/block/ruby_block.png is missing"),
            "{e}"
        );
        let bad = |extra: &str| {
            let text = format!("[mod]\nid = \"m\"\nname = \"M\"\nversion = \"1\"\n{extra}");
            parse(&text).and_then(|(_, f)| generate(&f, &raw(&[]), |_| unreachable!()))
        };
        assert!(
            bad("[block.a]\ntemplate = \"minecraft:stone\"\nmodel = \"none\"\ntool = \"sword\"")
                .unwrap_err()
                .contains("tool")
        );
        assert!(
            bad("[block.a]\ntemplate = \"minecraft:stone\"\ncolour = 1")
                .unwrap_err()
                .contains("unknown field")
        );
        assert!(
            bad("[block.A]\ntemplate = \"minecraft:stone\"")
                .unwrap_err()
                .contains("name")
        );
        assert!(
            bad("[block.a]\ntemplate = \"x\"\n[item.a]\ntemplate = \"y\"")
                .unwrap_err()
                .contains("both a block and an item")
        );
        assert!(
            bad("[block.a]\ntemplate = \"minecraft:stone\"\nmodel = \"none\"\nitem = false")
                .unwrap_err()
                .contains("no item")
        );
        assert!(
            parse("[mod]\nid = \"Bad\"\nname = \"x\"\nversion = \"1\"")
                .unwrap_err()
                .contains("inox.toml: id")
        );
    }

    #[test]
    fn folders_and_imod_files_load_as_content_mods() {
        use crate::{Content, Package, find};
        let dir = std::env::temp_dir().join(format!("horizon-imod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = dir.join("ruby");
        for t in TEXTURES {
            let path = folder.join(t);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"png").unwrap();
        }
        std::fs::write(folder.join(IMOD_MANIFEST), TOML).unwrap();
        let png: &[u8] = b"png";
        let own: Vec<(&str, &[u8])> = TEXTURES.iter().map(|t| (*t, png)).collect();
        std::fs::write(dir.join("ruby.imod"), write(TOML, &own).unwrap()).unwrap();
        assert_eq!(find(&dir), [dir.join("ruby"), dir.join("ruby.imod")]);
        for path in find(&dir) {
            let package = Package::read(&path).unwrap();
            assert!(package.wasm.is_none());
            assert_eq!(package.manifest.name, "Ruby");
            let content = Content::read(&path, &package.manifest).unwrap();
            let blocks: Vec<_> = content.blocks.iter().map(|b| b.id.as_str()).collect();
            let items: Vec<_> = content.items.iter().map(|i| i.id.as_str()).collect();
            assert_eq!(blocks, ["ruby:ruby_block", "ruby:ruby_ore"]);
            assert_eq!(items, ["ruby:ruby", "ruby:ruby_sword"]);
            assert_eq!(content.blocks[0].properties.light, Some(3));
            assert!(
                content
                    .files
                    .contains(&"assets/ruby/lang/en_us.json".to_owned())
            );
            assert!(content.files.contains(&TEXTURES[0].to_owned()));
            let lang = content.file("assets/ruby/lang/en_us.json").unwrap();
            assert!(String::from_utf8(lang).unwrap().contains("Ruby Sword"));
            assert_eq!(content.file(TEXTURES[0]).unwrap(), b"png");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_become_titles() {
        assert_eq!(title("ruby_block"), "Ruby Block");
        assert_eq!(title("x"), "X");
    }

    const GEAR: &str = r#"
[mod]
id = "gear"
name = "Gear"
version = "1"

[block.gem_ore]
template = "minecraft:iron_ore"
texture = "minecraft:block/stone"
tags = ["c:ores"]

[ore.gem_ore]
block = "gem_ore"
deepslate_block = "minecraft:deepslate_iron_ore"
size = 8
count = 5
min_y = -32
max_y = 48
shape = "triangle"

[item.gem]
template = "minecraft:diamond"
texture = "minecraft:item/diamond"
tags = ["c:gems"]

[item.gem_sword]
template = "minecraft:iron_sword"
texture = "minecraft:item/iron_sword"
model = "handheld"
durability = 900
attack_damage = 7
attack_speed = 1.6
repair = "gear:gem"
tags = ["minecraft:swords"]

[item.gem_apple]
template = "minecraft:apple"
texture = "minecraft:item/apple"
food = { nutrition = 6, saturation = 12.0, always_eat = true, effects = [
  { id = "minecraft:speed", seconds = 30, level = 2 },
] }

[armor.gem]
durability = 25
defense = [3, 7, 6, 3]
toughness = 1.0
repair = "gear:gem"
"#;

    fn gear_textures() -> BTreeSet<String> {
        let mut t = raw(&[
            "assets/gear/textures/entity/equipment/humanoid/gem.png",
            "assets/gear/textures/entity/equipment/humanoid_leggings/gem.png",
        ]);
        for p in ["helmet", "chestplate", "leggings", "boots"] {
            t.insert(format!("assets/gear/textures/item/gem_{p}.png"));
        }
        t
    }

    #[test]
    fn ores_make_features_and_a_biome_modifier() {
        let (_, file) = parse(GEAR).unwrap();
        let files = generate(&file, &gear_textures(), |_| unreachable!()).unwrap();
        let configured = json_of(&files, "data/gear/worldgen/configured_feature/gem_ore.json");
        assert_eq!(configured["type"], "minecraft:ore");
        assert_eq!(configured["size"], 8);
        let targets = &configured["targets"];
        assert_eq!(targets[0]["state"], "gear:gem_ore");
        assert_eq!(
            targets[1]["target"]["tag"],
            "minecraft:deepslate_ore_replaceables"
        );
        let placed = json_of(&files, "data/gear/worldgen/placed_feature/gem_ore.json");
        assert_eq!(placed["feature"], "gear:gem_ore");
        assert_eq!(placed["placement"][0]["count"], 5);
        assert_eq!(
            placed["placement"][2]["height"]["type"],
            "minecraft:trapezoid"
        );
        assert_eq!(
            placed["placement"][2]["height"]["min_inclusive"]["absolute"],
            -32
        );
        assert_eq!(
            json_of(&files, "data/gear/neoforge/biome_modifier/gem_ore.json"),
            json!({ "type": "neoforge:add_features", "biomes": "#minecraft:is_overworld",
                    "features": "gear:gem_ore", "step": "underground_ores" })
        );
        assert_eq!(
            json_of(&files, "data/c/tags/block/ores.json")["values"][0],
            "gear:gem_ore"
        );
    }

    #[test]
    fn items_get_stats_food_and_tags() {
        let (_, file) = parse(GEAR).unwrap();
        let files = generate(&file, &gear_textures(), |_| unreachable!()).unwrap();
        let sword = json_of(&files, "data/gear/horizon/item/gem_sword.json");
        let c = &sword["components"];
        assert_eq!(c["minecraft:max_damage"], 900);
        assert_eq!(c["minecraft:attribute_modifiers"][0]["amount"], 6.0);
        assert!(
            (c["minecraft:attribute_modifiers"][1]["amount"]
                .as_f64()
                .unwrap()
                + 2.4)
                .abs()
                < 1e-9
        );
        assert_eq!(c["minecraft:repairable"]["items"], "gear:gem");
        assert_eq!(
            json_of(&files, "data/minecraft/tags/item/swords.json")["values"][0],
            "gear:gem_sword"
        );
        let apple = json_of(&files, "data/gear/horizon/item/gem_apple.json");
        let c = &apple["components"];
        assert_eq!(c["minecraft:food"]["nutrition"], 6);
        assert_eq!(c["minecraft:food"]["can_always_eat"], true);
        let effect = &c["minecraft:consumable"]["on_consume_effects"][0]["effects"][0];
        assert_eq!(effect["duration"], 600);
        assert_eq!(effect["amplifier"], 1);
    }

    #[test]
    fn armor_sets_make_four_pieces_and_an_equipment_asset() {
        let (_, file) = parse(GEAR).unwrap();
        let files = generate(&file, &gear_textures(), |_| unreachable!()).unwrap();
        let chest = json_of(&files, "data/gear/horizon/item/gem_chestplate.json");
        assert_eq!(chest["template"], "minecraft:iron_chestplate");
        let c = &chest["components"];
        assert_eq!(c["minecraft:max_damage"], 16 * 25);
        assert_eq!(c["minecraft:equippable"]["slot"], "chest");
        assert_eq!(c["minecraft:equippable"]["asset_id"], "gear:gem");
        let modifiers = c["minecraft:attribute_modifiers"].as_array().unwrap();
        assert_eq!(modifiers[0]["amount"], 7.0);
        assert_eq!(modifiers[1]["type"], "minecraft:armor_toughness");
        assert_eq!(
            json_of(&files, "assets/gear/equipment/gem.json"),
            json!({ "layers": { "humanoid": [{ "texture": "gear:gem" }],
                                "humanoid_leggings": [{ "texture": "gear:gem" }] } })
        );
        let lang = json_of(&files, "assets/gear/lang/en_us.json");
        assert_eq!(lang["item.gear.gem_leggings"], "Gem Leggings");
        assert_eq!(
            json_of(&files, "data/minecraft/tags/item/leg_armor.json")["values"][0],
            "gear:gem_leggings"
        );
        // The armour's textures are checked like any other.
        let mut missing = gear_textures();
        missing.remove("assets/gear/textures/entity/equipment/humanoid/gem.png");
        let e = generate(&file, &missing, |_| unreachable!()).unwrap_err();
        assert!(
            e.contains("entity/equipment/humanoid/gem.png is missing"),
            "{e}"
        );
    }

    #[test]
    fn stat_and_ore_mistakes_are_refused() {
        let with = |extra: &str| {
            let text = format!("[mod]\nid = \"m\"\nname = \"M\"\nversion = \"1\"\n{extra}");
            parse(&text).and_then(|(_, f)| generate(&f, &raw(&[]), |_| unreachable!()))
        };
        let item = "[item.s]\ntemplate = \"minecraft:iron_sword\"\nmodel = \"none\"\n";
        assert!(
            with(&format!("{item}attack_damage = 7"))
                .unwrap_err()
                .contains("together")
        );
        assert!(
            with(&format!(
                "{item}durability = 9\ncomponents = {{ \"minecraft:max_damage\" = 3 }}"
            ))
            .unwrap_err()
            .contains("also set")
        );
        let ore = "[ore.o]\nblock = \"nope\"\nsize = 4\ncount = 2\nmin_y = 0\nmax_y = 10";
        assert!(with(ore).unwrap_err().contains("not a block of this mod"));
        let ore =
            "[ore.o]\nblock = \"minecraft:stone\"\nsize = 99\ncount = 2\nmin_y = 0\nmax_y = 10";
        assert!(with(ore).unwrap_err().contains("size"));
        let armor = "[armor.a]\ndurability = 5\ndefense = [1, 2, 3, 4]\npieces = [\"gloves\"]";
        assert!(with(armor).unwrap_err().contains("gloves"));
        let clash = "[armor.a]\ndurability = 5\ndefense = [1, 2, 3, 4]\n[item.a_boots]\ntemplate = \"minecraft:stick\"";
        assert!(with(clash).unwrap_err().contains("also declared"));
    }

    const KIT: &str = r##"
[mod]
id = "kit"
name = "Kit"
version = "1"

[block.gem_block]
template = "minecraft:iron_block"
texture = "minecraft:block/stone"
creative_tabs = ["minecraft:building_blocks"]

[item.gem]
template = "minecraft:diamond"
texture = "minecraft:item/diamond"

[tools.gem]
tier = "iron"
durability = 800
speed = 7.0
damage = 2.5
repair = "gem"
creative_tabs = ["minecraft:tools_and_utilities"]

[tag.item.gems]
values = ["gem", "minecraft:diamond", "#c:gems"]

[tag.block."minecraft:beacon_base_blocks"]
values = ["gem_block"]

[tag."worldgen/biome".gem_lands]
values = ["minecraft:plains"]
replace = true
"##;

    fn kit_textures() -> BTreeSet<String> {
        ["sword", "pickaxe", "axe", "shovel", "hoe"]
            .iter()
            .map(|t| format!("assets/kit/textures/item/gem_{t}.png"))
            .collect()
    }

    #[test]
    fn tool_sets_make_five_tools_of_a_tier() {
        let (_, file) = parse(KIT).unwrap();
        let files = generate(&file, &kit_textures(), |_| unreachable!()).unwrap();
        let pickaxe = json_of(&files, "data/kit/horizon/item/gem_pickaxe.json");
        assert_eq!(pickaxe["template"], "minecraft:iron_pickaxe");
        let c = &pickaxe["components"];
        assert_eq!(c["minecraft:max_damage"], 800);
        assert_eq!(c["minecraft:repairable"]["items"], "kit:gem");
        let rules = &c["minecraft:tool"]["rules"];
        assert_eq!(rules[0]["blocks"], "#minecraft:incorrect_for_iron_tool");
        assert_eq!(rules[1]["blocks"], "#minecraft:mineable/pickaxe");
        assert_eq!(rules[1]["speed"], 7.0);
        // Shown 2 + 2.5 = 4.5: the modifier is that less the player's own 1.
        assert_eq!(c["minecraft:attribute_modifiers"][0]["amount"], 3.5);
        let sword = json_of(&files, "data/kit/horizon/item/gem_sword.json");
        assert!(sword["components"].get("minecraft:tool").is_none());
        assert_eq!(
            sword["components"]["minecraft:attribute_modifiers"][0]["amount"],
            5.5
        );
        let hoe = json_of(&files, "data/kit/horizon/item/gem_hoe.json");
        let speed = hoe["components"]["minecraft:attribute_modifiers"][1]["amount"].as_f64();
        assert_eq!(speed, Some(3.5 - 4.0));
        assert_eq!(
            json_of(&files, "data/minecraft/tags/item/axes.json")["values"][0],
            "kit:gem_axe"
        );
        assert_eq!(
            json_of(&files, "assets/kit/models/item/gem_shovel.json")["parent"],
            "minecraft:item/handheld"
        );
        assert_eq!(
            json_of(&files, "assets/kit/lang/en_us.json")["item.kit.gem_hoe"],
            "Gem Hoe"
        );
    }

    #[test]
    fn tags_are_declared_in_any_registry() {
        let (_, file) = parse(KIT).unwrap();
        let files = generate(&file, &kit_textures(), |_| unreachable!()).unwrap();
        assert_eq!(
            json_of(&files, "data/kit/tags/item/gems.json"),
            json!({ "values": ["kit:gem", "minecraft:diamond", "#c:gems"] })
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/beacon_base_blocks.json")["values"],
            json!(["kit:gem_block"])
        );
        assert_eq!(
            json_of(&files, "data/kit/tags/worldgen/biome/gem_lands.json"),
            json!({ "values": ["minecraft:plains"], "replace": true })
        );
    }

    #[test]
    fn mods_get_a_creative_tab_and_places_in_others() {
        let (_, file) = parse(KIT).unwrap();
        let files = generate(&file, &kit_textures(), |_| unreachable!()).unwrap();
        let tab = json_of(&files, "data/kit/horizon/creative_tab/kit.json");
        assert_eq!(tab["title"], "Kit");
        let items = tab["items"].as_array().unwrap();
        assert_eq!(items[0], "kit:gem_block");
        assert_eq!(items[1], "kit:gem");
        assert_eq!(items.len(), 7);
        assert_eq!(
            json_of(
                &files,
                "data/kit/horizon/creative_tab/add_to_minecraft_tools_and_utilities.json"
            ),
            json!({ "horizon:add_to": "minecraft:tools_and_utilities",
                    "items": ["kit:gem_sword", "kit:gem_pickaxe", "kit:gem_axe", "kit:gem_shovel", "kit:gem_hoe"] })
        );
        assert_eq!(
            json_of(
                &files,
                "data/kit/horizon/creative_tab/add_to_minecraft_building_blocks.json"
            )["items"],
            json!(["kit:gem_block"])
        );
        // Tabs of its own replace the default one.
        let text = format!(
            "{KIT}\n[tab.gear]\nname = \"Gear\"\nicon = \"gem\"\nitems = [\"gem\", \"gem_sword\"]\n"
        );
        let (_, file) = parse(&text).unwrap();
        let files = generate(&file, &kit_textures(), |_| unreachable!()).unwrap();
        assert!(!files.contains_key("data/kit/horizon/creative_tab/kit.json"));
        assert_eq!(
            json_of(&files, "data/kit/horizon/creative_tab/gear.json"),
            json!({ "title": "Gear", "icon": "kit:gem", "items": ["kit:gem", "kit:gem_sword"] })
        );
    }

    #[test]
    fn tool_and_tag_mistakes_are_refused() {
        let with = |extra: &str| {
            let text = format!("[mod]\nid = \"m\"\nname = \"M\"\nversion = \"1\"\n{extra}");
            parse(&text).and_then(|(_, f)| generate(&f, &raw(&[]), |_| unreachable!()))
        };
        let tools = "[tools.t]\ntier = \"copper\"\ndurability = 9\nspeed = 2.0\ndamage = 1.0";
        assert!(with(tools).unwrap_err().contains("tier"));
        let clash = "[tools.t]\ntier = \"iron\"\ndurability = 9\nspeed = 2.0\ndamage = 1.0\n[item.t_axe]\ntemplate = \"minecraft:stick\"";
        assert!(with(clash).unwrap_err().contains("also declared"));
        let tag = "[tag.\"Item\".x]\nvalues = []";
        assert!(with(tag).unwrap_err().contains("registry"));
    }
}
