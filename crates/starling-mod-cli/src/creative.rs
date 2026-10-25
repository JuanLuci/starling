//! The pretend world's creative tabs (`creative`): the tabs and stacks of a prepared resources
//! folder's `creative-inventory.json` (the game's own export), else a few made-up ones, searched
//! by a stack's name, tooltip lines, id (`namespace:path`) and tags (`#tag`) as plain substrings,
//! a simpler search than the game's.
use crate::world::Data;
use horizon_mods::{CreativeCatalog, ItemCatalog, ItemPage, ItemStack, Tab, TabKind};
use serde_json::Value;
use std::path::Path;

struct Stack {
    stack: ItemStack,
    /// What the search looks in, lower-cased: the name and tooltip lines, and the tags.
    lines: Vec<String>,
    tags: Vec<String>,
}

pub struct Pretend {
    tabs: Vec<Tab>,
    items: Vec<Vec<Stack>>,
}

impl CreativeCatalog for Pretend {
    fn tabs(&self) -> Vec<Tab> {
        self.tabs.clone()
    }
    fn items(&self, tab: &str, search: &str, start: usize, count: usize) -> ItemPage {
        let Some(i) = self.tabs.iter().position(|t| t.id == tab) else {
            return ItemPage {
                total: 0,
                items: Vec::new(),
            };
        };
        let term = search.to_lowercase();
        let found: Vec<&Stack> = self.items[i]
            .iter()
            .filter(|s| self.tabs[i].kind != TabKind::Search || matches(s, &term))
            .collect();
        ItemPage {
            total: found.len() as u32,
            items: found
                .into_iter()
                .skip(start)
                .take(count)
                .map(|s| s.stack.clone())
                .collect(),
        }
    }
    fn order(&self, stack: &ItemStack) -> Option<u32> {
        let i = self.tabs.iter().position(|t| t.kind == TabKind::Search)?;
        let listed = &self.items[i];
        let place = |exact: bool| {
            listed.iter().position(|s| {
                s.stack.id == stack.id && (!exact || s.stack.components == stack.components)
            })
        };
        place(true).or_else(|| place(false)).map(|p| p as u32)
    }
}

fn matches(s: &Stack, term: &str) -> bool {
    if term.is_empty() {
        return true;
    }
    if let Some(tag) = term.strip_prefix('#') {
        return s.tags.iter().any(|t| t.contains(tag));
    }
    if let Some((ns, path)) = term.split_once(':') {
        let (own_ns, own_path) = s
            .stack
            .id
            .split_once(':')
            .unwrap_or(("minecraft", &s.stack.id));
        return own_ns.contains(ns.trim()) && own_path.contains(path.trim());
    }
    s.lines.iter().any(|l| l.contains(term))
}

impl Pretend {
    /// The tabs of `root`'s `creative-inventory.json`, else made-up ones.
    pub fn read(root: Option<&Path>, data: &Data) -> Pretend {
        let raw: Option<Value> = root
            .and_then(|r| std::fs::read(r.join("creative-inventory.json")).ok())
            .and_then(|b| serde_json::from_slice(&b).ok());
        let raw = raw.unwrap_or_else(made_up);
        let mut tabs = Vec::new();
        let mut items = Vec::new();
        for t in raw["tabs"].as_array().into_iter().flatten() {
            let kind = match t["type"].as_str() {
                Some("SEARCH") => TabKind::Search,
                Some("INVENTORY") => TabKind::Inventory,
                Some("CATEGORY") => TabKind::Category,
                _ => continue,
            };
            if t["visible"] == false {
                continue;
            }
            let strings = |v: &Value| -> Vec<String> {
                v.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().map(str::to_lowercase))
                    .collect()
            };
            let stacks: Vec<Stack> = t["items"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(i, v)| {
                    let stack = parse_stack(v.as_str()?, data)?;
                    let mut lines = strings(&t["lines"][i]);
                    lines.push(stack.name.to_lowercase());
                    Some(Stack {
                        stack,
                        lines,
                        tags: strings(&t["tags"][i]),
                    })
                })
                .collect();
            let icon = t["icon"]
                .as_str()
                .and_then(|s| parse_stack(s, data))
                .map_or_else(|| "minecraft:stone".to_owned(), |s| s.id);
            tabs.push(Tab {
                id: t["id"].as_str().unwrap_or_default().to_owned(),
                title: t["title"].as_str().unwrap_or_default().to_owned(),
                icon,
                kind,
                top: t["row"] == "TOP",
                column: t["column"].as_u64().unwrap_or(0) as u32,
                size: stacks.len() as u32,
            });
            items.push(stacks);
        }
        Pretend { tabs, items }
    }
}

/// A few tabs for a run without resources.
fn made_up() -> Value {
    let stacks = |ids: &[&str]| -> Vec<String> {
        ids.iter()
            .map(|id| format!("{{count:1,id:\"minecraft:{id}\"}}"))
            .collect()
    };
    let building = [
        "stone",
        "granite",
        "dirt",
        "oak_log",
        "oak_planks",
        "bricks",
        "glass",
    ];
    let tools = [
        "diamond_pickaxe",
        "diamond_sword",
        "bow",
        "arrow",
        "elytra",
        "shield",
    ];
    let all: Vec<&str> = building.iter().chain(&tools).copied().collect();
    serde_json::json!({ "tabs": [
        { "id": "minecraft:building_blocks", "type": "CATEGORY", "row": "TOP", "column": 0,
          "title": "Building Blocks", "icon": "{count:1,id:\"minecraft:bricks\"}",
          "items": stacks(&building) },
        { "id": "minecraft:tools_and_utilities", "type": "CATEGORY", "row": "TOP", "column": 1,
          "title": "Tools & Utilities", "icon": "{count:1,id:\"minecraft:diamond_pickaxe\"}",
          "items": stacks(&tools) },
        { "id": "minecraft:search", "type": "SEARCH", "row": "TOP", "column": 6,
          "title": "Search Items", "icon": "{count:1,id:\"minecraft:compass\"}",
          "items": stacks(&all) },
        { "id": "minecraft:inventory", "type": "INVENTORY", "row": "BOTTOM", "column": 6,
          "title": "Survival Inventory", "icon": "{count:1,id:\"minecraft:chest\"}", "items": [] }
    ]})
}

/// A stack's SNBT as the export writes it (`{count:1,id:"minecraft:stone"}`, with `components`
/// when they differ from the item's), its components kept as written.
fn parse_stack(snbt: &str, data: &Data) -> Option<ItemStack> {
    let body = snbt.trim().strip_prefix('{')?.strip_suffix('}')?;
    let (mut id, mut count, mut components) = (None, 1, String::new());
    for (key, value) in fields(body) {
        match key.trim_matches('"') {
            "id" => id = Some(value.trim_matches('"').to_owned()),
            "count" => count = value.trim_end_matches(['b', 's']).parse().unwrap_or(1),
            "components" => components = value.to_owned(),
            _ => {}
        }
    }
    let id = id?;
    let info = data.info(&id);
    Some(ItemStack {
        count,
        damage: 0,
        max_damage: info.as_ref().map_or(0, |i| i.max_damage),
        name: info.map_or_else(
            || id.rsplit(':').next().unwrap_or(&id).replace('_', " "),
            |i| i.name,
        ),
        components,
        id,
    })
}

/// A compound's top-level `key:value` pairs, the values as written.
fn fields(body: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let (mut depth, mut quoted, mut start) = (0i32, None::<char>, 0);
    let bytes: Vec<(usize, char)> = body.char_indices().collect();
    let mut escaped = false;
    for &(i, c) in &bytes {
        if let Some(q) = quoted {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quoted = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quoted = Some(c),
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.extend(pair(&body[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.extend(pair(&body[start..]));
    out
}

fn pair(field: &str) -> Option<(&str, &str)> {
    let field = field.trim();
    // The key ends at the first colon outside quotes (keys may be quoted, with colons inside).
    let end = if let Some(rest) = field.strip_prefix('"') {
        rest.find('"')? + 2
    } else {
        field.find(':')?
    };
    let (key, value) = field.split_at(end);
    Some((key.trim(), value.trim_start().strip_prefix(':')?.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_exports_stacks() {
        let data = Data::default();
        let s = parse_stack("{count:1,id:\"minecraft:stone\"}", &data).unwrap();
        assert_eq!(
            (s.id.as_str(), s.count, s.components.as_str()),
            ("minecraft:stone", 1, "")
        );
        let s = parse_stack(
            "{components:{\"minecraft:enchantments\":{\"minecraft:sharpness\":5}},count:1,id:\"minecraft:enchanted_book\"}",
            &data,
        )
        .unwrap();
        assert_eq!(s.id, "minecraft:enchanted_book");
        assert_eq!(
            s.components,
            "{\"minecraft:enchantments\":{\"minecraft:sharpness\":5}}"
        );
    }

    #[test]
    fn made_up_tabs_search() {
        let p = Pretend::read(None, &Data::default());
        assert_eq!(p.tabs.len(), 4);
        let found = p.items("minecraft:search", "pick", 0, 10);
        assert_eq!(found.total, 1);
        assert_eq!(found.items[0].id, "minecraft:diamond_pickaxe");
        assert_eq!(p.items("minecraft:search", "", 0, 3).items.len(), 3);
        assert_eq!(p.items("minecraft:search", "", 0, 3).total, 13);
        assert_eq!(p.items("minecraft:building_blocks", "zzz", 2, 2).total, 7);
    }
}
