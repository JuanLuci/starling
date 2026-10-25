//! The pretend world's recipes (`recipes`): those of the resources' data pack a viewer shows
//! (shaped and shapeless crafting, the furnaces' and the campfire's, stonecutting, smithing
//! upgrades), their item tags expanded; the game's transmuting, trim and brewing recipes are left
//! out. Without resources, two made up.
use crate::world::{Data, expand};
use horizon_mods::{ItemCatalog, ItemStack, Lookup, Recipe, RecipeCatalog, RecipeKind};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

pub struct Pretend {
    all: Vec<Recipe>,
}

impl Pretend {
    pub fn read(root: Option<&Path>, data: &Data) -> Pretend {
        let Some(root) = root.filter(|r| r.join("data/minecraft/recipe").is_dir()) else {
            return made_up(data);
        };
        let mut raw: HashMap<String, Vec<String>> = HashMap::new();
        let tags = root.join("data/minecraft/tags/item");
        for entry in std::fs::read_dir(&tags).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(v) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let values = v["values"].as_array().into_iter().flatten();
            raw.insert(
                format!("minecraft:{name}"),
                values
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect(),
            );
        }
        let stack = |id: &str, count: u32| make(data, id, count);
        let ingredient = |v: &Value| -> Vec<ItemStack> {
            let names: Vec<&str> = match v {
                Value::String(s) => vec![s.as_str()],
                Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            let mut ids: Vec<String> = Vec::new();
            for name in names {
                match name.strip_prefix('#') {
                    Some(tag) => {
                        let mut items = std::collections::HashSet::new();
                        expand(tag, &raw, &mut items, &mut vec![tag.to_owned()]);
                        let mut items: Vec<String> = items.into_iter().collect();
                        items.sort();
                        ids.extend(items);
                    }
                    None => ids.push(name.to_owned()),
                }
            }
            ids.iter().map(|id| stack(id, 1)).collect()
        };
        let result = |v: &Value| {
            stack(v["id"].as_str().unwrap_or("minecraft:air"), {
                v["count"].as_u64().unwrap_or(1) as u32
            })
        };
        let mut files: Vec<_> = std::fs::read_dir(root.join("data/minecraft/recipe"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        let mut all = Vec::new();
        for path in files {
            let Some(v) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let recipe = |kind, inputs, outputs, station: &str| Recipe {
                id: format!("minecraft:{name}"),
                kind,
                width: 0,
                height: 0,
                inputs,
                outputs,
                station: station.to_owned(),
                time: v["cookingtime"].as_u64().unwrap_or(0) as u32,
                experience: v["experience"].as_f64().unwrap_or(0.0) as f32,
            };
            let cooking = |kind, station| {
                recipe(
                    kind,
                    vec![ingredient(&v["ingredient"])],
                    vec![result(&v["result"])],
                    station,
                )
            };
            all.push(match v["type"].as_str().unwrap_or("") {
                "minecraft:crafting_shaped" => {
                    let rows: Vec<&str> = v["pattern"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
                    let inputs = rows
                        .iter()
                        .flat_map(|r| {
                            let mut cells: Vec<char> = r.chars().collect();
                            cells.resize(width, ' ');
                            cells
                        })
                        .map(|c| match c {
                            ' ' => Vec::new(),
                            c => ingredient(&v["key"][c.to_string()]),
                        })
                        .collect();
                    Recipe {
                        width: width as u32,
                        height: rows.len() as u32,
                        ..recipe(
                            RecipeKind::Crafting,
                            inputs,
                            vec![result(&v["result"])],
                            "minecraft:crafting_table",
                        )
                    }
                }
                "minecraft:crafting_shapeless" => recipe(
                    RecipeKind::Crafting,
                    v["ingredients"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(&ingredient)
                        .collect(),
                    vec![result(&v["result"])],
                    "minecraft:crafting_table",
                ),
                "minecraft:smelting" => cooking(RecipeKind::Smelting, "minecraft:furnace"),
                "minecraft:blasting" => cooking(RecipeKind::Blasting, "minecraft:blast_furnace"),
                "minecraft:smoking" => cooking(RecipeKind::Smoking, "minecraft:smoker"),
                "minecraft:campfire_cooking" => {
                    cooking(RecipeKind::CampfireCooking, "minecraft:campfire")
                }
                "minecraft:stonecutting" => recipe(
                    RecipeKind::Stonecutting,
                    vec![ingredient(&v["ingredient"])],
                    vec![result(&v["result"])],
                    "minecraft:stonecutter",
                ),
                "minecraft:smithing_transform" => recipe(
                    RecipeKind::Smithing,
                    ["template", "base", "addition"]
                        .iter()
                        .map(|k| ingredient(&v[*k]))
                        .collect(),
                    vec![result(&v["result"])],
                    "minecraft:smithing_table",
                ),
                _ => continue,
            });
        }
        Pretend { all }
    }
    /// The recipes `lookup` finds for `stack`, as the game finds them.
    fn found(&self, stack: &ItemStack, lookup: Lookup) -> impl Iterator<Item = &Recipe> {
        let same = |s: &ItemStack| {
            s.id == stack.id && (stack.components.is_empty() || s.components == stack.components)
        };
        self.all.iter().filter(move |r| match lookup {
            Lookup::Recipes => r.outputs.iter().any(same),
            Lookup::Uses => r.inputs.iter().flatten().any(same),
        })
    }
}

impl RecipeCatalog for Pretend {
    fn count(&self, stack: &ItemStack, lookup: Lookup) -> Vec<(RecipeKind, u32)> {
        let mut counts: Vec<(RecipeKind, u32)> = Vec::new();
        for r in self.found(stack, lookup) {
            match counts.iter_mut().find(|(k, _)| *k == r.kind) {
                Some((_, n)) => *n += 1,
                None => counts.push((r.kind, 1)),
            }
        }
        counts.sort_by_key(|(k, _)| *k as u8);
        counts
    }
    fn recipes(
        &self,
        stack: &ItemStack,
        lookup: Lookup,
        kind: RecipeKind,
        start: usize,
        count: usize,
    ) -> Vec<Recipe> {
        self.found(stack, lookup)
            .filter(|r| r.kind == kind)
            .skip(start)
            .take(count)
            .cloned()
            .collect()
    }
    fn shape(&self, id: &str) -> Option<(RecipeKind, u32, u32)> {
        let r = self.all.iter().find(|r| r.id == id)?;
        Some(match (r.kind, r.height) {
            (RecipeKind::Crafting, 0) => (r.kind, r.inputs.len() as u32, 0),
            (RecipeKind::Crafting, h) => (r.kind, r.width, h),
            (kind, _) => (kind, r.inputs.len() as u32, 0),
        })
    }
}

fn make(data: &Data, id: &str, count: u32) -> ItemStack {
    let info = data.info(id);
    ItemStack {
        id: id.to_owned(),
        count,
        damage: 0,
        max_damage: info.as_ref().map_or(0, |i| i.max_damage),
        name: info.map_or_else(
            || id.rsplit(':').next().unwrap_or(id).replace('_', " "),
            |i| i.name,
        ),
        components: String::new(),
    }
}

/// Planks into sticks, and a torch.
fn made_up(data: &Data) -> Pretend {
    let s = |id: &str, count| make(data, id, count);
    let crafting = |id: &str, width, height, inputs, outputs| Recipe {
        id: id.to_owned(),
        kind: RecipeKind::Crafting,
        width,
        height,
        inputs,
        outputs,
        station: "minecraft:crafting_table".into(),
        time: 0,
        experience: 0.0,
    };
    let planks = vec![
        s("minecraft:oak_planks", 1),
        s("minecraft:spruce_planks", 1),
    ];
    Pretend {
        all: vec![
            crafting(
                "minecraft:stick",
                1,
                2,
                vec![planks.clone(), planks],
                vec![s("minecraft:stick", 4)],
            ),
            crafting(
                "minecraft:torch",
                1,
                2,
                vec![vec![s("minecraft:coal", 1)], vec![s("minecraft:stick", 1)]],
                vec![s("minecraft:torch", 4)],
            ),
        ],
    }
}
