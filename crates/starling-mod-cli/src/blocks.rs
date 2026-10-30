//! `starling-mod blocks <content mod> --resources <dir> --out <dir>`: a content mod's blocks drawn
//! on contact sheets without the game, and what it would draw wrong said as JSON lines.
//!
//! Each block is a tile: its blockstate's first variant (a crank's pointing up, a shaft's along
//! x), its models baked as the game bakes them (horizon_assets: parents, composite and OBJ
//! models, the variant's turns), the faces that look at the viewer shaded as the terrain shades
//! them. A kinetic part gets a second tile an eighth of a turn on, with its extra model and its
//! shaft ends turned as the game turns them, so a model turning about the wrong place shows.
//!
//! Lines: `{"event": "issue", "block", "kind", "detail"}` for a model the game would not find
//! (`missing-model`), a texture slot no parent sets (`unresolved-slot`), a texture file that is
//! not there (`missing-texture`), an OBJ mesh that does not read (`mesh`), a block with no face
//! to draw (`nothing-drawn`: a model a loader of the mod's own code builds), and two faces of one
//! tile in one place facing the same way with different textures (`fighting-faces`: they
//! flicker); then `{"event": "sheets", "pages", "blocks", "issues"}`. The models and textures
//! come from the mod and the prepared resources (`--resources`), as the game reads them; nothing
//! is bundled. What the world adds (light, the blocks around) is not drawn: that needs the game.
use crate::emit;
use crate::render::{Image, Resources, label};
use horizon_assets::{
    bake::{BlockModelTransform, ModelQuad, bake_block_model},
    model::CuboidModel,
    resolve::ModelLibrary,
};
use horizon_mods::content::{Kinetic, KineticPart, direction, turns_from, turns_to};
use serde_json::{Value, json};
use std::f32::consts::FRAC_1_SQRT_2;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    io::Read,
    path::{Path, PathBuf},
};

const USAGE: &str = "usage: starling-mod blocks <content mod .hmod or folder> --resources <dir> \
    --out <dir> [--only <text>] [--tile <pixels>]";
/// Tiles across and down a page.
const COLUMNS: u32 = 8;
const ROWS: u32 = 6;
/// A kinetic part's second tile, this far turned (degrees).
const TURNED: f32 = 45.0;

pub fn blocks(args: &[String]) -> Result<(), String> {
    let mut path = None;
    let (mut resources, mut out, mut only, mut tile) = (None, None, None, 160u32);
    let mut i = 0;
    while i < args.len() {
        let value = || args.get(i + 1).cloned().ok_or_else(|| USAGE.to_owned());
        match args[i].as_str() {
            "--resources" => resources = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--only" => only = Some(value()?),
            "--tile" => tile = value()?.parse().map_err(|_| USAGE.to_owned())?,
            flag if flag.starts_with("--") => return Err(USAGE.into()),
            other => {
                path = Some(PathBuf::from(other));
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    let (Some(path), Some(resources), Some(out)) = (path, resources, out) else {
        return Err(USAGE.into());
    };
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let files = Files::open(&path, &resources)?;
    let mut res = Resources::new(Some(resources)).with_mod(&path);
    let mut issues = Issues::default();

    // The mod's blocks, from their declarations, and the state each tile shows.
    let mut blocks = Vec::new();
    for name in files.own() {
        let Some(rest) = name.strip_prefix("data/") else {
            continue;
        };
        let Some((ns, block)) = rest.split_once("/horizon/block/") else {
            continue;
        };
        let Some(block) = block.strip_suffix(".json") else {
            continue;
        };
        let id = format!("{ns}:{block}");
        if only.as_ref().is_some_and(|o| !id.contains(o.as_str())) {
            continue;
        }
        let decl: Value = files
            .read(&name)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let kinetic = serde_json::from_value::<Kinetic>(decl["kinetic"].clone()).ok();
        let (props, variants) = files
            .read(&format!("assets/{ns}/blockstates/{block}.json"))
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .map(|s| pick_state(&s))
            .unwrap_or_default();
        if variants.is_empty() {
            issues.add(&id, "missing-model", "no blockstate variant");
        }
        let spin = kinetic.as_ref().and_then(|k| Spin::new(k, &props));
        blocks.push(Block {
            id,
            animated: decl["properties"]["render_shape"] == "ENTITYBLOCK_ANIMATED",
            kinetic,
            variants,
            spin,
        });
    }

    // Every model the tiles need, with their parents; then one library resolves them all.
    let mut wanted: Vec<String> = Vec::new();
    for b in &blocks {
        wanted.extend(b.variants.iter().map(|v| v.model.clone()));
        if let Some(k) = &b.kinetic {
            wanted.extend(k.half.iter().chain(&k.extra).cloned());
        }
    }
    let models = load_models(&files, wanted, &mut issues);
    let library = ModelLibrary::new(models).map_err(|e| e.to_string())?;

    let mut tiles: Vec<(String, Vec<Quad>)> = Vec::new();
    for b in &blocks {
        let angles: &[f32] = if b.spin.is_some() {
            &[0.0, TURNED]
        } else {
            &[0.0]
        };
        for &angle in angles {
            let quads = assemble(&library, b, angle, &mut issues);
            if angle == 0.0 {
                if quads.is_empty() {
                    issues.add(&b.id, "nothing-drawn", "no face of its models shows");
                }
                check(&b.id, &quads, &mut res, &mut issues);
            }
            let name = b.id.split_once(':').map_or(b.id.as_str(), |(_, p)| p);
            let name = if angle == 0.0 {
                name.to_owned()
            } else {
                format!("{name} (turned)")
            };
            tiles.push((name, quads));
        }
    }

    let per_page = (COLUMNS * ROWS) as usize;
    let mut pages = Vec::new();
    for (n, chunk) in tiles.chunks(per_page).enumerate() {
        let mut sheet = Image::new(COLUMNS * tile, ROWS * tile, [0x20, 0x21, 0x24, 0xFF]);
        for (k, (name, quads)) in chunk.iter().enumerate() {
            let (cx, cy) = ((k as u32 % COLUMNS) * tile, (k as u32 / COLUMNS) * tile);
            draw_tile(&mut sheet, cx, cy, tile, quads, &mut res);
            let text: String = name.chars().take((tile as usize - 4) / 6).collect();
            sheet = label(
                sheet,
                &mut res,
                cx as f32 + 3.0,
                (cy + tile) as f32 - 11.0,
                &text,
            );
        }
        let file = out.join(format!("blocks-{:02}.png", n + 1));
        sheet.save(&file)?;
        pages.push(file.display().to_string());
    }
    emit(json!({
        "event": "sheets",
        "pages": pages,
        "blocks": blocks.len(),
        "issues": issues.count,
    }));
    Ok(())
}

/// The mod's files (a `.hmod` or a folder), then the prepared resources'.
struct Files {
    zip: Option<RefCell<zip::ZipArchive<std::fs::File>>>,
    dir: Option<PathBuf>,
    resources: PathBuf,
}

impl Files {
    fn open(path: &Path, resources: &Path) -> Result<Self, String> {
        let (zip, dir) = if path.is_dir() {
            (None, Some(path.to_owned()))
        } else {
            let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            (Some(RefCell::new(zip)), None)
        };
        Ok(Files {
            zip,
            dir,
            resources: resources.to_owned(),
        })
    }
    /// The mod's own file names (`/`-separated).
    fn own(&self) -> Vec<String> {
        if let Some(zip) = &self.zip {
            return zip.borrow().file_names().map(str::to_owned).collect();
        }
        let mut out = Vec::new();
        let base = self.dir.clone().unwrap_or_default();
        let mut stack = vec![base.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(&base) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        out
    }
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let own = match (&self.zip, &self.dir) {
            (Some(zip), _) => {
                let mut zip = zip.borrow_mut();
                let mut bytes = Vec::new();
                zip.by_name(name)
                    .ok()
                    .and_then(|mut f| f.read_to_end(&mut bytes).ok())
                    .map(|_| bytes)
            }
            (_, Some(dir)) => std::fs::read(dir.join(name)).ok(),
            _ => None,
        };
        own.or_else(|| std::fs::read(self.resources.join(name)).ok())
    }
}

/// A block's tile: the state it shows and how it turns.
struct Block {
    id: String,
    /// Its render shape is `ENTITYBLOCK_ANIMATED`: the game, not the terrain, draws it.
    animated: bool,
    kinetic: Option<Kinetic>,
    variants: Vec<Variant>,
    spin: Option<Spin>,
}

/// One model of a variant, with its quarter turns.
#[derive(Clone, Debug)]
struct Variant {
    model: String,
    turns: [u8; 3],
    uv_lock: bool,
}

/// The state a tile shows: its properties and its models. `variants`: the first key (a crank's
/// pointing up, a block's facing north when it faces), its first weighted model; `multipart`:
/// the parts of the state with every property it names false (a pane or a wall alone), else
/// the first part.
fn pick_state(blockstate: &Value) -> (BTreeMap<String, String>, Vec<Variant>) {
    let variant = |v: &Value| {
        let v = v.as_array().and_then(|a| a.first()).unwrap_or(v);
        let quarter = |key: &str| (v[key].as_i64().unwrap_or(0).rem_euclid(360) / 90) as u8;
        Some(Variant {
            model: identifier(v["model"].as_str()?),
            turns: [quarter("x"), quarter("y"), 0],
            uv_lock: v["uvlock"].as_bool().unwrap_or(false),
        })
    };
    if let Some(variants) = blockstate["variants"].as_object() {
        let mut keys: Vec<&String> = variants.keys().collect();
        keys.sort_by_key(|k| {
            (
                !k.contains("facing=up"),
                !k.contains("facing=north"),
                (*k).clone(),
            )
        });
        let Some(key) = keys.first() else {
            return Default::default();
        };
        let props = key
            .split(',')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        return (props, variant(&variants[*key]).into_iter().collect());
    }
    let parts = blockstate["multipart"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let alone: Vec<Variant> = parts
        .iter()
        .filter(|p| p.get("when").is_none_or(alone_applies))
        .filter_map(|p| variant(&p["apply"]))
        .collect();
    let models = if alone.is_empty() {
        parts
            .first()
            .and_then(|p| variant(&p["apply"]))
            .into_iter()
            .collect()
    } else {
        alone
    };
    (BTreeMap::new(), models)
}

/// Whether a multipart `when` holds on the state with every property false: `{"north":
/// "false|low"}`, `OR` and `AND` lists of those.
fn alone_applies(when: &Value) -> bool {
    if let Some(any) = when["OR"].as_array() {
        return any.iter().any(alone_applies);
    }
    if let Some(all) = when["AND"].as_array() {
        return all.iter().all(alone_applies);
    }
    when.as_object().is_some_and(|conditions| {
        conditions.values().all(|v| {
            let v = match v {
                Value::Bool(b) => b.to_string(),
                v => v.as_str().unwrap_or_default().to_owned(),
            };
            v.split('|').any(|alt| alt == "false")
        })
    })
}

fn identifier(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

/// `wanted` and their parents, each read and decoded as the game does (OBJ meshes read beside
/// them); one that is not there is the resolver's missing model, said once.
fn load_models(
    files: &Files,
    wanted: Vec<String>,
    issues: &mut Issues,
) -> BTreeMap<String, CuboidModel> {
    let mut models = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut pending = wanted;
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let (ns, path) = id.split_once(':').unwrap_or(("minecraft", &id));
        let text = files
            .read(&format!("assets/{ns}/models/{path}.json"))
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        let Some(text) = text else {
            issues.add(&id, "missing-model", &id);
            continue;
        };
        let mut model = match CuboidModel::from_json(&text) {
            Ok(model) => model,
            Err(e) => {
                issues.add(&id, "missing-model", &format!("{id}: {e}"));
                continue;
            }
        };
        if let Some(source) = &model.obj {
            let read = |p: &str| {
                files
                    .read(p)
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
            };
            match horizon_assets::obj::read_mesh(source, read) {
                Ok(faces) => model.mesh = Some(std::sync::Arc::new(faces)),
                Err(e) => issues.add(&id, "mesh", &format!("{}: {e}", source.model)),
            }
        }
        if let Some(parent) = &model.parent {
            pending.push(parent.clone());
        }
        models.insert(id, model);
    }
    models
}

/// How a kinetic part turns on the tile's state: about which axis, its ends, its extra's turns.
struct Spin {
    axis: usize,
    ends: Vec<[i32; 3]>,
    extra_turns: Option<[u8; 3]>,
}

impl Spin {
    fn new(k: &Kinetic, props: &BTreeMap<String, String>) -> Option<Self> {
        let facing = props.get("facing").and_then(|f| direction(f));
        let axis_of = |v: [i32; 3]| v.iter().position(|c| *c != 0).unwrap_or(1);
        let unit = |axis: usize, sign: i32| {
            let mut v = [0; 3];
            v[axis] = sign;
            v
        };
        let by_name = props.get("axis").and_then(|a| match a.as_str() {
            "x" => Some(0),
            "y" => Some(1),
            "z" => Some(2),
            _ => None,
        });
        let (axis, ends) = match k.part {
            KineticPart::Motor => (axis_of(facing?), vec![facing?]),
            KineticPart::Crank | KineticPart::Drill => {
                let back = facing?.map(|v| -v);
                (axis_of(back), vec![back])
            }
            KineticPart::Gearbox => {
                let axis = by_name?;
                let ends = (0..3)
                    .filter(|a| *a != axis)
                    .flat_map(|a| [unit(a, 1), unit(a, -1)])
                    .collect();
                (axis, ends)
            }
            _ => {
                let axis = by_name.or_else(|| facing.map(axis_of))?;
                (axis, vec![unit(axis, 1), unit(axis, -1)])
            }
        };
        let extra_turns = match (k.extra_facing.as_deref(), facing) {
            (Some(made), Some(f)) => Some(turns_from(direction(made)?, f)),
            _ => None,
        };
        Some(Spin {
            axis,
            ends,
            extra_turns,
        })
    }
}

/// A face to draw: corners in block units, unit UVs, its texture.
#[derive(Clone, Debug)]
struct Quad {
    positions: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    sprite: String,
}

/// The tile's faces, as the game draws the block: the variant's models (turned `angle` about the
/// part's axis when the part turns whole; none when the game draws it and it does not), its
/// extra model and its shaft ends as the game turns them.
fn assemble(library: &ModelLibrary, block: &Block, angle: f32, issues: &mut Issues) -> Vec<Quad> {
    let (id, variants) = (block.id.as_str(), &block.variants);
    let (kinetic, spin) = (block.kinetic.as_ref(), block.spin.as_ref());
    let mut out = Vec::new();
    let mut bake = |model: &str,
                    turns: [u8; 3],
                    uv_lock: bool,
                    axis: usize,
                    angle: f32,
                    issues: &mut Issues| {
        let quads: Vec<ModelQuad> = match library.resolve(model).and_then(|m| {
            if !m.unresolved_textures.is_empty() {
                let slots: Vec<&str> = m.unresolved_textures.iter().map(String::as_str).collect();
                issues.add(
                    id,
                    "unresolved-slot",
                    &format!("{model}: #{}", slots.join(" #")),
                );
            }
            bake_block_model(&m, BlockModelTransform::new(turns, uv_lock))
        }) {
            Ok(quads) => quads,
            Err(e) => {
                issues.add(id, "missing-model", &format!("{model}: {e}"));
                return;
            }
        };
        for q in quads {
            out.push(Quad {
                positions: q.positions.map(|p| turned(p, axis, angle)),
                uv: q.uv,
                sprite: q.material.sprite,
            });
        }
    };
    let spins = kinetic.is_some_and(|k| k.spins);
    let axis = spin.map_or(1, |s| s.axis);
    for v in variants.iter().filter(|_| spins || !block.animated) {
        let a = if spins { angle } else { 0.0 };
        bake(&v.model, v.turns, v.uv_lock, axis, a, issues);
    }
    if let (Some(k), Some(s)) = (kinetic, spin) {
        if let Some(extra) = &k.extra {
            let turns = s
                .extra_turns
                .or_else(|| variants.first().map(|v| v.turns))
                .unwrap_or([0; 3]);
            bake(extra, [turns[0], turns[1], 0], false, s.axis, angle, issues);
        }
        if let Some(half) = &k.half {
            for &face in &s.ends {
                let end_axis = face.iter().position(|c| *c != 0).unwrap_or(1);
                bake(half, turns_to(face), false, end_axis, angle, issues);
            }
        }
    }
    out
}

/// `p` turned `degrees` about `axis` through the block's middle.
fn turned(p: [f32; 3], axis: usize, degrees: f32) -> [f32; 3] {
    if degrees == 0.0 {
        return p;
    }
    let (s, c) = degrees.to_radians().sin_cos();
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let (a, b) = (p[u] - 0.5, p[v] - 0.5);
    let mut out = p;
    out[u] = a * c - b * s + 0.5;
    out[v] = a * s + b * c + 0.5;
    out
}

/// The issues said so far, each once per block.
#[derive(Default)]
struct Issues {
    said: BTreeSet<(String, String, String)>,
    count: usize,
}

impl Issues {
    fn add(&mut self, block: &str, kind: &str, detail: &str) {
        if self
            .said
            .insert((block.to_owned(), kind.to_owned(), detail.to_owned()))
        {
            self.count += 1;
            emit(json!({"event": "issue", "block": block, "kind": kind, "detail": detail}));
        }
    }
}

/// The textures a tile's faces name that are not there, and its faces that fight: in one plane,
/// facing the same way, overlapping, with different pictures.
fn check(id: &str, quads: &[Quad], res: &mut Resources, issues: &mut Issues) {
    for q in quads {
        if q.sprite == "minecraft:missingno" {
            continue;
        }
        if res.texture(&q.sprite).is_none() {
            issues.add(id, "missing-texture", &q.sprite);
        }
    }
    let mut planes: HashMap<([i64; 3], i64), Vec<usize>> = HashMap::new();
    for (i, q) in quads.iter().enumerate() {
        let Some(n) = normal(&q.positions) else {
            continue;
        };
        let d = dot(n, q.positions[0]);
        let key = (
            n.map(|c| (c * 1000.0).round() as i64),
            (d * 1000.0).round() as i64,
        );
        planes.entry(key).or_default().push(i);
    }
    let mut fights = 0;
    for faces in planes.values() {
        for (k, &a) in faces.iter().enumerate() {
            for &b in &faces[k + 1..] {
                // The same picture over itself is the mod's own overlap, as it looks anywhere.
                let (qa, qb) = (&quads[a], &quads[b]);
                if qa.sprite == qb.sprite || !overlap(qa, qb) {
                    continue;
                }
                fights += 1;
                if fights <= 3 {
                    issues.add(
                        id,
                        "fighting-faces",
                        &format!("{} / {} at {:?}", qa.sprite, qb.sprite, qa.positions[0]),
                    );
                }
            }
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
/// A face's unit normal, from its first three corners (as the game's winding).
fn normal(p: &[[f32; 3]; 4]) -> Option<[f32; 3]> {
    let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
    let len = dot(n, n).sqrt();
    (len > 1e-6).then(|| n.map(|c| c / len))
}

/// Two faces of one plane overlapping by more than a sliver (no edge of either separates them).
fn overlap(a: &Quad, b: &Quad) -> bool {
    let Some(n) = normal(&a.positions) else {
        return false;
    };
    // In the plane: the two axes but the normal's largest.
    let drop = (0..3)
        .max_by(|i, j| n[*i].abs().total_cmp(&n[*j].abs()))
        .unwrap_or(0);
    let (u, v) = ((drop + 1) % 3, (drop + 2) % 3);
    let flat = |q: &Quad| q.positions.map(|p| [p[u], p[v]]);
    let (pa, pb) = (flat(a), flat(b));
    for poly in [&pa, &pb] {
        for i in 0..4 {
            let (p, q) = (poly[i], poly[(i + 1) % 4]);
            let axis = [q[1] - p[1], p[0] - q[0]];
            if axis[0].abs() + axis[1].abs() < 1e-6 {
                continue;
            }
            let span = |pts: &[[f32; 2]; 4]| {
                pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                    let d = p[0] * axis[0] + p[1] * axis[1];
                    (lo.min(d), hi.max(d))
                })
            };
            let ((a0, a1), (b0, b1)) = (span(&pa), span(&pb));
            let len = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt();
            if a1.min(b1) - a0.max(b0) <= 1e-3 * len {
                return false;
            }
        }
    }
    true
}

/// The view: from the south-east above, the way a block shows in an inventory.
const RIGHT: [f32; 3] = [FRAC_1_SQRT_2, 0.0, -FRAC_1_SQRT_2];
const UP: [f32; 3] = [-0.408_248_3, 0.816_496_6, -0.408_248_3];
const TOWARD: [f32; 3] = [0.577_350_3, 0.577_350_3, 0.577_350_3];

/// A tile's faces drawn into `sheet` at `x`, `y`: those looking at the viewer, nearest first per
/// pixel, shaded by where they point as the terrain shades them, scaled to fit the tile (a block
/// at most `tile` / 2.6 pixels wide) above its label.
fn draw_tile(sheet: &mut Image, x: u32, y: u32, tile: u32, quads: &[Quad], res: &mut Resources) {
    let project = |p: [f32; 3]| {
        let p = sub(p, [0.5; 3]);
        (dot(p, RIGHT), -dot(p, UP), dot(p, TOWARD))
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for q in quads {
        for p in q.positions {
            let (sx, sy, _) = project(p);
            lo = [lo[0].min(sx), lo[1].min(sy)];
            hi = [hi[0].max(sx), hi[1].max(sy)];
        }
    }
    if quads.is_empty() {
        return;
    }
    let room = (tile - 8) as f32;
    let tall = (tile - 20) as f32;
    let scale = (tile as f32 / 2.6)
        .min(room / (hi[0] - lo[0]).max(1e-3))
        .min(tall / (hi[1] - lo[1]).max(1e-3));
    let (cx, cy) = (
        x as f32 + tile as f32 / 2.0 - (lo[0] + hi[0]) / 2.0 * scale,
        y as f32 + (tile as f32 - 12.0) / 2.0 - (lo[1] + hi[1]) / 2.0 * scale,
    );
    let mut depth = vec![f32::MIN; (tile * tile) as usize];
    for q in quads {
        let Some(n) = normal(&q.positions) else {
            continue;
        };
        if dot(n, TOWARD) <= 0.0 {
            continue;
        }
        let shade = n[0] * n[0] * 0.6
            + n[1] * n[1] * if n[1] > 0.0 { 1.0 } else { 0.5 }
            + n[2] * n[2] * 0.8;
        let texture = res.texture(&q.sprite);
        let pts = q.positions.map(|p| {
            let (sx, sy, d) = project(p);
            (cx + sx * scale, cy + sy * scale, d)
        });
        for tri in [[0, 1, 2], [0, 2, 3]] {
            let [a, b, c] = tri.map(|i| pts[i]);
            let [ua, ub, uc] = tri.map(|i| q.uv[i]);
            let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if area.abs() < 1e-6 {
                continue;
            }
            let x0 = a.0.min(b.0).min(c.0).floor().max(x as f32) as u32;
            let x1 = a.0.max(b.0).max(c.0).ceil().min((x + tile) as f32) as u32;
            let y0 = a.1.min(b.1).min(c.1).floor().max(y as f32) as u32;
            let y1 = a.1.max(b.1).max(c.1).ceil().min((y + tile - 12) as f32) as u32;
            for py in y0..y1 {
                for px in x0..x1 {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((b.0 - fx) * (c.1 - fy) - (b.1 - fy) * (c.0 - fx)) / area;
                    let w1 = ((c.0 - fx) * (a.1 - fy) - (c.1 - fy) * (a.0 - fx)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = w0 * a.2 + w1 * b.2 + w2 * c.2;
                    let slot = ((py - y) * tile + (px - x)) as usize;
                    if d <= depth[slot] {
                        continue;
                    }
                    let u = w0 * ua[0] + w1 * ub[0] + w2 * uc[0];
                    let v = w0 * ua[1] + w1 * ub[1] + w2 * uc[1];
                    let Some(color) = sample(texture.as_deref().map(|t| &t.0), u, v) else {
                        continue;
                    };
                    depth[slot] = d;
                    let lit = color.map(|c| (f32::from(c) * shade) as u8);
                    sheet.blend(i64::from(px), i64::from(py), [lit[0], lit[1], lit[2], 255]);
                }
            }
        }
    }
}

/// A texture's colour at unit `u`, `v` (an animated one's first frame, repeating past its
/// edges); none where it is cut out. A missing texture is the game's magenta and black.
fn sample(texture: Option<&Image>, u: f32, v: f32) -> Option<[u8; 3]> {
    let Some(t) = texture else {
        let checker = ((u * 2.0).floor() as i32 + (v * 2.0).floor() as i32).rem_euclid(2) == 0;
        return Some(if checker {
            [0xF8, 0x00, 0xF8]
        } else {
            [0, 0, 0]
        });
    };
    let frame = t.width.min(t.height);
    let tx = ((u.rem_euclid(1.0) * t.width as f32) as u32).min(t.width - 1);
    let ty = ((v.rem_euclid(1.0) * frame as f32) as u32).min(frame - 1);
    let c = t.get(tx, ty);
    (c[3] >= 128).then_some([c[0], c[1], c[2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(positions: [[f32; 3]; 4], sprite: &str) -> Quad {
        Quad {
            positions,
            uv: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            sprite: sprite.into(),
        }
    }

    #[test]
    fn faces_in_one_plane_fight_only_where_they_overlap() {
        let up = |x0: f32, x1: f32, y: f32, sprite: &str| {
            quad(
                [[x0, y, 0.0], [x0, y, 1.0], [x1, y, 1.0], [x1, y, 0.0]],
                sprite,
            )
        };
        assert!(overlap(&up(0.0, 1.0, 1.0, "a"), &up(0.5, 1.5, 1.0, "b")));
        // Side by side, touching along an edge: no.
        assert!(!overlap(&up(0.0, 0.5, 1.0, "a"), &up(0.5, 1.0, 1.0, "b")));
        let mut issues = Issues::default();
        let mut res = Resources::new(None);
        check(
            "x:fight",
            &[up(0.0, 1.0, 1.0, "a"), up(0.2, 0.8, 1.0, "b")],
            &mut res,
            &mut issues,
        );
        assert!(
            issues
                .said
                .iter()
                .any(|(_, kind, _)| kind == "fighting-faces")
        );
        // The same picture twice, or another height, does not fight.
        let mut calm = Issues::default();
        check(
            "x:calm",
            &[
                up(0.0, 1.0, 1.0, "a"),
                up(0.0, 1.0, 1.0, "a"),
                up(0.0, 1.0, 0.5, "b"),
            ],
            &mut res,
            &mut calm,
        );
        assert!(
            !calm
                .said
                .iter()
                .any(|(_, kind, _)| kind == "fighting-faces")
        );
    }

    #[test]
    fn a_crank_turns_about_its_back_and_its_handle_points_where_it_faces() {
        let crank = Kinetic {
            part: KineticPart::Crank,
            rpm: 32.0,
            spins: true,
            half: None,
            extra: Some("create:block/hand_crank/handle".into()),
            extra_facing: Some("north".into()),
        };
        let props = BTreeMap::from([("facing".to_owned(), "up".to_owned())]);
        let spin = Spin::new(&crank, &props).unwrap();
        assert_eq!((spin.axis, spin.ends.clone()), (1, vec![[0, -1, 0]]));
        assert_eq!(spin.extra_turns, Some([3, 0, 0]));
        // A quarter turn about y takes the east edge's middle north, as the game turns it.
        let p = turned([1.0, 0.5, 0.5], 1, 90.0);
        assert!((p[0] - 0.5).abs() < 1e-5 && p[2].abs() < 1e-5);
    }

    #[test]
    fn the_first_state_is_the_crank_pointing_up() {
        let state: Value = serde_json::from_str(
            r#"{"variants":{"facing=down":{"model":"create:block/c","x":180},
                "facing=up":{"model":"create:block/c"},"facing=north":{"model":"block/c","x":90}}}"#,
        )
        .unwrap();
        let (props, variants) = pick_state(&state);
        assert_eq!(props["facing"], "up");
        assert_eq!(variants[0].turns, [0, 0, 0]);
        let north: Value = serde_json::from_str(
            r#"{"variants":{"facing=north":[{"model":"block/c","x":90,"y":270}]}}"#,
        )
        .unwrap();
        let (_, v) = pick_state(&north);
        assert_eq!(
            (v[0].model.as_str(), v[0].turns),
            ("minecraft:block/c", [1, 3, 0])
        );
        // A pane alone: its post and the sides it has not.
        let pane: Value = serde_json::from_str(
            r#"{"multipart":[{"apply":{"model":"block/post"}},
                {"when":{"north":"true"},"apply":{"model":"block/side"}},
                {"when":{"north":"false"},"apply":{"model":"block/noside"}},
                {"when":{"OR":[{"east":"low|tall"},{"west":"false"}]},"apply":{"model":"block/w"}}]}"#,
        )
        .unwrap();
        let (_, v) = pick_state(&pane);
        let models: Vec<&str> = v.iter().map(|v| v.model.as_str()).collect();
        assert_eq!(
            models,
            [
                "minecraft:block/post",
                "minecraft:block/noside",
                "minecraft:block/w"
            ]
        );
    }
}
