//! The pretend world `starling-mod run` plays a mod in: a player standing in the overworld at
//! noon with the scenario's fields over that, what the crosshair is on, and the game's items,
//! blocks and creatures from a prepared resources folder.
use horizon_mods::{
    BlockHit, BlockInfo, BlockPos, Clock, Direction, Entity, EntityHit, EntityInfo, Food, GameMode,
    ItemCatalog, ItemInfo, ItemStack, LevelView, Mining, Player, Snapshot, Target,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

/// A block as `level.get-block-info` tells it, but for the held item.
struct Block {
    name: String,
    item: Option<String>,
    hardness: f32,
    needs_tool: bool,
}

/// The game's items, blocks and creatures, from a prepared resources folder; empty without one.
#[derive(Default)]
pub struct Data {
    items: HashMap<String, ItemInfo>,
    /// Each tool's `minecraft:tool` rules: the blocks a rule covers, and whether they drop.
    tools: HashMap<String, Vec<(Vec<String>, Option<bool>)>>,
    blocks: HashMap<String, Block>,
    /// Block tags, with the tags they name resolved.
    tags: HashMap<String, HashSet<String>>,
    /// Each creature's max health and armor.
    kinds: HashMap<String, (f32, f32)>,
    lang: HashMap<String, String>,
}

impl ItemCatalog for Data {
    fn info(&self, id: &str) -> Option<ItemInfo> {
        self.items.get(id).cloned()
    }
}

impl Data {
    /// `root` holds `items.json`, the block and creature facts and `assets/minecraft`; None when
    /// it has no items.
    pub fn read(root: &Path) -> Option<Data> {
        let read = |path: &str| -> Option<Value> {
            serde_json::from_slice(&std::fs::read(root.join(path)).ok()?).ok()
        };
        let lang = read("assets/minecraft/lang/en_us.json")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let mut data = Data {
            lang,
            ..Data::default()
        };
        for (id, c) in read("items.json")?.as_object()? {
            let key = c["minecraft:item_name"]["translate"]
                .as_str()
                .unwrap_or_default();
            let food = c.get("minecraft:food").map(|f| Food {
                nutrition: f["nutrition"].as_u64().unwrap_or(0) as u32,
                saturation: f["saturation"].as_f64().unwrap_or(0.0) as f32,
                can_always_eat: f["can_always_eat"].as_bool().unwrap_or(false),
            });
            let info = ItemInfo {
                id: id.clone(),
                name: data.lang.get(key).unwrap_or(id).clone(),
                max_stack_size: c["minecraft:max_stack_size"].as_u64().unwrap_or(64) as u32,
                max_damage: c["minecraft:max_damage"].as_u64().unwrap_or(0) as u32,
                food,
            };
            data.items.insert(id.clone(), info);
            let rules: Vec<_> = c["minecraft:tool"]["rules"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    let blocks = match &r["blocks"] {
                        Value::String(s) => vec![s.clone()],
                        Value::Array(a) => a
                            .iter()
                            .filter_map(|b| b.as_str().map(str::to_owned))
                            .collect(),
                        _ => Vec::new(),
                    };
                    (blocks, r["correct_for_drops"].as_bool())
                })
                .collect();
            if !rules.is_empty() {
                data.tools.insert(id.clone(), rules);
            }
        }
        // A block's item (block-behaviours.json), and its hardness at the default state
        // (facts.json's state, gameplay-facts.json's facts).
        let behaviours = read("block-behaviours.json");
        let defaults = read("facts.json");
        let gameplay = read("gameplay-facts.json");
        let blocks = behaviours.as_ref().and_then(|b| b["blocks"].as_array());
        for b in blocks.into_iter().flatten() {
            let Some(id) = b["name"].as_str() else {
                continue;
            };
            let state = defaults.as_ref().and_then(|d| d["defaults"][id].as_u64());
            let facts = state.and_then(|s| gameplay.as_ref()?["states"].get(s as usize));
            let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let block = Block {
                name: data
                    .lang
                    .get(&format!("block.{namespace}.{path}"))
                    .cloned()
                    .unwrap_or_else(|| path.replace('_', " ")),
                item: b["item"]
                    .as_str()
                    .filter(|i| *i != "minecraft:air")
                    .map(str::to_owned),
                hardness: facts.and_then(|f| f["hardness"].as_f64()).unwrap_or(0.0) as f32,
                needs_tool: facts
                    .and_then(|f| f["requires_tool"].as_bool())
                    .unwrap_or(false),
            };
            data.blocks.insert(id.to_owned(), block);
        }
        // block-tags.json holds each tag file's text.
        let raw: HashMap<String, Vec<String>> = read("block-tags.json")
            .as_ref()
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(tag, file)| {
                let file: Value = match file {
                    Value::String(text) => serde_json::from_str(text).ok()?,
                    other => other.clone(),
                };
                let values = file["values"]
                    .as_array()?
                    .iter()
                    .filter_map(|e| e.as_str().or_else(|| e["id"].as_str()).map(str::to_owned))
                    .collect();
                Some((tag.clone(), values))
            })
            .collect();
        for tag in raw.keys() {
            let mut blocks = HashSet::new();
            expand(tag, &raw, &mut blocks, &mut vec![tag.clone()]);
            data.tags.insert(tag.clone(), blocks);
        }
        for (id, kind) in read("entity-types.json")
            .as_ref()
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let attributes = &kind["attributes"];
            if let Some(health) = attributes["minecraft:max_health"].as_f64() {
                let armor = attributes["minecraft:armor"].as_f64().unwrap_or(0.0);
                data.kinds.insert(id.clone(), (health as f32, armor as f32));
            }
        }
        Some(data)
    }

    /// `Tool.isCorrectForDrops` of the held item on `block`.
    fn correct_for_drops(&self, held: Option<&str>, block: &str) -> bool {
        let Some(rules) = held.and_then(|h| self.tools.get(h)) else {
            return false;
        };
        for (blocks, correct) in rules {
            match correct {
                Some(correct) if blocks.iter().any(|b| self.matches(b, block)) => return *correct,
                _ => {}
            }
        }
        false
    }

    fn matches(&self, spec: &str, block: &str) -> bool {
        match spec.strip_prefix('#') {
            Some(tag) => self.tags.get(tag).is_some_and(|s| s.contains(block)),
            None => spec == block,
        }
    }

    /// What `level.get-block-info` says of `state` for a player holding `held`.
    fn block_info(&self, state: &str, held: Option<&str>) -> BlockInfo {
        let id = state.split('[').next().unwrap_or(state);
        let block = self.blocks.get(id);
        let needs_tool = block.is_some_and(|b| b.needs_tool);
        let tool = ["pickaxe", "axe", "shovel", "hoe"]
            .into_iter()
            .find(|tool| {
                self.tags
                    .get(&format!("minecraft:mineable/{tool}"))
                    .is_some_and(|s| s.contains(id))
            });
        BlockInfo {
            block: state.to_owned(),
            name: block.map_or_else(|| short(id), |b| b.name.clone()),
            item: block.map_or_else(|| Some(id.to_owned()), |b| b.item.clone()),
            hardness: block.map_or(1.0, |b| b.hardness),
            tool: tool.map(str::to_owned),
            needs_tool,
            harvestable: !needs_tool || self.correct_for_drops(held, id),
        }
    }
}

/// `tag`'s blocks into `blocks`, through the tags it names (`seen` against loops).
pub(crate) fn expand(
    tag: &str,
    raw: &HashMap<String, Vec<String>>,
    blocks: &mut HashSet<String>,
    seen: &mut Vec<String>,
) {
    for value in raw.get(tag).into_iter().flatten() {
        match value.strip_prefix('#') {
            Some(inner) if !seen.iter().any(|s| s == inner) => {
                seen.push(inner.to_owned());
                expand(inner, raw, blocks, seen);
            }
            Some(_) => {}
            None => {
                blocks.insert(value.clone());
            }
        }
    }
}

/// `minecraft:oak_log` as `oak log`, for what the resources do not name.
fn short(id: &str) -> String {
    id.rsplit(':').next().unwrap_or(id).replace('_', " ")
}

/// The blocks the scenario places (what the crosshair is on among them), air elsewhere, all in
/// the scenario's `light` (`[block, sky]`, `[0, 15]` by default).
struct Level {
    blocks: HashMap<[i32; 3], String>,
    data: Arc<Data>,
    held: Option<String>,
    light: (u8, u8),
}
impl Level {
    fn air(&self, pos: [i32; 3]) -> bool {
        self.blocks
            .get(&pos)
            .is_none_or(|b| b.starts_with("minecraft:air"))
    }
}

impl LevelView for Level {
    fn block(&self, pos: [i32; 3]) -> Option<String> {
        Some(
            self.blocks
                .get(&pos)
                .cloned()
                .unwrap_or_else(|| "minecraft:air".into()),
        )
    }
    fn biome(&self, _pos: [i32; 3]) -> Option<String> {
        Some("minecraft:plains".into())
    }
    fn light(&self, _pos: [i32; 3]) -> Option<(u8, u8)> {
        Some(self.light)
    }
    /// Air with air above, on a block that is neither air nor a liquid: the pretend world
    /// knows no shapes.
    fn spawn_spots(&self, from: [i32; 3], to: [i32; 3]) -> horizon_mods::SpawnSpots {
        let mut spots = horizon_mods::SpawnSpots::default();
        let mut places: Vec<[i32; 3]> = self
            .blocks
            .iter()
            .filter(|(_, b)| {
                !b.starts_with("minecraft:air")
                    && !b.starts_with("minecraft:water")
                    && !b.starts_with("minecraft:lava")
            })
            .map(|(p, _)| [p[0], p[1] + 1, p[2]])
            .filter(|p| (0..3).all(|i| (from[i]..=to[i]).contains(&p[i])))
            .filter(|p| self.air(*p) && self.air([p[0], p[1] + 1, p[2]]))
            .collect();
        places.sort_by_key(|p| (p[1], p[2], p[0]));
        for p in places {
            spots.push(p, self.light.0, self.light.1, 0.0);
        }
        spots
    }
    fn block_info(&self, pos: [i32; 3]) -> Option<BlockInfo> {
        Some(
            self.data
                .block_info(&self.block(pos)?, self.held.as_deref()),
        )
    }
}

/// The world for `scenario`: its `player` (`held` an item id in the first hotbar slot), its
/// `target` (`{"block": <state>, "pos": [x, y, z], "face": "north"}` or `{"entity": <kind>,
/// "health": 14, "max_health": 20, "armor": 2, "name": "Bob", "baby": false}`, in front of the
/// player by default), `mining` (the target block's breaking progress, 0 to 1), `blocks`
/// (`[{"pos": [x, y, z], "block": <state>}]`) and the `light` everywhere (`[block, sky]`).
pub fn world(scenario: &Value, data: Arc<Data>) -> Snapshot {
    let over = &scenario["player"];
    let mut world = player(over);
    let (px, py, pz) = world
        .player
        .as_ref()
        .map_or((0.5, 64.0, 0.5), |p| (p.x, p.y, p.z));
    let held = over["held"].as_str().map(str::to_owned);
    if let Some(id) = &held {
        let info = data.info(id);
        world.inventory = vec![None; 41];
        world.inventory[0] = Some(ItemStack {
            id: id.clone(),
            count: 1,
            damage: 0,
            max_damage: info.as_ref().map_or(0, |i| i.max_damage),
            name: info.map_or_else(|| short(id), |i| i.name),
            components: String::new(),
        });
    }
    // The open menu (`slots.get-menu`): the scenario's `menu`, `{"kind": "minecraft:generic_9x3",
    // "slots": {"6": "minecraft:elytra", "20": {"id": "minecraft:arrow", "count": 16}},
    // "size": 63}`; the player's inventory menu by default (46 slots, the held item in 36).
    let stack = |v: &Value| -> Option<ItemStack> {
        let id = v.as_str().or_else(|| v["id"].as_str())?.to_owned();
        let info = data.info(&id);
        Some(ItemStack {
            count: v["count"].as_u64().unwrap_or(1) as u32,
            damage: v["damage"].as_u64().unwrap_or(0) as u32,
            max_damage: info.as_ref().map_or(0, |i| i.max_damage),
            name: info.map_or_else(|| short(&id), |i| i.name),
            components: String::new(),
            id,
        })
    };
    // The player's own slots (`inventory.get-slot`): `"inventory": {"39": {"id":
    // "minecraft:diamond_helmet", "damage": 120}}`, 0 to 8 the hotbar, 36 to 39 the armour (feet
    // to head), 40 the offhand.
    if let Some(slots) = over["inventory"].as_object() {
        world.inventory.resize(world.inventory.len().max(41), None);
        for (i, v) in slots {
            if let (Ok(i @ 0..=40), Some(s)) = (i.parse::<usize>(), stack(v)) {
                world.inventory[i] = Some(s);
            }
        }
    }
    let menu = &scenario["menu"];
    let size = menu["size"].as_u64().unwrap_or(46) as usize;
    let mut slots = vec![None; size.min(128)];
    if menu.is_null()
        && let Some(held) = world.inventory.first().cloned().flatten()
        && slots.len() > 36
    {
        slots[36] = Some(held);
    }
    for (i, v) in menu["slots"].as_object().into_iter().flatten() {
        if let (Ok(i), Some(s)) = (i.parse::<usize>(), stack(v))
            && i < slots.len()
        {
            slots[i] = Some(s);
        }
    }
    world.menu = Some(horizon_mods::Menu {
        kind: menu["kind"]
            .as_str()
            .unwrap_or("minecraft:inventory")
            .to_owned(),
        slots,
        carried: stack(&menu["carried"]),
    });
    let pos = |v: &Value| -> Option<[i32; 3]> {
        let a = v.as_array()?;
        Some([
            a.first()?.as_i64()? as i32,
            a.get(1)?.as_i64()? as i32,
            a.get(2)?.as_i64()? as i32,
        ])
    };
    let mut blocks = HashMap::new();
    for b in scenario["blocks"].as_array().into_iter().flatten() {
        if let (Some(at), Some(block)) = (pos(&b["pos"]), b["block"].as_str()) {
            blocks.insert(at, block.to_owned());
        }
    }
    let target = &scenario["target"];
    if let Some(block) = target["block"].as_str() {
        // At eye height two blocks south, where a player facing south (yaw 0) looks.
        let at = pos(&target["pos"]).unwrap_or([
            px.floor() as i32,
            py.floor() as i32 + 1,
            pz.floor() as i32 + 2,
        ]);
        blocks.insert(at, block.to_owned());
        let face = match target["face"].as_str() {
            Some("down") => Direction::Down,
            Some("up") => Direction::Up,
            Some("south") => Direction::South,
            Some("west") => Direction::West,
            Some("east") => Direction::East,
            _ => Direction::North,
        };
        let block_pos = BlockPos {
            x: at[0],
            y: at[1],
            z: at[2],
        };
        world.target = Some(Target::Block(BlockHit {
            pos: block_pos,
            face,
            block: block.to_owned(),
        }));
        world.mining = scenario["mining"].as_f64().map(|progress| Mining {
            pos: block_pos,
            progress: progress as f32,
        });
    } else if let Some(kind) = target["entity"].as_str() {
        let (max_health, armor) = data.kinds.get(kind).copied().unwrap_or((20.0, 0.0));
        let max_health = target["max_health"]
            .as_f64()
            .map_or(max_health, |h| h as f32);
        let health = target["health"].as_f64().map_or(max_health, |h| h as f32);
        let name = target["name"].as_str().map(str::to_owned);
        let (x, y, z) = (px, py, pz + 2.0);
        world.entities = vec![Entity {
            id: 1,
            kind: kind.to_owned(),
            x,
            y,
            z,
            yaw: 180.0,
            health: Some(health),
            name: name.clone(),
        }];
        let kind_name = || {
            data.lang
                .get(&format!("entity.{}", kind.replace(':', ".")))
                .cloned()
                .unwrap_or_else(|| short(kind))
        };
        world.entity_info = vec![EntityInfo {
            id: 1,
            kind: kind.to_owned(),
            name: name.unwrap_or_else(kind_name),
            health: Some(health),
            max_health: Some(max_health),
            armor: target["armor"].as_f64().map_or(armor, |a| a as f32),
            baby: target["baby"].as_bool().unwrap_or(false),
        }];
        world.target = Some(Target::Entity(EntityHit {
            id: 1,
            kind: kind.to_owned(),
            x,
            y,
            z,
        }));
    }
    let light = |i: usize, or: u64| scenario["light"][i].as_u64().unwrap_or(or).min(15) as u8;
    world.level = Some(Arc::new(Level {
        blocks,
        data: data.clone(),
        held,
        light: (light(0, 0), light(1, 15)),
    }));
    world.items = Some(data);
    world
}

/// A player standing in the overworld at noon, with `over`'s fields over the defaults.
fn player(over: &Value) -> Snapshot {
    let f = |name: &str, default: f64| over[name].as_f64().unwrap_or(default);
    let u = |name: &str, default: u64| over[name].as_u64().unwrap_or(default);
    Snapshot {
        player: Some(Player {
            x: f("x", 0.5),
            y: f("y", 64.0),
            z: f("z", 0.5),
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            yaw: f("yaw", 0.0) as f32,
            pitch: f("pitch", 0.0) as f32,
            health: f("health", 20.0) as f32,
            max_health: 20.0,
            absorption: 0.0,
            food: u("food", 20) as u32,
            saturation: f("saturation", 5.0) as f32,
            exhaustion: 0.0,
            air: 300,
            max_air: 300,
            armor: u("armor", 0) as u32,
            experience_level: u("level", 0) as u32,
            experience_progress: 0.0,
            selected_slot: 0,
            game_mode: match over["game_mode"].as_str() {
                Some("creative") => GameMode::Creative,
                Some("adventure") => GameMode::Adventure,
                Some("spectator") => GameMode::Spectator,
                _ => GameMode::Survival,
            },
            on_ground: true,
            sneaking: false,
            sprinting: false,
            swimming: false,
            flying: false,
            gliding: false,
            dimension: over["dimension"]
                .as_str()
                .unwrap_or("minecraft:overworld")
                .into(),
            name: over["name"].as_str().unwrap_or("Player").into(),
            uuid: "00000000-0000-0000-0000-000000000000".into(),
        }),
        clock: Some(Clock {
            game_time: 0,
            day_time: u("day_time", 6000),
        }),
        touch: over["touch"].as_bool().unwrap_or(false),
        ..Snapshot::default()
    }
}
