//! Kinetic networks: rotation passed from block to block, as Create's (written from its
//! documented behaviour, its Ponder scenes and wiki, never its code): shafts along their axis,
//! cogwheels meshing with the cogwheels beside them, gearboxes around a corner, and the sources
//! that turn them (motors, cranks). Content mods say which of their blocks are what
//! (`kinetic` in a block's declaration, gathered in the root's kinetic.json).
//!
//! A part's speed is one number, in RPM, solved from the block states alone (and the cranks
//! being turned). A shaft-like part's (shaft, cogwheels) is about the positive direction of its
//! axis; a gearbox's, a motor's and a crank's is about each face outwards. Two parts joined end
//! to end turn their shared shaft alike; meshed cogwheels move alike where they touch: a small
//! cogwheel turns the one beside it the other way, a large one half as fast (the small one
//! twice), two large ones at right angles as fast.
use crate::Result;
use horizon_world::registry::{BlockRegistry, BlockStateId};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

pub use horizon_mods::content::{Kinetic, KineticPart};

/// The fastest a part turns (Create's default speed limit).
pub const MAX_RPM: f32 = 256.0;
/// Degrees a part at 1 RPM turns in a tick (360 degrees a minute, 20 ticks a second).
pub const DEGREES_PER_TICK: f32 = 0.3;

/// A kinetic state: its part and how its state turns it.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub kind: KineticPart,
    /// The axis it turns about (0 x, 1 y, 2 z): a shaft's, a gearbox's, a motor's or crank's
    /// facing's.
    pub axis: usize,
    /// A motor's facing; a crank's back (the block it turns). Zero for the others.
    pub face: [i32; 3],
    /// A source's speed: a motor's, a crank's while turned.
    pub rpm: f32,
    pub spins: bool,
    pub half: Option<String>,
    pub extra: Option<String>,
    /// The quarter turns that point `extra` at the state's facing, when it is made pointing
    /// elsewhere than the block's own model (None: the blockstate's turns).
    pub extra_turns: Option<[u8; 3]>,
}

impl Part {
    /// A declared part on a state with these `axis` and `facing` properties; None when the
    /// state has not the one its part needs. A shaft-like part with no `axis` turns about its
    /// facing's (a water wheel).
    pub fn new(kinetic: &Kinetic, axis: Option<&str>, facing: Option<&str>) -> Option<Self> {
        let by_name = |a: &str| match a {
            "x" => Some(0),
            "y" => Some(1),
            "z" => Some(2),
            _ => None,
        };
        let (axis, face) = match kinetic.part {
            KineticPart::Motor | KineticPart::Crank => {
                let facing = direction(facing?)?;
                let face = if kinetic.part == KineticPart::Crank {
                    facing.map(|v| -v)
                } else {
                    facing
                };
                (axis_of(face), face)
            }
            _ => match axis {
                Some(axis) => (by_name(axis)?, [0; 3]),
                None => (axis_of(direction(facing?)?), [0; 3]),
            },
        };
        let extra_turns = match (kinetic.extra_facing.as_deref(), facing) {
            (Some(made), Some(facing)) => Some(turns_from(direction(made)?, direction(facing)?)),
            _ => None,
        };
        Some(Part {
            kind: kinetic.part,
            axis,
            face,
            rpm: kinetic.rpm.clamp(-MAX_RPM, MAX_RPM),
            spins: kinetic.spins,
            half: kinetic.half.clone(),
            extra: kinetic.extra.clone(),
            extra_turns,
        })
    }
    /// A cogwheel's teeth (Create's models: 8 and 16, one along each axis at no turn).
    fn teeth(&self) -> f32 {
        match self.kind {
            KineticPart::LargeCogwheel => 16.0,
            _ => 8.0,
        }
    }
    /// A cogwheel's radius in blocks; 0 for parts that do not mesh.
    pub fn radius(&self) -> f32 {
        match self.kind {
            KineticPart::Cogwheel => 0.5,
            KineticPart::LargeCogwheel => 1.0,
            _ => 0.0,
        }
    }
    /// The faces it joins through and how its speed turns there (1: as its own, about its
    /// axis; ±1 by the face's sign: about the face outwards).
    pub fn ends(&self) -> Vec<([i32; 3], f32)> {
        let unit = |axis: usize, sign: i32| {
            let mut v = [0; 3];
            v[axis] = sign;
            v
        };
        match self.kind {
            KineticPart::Shaft | KineticPart::Cogwheel | KineticPart::LargeCogwheel => {
                vec![(unit(self.axis, 1), 1.0), (unit(self.axis, -1), 1.0)]
            }
            KineticPart::Gearbox => (0..3)
                .filter(|a| *a != self.axis)
                .flat_map(|a| [(unit(a, 1), 1.0), (unit(a, -1), -1.0)])
                .collect(),
            KineticPart::Motor | KineticPart::Crank => {
                vec![(self.face, self.face[self.axis] as f32)]
            }
        }
    }
    /// Whether it is a source: a motor always, a crank while turned.
    pub fn source(&self) -> bool {
        self.kind == KineticPart::Motor
    }
}

pub use horizon_mods::content::{direction, turns_from, turns_to};

fn axis_of(v: [i32; 3]) -> usize {
    v.iter().position(|c| *c != 0).unwrap_or(1)
}

/// Every state's part, from a content root's kinetic.json; empty without one.
#[derive(Default)]
pub struct KineticTable {
    parts: Vec<Option<Arc<Part>>>,
}

impl KineticTable {
    pub fn load(root: &Path, registry: &BlockRegistry) -> Result<Self> {
        let path = root.join(crate::mod_content::KINETIC);
        let Ok(bytes) = std::fs::read(&path) else {
            return Ok(Self::default());
        };
        let declared: HashMap<String, Kinetic> =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut parts = vec![None; registry.len()];
        for (i, slot) in parts.iter_mut().enumerate() {
            let Some(state) = registry.state(BlockStateId(i as u32)) else {
                continue;
            };
            let Some(kinetic) = declared.get(&state.name) else {
                continue;
            };
            let prop = |k: &str| state.properties.get(k).map(String::as_str);
            *slot = Part::new(kinetic, prop("axis"), prop("facing")).map(Arc::new);
        }
        Ok(Self { parts })
    }
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(Option::is_none)
    }
    pub fn get(&self, state: BlockStateId) -> Option<&Arc<Part>> {
        self.parts.get(state.0 as usize)?.as_ref()
    }
}

/// The parts `p` joins (by end or mesh) and the factor from its speed to theirs.
fn links(parts: &HashMap<[i32; 3], Arc<Part>>, p: [i32; 3], part: &Part) -> Vec<([i32; 3], f32)> {
    let at = |d: [i32; 3]| [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
    let mut out = Vec::new();
    for (face, k) in part.ends() {
        let q = at(face);
        let Some(other) = parts.get(&q) else {
            continue;
        };
        let back = face.map(|v| -v);
        if let Some((_, k2)) = other.ends().into_iter().find(|(f, _)| *f == back) {
            // The shaft between them turns alike from both sides.
            out.push((q, k * k2));
        }
    }
    let r = part.radius();
    if r == 0.0 {
        return out;
    }
    let a = part.axis;
    let (b, c) = ((a + 1) % 3, (a + 2) % 3);
    let offset = |pairs: [(usize, i32); 2]| {
        let mut d = [0; 3];
        for (axis, v) in pairs {
            d[axis] = v;
        }
        d
    };
    // In its plane: beside it (small ones mesh) and diagonally (small with large).
    for (u, v) in [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ] {
        let q = at(offset([(b, u), (c, v)]));
        let Some(other) = parts.get(&q) else {
            continue;
        };
        if other.axis != a || other.radius() == 0.0 {
            continue;
        }
        let diagonal = u != 0 && v != 0;
        let meshes = if diagonal {
            r != other.radius()
        } else {
            r == 0.5 && other.radius() == 0.5
        };
        if meshes {
            // Alike where they touch: the other way, slower by the radii.
            out.push((q, -r / other.radius()));
        }
    }
    // Large ones at right angles, on two faces of one block: d is one step along each axis.
    if r == 1.0 {
        for other_axis in [b, c] {
            for (alpha, beta) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let q = at(offset([(a, alpha), (other_axis, beta)]));
                let Some(other) = parts.get(&q) else {
                    continue;
                };
                if other.axis == other_axis && other.radius() == 1.0 {
                    out.push((q, (alpha * beta) as f32));
                }
            }
        }
    }
    out
}

/// Every part's speed, from the sources: the motors, and `turned` (cranks being turned, at
/// their own speed, outwards). Each network takes its fastest source's speed; a slower one
/// turning it the same way is overpowered. A network its sources (or a loop of gears) would
/// turn two ways at once stands still. Parts no source reaches are not listed.
pub fn solve(
    parts: &HashMap<[i32; 3], Arc<Part>>,
    turned: &HashMap<[i32; 3], f32>,
) -> HashMap<[i32; 3], f32> {
    let mut sources: Vec<([i32; 3], f32)> = parts
        .iter()
        .filter(|(_, part)| part.source())
        .map(|(p, part)| (*p, part.rpm))
        .chain(
            turned
                .iter()
                .filter(|(p, _)| parts.contains_key(*p))
                .map(|(p, v)| (*p, *v)),
        )
        .filter(|(_, v)| *v != 0.0)
        .collect();
    // The fastest first; ties in position order, so the outcome does not hang on map order.
    sources.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()).then(a.0.cmp(&b.0)));
    let mut speed: HashMap<[i32; 3], (f32, usize)> = HashMap::new();
    let mut stuck: Vec<bool> = Vec::new();
    // Parts held at the limit: their gears no longer match, which jams nothing.
    let mut capped: std::collections::HashSet<[i32; 3]> = std::collections::HashSet::new();
    for (start, value) in sources {
        if let Some((s, net)) = speed.get(&start) {
            // Reached from a faster source: the other way jams the network.
            if s * value < 0.0 {
                stuck[*net] = true;
            }
            continue;
        }
        let net = stuck.len();
        stuck.push(false);
        speed.insert(start, (value.clamp(-MAX_RPM, MAX_RPM), net));
        let mut queue = VecDeque::from([start]);
        while let Some(p) = queue.pop_front() {
            let (s, _) = speed[&p];
            for (q, factor) in links(parts, p, &parts[&p]) {
                let raw = s * factor;
                let want = raw.clamp(-MAX_RPM, MAX_RPM);
                match speed.get(&q) {
                    None => {
                        if want != raw {
                            capped.insert(q);
                        }
                        speed.insert(q, (want, net));
                        queue.push_back(q);
                    }
                    Some((had, _))
                        if (had - want).abs() > 1e-3
                            && !capped.contains(&p)
                            && !capped.contains(&q) =>
                    {
                        stuck[net] = true
                    }
                    Some(_) => {}
                }
            }
        }
    }
    speed
        .into_iter()
        .map(|(p, (s, net))| (p, if stuck[net] { 0.0 } else { s }))
        .collect()
}

/// Every part's turn when the world's clock began, in degrees, so meshed cogwheels' teeth go into
/// each other's gaps as they turn: a small one beside another half a tooth (22.5 degrees) from it,
/// a large one diagonal to a small one half of its own tooth (11.25). Parts on one axle keep one
/// turn; each separate set of parts starts at 0 from its least position.
pub fn phases(parts: &HashMap<[i32; 3], Arc<Part>>) -> HashMap<[i32; 3], f32> {
    let mut starts: Vec<[i32; 3]> = parts.keys().copied().collect();
    starts.sort();
    let mut phase = HashMap::new();
    for start in starts {
        if phase.contains_key(&start) {
            continue;
        }
        phase.insert(start, 0.0);
        let mut queue = VecDeque::from([start]);
        while let Some(p) = queue.pop_front() {
            let (a, c) = (&parts[&p], phase[&p]);
            for (q, _) in links(parts, p, a) {
                if phase.contains_key(&q) {
                    continue;
                }
                let b = &parts[&q];
                let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                let meshed =
                    a.radius() > 0.0 && b.radius() > 0.0 && b.axis == a.axis && d[a.axis] == 0;
                phase.insert(q, if meshed { mesh_phase(a, b, d, c) } else { c });
                queue.push_back(q);
            }
        }
    }
    phase
}

/// The turn of cogwheel `b`, `d` from `a` in `a`'s plane, that meshes with `a` at turn `a_turn`:
/// with angles in the plane about the axis (from its next axis to the one after, as the turn
/// goes), a tooth of `a` pointing at `b` faces a gap of `b`. Where they touch, both move alike
/// as they turn (speeds the other way by their radii, as their teeth's counts), so it holds.
fn mesh_phase(a: &Part, b: &Part, d: [i32; 3], a_turn: f32) -> f32 {
    let (u, v) = ((a.axis + 1) % 3, (a.axis + 2) % 3);
    let toward = (d[v] as f32).atan2(d[u] as f32).to_degrees();
    let (pa, pb) = (360.0 / a.teeth(), 360.0 / b.teeth());
    (toward + 180.0 - pb * (0.5 - (toward - a_turn) / pa)).rem_euclid(pb)
}

/// The axis (0 x, 1 y, 2 z) and speed about its positive direction a model turning at face `f`
/// of a part whose speed there is `s` outwards turns at.
pub fn at_face(f: [i32; 3], s: f32) -> (usize, f32) {
    let axis = axis_of(f);
    (axis, s * f[axis] as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: KineticPart, axis: Option<&str>, facing: Option<&str>, rpm: f32) -> Arc<Part> {
        let kinetic = Kinetic {
            part: kind,
            rpm,
            spins: true,
            half: None,
            extra: None,
            extra_facing: None,
        };
        Arc::new(Part::new(&kinetic, axis, facing).unwrap())
    }

    #[test]
    fn turns_point_a_model_from_the_way_it_is_made() {
        // From south, as the shaft ends are made.
        let faces = [
            ([0, 0, 1], [0, 0, 0]),
            ([-1, 0, 0], [0, 1, 0]),
            ([0, 0, -1], [0, 2, 0]),
            ([1, 0, 0], [0, 3, 0]),
            ([0, 1, 0], [1, 0, 0]),
            ([0, -1, 0], [3, 0, 0]),
        ];
        for (face, turns) in faces {
            assert_eq!(turns_to(face), turns);
        }
        // Create's crank handle is made pointing north, its block up: a crank facing up turns
        // the handle up, one facing east only about y.
        let kinetic = Kinetic {
            part: KineticPart::Crank,
            rpm: 32.0,
            spins: true,
            half: None,
            extra: Some("create:block/hand_crank/handle".into()),
            extra_facing: Some("north".into()),
        };
        let up = Part::new(&kinetic, None, Some("up")).unwrap();
        assert_eq!(up.extra_turns, Some([3, 0, 0]));
        let east = Part::new(&kinetic, None, Some("east")).unwrap();
        assert_eq!(east.extra_turns, Some([0, 1, 0]));
        // Without it, the blockstate's turns.
        assert_eq!(shaft("x").extra_turns, None);
    }
    fn shaft(axis: &str) -> Arc<Part> {
        part(KineticPart::Shaft, Some(axis), None, 0.0)
    }
    fn cog(axis: &str) -> Arc<Part> {
        part(KineticPart::Cogwheel, Some(axis), None, 0.0)
    }
    fn large(axis: &str) -> Arc<Part> {
        part(KineticPart::LargeCogwheel, Some(axis), None, 0.0)
    }
    fn motor(facing: &str, rpm: f32) -> Arc<Part> {
        part(KineticPart::Motor, None, Some(facing), rpm)
    }

    #[test]
    fn shafts_relay_rotation_in_a_straight_line() {
        // A motor facing east turns the shafts east of it.
        let mut parts = HashMap::new();
        parts.insert([0, 0, 0], motor("east", 16.0));
        for x in 1..=3 {
            parts.insert([x, 0, 0], shaft("x"));
        }
        // One beside the line, on another axis, is not joined.
        parts.insert([1, 1, 0], shaft("y"));
        let speeds = solve(&parts, &HashMap::new());
        for x in 1..=3 {
            assert_eq!(speeds[&[x, 0, 0]], 16.0);
        }
        assert!(!speeds.contains_key(&[1, 1, 0]));
        // Facing west, its shafts turn the other way about their axis (alike outwards).
        let mut parts = HashMap::new();
        parts.insert([0, 0, 0], motor("west", 16.0));
        parts.insert([-1, 0, 0], shaft("x"));
        assert_eq!(solve(&parts, &HashMap::new())[&[-1, 0, 0]], -16.0);
        // A shaft-like part with only a facing (a water wheel) is an axle along it.
        let wheel = part(KineticPart::Shaft, None, Some("west"), 0.0);
        assert_eq!(wheel.axis, 0);
        parts.insert([-2, 0, 0], wheel);
        parts.insert([-3, 0, 0], shaft("x"));
        assert_eq!(solve(&parts, &HashMap::new())[&[-3, 0, 0]], -16.0);
    }

    #[test]
    fn neighbouring_cogwheels_turn_opposite_ways_and_large_ones_change_speed() {
        let mut parts = HashMap::new();
        parts.insert([0, -1, 0], motor("up", 32.0));
        parts.insert([0, 0, 0], cog("y"));
        parts.insert([1, 0, 0], cog("y"));
        // Diagonal from the first: a large one, half as fast.
        parts.insert([-1, 0, 1], large("y"));
        let speeds = solve(&parts, &HashMap::new());
        assert_eq!(speeds[&[0, 0, 0]], 32.0);
        assert_eq!(speeds[&[1, 0, 0]], -32.0);
        assert_eq!(speeds[&[-1, 0, 1]], -16.0);
        // Large to small doubles.
        let mut parts = HashMap::new();
        parts.insert([0, -1, 0], motor("up", 8.0));
        parts.insert([0, 0, 0], large("y"));
        parts.insert([1, 0, 1], cog("y"));
        assert_eq!(solve(&parts, &HashMap::new())[&[1, 0, 1]], -16.0);
    }

    #[test]
    fn large_cogwheels_join_at_right_angles() {
        // Two large cogwheels on two faces of the block at the origin: one above it (axis y),
        // one east of it (axis x).
        let mut parts = HashMap::new();
        parts.insert([0, 2, 0], motor("down", 16.0));
        parts.insert([0, 1, 0], large("y"));
        parts.insert([1, 0, 0], large("x"));
        let speeds = solve(&parts, &HashMap::new());
        // As fast; the way their meeting edge moves alike (one step along -y, then +x).
        assert_eq!(speeds[&[0, 1, 0]], -16.0);
        assert_eq!(speeds[&[1, 0, 0]].abs(), 16.0);
        // Facing the same way along the edge both ways round gives one answer: the solve
        // from either end agrees.
        let mut back = HashMap::new();
        back.insert([2, 0, 0], motor("west", 16.0));
        back.insert([1, 0, 0], large("x"));
        back.insert([0, 1, 0], large("y"));
        let s = solve(&back, &HashMap::new());
        assert_eq!(
            s[&[0, 1, 0]] / s[&[1, 0, 0]],
            speeds[&[0, 1, 0]] / speeds[&[1, 0, 0]]
        );
    }

    #[test]
    fn a_gearbox_reverses_straight_through_and_mirrors_round_corners() {
        let mut parts = HashMap::new();
        parts.insert([-2, 0, 0], motor("east", 16.0));
        parts.insert([-1, 0, 0], shaft("x"));
        parts.insert([0, 0, 0], part(KineticPart::Gearbox, Some("y"), None, 0.0));
        parts.insert([1, 0, 0], shaft("x"));
        parts.insert([0, 0, 1], shaft("z"));
        parts.insert([0, 0, -1], shaft("z"));
        let s = solve(&parts, &HashMap::new());
        assert_eq!(s[&[-1, 0, 0]], 16.0);
        // Straight through: reversed.
        assert_eq!(s[&[1, 0, 0]], -16.0);
        // Round the corners, the two sides mirror each other.
        assert_eq!(s[&[0, 0, 1]], -s[&[0, 0, -1]]);
    }

    #[test]
    fn sources_turning_a_network_two_ways_jam_it_and_the_faster_one_leads() {
        // Two motors facing each other turn the shafts between them two ways: they stand still.
        let mut parts = HashMap::new();
        parts.insert([0, 0, 0], motor("east", 16.0));
        parts.insert([1, 0, 0], shaft("x"));
        parts.insert([2, 0, 0], motor("west", 16.0));
        assert_eq!(solve(&parts, &HashMap::new())[&[1, 0, 0]], 0.0);
        // The same way (the west motor turned back), the faster one's speed.
        parts.insert([2, 0, 0], motor("west", -32.0));
        assert_eq!(solve(&parts, &HashMap::new())[&[1, 0, 0]], 32.0);
        // A ring of four small cogwheels turns each neighbour the other way: it holds.
        let mut ring = HashMap::new();
        ring.insert([0, -1, 0], motor("up", 16.0));
        for p in [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]] {
            ring.insert(p, cog("y"));
        }
        let s = solve(&ring, &HashMap::new());
        assert_eq!(s[&[1, 0, 1]], 16.0);
        assert_eq!(s[&[1, 0, 0]], -16.0);
    }

    #[test]
    fn a_crank_turns_its_back_while_turned_and_speeds_stay_under_the_limit() {
        let crank = part(KineticPart::Crank, None, Some("up"), 32.0);
        assert_eq!(crank.face, [0, -1, 0]);
        let mut parts = HashMap::new();
        parts.insert([0, 1, 0], crank);
        parts.insert([0, 0, 0], shaft("y"));
        assert!(solve(&parts, &HashMap::new()).is_empty());
        let turned = HashMap::from([([0, 1, 0], -32.0)]);
        assert_eq!(solve(&parts, &turned)[&[0, 0, 0]], 32.0);
        // Doubled past the limit: held at it.
        let mut fast = HashMap::new();
        fast.insert([0, -1, 0], motor("up", 200.0));
        fast.insert([0, 0, 0], large("y"));
        fast.insert([1, 0, 1], cog("y"));
        assert_eq!(solve(&fast, &HashMap::new())[&[1, 0, 1]], -MAX_RPM);
    }

    #[test]
    fn meshed_teeth_go_into_the_gaps() {
        let mut parts = HashMap::new();
        parts.insert([0, 0, 0], cog("y"));
        parts.insert([1, 0, 0], cog("y"));
        parts.insert([0, 1, 0], cog("y"));
        parts.insert([-1, 0, 1], large("y"));
        let p = phases(&parts);
        // Beside it: half a tooth off; on the same axle: as it is.
        assert!(((p[&[1, 0, 0]] - p[&[0, 0, 0]]).rem_euclid(45.0) - 22.5).abs() < 1e-3);
        assert_eq!(p[&[0, 1, 0]], p[&[0, 0, 0]]);
        // And the teeth stay in the gaps as they turn (each at its speed: the neighbour the other
        // way, the large one half as fast): a tooth of one pointing at the other means a gap of
        // the other pointing back, at any time.
        let meets = |a: [i32; 3], b: [i32; 3], speed_b: f32, t: f32| {
            let (pa, pb) = (&parts[&a], &parts[&b]);
            let (ta, tb) = (p[&a] + 16.0 * t, p[&b] + speed_b * t);
            // In the plane about y: z, then x.
            let toward = ((b[0] - a[0]) as f32)
                .atan2((b[2] - a[2]) as f32)
                .to_degrees();
            let teeth =
                (toward - ta) / (360.0 / pa.teeth()) + (toward + 180.0 - tb) / (360.0 / pb.teeth());
            (teeth.rem_euclid(1.0) - 0.5).abs() < 1e-3
        };
        for t in [0.0f32, 0.7, 3.3, 51.0] {
            assert!(meets([0, 0, 0], [1, 0, 0], -16.0, t), "beside, at {t}");
            assert!(meets([0, 0, 0], [-1, 0, 1], -8.0, t), "diagonal, at {t}");
        }
    }

    #[test]
    fn models_made_pointing_south_turn_to_each_face() {
        assert_eq!(turns_to([0, 0, 1]), [0, 0, 0]);
        assert_eq!(turns_to([0, 0, -1]), [0, 2, 0]);
        assert_eq!(turns_to([0, 1, 0]), [1, 0, 0]);
        assert_eq!(at_face([0, 0, -1], 16.0), (2, -16.0));
        assert_eq!(at_face([1, 0, 0], 16.0), (0, 16.0));
    }
}
