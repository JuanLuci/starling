//! Content mods' kinetic blocks in play (horizon_client::kinetics): found near the player in
//! the loaded chunks and their networks solved again when blocks change, a crank turned while
//! the player holds use on it, and their turning models drawn every frame. A part that turns
//! whole is left out of the terrain (its render shape is `ENTITYBLOCK_ANIMATED`); a gearbox's or
//! a motor's casing stays there and only its shaft ends turn.
//!
//! The speeds come from the block states alone, so the window and a world's server solve the
//! same networks without telling each other; only the window draws them.
use super::{Game, Screen};
use horizon_client::kinetics::{self, DEGREES_PER_TICK, KineticPart, Part};
use horizon_client::mesher::Vertex;
use horizon_world::registry::BlockStateId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// How many chunks from the player's the parts are looked for (at most the render distance).
const REACH: i32 = 8;
/// The world, while its blocks keep changing, is looked over again at most this often...
const RESCAN: Duration = Duration::from_millis(250);
/// ...and at least this often (chunks sent again replace their blocks without a write).
const REFRESH: Duration = Duration::from_secs(2);
/// A crank keeps turning this long after the last frame its player held use on it.
const CRANK_HOLD: Duration = Duration::from_millis(150);
/// Parts farther from the camera than this (blocks) are not drawn.
const DRAW_DISTANCE: f64 = 96.0;
/// How fast a part's drawn speed follows its network's (ticks for it to come about two thirds
/// of the way): a network that stops or starts slows and speeds up in a moment instead of at
/// once. Every part of a network follows alike, so meshed teeth stay in their gaps.
const EASE_TICKS: f64 = 4.0;

#[derive(Default)]
pub(super) struct Kinetics {
    /// The parts near the player, by position.
    parts: HashMap<[i32; 3], (BlockStateId, Arc<Part>)>,
    /// What each turns at, its own way (kinetics.rs): only those a source reaches.
    speeds: HashMap<[i32; 3], f32>,
    /// What each is drawn turning at, following `speeds` (EASE_TICKS).
    shown: HashMap<[i32; 3], f32>,
    /// Each part's turn when the parts last changed (degrees): meshed teeth go into the gaps.
    phases: HashMap<[i32; 3], f32>,
    /// How far each has turned its own way since (degrees, within a turn), as of the clock
    /// `at` (ticks): a part that stops stays where it is.
    run: HashMap<[i32; 3], f32>,
    at: f64,
    /// The world as it was last looked over: its block revision, its chunks, when.
    seen: Option<(u64, u64, Instant)>,
    /// The crank the player turns, its speed outwards, and until when.
    crank: Option<([i32; 3], f32, Instant)>,
}

impl Game<'_> {
    /// Once a frame: the cranks, and the networks again when the world changed.
    pub(super) fn kinetics_frame(&mut self) {
        let live = self.res.rest_ready() && self.player.is_some() && self.title.is_none();
        if !live || self.res.kinetics().is_empty() {
            if self.kinetics.seen.is_some() {
                self.kinetics = Kinetics::default();
            }
            return;
        }
        let now = Instant::now();
        let crank = self.turn_crank(now);
        let world = (self.world.revision, self.world.chunks.membership_revision());
        let stale = self.kinetics.seen.is_none_or(|(revision, chunks, at)| {
            let since = now.duration_since(at);
            ((revision, chunks) != world && since >= RESCAN) || since >= REFRESH
        });
        let changed = stale && self.find_kinetic_parts();
        if stale {
            self.kinetics.seen = Some((world.0, world.1, now));
        }
        if stale || crank {
            let turned: HashMap<_, _> = self
                .kinetics
                .crank
                .map(|(p, v, _)| (p, v))
                .into_iter()
                .collect();
            let parts = self
                .kinetics
                .parts
                .iter()
                .map(|(p, (_, part))| (*p, part.clone()))
                .collect();
            self.kinetics.speeds = kinetics::solve(&parts, &turned);
            if changed {
                // Each part's turn now, those joining meshed with those already turning.
                let k = &mut self.kinetics;
                let known = k
                    .phases
                    .iter()
                    .filter_map(|(p, phase)| {
                        let (_, part) = k.parts.get(p)?;
                        let run = k.run.get(p).copied().unwrap_or(0.0);
                        Some((*p, phase + own_way(part).1 * run))
                    })
                    .collect();
                k.phases = kinetics::phases_from(&parts, &known);
                k.run.clear();
                k.shown.retain(|p, _| k.parts.contains_key(p));
            }
        }
        self.advance_kinetics();
    }

    /// Each part's drawn speed a frame nearer its network's, and how far it turned since the last
    /// frame at it, by the clock the world draws at.
    fn advance_kinetics(&mut self) {
        let clock = self.world.game_time as f64 + self.clock.fraction();
        let k = &mut self.kinetics;
        let dt = (clock - k.at).clamp(0.0, 20.0);
        k.at = clock;
        if dt == 0.0 {
            return;
        }
        let follow = (1.0 - (-dt / EASE_TICKS).exp()) as f32;
        for p in k.parts.keys() {
            let target = k.speeds.get(p).copied().unwrap_or(0.0);
            let shown = k.shown.entry(*p).or_insert(0.0);
            let before = *shown;
            *shown += (target - *shown) * follow;
            if shown.abs() < 1e-3 && target == 0.0 {
                *shown = 0.0;
            }
            // The turn over the frame at the mean of its speeds then and now.
            let turned = (before + *shown) / 2.0 * DEGREES_PER_TICK * dt as f32;
            if turned != 0.0 {
                let run = k.run.entry(*p).or_insert(0.0);
                *run = (*run + turned).rem_euclid(360.0);
            }
        }
    }

    /// Whether a crank is at `pos` (a press on it turns it rather than breaks it).
    pub(super) fn kinetic_crank_at(&self, pos: [i32; 3]) -> bool {
        self.res.rest_ready()
            && self
                .res
                .kinetics()
                .get(self.world.block(pos[0], pos[1], pos[2]))
                .is_some_and(|part| part.kind == KineticPart::Crank)
    }

    /// The crank the player holds use on (the crosshair's, or the finger's with tap to
    /// interact): turned counter-clockwise as seen from its handle, sneaking the other way, and
    /// a moment after letting go. True when what is turned changed.
    fn turn_crank(&mut self, now: Instant) -> bool {
        let before = self.kinetics.crank.map(|(p, v, _)| (p, v));
        let held = self.right && self.screen == Screen::Playing;
        let target = held
            .then(|| self.pick())
            .flatten()
            .map(|hit| hit.pos)
            .and_then(|p| Some((p, self.kinetics.parts.get(&p)?.1.clone())))
            .filter(|(_, part)| part.kind == KineticPart::Crank);
        if let Some((p, part)) = target {
            // Its back turns about the face outwards: counter-clockwise from the front is the
            // other way about it.
            let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
            let speed = if sneaking { part.rpm } else { -part.rpm };
            self.kinetics.crank = Some((p, speed, now + CRANK_HOLD));
        } else if self
            .kinetics
            .crank
            .is_some_and(|(_, _, until)| now >= until)
        {
            self.kinetics.crank = None;
        }
        before != self.kinetics.crank.map(|(p, v, _)| (p, v))
    }

    /// Every kinetic part in the loaded chunks near the player, from the sections whose palette
    /// may hold one. True when they are not the ones there were.
    fn find_kinetic_parts(&mut self) -> bool {
        let table = self.res.kinetics();
        let Some(player) = &self.player else {
            return false;
        };
        let reach = REACH.min(self.settings.render_distance.max(2));
        let (pcx, pcz) = (
            (player.position.x.floor() as i32) >> 4,
            (player.position.z.floor() as i32) >> 4,
        );
        let kinetic = |id: u32| table.get(BlockStateId(id)).is_some();
        let mut parts = HashMap::new();
        for cx in pcx - reach..=pcx + reach {
            for cz in pcz - reach..=pcz + reach {
                let Some(chunk) = self.world.chunks.get(&(cx, cz)) else {
                    continue;
                };
                for (si, section) in chunk.sections.iter().enumerate() {
                    if !section.blocks.maybe_has(kinetic) {
                        continue;
                    }
                    for i in 0..4096 {
                        let state = BlockStateId(section.get(i));
                        if let Some(part) = table.get(state) {
                            let pos = [
                                cx * 16 + (i & 15) as i32,
                                horizon_client::chunk::MIN_Y + si as i32 * 16 + (i >> 8) as i32,
                                cz * 16 + ((i >> 4) & 15) as i32,
                            ];
                            parts.insert(pos, (state, part.clone()));
                        }
                    }
                }
            }
        }
        let old = &self.kinetics.parts;
        let changed = parts.len() != old.len()
            || parts
                .iter()
                .any(|(p, (state, _))| old.get(p).map(|(s, _)| s) != Some(state));
        self.kinetics.parts = parts;
        changed
    }

    /// The turning models: a part's own (shafts, cogwheels, a crank), the shaft ends at a
    /// gearbox's and a motor's faces, a crank's handle; each about its axis through the block's
    /// middle, as far as it has turned (advance_kinetics).
    pub(super) fn kinetic_geometry(&self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        if self.kinetics.parts.is_empty() {
            return;
        }
        let catalog = &self.pack.render.model_catalog;
        // Turned about `axis` `factor` times its run from `phase` (degrees), about the block's
        // middle.
        let turn = |axis: usize, factor: f32, phase: f32, run: f32| {
            let degrees = (phase + factor * run).rem_euclid(360.0);
            let mut unit = glam::Vec3::ZERO;
            unit[axis] = 1.0;
            glam::Mat4::from_translation(glam::Vec3::splat(0.5))
                * glam::Mat4::from_axis_angle(unit, degrees.to_radians())
                * glam::Mat4::from_translation(glam::Vec3::splat(-0.5))
        };
        for (pos, (state, part)) in &self.kinetics.parts {
            let corner = glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
            if (corner + glam::DVec3::splat(0.5)).distance(camera) > DRAW_DISTANCE {
                continue;
            }
            let at = glam::Mat4::from_translation((corner - camera).as_vec3());
            // The brightest of its block and the six beside it, sky and block light apart: a
            // part on a solid template (a crushing wheel's, a flywheel's) has none of its own.
            const AROUND: [[i32; 3]; 7] = [
                [0, 0, 0],
                [1, 0, 0],
                [-1, 0, 0],
                [0, 1, 0],
                [0, -1, 0],
                [0, 0, 1],
                [0, 0, -1],
            ];
            let (sky, block) = AROUND
                .into_iter()
                .map(|d| {
                    self.world
                        .light(pos[0] + d[0], pos[1] + d[1], pos[2] + d[2])
                })
                .fold((0, 0), |(s, b), l| (s.max(l >> 4), b.max(l & 15)));
            let light = (u32::from(sky) * 16) | ((u32::from(block) * 16) << 8);
            let run = self.kinetics.run.get(pos).copied().unwrap_or(0.0);
            let phase = self.kinetics.phases.get(pos).copied().unwrap_or(0.0);
            let (axis, factor) = own_way(part);
            let own = turn(axis, factor, phase, run);
            let Ok(models) = catalog.collect_seeded(*state, 0) else {
                continue;
            };
            let start = out.len();
            if part.spins {
                for (_, model) in &models {
                    self.baked_model_vertices(model, at * own, light, false, out);
                }
            }
            if let Some(extra) = &part.extra {
                let turns = part
                    .extra_turns
                    .or_else(|| models.first().map(|(v, _)| v.rotations))
                    .unwrap_or([0; 3]);
                if let Some(model) = catalog.partial(extra, [turns[0], turns[1], 0]) {
                    self.baked_model_vertices(model, at * own, light, false, out);
                }
            }
            if let Some(half) = &part.half {
                for (face, k) in part.ends() {
                    let Some(model) = catalog.partial(half, kinetics::turns_to(face)) else {
                        continue;
                    };
                    // As the shaft joined there turns: about its axis, by the end's factor.
                    let axis = face.iter().position(|c| *c != 0).unwrap_or(1);
                    let end = turn(axis, k, phase, run);
                    self.baked_model_vertices(model, at * end, light, false, out);
                }
            }
            cull_back_faces(out, start);
        }
    }
}

/// The axis a part's own model turns about and how its run turns there: a shaft-like part's
/// about its axis, a motor's and a crank's by their face's sign.
fn own_way(part: &Part) -> (usize, f32) {
    match part.kind {
        KineticPart::Motor | KineticPart::Crank => kinetics::at_face(part.face, 1.0),
        _ => (part.axis, 1.0),
    }
}

/// Drops the quads of `out[from..]` facing away from the camera (at the origin), as the
/// terrain's culled faces: the entity pass draws both sides, and a part's open side (a crank's
/// hollow base) would show its faces' backs against the shaft beside it.
fn cull_back_faces(out: &mut Vec<Vertex>, from: usize) {
    let mut kept = from;
    for quad in (from..out.len()).step_by(4) {
        let p = |i: usize| glam::Vec3::from_array(out[quad + i].pos);
        let normal = (p(1) - p(0)).cross(p(2) - p(0));
        if normal == glam::Vec3::ZERO || normal.dot(p(0)) < 0.0 {
            out.copy_within(quad..quad + 4, kept);
            kept += 4;
        }
    }
    out.truncate(kept);
}
