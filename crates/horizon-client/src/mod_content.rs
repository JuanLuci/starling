//! Content mods in the resource root (docs/modpacks.md, registries 0.2).
//!
//! Every loader reads one resource root, and most of its block and item
//! tables are exports from vanilla indexed by block-state id or keyed by
//! name. A content mod's block copies a vanilla *template*. So the mods'
//! root is an overlay, like the resource-pack one:
//! - the base root linked, with the per-state and per-name tables
//!   rewritten: each mod block gets its template's rows appended (states
//!   after vanilla's, mods and blocks in id order) with names, translation
//!   keys and in-table state ids moved over, and the declared overrides
//!   applied;
//! - each mod's own `assets/` and `data/` files;
//! - additions to tags merged into the tag files and bundles.
//!
//! The loaders need no mod code, vanilla state ids never move, and with no
//! content mod the root is the base root itself.
use crate::{Result, blocks::MAP_COLORS};
use horizon_mods::content::{Block, Content, Rarity};
use horizon_world::nbt::{self, JavaString, Limits, Tag};
use serde::{Deserialize, Serialize, de};
use std::{
    collections::BTreeMap,
    fmt, fs,
    ops::Range,
    path::{Path, PathBuf},
};

/// A content mod to put in the root: its id and content.
pub struct ContentMod {
    pub id: String,
    pub content: Content,
}

/// JSON with its object keys in file order, so a rewritten table differs
/// from the original only by what was added.
#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(serde_json::Number),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}
impl<'de> Deserialize<'de> for J {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = J;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> std::result::Result<J, E> {
                Ok(J::Null)
            }
            fn visit_bool<E>(self, v: bool) -> std::result::Result<J, E> {
                Ok(J::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> std::result::Result<J, E> {
                Ok(J::Num(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> std::result::Result<J, E> {
                Ok(J::Num(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<J, E> {
                serde_json::Number::from_f64(v)
                    .map(J::Num)
                    .ok_or_else(|| E::custom("not a JSON number"))
            }
            fn visit_str<E>(self, v: &str) -> std::result::Result<J, E> {
                Ok(J::Str(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> std::result::Result<J, E> {
                Ok(J::Str(v))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element()? {
                    out.push(v);
                }
                Ok(J::Arr(out))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = Vec::new();
                while let Some(kv) = a.next_entry()? {
                    out.push(kv);
                }
                Ok(J::Obj(out))
            }
        }
        d.deserialize_any(V)
    }
}
impl Serialize for J {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Num(n) => n.serialize(s),
            J::Str(t) => s.serialize_str(t),
            J::Arr(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            J::Obj(o) => {
                let mut map = s.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}
impl J {
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut J> {
        match self {
            J::Obj(o) => o.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// Sets `key`, in place if present, else at the end.
    fn set(&mut self, key: &str, value: J) {
        if let Some(v) = self.get_mut(key) {
            *v = value;
        } else if let J::Obj(o) = self {
            o.push((key.to_owned(), value));
        }
    }
    /// The value at a `/`-separated path ("" is the document).
    fn at(&mut self, path: &str) -> Option<&mut J> {
        path.split('/')
            .filter(|p| !p.is_empty())
            .try_fold(self, |j, key| j.get_mut(key))
    }
    fn f64(v: f64) -> J {
        serde_json::Number::from_f64(v).map_or(J::Null, J::Num)
    }
    fn as_u32(&self) -> Option<u32> {
        match self {
            J::Num(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
            _ => None,
        }
    }
    /// Renames strings and, with `ids`, moves state ids, everywhere inside.
    fn rename(&mut self, r: &Rename, ids: bool) {
        match self {
            J::Str(s) => {
                if let Some(new) = r.string(s) {
                    *s = new;
                }
            }
            J::Num(n) if ids => {
                if let Some(new) = n
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .and_then(|id| r.id(id))
                {
                    *n = new.into();
                }
            }
            J::Arr(a) => a.iter_mut().for_each(|v| v.rename(r, ids)),
            J::Obj(o) => o.iter_mut().for_each(|(_, v)| v.rename(r, ids)),
            _ => {}
        }
    }
}

/// One id moving to another: `minecraft:iron_block` → `ruby:ruby_block`,
/// with its state range for blocks.
#[derive(Clone)]
struct Rename {
    from: String,
    to: String,
    states: Range<u32>,
    first: u32,
}
impl Rename {
    fn new(from: &str, to: &str) -> Self {
        Rename {
            from: from.to_owned(),
            to: to.to_owned(),
            states: 0..0,
            first: 0,
        }
    }
    /// The id itself, `Block{id}`, and `block.<ns>.<path>` / `item.<ns>.<path>` keys.
    fn string(&self, s: &str) -> Option<String> {
        if s == self.from {
            return Some(self.to.clone());
        }
        if s.strip_prefix("Block{").and_then(|x| x.strip_suffix('}')) == Some(&self.from) {
            return Some(format!("Block{{{}}}", self.to));
        }
        let key = |id: &str| id.replacen(':', ".", 1).replace('/', ".");
        for kind in ["block", "item"] {
            if s.strip_prefix(kind).and_then(|x| x.strip_prefix('.')) == Some(&key(&self.from)) {
                return Some(format!("{kind}.{}", key(&self.to)));
            }
        }
        None
    }
    fn id(&self, id: u32) -> Option<u32> {
        self.states
            .contains(&id)
            .then(|| self.first + (id - self.states.start))
    }
}

/// A block being added: its declaration and where its states go.
struct NewBlock<'a> {
    decl: &'a Block,
    rename: Rename,
    /// The template's item, if it has one.
    item: Option<String>,
    /// Its declared shape for each of its states, in block units (`state_shapes`).
    shapes: Option<Vec<Vec<[f64; 6]>>>,
}
/// An item being added (declared, or a block's).
struct NewItem {
    rename: Rename,
    max_stack_size: Option<u8>,
    rarity: Option<Rarity>,
}

struct Plan<'a> {
    blocks: Vec<NewBlock<'a>>,
    items: Vec<NewItem>,
    /// Vanilla's state count (the tables' length before).
    states: usize,
    /// Each vanilla block's default state.
    defaults: BTreeMap<String, u32>,
    /// The gameplay rows' sound objects, by block, for `sounds` overrides.
    sounds: BTreeMap<String, J>,
}

fn read_json(path: &Path) -> Result<J> {
    Ok(serde_json::from_slice(&fs::read(path)?).map_err(|e| format!("{}: {e}", path.display()))?)
}

impl<'a> Plan<'a> {
    fn new(base: &Path, mods: &'a [ContentMod]) -> Result<Self> {
        let mut facts = read_json(&base.join("facts.json"))?;
        let names: Vec<String> = match facts.at("states") {
            Some(J::Arr(states)) => states
                .iter()
                .map(|s| match s {
                    J::Arr(row) => match row.first() {
                        Some(J::Str(name)) => Ok(name.clone()),
                        _ => Err("facts.json: a state without a name"),
                    },
                    _ => Err("facts.json: a state that is not a row"),
                })
                .collect::<std::result::Result<_, _>>()?,
            _ => return Err("facts.json: no states".into()),
        };
        let defaults: BTreeMap<String, u32> = match facts.at("defaults") {
            Some(J::Obj(o)) => o
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_u32()?)))
                .collect(),
            _ => return Err("facts.json: no defaults".into()),
        };
        let range = |name: &str| -> Option<Range<u32>> {
            let start = names.iter().position(|n| n == name)?;
            let len = names[start..].iter().take_while(|n| *n == name).count();
            // A block's states are one run in registry order.
            (names[start + len..].iter().all(|n| n != name))
                .then_some(start as u32..(start + len) as u32)
        };
        let mut blocks = Vec::new();
        let mut next = names.len() as u32;
        let mut sorted: Vec<&ContentMod> = mods.iter().collect();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        let mut wanted_sounds = Vec::new();
        for m in &sorted {
            let mut declared: Vec<&Block> = m.content.blocks.iter().collect();
            declared.sort_by(|a, b| a.id.cmp(&b.id));
            for decl in declared {
                if defaults.contains_key(&decl.id) {
                    return Err(format!("{}: {} is already a block", m.id, decl.id).into());
                }
                let states = range(&decl.template).ok_or_else(|| {
                    format!("{}: template {} is not a block", decl.id, decl.template)
                })?;
                let mut rename = Rename::new(&decl.template, &decl.id);
                rename.first = next;
                next += states.len() as u32;
                rename.states = states;
                if let Some(s) = &decl.properties.sounds {
                    if !defaults.contains_key(s) {
                        return Err(format!("{}: sounds {s} is not a block", decl.id).into());
                    }
                    wanted_sounds.push(s.clone());
                }
                let shapes = match &decl.properties.shape {
                    Some(boxes) => Some(state_shapes(boxes, &mut facts, &rename.states)?),
                    None => None,
                };
                blocks.push(NewBlock {
                    decl,
                    rename,
                    item: None,
                    shapes,
                });
            }
        }
        if next > MAX_STATES {
            let mut by_mod: BTreeMap<&str, u32> = BTreeMap::new();
            for b in &blocks {
                *by_mod
                    .entry(&b.decl.id[..b.decl.id.find(':').unwrap_or(0)])
                    .or_default() += b.rename.states.len() as u32;
            }
            let list: Vec<String> = by_mod.iter().map(|(m, n)| format!("{m} {n}")).collect();
            return Err(format!(
                "{next} block states, more than the {MAX_STATES} a world can hold ({} in the game; \
                 the mods: {})",
                names.len(),
                list.join(", ")
            )
            .into());
        }
        // Block items copy the template block's item.
        let gameplay = read_json(&base.join("gameplay-facts.json"))?;
        let rows = match gameplay.get("states") {
            Some(J::Arr(rows)) if rows.len() == names.len() => rows,
            _ => return Err("gameplay-facts.json does not match facts.json".into()),
        };
        let mut sounds = BTreeMap::new();
        for s in wanted_sounds {
            let row = &rows[defaults[&s] as usize];
            sounds.insert(s, row.get("sound").cloned().unwrap_or(J::Null));
        }
        let mut items = Vec::new();
        for b in &mut blocks {
            let row = &rows[b.rename.states.start as usize];
            b.item = match row.get("item") {
                Some(J::Str(i)) if i != "minecraft:air" => Some(i.clone()),
                _ => None,
            };
            if b.decl.item {
                let item = b.item.as_deref().ok_or_else(|| {
                    format!("{}: template {} has no item", b.decl.id, b.decl.template)
                })?;
                items.push(NewItem {
                    rename: Rename::new(item, &b.decl.id),
                    max_stack_size: None,
                    rarity: None,
                });
            }
        }
        for m in &sorted {
            for i in &m.content.items {
                items.push(NewItem {
                    rename: Rename::new(&i.template, &i.id),
                    max_stack_size: i.max_stack_size,
                    rarity: i.rarity,
                });
            }
        }
        Ok(Plan {
            blocks,
            items,
            states: names.len(),
            defaults,
            sounds,
        })
    }

    /// What moves in a new block's copies of its template's rows: the
    /// block, and the template's item when it has another name. A block
    /// without an item gets `minecraft:air` where rows name one.
    fn renames(&self, b: &NewBlock) -> Vec<Rename> {
        let mut out = vec![b.rename.clone()];
        if let Some(item) = b.item.as_deref().filter(|i| *i != b.decl.template)
            && b.decl.item
        {
            out.push(Rename::new(item, &b.decl.id));
        }
        out
    }

    /// Appends each new block's copies of the template's states to `list`;
    /// `gameplay` rows get their index and the declared overrides.
    fn states(&self, list: &mut Vec<J>, ids: bool, gameplay: bool) -> Result<()> {
        if list.len() != self.states {
            return Err(format!("{} states, not {}", list.len(), self.states).into());
        }
        for b in &self.blocks {
            let renames = self.renames(b);
            for (i, id) in b.rename.states.clone().enumerate() {
                let mut row = list[id as usize].clone();
                for r in &renames {
                    row.rename(r, ids);
                }
                if gameplay {
                    row.set("id", J::Num((b.rename.first + i as u32).into()));
                    if !b.decl.item {
                        row.set("item", J::Str("minecraft:air".into()));
                    }
                    self.override_gameplay(b, &mut row);
                    if let Some(shapes) = &b.shapes {
                        shape_gameplay(&mut row, &shapes[i]);
                    }
                }
                list.push(row);
            }
        }
        Ok(())
    }

    /// The declared properties, on a gameplay row.
    fn override_gameplay(&self, b: &NewBlock, row: &mut J) {
        let p = &b.decl.properties;
        if let Some(v) = p.hardness {
            row.set("hardness", J::f64(v as f64));
        }
        if let Some(v) = p.explosion_resistance {
            row.set("explosion_resistance", J::f64(v as f64));
        }
        if let Some(v) = p.light {
            row.set("emission", J::Num(v.into()));
        }
        if let Some(v) = p.requires_tool {
            row.set("requires_tool", J::Bool(v));
        }
        if let Some(v) = p.friction {
            row.set("friction", J::f64(v as f64));
        }
        if let Some(v) = p.map_color {
            let col = if v == 0 { 0 } else { MAP_COLORS[v as usize] };
            row.set("map_color", J::Num(col.into()));
        }
        if let Some(s) = &p.sounds {
            row.set("sound", self.sounds[s].clone());
        }
        if let Some(s) = &p.render_shape {
            row.set("render_shape", J::Str(s.clone()));
        }
    }

    /// Adds each new block's copy of the template's entry to a map by name.
    fn names(&self, map: &mut J, ids: bool) {
        for b in &self.blocks {
            if let Some(v) = map.get(&b.decl.template) {
                let mut v = v.clone();
                for r in &self.renames(b) {
                    v.rename(r, ids);
                }
                map.set(&b.decl.id, v);
            }
        }
    }

    /// Adds each new item's copy of its template's entry to a map by name;
    /// `components` applies the overrides (items.json).
    fn items(&self, map: &mut J, components: bool) {
        for i in &self.items {
            let Some(v) = map.get(&i.rename.from) else {
                continue;
            };
            let mut v = v.clone();
            v.rename(&i.rename, false);
            if components {
                if let Some(n) = i.max_stack_size {
                    v.set("minecraft:max_stack_size", J::Num(n.into()));
                }
                if let Some(r) = i.rarity {
                    let r = match r {
                        Rarity::Common => "common",
                        Rarity::Uncommon => "uncommon",
                        Rarity::Rare => "rare",
                        Rarity::Epic => "epic",
                    };
                    v.set("minecraft:rarity", J::Str(r.into()));
                }
            }
            map.set(&i.rename.to, v);
        }
    }

    /// One JSON table, rewritten.
    fn json(&self, name: &str, doc: &mut J) -> Result<()> {
        let states = |doc: &mut J, path: &str, ids: bool, gameplay: bool| -> Result<()> {
            match doc.at(path) {
                Some(J::Arr(a)) => self.states(a, ids, gameplay),
                _ => Err(format!("no list at /{path}").into()),
            }
        };
        match name {
            "facts.json" => {
                states(doc, "states", false, false)?;
                let defaults = doc.at("defaults").ok_or("no defaults")?;
                for b in &self.blocks {
                    let default = self.defaults[&b.decl.template];
                    let id = b.rename.id(default).ok_or("a default outside its block")?;
                    defaults.set(&b.decl.id, J::Num(id.into()));
                }
            }
            "gameplay-facts.json" => states(doc, "states", false, true)?,
            "block-behaviours.json" => {
                states(doc, "states", false, false)?;
                // A declared shape's support and solidity (blocks.rs reads `[0] & 63` and
                // `[1] & 1`), the rest the template's.
                if let Some(J::Arr(rows)) = doc.at("states") {
                    for b in &self.blocks {
                        for (i, boxes) in b.shapes.iter().flatten().enumerate() {
                            let Some(J::Arr(row)) = rows.get_mut(b.rename.first as usize + i)
                            else {
                                continue;
                            };
                            let (support, solid) = shape_support(boxes);
                            if let Some(slot) = row.get_mut(0) {
                                *slot = J::Num(support.into());
                            }
                            if let Some(slot) = row.get_mut(1) {
                                let flags = match slot {
                                    J::Num(n) => n.as_u64().unwrap_or(0),
                                    _ => 0,
                                };
                                *slot = J::Num(((flags & !1) | u64::from(solid)).into());
                            }
                        }
                    }
                }
                let Some(J::Arr(blocks)) = doc.at("blocks") else {
                    return Err("no blocks".into());
                };
                for b in &self.blocks {
                    let template = blocks.iter().find(
                        |x| matches!(x.get("name"), Some(J::Str(n)) if *n == b.decl.template),
                    );
                    if let Some(mut v) = template.cloned() {
                        for r in &self.renames(b) {
                            v.rename(r, false);
                        }
                        if !b.decl.item {
                            v.set("item", J::Str("minecraft:air".into()));
                        }
                        blocks.push(v);
                    }
                }
                self.items(doc.at("items").ok_or("no items")?, false);
                for odds in ["fire/burnOdds", "fire/igniteOdds"] {
                    if let Some(map) = doc.at(odds) {
                        self.names(map, false);
                    }
                }
            }
            "model-states.json"
            | "landing-definitions.json"
            | "worldgen/blocks/survival-rules.json" => self.names(doc, false),
            "render-definitions.json" => {
                self.names(doc, true);
                // A declared shape hides no neighbour's faces.
                for b in self.blocks.iter().filter(|b| b.shapes.is_some()) {
                    if let Some(v) = doc.get_mut(&b.decl.id) {
                        v.set("can_occlude", J::Bool(false));
                    }
                }
            }
            "worldgen/blocks/survival-extra-definitions.json" => {
                if let Some(map) = doc.at("ignition_odds") {
                    self.names(map, false);
                }
            }
            "items.json" => self.items(doc, true),
            "registries.json" => {
                // Network ids: the game and its server map items and blocks through
                // this report, so new ones get the ids after vanilla's, in plan order.
                let new_items = self.items.iter().map(|i| i.rename.to.clone());
                let new_blocks = self.blocks.iter().map(|b| b.decl.id.clone());
                for (registry, names) in [
                    ("minecraft:item", new_items.collect::<Vec<_>>()),
                    ("minecraft:block", new_blocks.collect()),
                ] {
                    let Some(J::Obj(entries)) =
                        doc.get_mut(registry).and_then(|r| r.get_mut("entries"))
                    else {
                        return Err(format!("no {registry} entries").into());
                    };
                    let next = entries
                        .iter()
                        .filter_map(|(_, e)| match e {
                            J::Obj(o) => o.iter().find(|(k, _)| k == "protocol_id")?.1.as_u32(),
                            _ => None,
                        })
                        .max()
                        .map_or(0, |m| m + 1);
                    for (i, name) in names.into_iter().enumerate() {
                        if entries.iter().any(|(k, _)| *k == name) {
                            continue;
                        }
                        let id = J::Num((next + i as u32).into());
                        entries.push((name, J::Obj(vec![("protocol_id".into(), id)])));
                    }
                }
            }
            "command-registries.json" => {
                self.names(doc.at("blocks").ok_or("no blocks")?, false);
                let new_items = || self.items.iter().map(|i| J::Str(i.rename.to.clone()));
                let new_blocks = || self.blocks.iter().map(|b| J::Str(b.decl.id.clone()));
                let Some(J::Arr(items)) = doc.at("items") else {
                    return Err("no items".into());
                };
                items.extend(new_items());
                // The registries commands check ids against.
                if let Some(J::Arr(ids)) = doc.at("by_registry/minecraft:item/ids") {
                    ids.extend(new_items());
                }
                if let Some(J::Arr(ids)) = doc.at("by_registry/minecraft:block/ids") {
                    ids.extend(new_blocks());
                }
            }
            "mob-data.json" => states(doc, "states", false, false)?,
            "worldgen/block_flags.json" => states(doc, "", false, false)?,
            "worldgen/blocks/structure-blocks.json" => {
                for key in ["rotate", "mirror", "full_block"] {
                    states(doc, key, true, false)?;
                }
                let added: usize = self.blocks.iter().map(|b| b.rename.states.len()).sum();
                doc.set("states", J::Num(((self.states + added) as u64).into()));
            }
            _ => return Err("no rule for this table".into()),
        }
        Ok(())
    }

    /// One NBT table: `states` lists, `cached_shapes` arrays and `blocks`
    /// compounds, wherever they are; declared shapes in its shape tables.
    fn nbt(&self, table: &str, tag: &mut Tag) {
        self.nbt_in(tag, table.ends_with("occlusion.nbt"));
    }
    fn nbt_in(&self, tag: &mut Tag, occlusion: bool) {
        let Tag::Compound(c) = tag else {
            return;
        };
        for (key, value) in c.iter_mut() {
            match (key.to_string_lossy().as_str(), value) {
                ("states", Tag::List(list)) if list.len() == self.states => {
                    for b in &self.blocks {
                        for id in b.rename.states.clone() {
                            let mut state = list[id as usize].clone();
                            if let Some(slot) = state.get_mut("id") {
                                *slot = Tag::String(b.decl.id.as_str().into());
                            }
                            list.push(state);
                        }
                    }
                }
                ("cached_shapes", Tag::IntArray(a)) if a.len() == self.states => {
                    for b in &self.blocks {
                        let r = &b.rename.states;
                        let copy = a[r.start as usize..r.end as usize].to_vec();
                        a.extend(copy);
                    }
                }
                ("blocks", Tag::Compound(map)) => {
                    for b in &self.blocks {
                        let Some(mut v) = map
                            .get(&JavaString::from(b.decl.template.as_str()))
                            .cloned()
                        else {
                            continue;
                        };
                        if let (Some(f), Some(slot)) =
                            (b.decl.properties.friction, v.get_mut("friction"))
                        {
                            *slot = Tag::Float(f.to_bits());
                        }
                        // block-properties.nbt's flags: a declared shape does not occlude.
                        if let (Some(_), Tag::Byte(flags)) = (&b.shapes, &mut v) {
                            *flags &= !CAN_OCCLUDE;
                        }
                        map.insert(b.decl.id.as_str().into(), v);
                    }
                }
                (name, inner @ Tag::Compound(_)) => {
                    self.nbt_in(inner, occlusion || name == "occlusion")
                }
                _ => {}
            }
        }
        self.place_shapes(c, occlusion);
    }

    /// In a shape table (`shapes` and their `cached_shapes`, the template's copied): each
    /// declared shape's states pointed at its grid, added once a block for each different one;
    /// at the empty shape in an occlusion table.
    fn place_shapes(&self, c: &mut nbt::Compound, occlusion: bool) {
        let shaped: Vec<&NewBlock> = self.blocks.iter().filter(|b| b.shapes.is_some()).collect();
        if shaped.is_empty() || !c.contains_key(&JavaString::from("cached_shapes")) {
            return;
        }
        let Some(Tag::List(entries)) = c.get_mut(&JavaString::from("shapes")) else {
            return;
        };
        let mut slots: Vec<(u32, i32)> = Vec::new();
        if occlusion {
            // The empty shape: the canonical one, else any with no cell occupied (occlusion
            // tables keep no canonical identities), else one added.
            let empty = entries
                .iter()
                .position(|e| e.get("canonical") == Some(&Tag::Byte(1)))
                .or_else(|| {
                    entries.iter().position(|e| {
                        matches!(e.get("occupied"), Some(Tag::ByteArray(cells)) if cells.iter().all(|c| *c == 0))
                    })
                });
            let empty = match empty {
                Some(i) => i,
                None => {
                    let Ok(entry) =
                        grid_entry([vec![0.0, 1.0], vec![0.0, 1.0], vec![0.0, 1.0]], &[false])
                    else {
                        return;
                    };
                    entries.push(entry);
                    entries.len() - 1
                }
            };
            // The new states (the template's own keep theirs).
            for b in &shaped {
                let count = b.rename.states.len() as u32;
                slots.extend((0..count).map(|i| (b.rename.first + i, empty as i32)));
            }
        } else {
            for b in &shaped {
                let mut made: Vec<(&Vec<[f64; 6]>, i32)> = Vec::new();
                for (i, boxes) in b.shapes.iter().flatten().enumerate() {
                    let index = match made.iter().find(|(m, _)| *m == boxes) {
                        Some((_, index)) => *index,
                        None => {
                            let Ok(entry) = shape_entry(boxes) else {
                                continue;
                            };
                            entries.push(entry);
                            made.push((boxes, entries.len() as i32 - 1));
                            entries.len() as i32 - 1
                        }
                    };
                    slots.push((b.rename.first + i as u32, index));
                }
            }
        }
        if let Some(Tag::IntArray(cached)) = c.get_mut(&JavaString::from("cached_shapes")) {
            for (state, index) in slots {
                if let Some(slot) = cached.get_mut(state as usize) {
                    *slot = index;
                }
            }
        }
    }
}

/// block-properties.nbt's flag for `BlockState.canOcclude`.
const CAN_OCCLUDE: i8 = 16;

/// A declared shape (`[x1, y1, z1, x2, y2, z2]` in sixteenths, made for a state facing up or
/// along y) for each of its states (`states`, its template's): turned to the state's `facing`,
/// else its `axis`, as Create's shapers turn theirs, in block units.
fn state_shapes(
    boxes: &[[f32; 6]],
    facts: &mut J,
    states: &Range<u32>,
) -> Result<Vec<Vec<[f64; 6]>>> {
    let Some(J::Arr(rows)) = facts.at("states") else {
        return Err("facts.json: no states".into());
    };
    Ok(states
        .clone()
        .map(|id| {
            // facts.json's rows: [name, .., {"properties": {...}, "id": ...}].
            let properties = match rows.get(id as usize) {
                Some(J::Arr(row)) => row.get(4).and_then(|r| r.get("properties")),
                _ => None,
            };
            let property = |key: &str| match properties.and_then(|p| p.get(key)) {
                Some(J::Str(s)) => Some(s.as_str()),
                _ => None,
            };
            let toward = match (property("facing"), property("axis")) {
                (Some(facing), _) => horizon_mods::content::direction(facing),
                (None, Some("x")) => Some([1, 0, 0]),
                (None, Some("z")) => Some([0, 0, 1]),
                _ => None,
            };
            let turns = toward.map_or([0; 3], |f| horizon_mods::content::turns_from([0, 1, 0], f));
            boxes
                .iter()
                .map(|b| turn_box(b, [turns[0], turns[1]]))
                .collect()
        })
        .collect())
}

/// A box in sixteenths turned about the block's middle as a blockstate variant turns a model
/// (`turns` quarters about x, then about y; a quarter about x takes south up, about y south
/// west), in block units.
fn turn_box(b: &[f32; 6], turns: [u8; 2]) -> [f64; 6] {
    let corner = |c: [f32; 3]| {
        let mut v = c.map(|v| f64::from(v) / 16.0 - 0.5);
        for _ in 0..turns[0] {
            v = [v[0], v[2], -v[1]];
        }
        for _ in 0..turns[1] {
            v = [-v[2], v[1], v[0]];
        }
        v.map(|v| v + 0.5)
    };
    let (a, c) = (corner([b[0], b[1], b[2]]), corner([b[3], b[4], b[5]]));
    [
        a[0].min(c[0]),
        a[1].min(c[1]),
        a[2].min(c[2]),
        a[0].max(c[0]),
        a[1].max(c[1]),
        a[2].max(c[2]),
    ]
}

/// A gameplay row's outline (the boxes and their edges) and its collision, occlusion and light
/// facts for a declared shape: full collision only as a whole block, no occlusion, light
/// through.
fn shape_gameplay(row: &mut J, boxes: &[[f64; 6]]) {
    let list = |v: &[f64]| J::Arr(v.iter().map(|v| J::f64(*v)).collect());
    row.set("outline", J::Arr(boxes.iter().map(|b| list(b)).collect()));
    let mut edges = Vec::new();
    for b in boxes {
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for (pu, pv) in [(0, 0), (3, 0), (0, 3), (3, 3)] {
                let mut edge = [0.0; 6];
                edge[axis] = b[axis];
                edge[axis + 3] = b[axis + 3];
                (edge[u], edge[u + 3]) = (b[u + pu], b[u + pu]);
                (edge[v], edge[v + 3]) = (b[v + pv], b[v + pv]);
                edges.push(list(&edge));
            }
        }
    }
    row.set("outline_edges", J::Arr(edges));
    for key in ["solid_render", "can_occlude", "shape_occlusion"] {
        row.set(key, J::Bool(false));
    }
    let whole = boxes == [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];
    row.set("full_collision", J::Bool(whole));
    row.set("dampening", J::Num(0.into()));
    row.set("skylight_down", J::Bool(true));
}

/// A declared shape's support bits (block-behaviours.json's first: FULL by direction ordinal,
/// CENTER six on, RIGID twelve on) and `isSolid`: a face is FULL and RIGID where one box covers
/// it whole, CENTER where one covers its middle; solid as `calculateSolid` judges the shape's
/// bounds (on average 0.729 or a block tall).
fn shape_support(boxes: &[[f64; 6]]) -> (u64, bool) {
    // Direction ordinals: down, up, north, south, west, east.
    const FACES: [(usize, bool); 6] = [
        (1, false),
        (1, true),
        (2, false),
        (2, true),
        (0, false),
        (0, true),
    ];
    let mut bits = 0u64;
    for (d, (axis, far)) in FACES.into_iter().enumerate() {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let touching = |b: &[f64; 6]| match far {
            true => b[axis] < 1.0 && 1.0 <= b[axis + 3],
            false => b[axis] <= 0.0 && 0.0 < b[axis + 3],
        };
        let covers = |b: &[f64; 6], lo: f64, hi: f64| {
            b[u] <= lo && hi <= b[u + 3] && b[v] <= lo && hi <= b[v + 3]
        };
        if boxes.iter().any(|b| touching(b) && covers(b, 0.0, 1.0)) {
            bits |= (1 << d) | (1 << (12 + d));
        }
        if boxes
            .iter()
            .any(|b| touching(b) && covers(b, 7.0 / 16.0, 9.0 / 16.0))
        {
            bits |= 1 << (6 + d);
        }
    }
    let (lo, hi) = boxes
        .iter()
        .fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), b| {
            (
                std::array::from_fn(|i| lo[i].min(b[i])),
                std::array::from_fn(|i| hi[i].max(b[i + 3])),
            )
        });
    let size: [f64; 3] = std::array::from_fn(|i| hi[i] - lo[i]);
    let solid = (size[0] + size[1] + size[2]) / 3.0 >= 0.729_166_666_666_666_6 || size[1] >= 1.0;
    (bits, solid)
}

/// A shape table entry (as `VanillaCollisionOracle.grid` writes them) of boxes in block units:
/// an array grid on their edges, a cell occupied inside a box.
fn shape_entry(boxes: &[[f64; 6]]) -> Result<Tag> {
    let coords: [Vec<f64>; 3] = std::array::from_fn(|axis| {
        let mut v: Vec<f64> = boxes.iter().flat_map(|b| [b[axis], b[axis + 3]]).collect();
        v.sort_by(f64::total_cmp);
        v.dedup();
        v
    });
    let sizes: [usize; 3] = std::array::from_fn(|i| coords[i].len().saturating_sub(1));
    let mid = |axis: usize, i: usize| (coords[axis][i] + coords[axis][i + 1]) / 2.0;
    let mut occupied = Vec::with_capacity(sizes.iter().product());
    for x in 0..sizes[0] {
        for y in 0..sizes[1] {
            for z in 0..sizes[2] {
                let p = [mid(0, x), mid(1, y), mid(2, z)];
                occupied.push(
                    boxes
                        .iter()
                        .any(|b| (0..3).all(|i| b[i] < p[i] && p[i] < b[i + 3])),
                );
            }
        }
    }
    grid_entry(coords, &occupied)
}

/// A shape table entry of an array grid: its edges by axis and its cells, `(x * sy + y) * sz + z`.
fn grid_entry(coords: [Vec<f64>; 3], occupied: &[bool]) -> Result<Tag> {
    let shape =
        horizon_core::geometry::VoxelShape::from_mixed_grid(coords.clone(), occupied, [false; 3])
            .map_err(|e| e.to_string())?;
    let (min, max) = shape.discrete_bounds();
    let axis = |i: usize| Tag::LongArray(coords[i].iter().map(|v| v.to_bits() as i64).collect());
    Ok(Tag::compound([
        ("x", axis(0)),
        ("y", axis(1)),
        ("z", axis(2)),
        (
            "occupied",
            Tag::ByteArray(occupied.iter().map(|o| u8::from(*o)).collect()),
        ),
        ("cube", Tag::Byte(0)),
        ("cube_axes", Tag::Byte(0)),
        ("discrete_min", Tag::IntArray(min.to_vec())),
        ("discrete_max", Tag::IntArray(max.to_vec())),
        ("canonical", Tag::Byte(0)),
    ]))
}

/// The JSON and NBT tables rewritten for new blocks and items.
const JSON_TABLES: [&str; 14] = [
    "facts.json",
    "registries.json",
    "gameplay-facts.json",
    "block-behaviours.json",
    "model-states.json",
    "render-definitions.json",
    "landing-definitions.json",
    "items.json",
    "command-registries.json",
    "mob-data.json",
    "worldgen/block_flags.json",
    "worldgen/blocks/structure-blocks.json",
    "worldgen/blocks/survival-rules.json",
    "worldgen/blocks/survival-extra-definitions.json",
];
const NBT_TABLES: [&str; 6] = [
    "occlusion.nbt",
    "cached-collision.nbt",
    "movement-properties.nbt",
    "worldgen/blocks/block-properties.nbt",
    "worldgen/blocks/cached-support.nbt",
    "worldgen/blocks/cached-collision.nbt",
];
use horizon_items::content_namespaces::MARKER;
pub use horizon_items::content_namespaces::{namespaces, tag_namespaces};
/// Block states a world can hold. Chunk sections widen their global palette
/// past vanilla's 16 bits to fit the registry (crate::chunk::set_block_states);
/// this caps it at 20 bits, about 29 times vanilla's 35,723 states. Mods past
/// it are refused, not wrapped onto other blocks.
pub const MAX_STATES: u32 = 1 << 20;

/// Bundles of block tag files (tag id → the tag file's text).
const TAG_BUNDLES: [&str; 2] = ["block-tags.json", "worldgen/block_tags.json"];
/// The worldgen loader's bundle of biome tags.
const BIOME_TAGS: &str = "worldgen/biome_tags.json";
/// Worldgen registries a content mod may add to, and the bundle the
/// worldgen loader reads each from (id → the file's text): what structure
/// mods hold. Biomes, noise and density functions change the terrain and
/// are not read from mods.
const WORLDGEN_BUNDLES: [(&str, &str); 6] = [
    ("structure", "worldgen/structures.json"),
    ("structure_set", "worldgen/structure_sets.json"),
    ("template_pool", "worldgen/template_pools.json"),
    ("processor_list", "worldgen/processor_lists.json"),
    ("configured_feature", "worldgen/features.json"),
    ("placed_feature", "worldgen/placed_features.json"),
];

/// A mod file that adds to a worldgen bundle: (bundle, id).
fn worldgen_entry(file: &str) -> Option<(&'static str, String)> {
    let (ns, rest) = file.strip_prefix("data/")?.split_once("/worldgen/")?;
    let (kind, path) = rest.split_once('/')?;
    let (_, bundle) = WORLDGEN_BUNDLES.iter().find(|(k, _)| *k == kind)?;
    Some((bundle, format!("{ns}:{}", path.strip_suffix(".json")?)))
}

/// The key of a mod's structure set or template pool file that adds its
/// entries to other sets or pools (vanilla's, another mod's) instead of
/// being one of its own. Vanilla data never has it.
const ADD_TO: &str = "horizon:add_to";
/// The bundles a mod may add to, and the list each entry goes into.
const ADDABLE: [(&str, &str); 2] = [
    ("worldgen/structure_sets.json", "structures"),
    ("worldgen/template_pools.json", "elements"),
];

/// A mod file's additions (`horizon:add_to`): its entries appended to the
/// list of each set or pool it names, as lithostitched's
/// `add_structure_set_entries` and `add_template_pool_elements` do for
/// NeoForge and Fabric packs.
struct Addition {
    targets: Vec<String>,
    list: &'static str,
    entries: Vec<J>,
}
impl Addition {
    /// What a mod file of `bundle` adds, if it is an addition file.
    fn read(bundle: &str, text: &str) -> Result<Option<Self>> {
        let Some((_, list)) = ADDABLE.iter().find(|(b, _)| *b == bundle) else {
            return Ok(None);
        };
        if !text.contains(ADD_TO) {
            return Ok(None);
        }
        let doc: J = serde_json::from_str(text)?;
        let Some(targets) = doc.get(ADD_TO) else {
            return Ok(None);
        };
        let targets = match targets {
            J::Str(s) => vec![s.clone()],
            J::Arr(a) => a
                .iter()
                .map(|v| match v {
                    J::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect::<Option<_>>()
                .ok_or(format!("{ADD_TO}: not a list of ids"))?,
            _ => return Err(format!("{ADD_TO}: not an id or a list of ids").into()),
        };
        let Some(J::Arr(entries)) = doc.get(list) else {
            return Err(format!("{ADD_TO} without a {list} list").into());
        };
        Ok(Some(Self {
            targets,
            list,
            entries: entries.clone(),
        }))
    }
    /// Appends the entries to each target in the bundle `doc` (id → the
    /// file's text). A target nobody defines is skipped, as a tag entry
    /// naming an unknown id is.
    fn apply(&self, doc: &mut J) -> Result<()> {
        for target in &self.targets {
            let Some(J::Str(text)) = doc.get(target) else {
                continue;
            };
            let mut v: J = serde_json::from_str(text).map_err(|e| format!("{target}: {e}"))?;
            let Some(J::Arr(list)) = v.get_mut(self.list) else {
                continue;
            };
            list.extend(self.entries.iter().cloned());
            doc.set(target, J::Str(serde_json::to_string(&v)?));
        }
        Ok(())
    }
}

/// The overlay being written: `out` mirrors `base` through links, and a
/// directory becomes real only when something inside it is written.
struct Overlay<'a> {
    base: &'a Path,
    out: &'a Path,
}
impl Overlay<'_> {
    fn new<'a>(base: &'a Path, out: &'a Path) -> Result<Overlay<'a>> {
        if fs::symlink_metadata(out).is_ok() {
            fs::remove_dir_all(out)?;
        }
        fs::create_dir_all(out)?;
        let overlay = Overlay { base, out };
        overlay.link_children(Path::new(""))?;
        Ok(overlay)
    }
    fn link_children(&self, rel: &Path) -> Result<()> {
        let Ok(read) = fs::read_dir(self.base.join(rel)) else {
            return Ok(());
        };
        for entry in read.flatten() {
            let rel = rel.join(entry.file_name());
            link(&self.base.join(&rel), &self.out.join(&rel))?;
        }
        Ok(())
    }
    /// Makes `rel`'s directories real, each holding links to the base's.
    fn dir(&self, rel: &Path) -> Result<()> {
        let mut at = PathBuf::new();
        for part in rel.iter() {
            at.push(part);
            let path = self.out.join(&at);
            let meta = fs::symlink_metadata(&path);
            if meta
                .as_ref()
                .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
            {
                continue;
            }
            if meta.is_ok() {
                fs::remove_file(&path)?;
            }
            fs::create_dir(&path)?;
            self.link_children(&at)?;
        }
        Ok(())
    }
    fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let rel = Path::new(rel);
        if let Some(parent) = rel.parent() {
            self.dir(parent)?;
        }
        let path = self.out.join(rel);
        if fs::symlink_metadata(&path).is_ok() {
            fs::remove_file(&path)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }
}
#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}
#[cfg(not(unix))]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        fs::create_dir_all(at)?;
        for entry in fs::read_dir(target)?.flatten() {
            link(&entry.path(), &at.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(target, at).map(|_| ())
    }
}

/// A tag file's JSON text with `more` appended to its values (no repeats).
fn merge_tag(text: Option<&str>, more: &[J], what: &str) -> Result<String> {
    let mut doc = match text {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("{what}: {e}"))?,
        None => J::Obj(vec![("values".into(), J::Arr(Vec::new()))]),
    };
    let Some(J::Arr(values)) = doc.get_mut("values") else {
        return Err(format!("{what}: no values").into());
    };
    for v in more {
        if !values.contains(v) {
            values.push(v.clone());
        }
    }
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// The block and item ids and tags a content root has, for tag entries.
struct Known {
    blocks: std::collections::BTreeSet<String>,
    items: std::collections::BTreeSet<String>,
    /// The worldgen loader's biomes and biome tags.
    biomes: std::collections::BTreeSet<String>,
    biome_tags: std::collections::BTreeSet<String>,
    /// `data/<ns>/tags/<kind>/<path>.json` files the mods add.
    tag_files: std::collections::BTreeSet<String>,
}
impl Known {
    fn new(base: &Path, plan: &Plan, tags: &BTreeMap<String, TagMerge>) -> Result<Self> {
        let mut blocks: std::collections::BTreeSet<String> =
            plan.defaults.keys().cloned().collect();
        blocks.extend(plan.blocks.iter().map(|b| b.decl.id.clone()));
        let mut items = std::collections::BTreeSet::new();
        if let J::Obj(o) = read_json(&base.join("items.json"))? {
            items.extend(o.into_iter().map(|(k, _)| k));
        }
        items.extend(plan.items.iter().map(|i| i.rename.to.clone()));
        let keys = |name: &str| -> Result<std::collections::BTreeSet<String>> {
            Ok(match base.join(name).exists() {
                true => match read_json(&base.join(name))? {
                    J::Obj(o) => o.into_iter().map(|(k, _)| k).collect(),
                    _ => Default::default(),
                },
                false => Default::default(),
            })
        };
        Ok(Known {
            blocks,
            items,
            biomes: keys("worldgen/biomes.json")?,
            biome_tags: keys(BIOME_TAGS)?,
            tag_files: tags.keys().cloned().collect(),
        })
    }
    /// Whether tag `file` keeps entry `v`: a known id or an existing tag,
    /// for block, item and biome tags (other registries are kept as they
    /// are). The worldgen loader refuses a biome tag naming what it lacks
    /// (another mod's biomes, a loader's tags), so those entries go.
    fn keeps(&self, file: &str, v: &J) -> bool {
        let id = match v {
            J::Str(s) => s.as_str(),
            J::Obj(_) => match v.get("id") {
                Some(J::Str(s)) => s.as_str(),
                _ => return true,
            },
            _ => return true,
        };
        let Some(rest) = file
            .strip_prefix("data/")
            .and_then(|r| r.split_once("/tags/"))
            .map(|(_, rest)| rest)
        else {
            return true;
        };
        let kind = match rest.strip_prefix("worldgen/biome/") {
            Some(_) => "worldgen/biome",
            None => rest.split_once('/').map_or("", |(k, _)| k),
        };
        let ids = match kind {
            "block" => &self.blocks,
            "item" => &self.items,
            "worldgen/biome" => &self.biomes,
            _ => return true,
        };
        match id.strip_prefix('#') {
            Some(tag) => {
                let (tns, path) = tag.split_once(':').unwrap_or(("minecraft", tag));
                let rel = format!("data/{tns}/tags/{kind}/{path}.json");
                self.tag_files.contains(&rel)
                    || match kind {
                        "worldgen/biome" => self.biome_tags.contains(&format!("{tns}:{path}")),
                        _ => tns == "minecraft",
                    }
            }
            None => ids.contains(id),
        }
    }
}

/// What the mods do to one tag file, in load order (a data pack's
/// TagLoader): each adds its values, and one that replaces the tag first
/// drops vanilla's entries and the mods' before it.
#[derive(Default)]
struct TagMerge {
    /// A mod replaced the tag: vanilla's entries are left out.
    replaced: bool,
    /// The mods whose values are in `values`.
    mods: Vec<String>,
    values: Vec<J>,
}

impl TagMerge {
    /// Adds one mod's tag file. A replace that drops entries (vanilla's in a
    /// `shared` tag, one not in the mod's namespaces, or other mods') warns.
    fn add(
        &mut self,
        id: &str,
        file: &str,
        replace: bool,
        shared: bool,
        values: &[J],
    ) -> Option<String> {
        let mut warning = None;
        if replace {
            let mut dropped: Vec<String> = Vec::new();
            if shared && !self.replaced {
                dropped.push("vanilla's entries".into());
            }
            if !self.mods.is_empty() {
                dropped.push(format!("those of {}", self.mods.join(", ")));
            }
            if !dropped.is_empty() {
                warning = Some(format!(
                    "mod {id} replaces {file}: {} are left out, as a data pack's replace \
                     does; the mods after it in load order still add to it",
                    dropped.join(" and ")
                ));
            }
            self.replaced = true;
            self.mods.clear();
            self.values.clear();
        }
        if !self.mods.iter().any(|m| m == id) {
            self.mods.push(id.to_owned());
        }
        self.values.extend(values.iter().cloned());
        warning
    }
}

/// Threads rewriting the tables at once. Each holds a whole table as a tree
/// (gameplay-facts.json's is over a gigabyte with ATM10's mods), so more
/// would mostly add memory.
const TABLE_THREADS: usize = 3;

/// Rewrites the base's per-state tables for the mods' states into the
/// overlay. They are independent: [`TABLE_THREADS`] threads take them,
/// the largest first.
fn tables(base: &Path, plan: &Plan, overlay: &Overlay<'_>) -> Result<()> {
    let mut jobs: Vec<(&str, u64)> = JSON_TABLES
        .iter()
        .chain(&NBT_TABLES)
        .filter_map(|name| Some((*name, fs::metadata(base.join(name)).ok()?.len())))
        .collect();
    jobs.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    // Their directories become real here, so the threads only write files.
    for (name, _) in &jobs {
        if let Some(parent) = Path::new(name).parent() {
            overlay.dir(parent)?;
        }
    }
    let table = |name: &str| -> Result<()> {
        let path = base.join(name);
        let bytes = if JSON_TABLES.contains(&name) {
            let mut doc = read_json(&path)?;
            plan.json(name, &mut doc)
                .map_err(|e| format!("{name}: {e}"))?;
            serde_json::to_vec(&doc)?
        } else {
            let (mut named, _) = nbt::read_named(&fs::read(&path)?, Limits::tables())
                .map_err(|e| format!("{name}: {e}"))?;
            plan.nbt(name, &mut named.tag);
            nbt::write_named(&named, Limits::tables()).map_err(|e| format!("{name}: {e}"))?
        };
        overlay.write(name, &bytes)
    };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(TABLE_THREADS))
            .map(|_| {
                scope.spawn(|| -> std::result::Result<(), String> {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((name, _)) = jobs.get(i) else {
                            return Ok(());
                        };
                        table(name).map_err(|e| e.to_string())?;
                    }
                })
            })
            .collect();
        workers.into_iter().try_for_each(|w| {
            w.join()
                .unwrap_or_else(|_| Err("a table thread panicked".into()))
        })
    })?;
    Ok(())
}

/// Builds the content root for `mods` over `base` in `out`. Returns the
/// warnings for the log: the shared tags a mod replaced.
pub fn build(base: &Path, mods: &[ContentMod], out: &Path) -> Result<Vec<String>> {
    // Two mods owning one namespace would mix their files.
    for (i, a) in mods.iter().enumerate() {
        for b in &mods[..i] {
            let (b, a) = (
                (b.id.as_str(), b.content.namespaces.as_slice()),
                (a.id.as_str(), a.content.namespaces.as_slice()),
            );
            if let Some(e) = horizon_mods::content::clash(b, a) {
                return Err(e.into());
            }
        }
    }
    // The overlay links to the base: a relative base would leave them dangling.
    let base = &std::path::absolute(base)?;
    let plan = Plan::new(base, mods)?;
    let overlay = Overlay::new(base, out)?;
    tables(base, &plan, &overlay)?;
    // Mod files, and every tag addition gathered per tag file.
    let mut tags: BTreeMap<String, TagMerge> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut worldgen: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    let mut additions: BTreeMap<&str, Vec<Addition>> = BTreeMap::new();
    for m in mods {
        let declarations = format!("data/{}/horizon/", m.id);
        let mut reader = m.content.reader()?;
        for file in &m.content.files {
            let bytes = reader.read(file)?;
            let is_tag = file.starts_with("data/") && file.contains("/tags/");
            if is_tag && file.ends_with(".json") {
                let doc: J = serde_json::from_slice(&bytes).map_err(|e| format!("{file}: {e}"))?;
                let Some(J::Arr(values)) = doc.get("values") else {
                    return Err(format!("{}: {file}: no values", m.id).into());
                };
                let replace = doc.get("replace") == Some(&J::Bool(true));
                let ns = file.split('/').nth(1).unwrap_or_default();
                let shared = !m.content.namespaces.iter().any(|n| n == ns);
                let merge = tags.entry(file.clone()).or_default();
                warnings.extend(merge.add(&m.id, file, replace, shared, values));
            } else if !file.starts_with(&declarations) {
                overlay.write(file, &bytes)?;
                if let Some((bundle, id)) = worldgen_entry(file) {
                    let text = String::from_utf8(bytes).map_err(|e| format!("{file}: {e}"))?;
                    let entries = worldgen.entry(bundle).or_default();
                    match Addition::read(bundle, &text).map_err(|e| format!("{file}: {e}"))? {
                        Some(addition) => additions.entry(bundle).or_default().push(addition),
                        None => entries.push((id, text)),
                    }
                } else if let Some(template) = file
                    .strip_prefix("data/")
                    .and_then(|r| r.split_once("/structure/"))
                    .filter(|(_, path)| path.ends_with(".nbt"))
                {
                    // The worldgen loader's templates: worldgen/structure/<ns>/<path>.nbt.
                    let (ns, path) = template;
                    overlay.write(&format!("worldgen/structure/{ns}/{path}"), &bytes)?;
                }
            }
        }
    }
    // A tag naming an id nobody registers fails as a whole in vanilla, and
    // carried-over mods name blocks left out or other mods' ids: such
    // entries are dropped instead, so the rest of the tag holds.
    let known = Known::new(base, &plan, &tags)?;
    for (file, merge) in &mut tags {
        merge.values.retain(|v| known.keeps(file, v));
    }
    let mut bundles: Vec<(&str, J)> = Vec::new();
    for name in TAG_BUNDLES {
        if base.join(name).exists() {
            bundles.push((name, read_json(&base.join(name))?));
        }
    }
    for (file, merge) in &tags {
        let vanilla = fs::read_to_string(base.join(file))
            .ok()
            .filter(|_| !merge.replaced);
        overlay.write(
            file,
            merge_tag(vanilla.as_deref(), &merge.values, file)?.as_bytes(),
        )?;
        // data/<ns>/tags/block/<path>.json is the block tag <ns>:<path>.
        let Some((ns, path)) = file
            .strip_prefix("data/")
            .and_then(|r| r.split_once("/tags/block/"))
            .and_then(|(ns, p)| Some((ns, p.strip_suffix(".json")?)))
        else {
            continue;
        };
        let id = format!("{ns}:{path}");
        for (name, doc) in &mut bundles {
            let text = match doc.get(&id) {
                Some(J::Str(t)) if !merge.replaced => Some(t.clone()),
                _ => None,
            };
            let merged = merge_tag(text.as_deref(), &merge.values, name)?;
            doc.set(&id, J::Str(merged));
        }
    }
    for (name, doc) in &bundles {
        overlay.write(name, &serde_json::to_vec(doc)?)?;
    }
    // Biome tags, merged into the worldgen loader's bundle as block tags are.
    let biome_tags: Vec<(String, &TagMerge)> = tags
        .iter()
        .filter_map(|(file, merge)| {
            let (ns, path) = file
                .strip_prefix("data/")?
                .split_once("/tags/worldgen/biome/")?;
            Some((format!("{ns}:{}", path.strip_suffix(".json")?), merge))
        })
        .collect();
    if !biome_tags.is_empty() && base.join(BIOME_TAGS).exists() {
        let mut doc = read_json(&base.join(BIOME_TAGS))?;
        for (id, merge) in biome_tags {
            let text = match doc.get(&id) {
                Some(J::Str(t)) if !merge.replaced => Some(t.clone()),
                _ => None,
            };
            let merged = merge_tag(text.as_deref(), &merge.values, BIOME_TAGS)?;
            doc.set(&id, J::Str(merged));
        }
        overlay.write(BIOME_TAGS, &serde_json::to_vec(&doc)?)?;
    }
    // Structures and the pools, sets, processors and features they use, then
    // what mods add to sets and pools, in mod and file order.
    for (bundle, entries) in &worldgen {
        if !base.join(bundle).exists() {
            continue;
        }
        let mut doc = read_json(&base.join(bundle))?;
        for (id, text) in entries {
            doc.set(id, J::Str(text.clone()));
        }
        for addition in additions.get(bundle).into_iter().flatten() {
            addition
                .apply(&mut doc)
                .map_err(|e| format!("{bundle}: {e}"))?;
        }
        overlay.write(bundle, &serde_json::to_vec(&doc)?)?;
    }
    // A block without its own blockstate looks like its template, so every
    // state still has a model.
    for m in mods {
        for block in &m.content.blocks {
            let (ns, path) = block.id.split_once(':').unwrap_or((&m.id, &block.id));
            let own = format!("assets/{ns}/blockstates/{path}.json");
            if m.content.files.contains(&own) {
                continue;
            }
            let template = block.template.trim_start_matches("minecraft:");
            let vanilla = base.join(format!("assets/minecraft/blockstates/{template}.json"));
            overlay.write(&own, &fs::read(&vanilla)?)?;
        }
    }
    // The blocks' kinetic parts, by block id: the game's networks (kinetics.rs) and the models
    // it turns (the model catalog bakes those no blockstate names).
    let kinetic: BTreeMap<&str, &horizon_mods::content::Kinetic> = mods
        .iter()
        .flat_map(|m| &m.content.blocks)
        .filter_map(|b| Some((b.id.as_str(), b.kinetic.as_ref()?)))
        .collect();
    if !kinetic.is_empty() {
        overlay.write(KINETIC, &serde_json::to_vec(&kinetic)?)?;
    }
    let mut namespaces: Vec<&str> = mods
        .iter()
        .flat_map(|m| m.content.namespaces.iter().map(String::as_str))
        .collect();
    namespaces.sort();
    overlay.write(MARKER, &serde_json::to_vec(&namespaces)?)?;
    overlay.write(WARNINGS, warnings.join("\n").as_bytes())?;
    Ok(warnings)
}

/// The root to load: `base` itself with no content, else the mods' overlay
/// over it, built in `cache/mod-content/<set>`.
///
/// The root is built once and kept: later starts with the same build, base
/// and mod files reuse it. With ATM10's carried mods a build takes about
/// 25 s and peaks above 3 GB, more than everything the game keeps.
pub fn resource_root(base: &Path, mods: &[ContentMod], cache: &Path) -> Result<PathBuf> {
    resource_root_with(base, mods, cache, &mut || {})
}

/// Sets of mods whose roots stay built: worlds with other sets start again with theirs
/// (Starling's game/world_content.rs), and going back to one of these needs no build.
const KEPT_SETS: usize = 3;

/// [`resource_root`], calling `on_build` before a build (none when the root is reused), so a
/// loading screen can say why it waits.
pub fn resource_root_with(
    base: &Path,
    mods: &[ContentMod],
    cache: &Path,
    on_build: &mut dyn FnMut(),
) -> Result<PathBuf> {
    if mods.iter().all(|m| m.content.is_empty()) {
        return Ok(base.to_owned());
    }
    let sets = cache.join("mod-content");
    // The first layout kept one root in mod-content itself.
    if sets.join(STAMP).is_file() {
        let _ = fs::remove_dir_all(&sets);
    }
    let out = sets.join(set_name(mods));
    let stamp = stamp(base, mods)?;
    if fs::read(out.join(STAMP)).is_ok_and(|s| s == stamp.as_bytes()) {
        // A reused root warns as the build did; rewriting the stamp marks it as used.
        let warnings = fs::read_to_string(out.join(WARNINGS)).unwrap_or_default();
        warnings.lines().for_each(|w| log::warn!("Horizon: {w}"));
        let _ = fs::write(out.join(STAMP), &stamp);
        return Ok(out);
    }
    on_build();
    for warning in build(base, mods, &out)? {
        log::warn!("Horizon: {warning}");
    }
    fs::write(out.join(STAMP), stamp)?;
    prune_sets(&sets, &out);
    Ok(out)
}

/// A set's folder: its mods' ids, hashed.
fn set_name(mods: &[ContentMod]) -> String {
    use sha2::{Digest, Sha256};
    let mut ids: Vec<&str> = mods.iter().map(|m| m.id.as_str()).collect();
    ids.sort_unstable();
    let hash = Sha256::digest(ids.join("\n").as_bytes());
    hash.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// Removes the roots of sets not used for longest, past [`KEPT_SETS`], never `keep`.
fn prune_sets(sets: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(sets) else {
        return;
    };
    let mut used: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p != keep && p.is_dir())
        .map(|p| {
            let when = fs::metadata(p.join(STAMP))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (when, p)
        })
        .collect();
    used.sort();
    let extra = (used.len() + 1).saturating_sub(KEPT_SETS);
    for (_, path) in used.into_iter().take(extra) {
        let _ = fs::remove_dir_all(path);
    }
}

/// In a root built by [`build`]: its warnings, one per line.
const WARNINGS: &str = "horizon-mod-content-warnings.txt";
/// In a root built by [`build`] with kinetic blocks: their parts, by block id (kinetics.rs).
pub const KINETIC: &str = "kinetic.json";

/// In a root built by [`resource_root`]: what it was built from.
const STAMP: &str = "horizon-mod-content.stamp";

/// What a content root is built from: this build of the game, the base's
/// manifest and the tables [`build`] rewrites, and every file of every mod
/// in order. Files count by size and modification time, as build tools do.
fn stamp(base: &Path, mods: &[ContentMod]) -> Result<String> {
    use sha2::{Digest, Sha256};
    let meta = |path: &Path| -> Result<String> {
        let m = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let modified = m
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Ok(format!(
            "{} {} {}",
            path.display(),
            m.len(),
            modified.as_nanos()
        ))
    };
    let mut parts = vec!["horizon mod content 1".to_owned()];
    parts.push(meta(&std::env::current_exe()?)?);
    // On Android the program is the system's app_process: the game is the APK's library, or the
    // build with native mods loaded in its place.
    let library = std::env::var_os("STARLING_GAME_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| crate::platform::library_dir().map(|d| d.join("libhorizon_android.so")));
    if let Some(library) = library.filter(|p| p.is_file()) {
        parts.push(meta(&library)?);
    }
    let base = std::path::absolute(base)?;
    parts.push(
        String::from_utf8_lossy(&fs::read(base.join("horizon-resources.json"))?).into_owned(),
    );
    for name in JSON_TABLES.iter().chain(&NBT_TABLES).chain(&TAG_BUNDLES) {
        if base.join(name).exists() {
            parts.push(meta(&base.join(name))?);
        }
    }
    for m in mods {
        parts.push(m.id.clone());
        let source = m.content.source();
        if source.is_dir() {
            for file in &m.content.files {
                parts.push(meta(&source.join(file))?);
            }
        } else {
            parts.push(meta(source)?);
        }
    }
    let mut hasher = Sha256::new();
    for part in &parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
#[path = "mod_content_tests.rs"]
mod tests;
