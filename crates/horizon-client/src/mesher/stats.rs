//! An estimate of what greedy meshing would take off a section mesh: how many of its solid
//! quads are whole block faces (a unit square on an axis plane) lit and tinted the same at
//! their four corners, and how many quads those would be once coplanar neighbours with the
//! same sprite, tint, light and facing merge into rectangles. Logged while
//! `HORIZON_MESH_STATS` is set (gfx.rs `Renderer::upload`), to size the idea before building it:
//! a merged face needs its sprite repeated, which the section vertex's UV (0 to 2) cannot say.
use super::TerrainVertex;
use crate::fast_hash::FxHashMap;
use std::sync::Mutex;

/// Running totals over every mesh uploaded since the game started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub sections: u64,
    pub quads: u64,
    /// Whole, flat-lit block faces: the quads greedy meshing could merge.
    pub mergeable: u64,
    /// The quads left once the mergeable ones merge into rectangles.
    pub after: u64,
}

static TOTALS: Mutex<Totals> = Mutex::new(Totals {
    sections: 0,
    quads: 0,
    mergeable: 0,
    after: 0,
});
const LOG_EVERY: u64 = 500;

/// Whether `HORIZON_MESH_STATS` asks for the estimate.
pub fn wanted() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("HORIZON_MESH_STATS").is_some())
}

/// Adds a mesh's solid quads to the totals, and logs them every `LOG_EVERY` sections.
pub fn record(solid: &[TerrainVertex]) {
    let (quads, mergeable, after) = estimate(solid);
    let mut t = TOTALS.lock().unwrap_or_else(|e| e.into_inner());
    t.sections += 1;
    t.quads += quads;
    t.mergeable += mergeable;
    t.after += after;
    if t.sections.is_multiple_of(LOG_EVERY) {
        log::info!(
            "mesh stats: {} sections, {} solid quads, {} mergeable ({:.0}%), {} after merging ({:.0}% fewer)",
            t.sections,
            t.quads,
            t.mergeable,
            100.0 * t.mergeable as f64 / t.quads.max(1) as f64,
            t.after,
            100.0 * (t.quads - t.after) as f64 / t.quads.max(1) as f64
        );
    }
}

/// `(quads, mergeable, after)` for one mesh's solid vertices (four per quad).
pub fn estimate(solid: &[TerrainVertex]) -> (u64, u64, u64) {
    // Per plane and material: the cells (u, v) its mergeable faces cover.
    let mut planes: FxHashMap<Key, Vec<(i32, i32)>> = FxHashMap::default();
    let quads = (solid.len() / 4) as u64;
    let mut mergeable = 0u64;
    for quad in solid.chunks_exact(4) {
        let Some((key, cell)) = unit_face(quad) else {
            continue;
        };
        mergeable += 1;
        planes.entry(key).or_default().push(cell);
    }
    let merged: u64 = planes.values().map(|cells| rectangles(cells)).sum();
    (quads, mergeable, quads - mergeable + merged)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    /// The axis the face is perpendicular to, and the plane's coordinate in 1/16 blocks.
    axis: u8,
    plane: i32,
    /// Which way it faces (its winding seen from +axis).
    front: bool,
    layer: u16,
    color: u32,
    light: u16,
}

/// A whole block face lit and tinted the same at its corners: its key and its cell.
fn unit_face(q: &[TerrainVertex]) -> Option<(Key, (i32, i32))> {
    let v0 = &q[0];
    if q.iter()
        .any(|v| v.color != v0.color || v.light != v0.light || v.layer != v0.layer)
    {
        return None;
    }
    let p: Vec<[f32; 3]> = q.iter().map(|v| v.pos).collect();
    let axis = (0..3).find(|&a| p.iter().all(|v| v[a] == p[0][a]))?;
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let min = |a: usize| p.iter().map(|v| v[a]).fold(f32::INFINITY, f32::min);
    let max = |a: usize| p.iter().map(|v| v[a]).fold(f32::NEG_INFINITY, f32::max);
    let (u0, u1, v0f, v1) = (min(ua), max(ua), min(va), max(va));
    if u1 - u0 != 1.0 || v1 - v0f != 1.0 || u0.fract() != 0.0 || v0f.fract() != 0.0 {
        return None;
    }
    // Every corner on a corner of the cell (a rotated or sloped quad is not a block face).
    if !p
        .iter()
        .all(|v| (v[ua] == u0 || v[ua] == u1) && (v[va] == v0f || v[va] == v1))
    {
        return None;
    }
    let plane = p[0][axis] * 16.0;
    if plane.fract() != 0.0 {
        return None;
    }
    // The winding: the normal's sign along the axis from the first three corners.
    let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    Some((
        Key {
            axis: axis as u8,
            plane: plane as i32,
            front: n[axis] > 0.0,
            layer: v0.layer,
            color: v0.color,
            light: v0.light,
        },
        (u0 as i32, v0f as i32),
    ))
}

/// How many rectangles greedy meshing makes of these unit cells: rows first, then equal rows.
fn rectangles(cells: &[(i32, i32)]) -> u64 {
    let mut set: std::collections::BTreeSet<(i32, i32)> = cells.iter().copied().collect();
    let mut count = 0;
    while let Some(&(u, v)) = set.iter().next() {
        let mut width = 1;
        while set.contains(&(u + width, v)) {
            width += 1;
        }
        let mut height = 1;
        while (0..width).all(|du| set.contains(&(u + du, v + height))) {
            height += 1;
        }
        for du in 0..width {
            for dv in 0..height {
                set.remove(&(u + du, v + dv));
            }
        }
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x: f32, z: f32, y: f32, light: u16) -> [TerrainVertex; 4] {
        // A top face (+Y) at height y over the cell (x, z), counter-clockwise seen from above.
        [
            TerrainVertex::new([x, y, z], [0.0, 0.0], 7, 0xff00ff00, light),
            TerrainVertex::new([x, y, z + 1.0], [0.0, 1.0], 7, 0xff00ff00, light),
            TerrainVertex::new([x + 1.0, y, z + 1.0], [1.0, 1.0], 7, 0xff00ff00, light),
            TerrainVertex::new([x + 1.0, y, z], [1.0, 0.0], 7, 0xff00ff00, light),
        ]
    }

    #[test]
    fn a_flat_floor_merges_into_one_quad_and_a_lit_edge_stays_apart() {
        let mut solid = Vec::new();
        for x in 0..4 {
            for z in 0..4 {
                solid.extend(face(x as f32, z as f32, 5.0, 0xf0));
            }
        }
        // A face lit differently at one corner: not mergeable.
        let mut odd = face(8.0, 8.0, 5.0, 0xf0);
        odd[2].light = 0x80;
        solid.extend(odd);
        // A face on another plane: its own rectangle.
        solid.extend(face(0.0, 0.0, 9.0, 0xf0));
        assert_eq!(estimate(&solid), (18, 17, 3));
    }

    #[test]
    fn rectangles_take_rows_then_equal_rows() {
        // An L shape: a 3x1 row plus one cell below its first: two rectangles.
        assert_eq!(rectangles(&[(0, 0), (1, 0), (2, 0), (0, 1)]), 2);
        assert_eq!(rectangles(&[]), 0);
    }
}
