//! Content mods' kinetic blocks in play (horizon_client::kinetics): found near the player in
//! the loaded chunks and their networks solved again when blocks change, a crank turned while
//! the player holds use on it, and their turning models drawn every frame. A part that turns
//! whole is left out of the terrain (its render shape is `ENTITYBLOCK_ANIMATED`); a gearbox's or
//! a motor's casing stays there and only its shaft ends turn.
//!
//! The speeds come from the block states alone, so the window and a world's server solve the
//! same networks without telling each other; only the window draws them.
use super::machine_entities::Kind;
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
/// A crank keeps turning this long after the last frame its player held use on it...
const CRANK_HOLD: Duration = Duration::from_millis(150);
/// ...and at least this long after a press on it starts (a tap turns it about a turn at 32 RPM).
const CRANK_TAP: Duration = Duration::from_millis(1800);
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
    /// The machines with block entities near the player (machine_entities.rs): what they hold
    /// and their moving parts are drawn.
    machines: HashMap<[i32; 3], Kind>,
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
    /// Use was held on a crank last frame: a press that starts turns it a while (a tap).
    cranking: bool,
    /// The belts near the player (belts.rs), and each one's items as the window moves them
    /// between their tag's updates (the update's game time with them).
    belts: Vec<super::belts::Chain>,
    belt_items: HashMap<[i32; 3], (i64, Vec<super::belts::Transported>)>,
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
            let parts = self
                .kinetics
                .parts
                .iter()
                .map(|(p, (_, part))| (*p, part.clone()))
                .collect();
            // The crank turned, and the water wheels the water past them turns.
            let (world, fluids) = (&self.world, &self.res.motion().fluids);
            let mut turned = kinetics::water_wheels(&parts, |p| {
                let v = horizon_client::entities::flow_vector(world, fluids, p);
                [v.x, v.y, v.z]
            });
            turned.extend(self.kinetics.crank.map(|(p, v, _)| (p, v)));
            // Each belt's segments turn together.
            let groups: Vec<Vec<[i32; 3]>> = self
                .kinetics
                .belts
                .iter()
                .map(|c| c.segments.clone())
                .collect();
            self.kinetics.speeds = kinetics::solve_linked(&parts, &turned, &groups);
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
        self.advance_belts(dt as f32);
    }

    /// Each belt's items along as its server moves them (BeltInventory's moving, which Create's
    /// client runs too), from where its tag last put them.
    fn advance_belts(&mut self, dt: f32) {
        for chain in self.kinetics.belts.clone() {
            let start = chain.start();
            let tag = super::spill::saved_entity(&self.world, start);
            let since = match tag.as_ref().and_then(|t| t.get("Since")) {
                Some(horizon_world::nbt::Tag::Long(v)) => *v,
                _ => 0,
            };
            let stale = self
                .kinetics
                .belt_items
                .get(&start)
                .is_none_or(|(at, _)| *at != since);
            if stale {
                let items = tag
                    .as_ref()
                    .map(super::belts::read_items)
                    .unwrap_or_default();
                self.kinetics.belt_items.insert(start, (since, items));
            }
            let speed = self.kinetics.shown.get(&start).copied().unwrap_or(0.0);
            let movement = chain.movement(speed) * dt;
            if let Some((_, items)) = self.kinetics.belt_items.get_mut(&start) {
                super::belts::advance(items, chain.len(), movement, super::belts::Ending::Eject);
            }
        }
    }

    /// Whether a crank is at `pos` (a use on it turns it, placing nothing against it).
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
    /// a moment after letting go; each press that starts on it (a tap: touch holds break it, as
    /// any block) turns it a while. True when what is turned changed.
    fn turn_crank(&mut self, now: Instant) -> bool {
        let before = self.kinetics.crank.map(|(p, v, _)| (p, v));
        let held = self.right && self.screen == Screen::Playing;
        let target = held
            .then(|| self.pick())
            .flatten()
            .map(|hit| hit.pos)
            .and_then(|p| Some((p, self.kinetics.parts.get(&p)?.1.clone())))
            .filter(|(_, part)| part.kind == KineticPart::Crank);
        let pressed = target.is_some() && !self.kinetics.cranking;
        self.kinetics.cranking = target.is_some();
        if let Some((p, part)) = target {
            // Its back turns about the face outwards: counter-clockwise from the front is the
            // other way about it.
            let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
            let speed = if sneaking { part.rpm } else { -part.rpm };
            let turning = self
                .kinetics
                .crank
                .filter(|(q, v, _)| *q == p && *v == speed)
                .map(|(_, _, until)| until);
            let until =
                (now + if pressed { CRANK_TAP } else { CRANK_HOLD }).max(turning.unwrap_or(now));
            self.kinetics.crank = Some((p, speed, until));
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
        let kinetic = |id: u32| {
            table.get(BlockStateId(id)).is_some() || table.machine(BlockStateId(id)).is_some()
        };
        let mut parts = HashMap::new();
        let mut machines = HashMap::new();
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
                        let part = table.get(state);
                        let machine = table.machine(state);
                        if part.is_none() && machine.is_none() {
                            continue;
                        }
                        let pos = [
                            cx * 16 + (i & 15) as i32,
                            horizon_client::chunk::MIN_Y + si as i32 * 16 + (i >> 8) as i32,
                            cz * 16 + ((i >> 4) & 15) as i32,
                        ];
                        if let Some(kind) = Kind::of(machine, part.map(|p| p.kind)) {
                            machines.insert(pos, kind);
                        }
                        if let Some(part) = part {
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
        let belt_positions: Vec<[i32; 3]> = machines
            .iter()
            .filter(|(_, k)| **k == Kind::Belt)
            .map(|(p, _)| *p)
            .collect();
        let belts = super::belts::chains(&self.belt_segments(&belt_positions));
        self.kinetics
            .belt_items
            .retain(|start, _| belts.iter().any(|c| c.start() == *start));
        self.kinetics.belts = belts;
        self.kinetics.parts = parts;
        self.kinetics.machines = machines;
        changed
    }

    /// The turning models: a part's own (shafts, cogwheels, a crank), the shaft ends at a
    /// gearbox's and a motor's faces, a crank's handle; each about its axis through the block's
    /// middle, as far as it has turned (advance_kinetics).
    pub(super) fn kinetic_geometry(&self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        self.machine_item_geometry(camera, out);
        self.belt_item_geometry(camera, out);
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
            // A belt's segment and pulley (BeltRenderer).
            if part.kind == KineticPart::Belt {
                self.belt_segment_geometry(*state, part, at, own, light, out);
            }
            // A saw's blade (SawRenderer.renderBlade).
            if part.kind == KineticPart::Saw {
                self.saw_blade_geometry(*state, *pos, part, at, light, out);
            }
            // A press's head, a mixer's pole and head, as far down as their work has them
            // (MechanicalPressRenderer, MechanicalMixerRenderer); a mixer's head turns.
            if part.head.is_some() || part.pole.is_some() {
                let turns = models.first().map_or([0; 3], |(v, _)| v.rotations);
                let (down, spin) = match part.kind {
                    KineticPart::Mixer => self.mixer_head(*pos),
                    _ => (self.press_head_down(*pos), 0.0),
                };
                let drop = glam::Mat4::from_translation(glam::Vec3::new(0.0, -down, 0.0));
                for (model, spins) in [(&part.head, true), (&part.pole, false)] {
                    let Some(model) = model
                        .as_deref()
                        .and_then(|m| catalog.partial(m, [turns[0], turns[1], 0]))
                    else {
                        continue;
                    };
                    let turned = if spins {
                        turn(1, 1.0, spin, 0.0)
                    } else {
                        glam::Mat4::IDENTITY
                    };
                    self.baked_model_vertices(model, at * drop * turned, light, false, out);
                }
            }
            cull_back_faces(out, start);
        }
    }

    /// `BeltRenderer.renderSafe` for one segment: its part of the belt (its top and its bottom,
    /// or a diagonal's one), turned to its facing and slope, and its pulley turning with it.
    /// `part.models` are the belt's: start, middle, end, their bottoms, the diagonals', the
    /// pulley.
    fn belt_segment_geometry(
        &self,
        state: BlockStateId,
        part: &Part,
        at: glam::Mat4,
        own: glam::Mat4,
        light: u32,
        out: &mut Vec<Vertex>,
    ) {
        use super::belts::{Part as Piece, Segment, Slope};
        let catalog = &self.pack.render.model_catalog;
        let Some(segment) = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| Segment::of(&s.properties))
        else {
            return;
        };
        let model = |i: usize| {
            part.models
                .get(i)
                .and_then(|m| catalog.partial(m, [0, 0, 0]))
        };
        let f = segment.facing;
        let (upward, downward) = (
            segment.slope == Slope::Upward,
            segment.slope == Slope::Downward,
        );
        let diagonal = upward || downward;
        let sideways = segment.slope == Slope::Sideways;
        let vertical = segment.slope == Slope::Vertical;
        // AngleHelper.horizontalAngle: south 0, east 90, north 180, west 270.
        let facing_angle = match f {
            [1, 0, 0] => 90.0f32,
            [0, 0, -1] => 180.0,
            [-1, 0, 0] => 270.0,
            _ => 0.0,
        };
        let y =
            facing_angle + if upward { 180.0 } else { 0.0 } + if sideways { 270.0 } else { 0.0 };
        let center = glam::Mat4::from_translation(glam::Vec3::splat(0.5));
        let local = center
            * glam::Mat4::from_rotation_y(y.to_radians())
            * glam::Mat4::from_rotation_z(if sideways { 90f32.to_radians() } else { 0.0 })
            * glam::Mat4::from_rotation_x(if !diagonal && segment.slope != Slope::Horizontal {
                90f32.to_radians()
            } else {
                0.0
            })
            * glam::Mat4::from_translation(glam::Vec3::splat(-0.5));
        let (mut start, mut end) = (segment.part == Piece::Start, segment.part == Piece::End);
        let positive = f[0] + f[2] > 0;
        if downward || (vertical && positive) {
            std::mem::swap(&mut start, &mut end);
        }
        let which = |bottom: bool| match (diagonal, bottom, start, end) {
            (true, _, true, _) => 6,
            (true, _, _, true) => 8,
            (true, _, _, _) => 7,
            (false, true, true, _) => 3,
            (false, true, _, true) => 5,
            (false, true, _, _) => 4,
            (false, false, true, _) => 0,
            (false, false, _, true) => 2,
            (false, false, _, _) => 1,
        };
        for bottom in [true, false] {
            if let Some(m) = model(which(bottom)) {
                self.baked_model_vertices(m, at * local, light, false, out);
            }
            // A diagonal belt has no separate bottom.
            if diagonal {
                break;
            }
        }
        if part.pulley {
            // Its axis across the belt (up for a sideways one): made along z, laid along x or y.
            let dir_x = !sideways && f[2] != 0;
            let lay = center
                * glam::Mat4::from_rotation_y(if dir_x { 90f32.to_radians() } else { 0.0 })
                * glam::Mat4::from_rotation_x(if sideways { 90f32.to_radians() } else { 0.0 })
                * glam::Mat4::from_rotation_x(90f32.to_radians())
                * glam::Mat4::from_translation(glam::Vec3::splat(-0.5));
            if let Some(m) = model(9) {
                self.baked_model_vertices(m, at * own * lay, light, false, out);
            }
        }
    }

    /// `BeltRenderer.renderItems`: every belt's items where the window has them, lying flat (a
    /// block upright), tilted on a slope, off the middle as they lie.
    fn belt_item_geometry(&self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        use super::belts::Slope;
        use horizon_client::block_entity_render::vertex_light;
        for chain in &self.kinetics.belts {
            let Some((_, items)) = self.kinetics.belt_items.get(&chain.start()) else {
                continue;
            };
            if items.is_empty() {
                continue;
            }
            let s = chain.start();
            let corner = glam::DVec3::new(f64::from(s[0]), f64::from(s[1]), f64::from(s[2]));
            if (corner + glam::DVec3::splat(0.5)).distance(camera) > DRAW_DISTANCE / 2.0 {
                continue;
            }
            let f = chain.facing.map(|v| v as f32);
            let base = glam::Mat4::from_translation((corner - camera).as_vec3())
                * glam::Mat4::from_translation(glam::Vec3::new(
                    -0.5 * f[0] + 0.5,
                    15.0 / 16.0,
                    -0.5 * f[2] + 0.5,
                ));
            let length = chain.len() as f32;
            let verticality = match chain.slope {
                Slope::Upward => 1.0,
                Slope::Downward => -1.0,
                _ => 0.0,
            };
            let start = out.len();
            for item in items {
                let offset = item.position;
                let up = if offset < 0.5 {
                    0.0
                } else {
                    verticality * (offset.min(length - 0.5) - 0.5)
                };
                // Off the middle across it: along x where its clockwise turn is.
                let along_x = chain.facing[2] != 0;
                let side = if along_x { item.side } else { -item.side };
                let place = glam::Vec3::new(
                    f[0] * offset + if along_x { side } else { 0.0 },
                    up,
                    f[2] * offset + if along_x { 0.0 } else { side },
                );
                let on_slope =
                    chain.slope != Slope::Horizontal && offset.clamp(0.5, length - 0.5) == offset;
                let forward = ((chain.slope == Slope::Downward)
                    ^ (chain.facing[0] + chain.facing[2] > 0))
                    == (chain.facing[2] != 0);
                let tilt = if on_slope {
                    if forward { -45f32 } else { 45.0 }
                } else {
                    0.0
                };
                let tilt = if chain.facing[0] != 0 {
                    glam::Mat4::from_rotation_z(tilt.to_radians())
                } else {
                    glam::Mat4::from_rotation_x(tilt.to_radians())
                };
                let lift = glam::Mat4::from_translation(glam::Vec3::new(
                    0.0,
                    if on_slope { 1.0 / 8.0 } else { 0.0 },
                    0.0,
                ));
                let segment = chain.position_for(offset.floor() as i32);
                let light = vertex_light(self.world.light(segment[0], segment[1] + 1, segment[2]));
                let pose = base * glam::Mat4::from_translation(place) * tilt * lift;
                self.depot_item(&item.stack, pose, item.angle as f32, light, out);
            }
            cull_back_faces(out, start);
        }
    }

    /// `SawRenderer.renderBlade`: the blade running, reversed or still (`part.models`: the
    /// horizontal ones, then the vertical ones, in that order), made facing south and turned to
    /// the saw's facing; an upright one along x a quarter turn more.
    fn saw_blade_geometry(
        &self,
        state: BlockStateId,
        pos: [i32; 3],
        part: &Part,
        at: glam::Mat4,
        light: u32,
        out: &mut Vec<Vertex>,
    ) {
        let Some(props) = self
            .pack
            .assets
            .registry
            .state(state)
            .map(|s| &s.properties)
        else {
            return;
        };
        let Some(facing) = props.get("facing").and_then(|f| kinetics::direction(f)) else {
            return;
        };
        let speed = self.kinetics.speeds.get(&pos).copied().unwrap_or(0.0);
        let running = if speed > 0.0 {
            0
        } else if speed < 0.0 {
            1
        } else {
            2
        };
        let upright = facing[1] != 0;
        let Some(model) = part.models.get(running + if upright { 3 } else { 0 }) else {
            return;
        };
        let mut turns = kinetics::turns_from([0, 0, 1], facing);
        if upright && props.get("axis_along_first").map(String::as_str) == Some("true") {
            // rotateCentered(90°, UP): three of a variant's quarter turns.
            turns[1] = (turns[1] + 3) % 4;
        }
        let catalog = &self.pack.render.model_catalog;
        if let Some(model) = catalog.partial(model, [turns[0], turns[1], 0]) {
            self.baked_model_vertices(model, at, light, false, out);
        }
    }

    /// `SawRenderer.renderItems`: what a saw facing up holds, on its way across it as it cuts
    /// (the input over the first half, the results over the second).
    fn saw_item_geometry(&self, pos: [i32; 3], camera: glam::DVec3, out: &mut Vec<Vertex>) {
        use super::saws::{CUT, IDLE, SCALE};
        use horizon_assets::item_model::DisplayContext;
        use horizon_client::block_entity_render::vertex_light;
        let corner = glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
        if (corner + glam::DVec3::splat(0.5)).distance(camera) > DRAW_DISTANCE / 2.0 {
            return;
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let Some(props) = self
            .pack
            .assets
            .registry
            .state(state)
            .map(|s| &s.properties)
        else {
            return;
        };
        if props.get("facing").map(String::as_str) != Some("up") {
            return;
        }
        let e = super::machine_entities::read(&self.world, pos, Kind::Saw);
        if e.stacks().next().is_none() {
            return;
        }
        let along_z = props.get("axis_along_first").map(String::as_str) != Some("true");
        let speed = self.kinetics.speeds.get(&pos).copied().unwrap_or(0.0);
        // Its time left, run on from when the server last said (at its speed over 24).
        let elapsed = (self.world.game_time - e.since) as f32;
        let per_tick = speed.abs().clamp(SCALE as f32, (128 * SCALE) as f32);
        let remaining = ((e.timer as f32 - elapsed * per_tick) / SCALE as f32).max(0.0);
        let duration = e.work as f32 / SCALE as f32;
        let partial = self.clock.fraction() as f32;
        let mut offset = if duration != 0.0 && e.running != IDLE {
            remaining / duration
        } else {
            0.0
        };
        if duration != 0.0 && e.running != IDLE {
            let processing = (speed.abs() / 32.0).clamp(1.0, 128.0);
            offset = (offset + ((-partial + 0.5) * processing) / duration).clamp(0.125, 1.0);
            if e.running != CUT {
                offset += 1.0;
            }
            offset /= 2.0;
        }
        if speed == 0.0 {
            offset = 0.5;
        }
        if (speed < 0.0) ^ along_z {
            offset = 1.0 - offset;
        }
        let outputs = e.items.iter().skip(1).flatten().count();
        let light = vertex_light(self.world.light(pos[0], pos[1] + 1, pos[2]));
        let mut base = glam::Mat4::from_translation((corner - camera).as_vec3());
        if along_z {
            base *= glam::Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        }
        base *= glam::Mat4::from_translation(glam::Vec3::new(
            if outputs <= 1 { 0.5 } else { 0.25 },
            0.0,
            offset,
        ));
        base *= glam::Mat4::from_translation(glam::Vec3::new(
            if along_z { -1.0 } else { 0.0 },
            0.0,
            0.0,
        ));
        let start = out.len();
        let mut rendered = 0;
        for (i, stack) in e.items.iter().enumerate() {
            let Some(stack) = stack else {
                continue;
            };
            let model = self.resolve_item(stack, DisplayContext::Fixed, None, 0);
            if model.is_empty() {
                continue;
            }
            let block = self
                .res
                .items()
                .get(&stack.item)
                .is_some_and(|it| it.block.is_some());
            let mut pose = base
                * glam::Mat4::from_translation(glam::Vec3::new(
                    0.0,
                    if block { 0.925 } else { 13.0 / 16.0 },
                    0.0,
                ));
            if i > 0 && outputs > 1 {
                pose *= glam::Mat4::from_translation(glam::Vec3::new(
                    0.5 / (outputs - 1) as f32 * rendered as f32,
                    0.0,
                    0.0,
                ));
            }
            pose *= glam::Mat4::from_scale(glam::Vec3::splat(0.5))
                * glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
            self.submit_item_world(&model, pose, light, out);
            rendered += 1;
        }
        cull_back_faces(out, start);
    }

    /// How far down the mixer at `pos` has its head and pole (blocks), and how far its head has
    /// turned (degrees): `getRenderedHeadOffset` and `getRenderedHeadRotationSpeed`, from its
    /// run since the server last said where it was.
    fn mixer_head(&self, pos: [i32; 3]) -> (f32, f32) {
        use super::basins::{MIX_DOWN, MIX_UP};
        let e = super::machine_entities::read(&self.world, pos, Kind::Mixer);
        let speed = self.kinetics.speeds.get(&pos).copied().unwrap_or(0.0);
        let time = self.world.game_time as f32 + self.clock.fraction() as f32;
        let elapsed = (self.world.game_time - e.since) as f32 + self.clock.fraction() as f32;
        let ticks = match e.running {
            MIX_DOWN => elapsed.clamp(0.0, 20.0),
            MIX_UP => (20.0 + elapsed).clamp(20.0, 40.0),
            _ => -1.0,
        };
        let eased = |t: f32| (2.0 - (t / 20.0 * std::f32::consts::PI).cos()) / 2.0 - 0.5;
        let offset = if ticks < 0.0 {
            0.0
        } else if ticks < 20.0 {
            eased(ticks)
        } else if ticks <= 20.0 {
            1.0
        } else {
            eased(40.0 - ticks)
        };
        let head_speed = match ticks {
            t if t < 0.0 => speed / 2.0,
            t if t < 15.0 => speed,
            t if t <= 20.0 => speed * 2.0,
            _ => speed,
        };
        (
            offset + 7.0 / 16.0,
            (time * head_speed * 0.6).rem_euclid(360.0),
        )
    }

    /// How far down the press at `pos` has its head (blocks): its run since it started, at its
    /// network's speed (`PressingBehaviour.getRenderedHeadOffset` times the mode's
    /// `headOffset`).
    fn press_head_down(&self, pos: [i32; 3]) -> f32 {
        use super::processing::{CYCLE, PRESS_BASIN, PRESS_BELT, press_head, press_rate};
        let e = super::machine_entities::read(&self.world, pos, Kind::Press);
        if e.running == 0 {
            return 0.0;
        }
        let speed = self.kinetics.speeds.get(&pos).copied().unwrap_or(0.0);
        let elapsed = (self.world.game_time - e.since) as f32 + self.clock.fraction() as f32;
        let ticks = (elapsed.max(0.0) * press_rate(speed) as f32).min(CYCLE as f32);
        let reach = match e.running {
            PRESS_BELT => 19.0 / 16.0,
            PRESS_BASIN => 22.0 / 16.0,
            _ => 1.0,
        };
        press_head(ticks) * reach
    }

    /// What the machines near hold, where Create draws it: a depot's stack lying on its top
    /// and its outputs around it (DepotRenderer).
    fn machine_item_geometry(&self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        use horizon_client::block_entity_render::vertex_light;
        for (pos, kind) in &self.kinetics.machines {
            if *kind == Kind::Basin {
                self.basin_item_geometry(*pos, camera, out);
                continue;
            }
            if *kind == Kind::Saw {
                self.saw_item_geometry(*pos, camera, out);
                continue;
            }
            if *kind != Kind::Depot {
                continue;
            }
            let corner = glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
            if (corner + glam::DVec3::splat(0.5)).distance(camera) > DRAW_DISTANCE / 2.0 {
                continue;
            }
            let e = super::machine_entities::read(&self.world, *pos, *kind);
            if e.stacks().next().is_none() {
                continue;
            }
            // The light above its top (the depot's own block is solid).
            let light = vertex_light(self.world.light(pos[0], pos[1] + 1, pos[2]));
            let top = glam::Mat4::from_translation((corner - camera).as_vec3())
                * glam::Mat4::from_translation(glam::Vec3::new(0.5, 15.0 / 16.0, 0.5));
            let start = out.len();
            for (slot, stack) in e.items.iter().enumerate() {
                let Some(stack) = stack else {
                    continue;
                };
                // The held stack in the middle; output i an eighth of a turn round from the last,
                // a little out.
                let (place, angle) = if slot == 0 {
                    (glam::Mat4::IDENTITY, 0.0)
                } else {
                    let i = (slot - 1) as f32;
                    let around = glam::Mat4::from_rotation_y((45.0 * i).to_radians())
                        * glam::Mat4::from_translation(glam::Vec3::new(0.35, 0.0, 0.0));
                    (around, (slot as f32 * 137.5) % 360.0)
                };
                self.depot_item(stack, top * place, angle, light, out);
            }
            cull_back_faces(out, start);
        }
    }

    /// What a basin holds (BasinRenderer): its stacks lying about its floor, turned each its
    /// own way, the outputs over the inputs.
    fn basin_item_geometry(&self, pos: [i32; 3], camera: glam::DVec3, out: &mut Vec<Vertex>) {
        use horizon_client::block_entity_render::vertex_light;
        let corner = glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
        if (corner + glam::DVec3::splat(0.5)).distance(camera) > DRAW_DISTANCE / 2.0 {
            return;
        }
        let e = super::machine_entities::read(&self.world, pos, Kind::Basin);
        if e.stacks().next().is_none() {
            return;
        }
        let light = vertex_light(self.world.light(pos[0], pos[1] + 1, pos[2]));
        let base = glam::Mat4::from_translation((corner - camera).as_vec3());
        let start = out.len();
        for (n, (slot, stack)) in e
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((i, s.as_ref()?)))
            .enumerate()
        {
            // A spiral out from the middle, a little higher for each, as Create piles them.
            let angle = n as f32 * 137.5;
            let radius = if n == 0 {
                0.0
            } else {
                0.22 + 0.04 * (n % 3) as f32
            };
            let (x, z) = (
                0.5 + radius * angle.to_radians().cos(),
                0.5 + radius * angle.to_radians().sin(),
            );
            let y = 5.0 / 16.0 + 0.02 * n as f32 + if slot >= 9 { 0.1 } else { 0.0 };
            let place = base * glam::Mat4::from_translation(glam::Vec3::new(x, y, z));
            self.depot_item(stack, place, angle, light, out);
        }
        cull_back_faces(out, start);
    }

    /// `DepotRenderer.renderItem`: a stack lying flat (a block upright), more copies the more
    /// it holds, turned `angle` degrees.
    fn depot_item(
        &self,
        stack: &horizon_client::items::ItemStack,
        pose: glam::Mat4,
        angle: f32,
        light: u32,
        out: &mut Vec<Vertex>,
    ) {
        use horizon_assets::item_model::DisplayContext;
        let state = self.resolve_item(stack, DisplayContext::Fixed, None, 0);
        if state.is_empty() {
            return;
        }
        let block = self
            .res
            .items()
            .get(&stack.item)
            .is_some_and(|i| i.block.is_some());
        let copies = (stack.count.max(1).ilog2() / 2) as usize;
        let mut pose = pose * glam::Mat4::from_rotation_y(angle.to_radians());
        for i in 0..=copies {
            let spread = if block {
                // A fixed step per copy where Create draws one from a random seeded by nothing.
                let s = 0.0625 * i as f32 * 0.5;
                glam::Mat4::from_translation(glam::Vec3::new(s, 0.0, s))
            } else {
                glam::Mat4::IDENTITY
            };
            let mut copy = pose * spread * glam::Mat4::from_scale(glam::Vec3::splat(0.5));
            if !block {
                copy = copy
                    * glam::Mat4::from_translation(glam::Vec3::new(0.0, -3.0 / 16.0, 0.0))
                    * glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
            }
            self.submit_item_world(&state, copy, light, out);
            if !block {
                pose *= glam::Mat4::from_rotation_y(10f32.to_radians());
            }
            pose *= glam::Mat4::from_translation(glam::Vec3::new(
                0.0,
                if block { 1.0 / 64.0 } else { 1.0 / 16.0 },
                0.0,
            ));
        }
    }
}

/// The axis a part's own model turns about and how its run turns there: a shaft-like part's
/// about its axis, a motor's and a crank's by their face's sign.
fn own_way(part: &Part) -> (usize, f32) {
    match part.kind {
        KineticPart::Motor | KineticPart::Crank | KineticPart::Drill | KineticPart::Fan => {
            kinetics::at_face(part.face, 1.0)
        }
        KineticPart::Saw if part.face != [0; 3] => kinetics::at_face(part.face, 1.0),
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
