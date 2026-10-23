//! Memoized palette eligibility for the game thread's random-tick snapshot.
use crate::{
    blocks::BlockFacts,
    chunk::{Chunk, MIN_Y, SECTIONS, Section},
    fast_hash::FxHashMap,
};
use horizon_world::registry::BlockStateId;
use std::sync::{Arc, Weak};

/// A value snapshot, in ascending section order, with no per-chunk allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sections(u32);
impl Iterator for Sections {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        Some(MIN_Y + index as i32 * 16)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count_ones() as usize;
        (len, Some(len))
    }
}
impl ExactSizeIterator for Sections {}

struct Cached {
    chunk: Weak<Chunk>,
    sections: Sections,
}
/// Where one version of a section holds states that take random ticks.
struct SectionPositions {
    section: Weak<Section>,
    /// By `chunk::index`.
    bits: Box<[u64; 64]>,
}
impl SectionPositions {
    fn new(section: &Arc<Section>, ticks: &impl Fn(u32) -> bool) -> Self {
        let mut bits = Box::new([0; 64]);
        let ticking: Option<Vec<u32>> = section
            .blocks
            .palette_values()
            .map(|values| values.iter().copied().filter(|&v| ticks(v)).collect());
        if !ticking.as_ref().is_some_and(Vec::is_empty) {
            let mut states = [0; 4096];
            section.unpack(&mut states);
            for (i, &state) in states.iter().enumerate() {
                let hit = match &ticking {
                    Some(ticking) => ticking.contains(&state),
                    None => ticks(state),
                };
                if hit {
                    bits[i / 64] |= 1 << (i % 64);
                }
            }
        }
        Self {
            section: Arc::downgrade(section),
            bits,
        }
    }
    /// Bit `w` set when `bits[w]` holds any position.
    fn summary(&self) -> u64 {
        self.bits
            .iter()
            .enumerate()
            .fold(0, |summary, (w, &bits)| summary | u64::from(bits != 0) << w)
    }
}
/// A chunk's [`SectionPositions`], found as its sections are asked about.
/// The summaries sit together: most picks need nothing else.
struct ChunkPositions {
    key: (i32, i32),
    /// The version the sections were checked against.
    chunk: Weak<Chunk>,
    /// The sections found, by bit.
    known: u32,
    summaries: [u64; SECTIONS],
    sections: [Option<SectionPositions>; SECTIONS],
}
/// Resident chunks are immutable behind `Arc`; edits use `Arc::make_mut`.
/// A weak reference detects both edits and replacement, without retaining block
/// data or forcing a deep copy on the next edit. It also prevents address reuse.
/// Clear this cache whenever the gameplay facts change (including resource reload).
#[derive(Default)]
pub struct RandomTickSections {
    chunks: FxHashMap<(i32, i32), Cached>,
    positions: Vec<ChunkPositions>,
    position_index: FxHashMap<(i32, i32), usize>,
    /// The chunk of the last [`Self::may_tick`]: a chunk's picks come in a row.
    last: Option<usize>,
}
impl RandomTickSections {
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.positions.clear();
        self.position_index.clear();
        self.last = None;
    }
    pub fn remove(&mut self, key: &(i32, i32)) {
        self.chunks.remove(key);
        if let Some(i) = self.position_index.remove(key) {
            self.positions.swap_remove(i);
            if let Some(moved) = self.positions.get(i) {
                self.position_index.insert(moved.key, i);
            }
            self.last = None;
        }
    }
    /// Whether the block at `(x, y, z)` of `chunk` (at `key`) may take a
    /// random tick: false only when its section, as it is now, holds no
    /// state `ticks` accepts there. Each version of a section is scanned
    /// once; the same `ticks` must be passed until the cache is cleared.
    pub fn may_tick(
        &mut self,
        key: (i32, i32),
        chunk: &Arc<Chunk>,
        x: i32,
        y: i32,
        z: i32,
        ticks: impl Fn(u32) -> bool,
    ) -> bool {
        let Some(s) = usize::try_from((y - MIN_Y) >> 4)
            .ok()
            .filter(|&s| s < chunk.sections.len())
        else {
            return true;
        };
        let i = match self.last {
            Some(i) if self.positions[i].key == key => i,
            _ => {
                let positions = &mut self.positions;
                let i = *self.position_index.entry(key).or_insert_with(|| {
                    positions.push(ChunkPositions {
                        key,
                        chunk: Weak::new(),
                        known: 0,
                        summaries: [0; SECTIONS],
                        sections: std::array::from_fn(|_| None),
                    });
                    positions.len() - 1
                });
                self.last = Some(i);
                i
            }
        };
        let positions = &mut self.positions[i];
        // An edit replaces the chunk (`Arc::make_mut` with this weak one):
        // keep the sections it still shares.
        if positions.chunk.as_ptr() != Arc::as_ptr(chunk) {
            for (s, section) in chunk.sections.iter().enumerate() {
                if positions.sections[s]
                    .as_ref()
                    .is_some_and(|p| p.section.as_ptr() != Arc::as_ptr(section))
                {
                    positions.known &= !(1 << s);
                }
            }
            positions.chunk = Arc::downgrade(chunk);
        }
        if positions.known & 1 << s == 0 {
            let found = SectionPositions::new(&chunk.sections[s], &ticks);
            positions.summaries[s] = found.summary();
            positions.sections[s] = Some(found);
            positions.known |= 1 << s;
        }
        let i = crate::chunk::index(x, y, z);
        positions.summaries[s] & 1 << (i / 64) != 0
            && positions.sections[s]
                .as_ref()
                .is_some_and(|p| p.bits[i / 64] & 1 << (i % 64) != 0)
    }
    pub fn get(&mut self, key: (i32, i32), chunk: &Arc<Chunk>, facts: &BlockFacts) -> Sections {
        self.get_with(key, chunk, |id| facts.get(BlockStateId(id)).random_ticks)
    }
    fn get_with(
        &mut self,
        key: (i32, i32),
        chunk: &Arc<Chunk>,
        mut random: impl FnMut(u32) -> bool,
    ) -> Sections {
        const {
            assert!(SECTIONS <= u32::BITS as usize);
        }
        let cached = self.chunks.entry(key).or_insert_with(|| Cached {
            chunk: Weak::new(),
            sections: Sections::default(),
        });
        if cached.chunk.as_ptr() != Arc::as_ptr(chunk) {
            let mut mask = 0;
            for (i, section) in chunk.sections.iter().enumerate() {
                // Preserve maybe_has, including unused entries and global palettes.
                if section.blocks.maybe_has(&mut random) {
                    mask |= 1 << i;
                }
            }
            cached.sections = Sections(mask);
            cached.chunk = Arc::downgrade(chunk);
        }
        cached.sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Section;

    #[test]
    fn edits_replacements_compaction_and_reload() {
        let mut cache = RandomTickSections::default();
        let mut chunk = Arc::new(Chunk::empty(0, 0));
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 1).len(), 0);
        cache.get_with((0, 0), &chunk, |_| panic!("unchanged chunk rescanned"));
        let worker = chunk.clone();
        Arc::make_mut(&mut chunk).set(0, MIN_Y, 0, 1);
        assert_eq!(
            cache
                .get_with((0, 0), &chunk, |id| id == 1)
                .collect::<Vec<_>>(),
            [MIN_Y]
        );
        assert_eq!(worker.get(0, MIN_Y, 0), 0);
        // Even after the last ticking block is removed, its unused palette entry counts.
        Arc::make_mut(&mut chunk).set(0, MIN_Y, 0, 0);
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 1).len(), 1);
        Arc::make_mut(&mut Arc::make_mut(&mut chunk).sections[0]).compact();
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 1).len(), 0);
        Arc::make_mut(&mut chunk).sections[SECTIONS - 1] = Arc::new(Section::default());
        cache.clear(); // resource reload, with changed facts for the same state IDs
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 0).len(), SECTIONS);
        cache.remove(&(0, 0));
        assert!(cache.chunks.is_empty());
    }

    #[test]
    fn ticking_positions_follow_edits() {
        let mut cache = RandomTickSections::default();
        let mut chunk = Arc::new(Chunk::empty(1, -1));
        let ticks = |id: u32| id == 7;
        let at = |cache: &mut RandomTickSections, chunk: &Arc<Chunk>, x, y, z| {
            cache.may_tick((1, -1), chunk, x, y, z, ticks)
        };
        assert!(!at(&mut cache, &chunk, 3, 70, 9));
        Arc::make_mut(&mut chunk).set(3, 70, 9, 7);
        assert!(at(&mut cache, &chunk, 3, 70, 9));
        assert!(!at(&mut cache, &chunk, 4, 70, 9));
        assert!(!at(&mut cache, &chunk, 3, 54, 9));
        // A held copy keeps its version: the worker still has the block.
        let worker = chunk.clone();
        Arc::make_mut(&mut chunk).set(3, 70, 9, 2);
        Arc::make_mut(&mut chunk).set(15, 79, 0, 7);
        assert!(!at(&mut cache, &chunk, 3, 70, 9));
        assert!(at(&mut cache, &chunk, 15, 79, 0));
        assert!(at(&mut cache, &worker, 3, 70, 9));
        // Outside the height there is no section to know about.
        assert!(at(&mut cache, &chunk, 0, MIN_Y - 1, 0));
        cache.remove(&(1, -1));
        assert!(cache.positions.is_empty() && cache.position_index.is_empty());
    }

    #[test]
    fn replacement_at_same_coordinates_and_early_snapshot() {
        let mut cache = RandomTickSections::default();
        let mut chunk = Arc::new(Chunk::empty(-2, 3));
        let before_scheduled = cache.get_with((-2, 3), &chunk, |id| id == 1);
        Arc::make_mut(&mut chunk).set(0, MIN_Y + 16, 0, 1);
        assert_eq!(before_scheduled.len(), 0);
        assert_eq!(
            cache
                .get_with((-2, 3), &chunk, |id| id == 1)
                .collect::<Vec<_>>(),
            [MIN_Y + 16]
        );
        let weak = Arc::downgrade(&chunk);
        drop(chunk);
        assert!(weak.upgrade().is_none()); // cache does not retain unloaded block data
        let replacement = Arc::new(Chunk::empty(-2, 3));
        assert_eq!(cache.get_with((-2, 3), &replacement, |id| id == 1).len(), 0);
    }

    #[test]
    fn global_palette_and_ascending_order() {
        let mut chunk = Chunk::empty(0, 0);
        for i in [0, 9, SECTIONS - 1] {
            Arc::make_mut(&mut chunk.sections[i]).blocks =
                Section::from_blocks(&std::array::from_fn(|j| (j % 300) as u32));
        }
        let chunk = Arc::new(chunk);
        let mut cache = RandomTickSections::default();
        // Global maybe_has considers the entire ID range, including absent states.
        assert_eq!(
            cache
                .get_with((0, 0), &chunk, |id| id == 40000)
                .collect::<Vec<_>>(),
            [MIN_Y, MIN_Y + 9 * 16, MIN_Y + (SECTIONS as i32 - 1) * 16]
        );
    }
}

#[cfg(test)]
mod tick_parity {
    use super::*;
    use horizon_core::{
        position::{BlockPos, ChunkPos},
        random::{LegacyRandom, RandomSource},
    };
    use horizon_world::level::{LevelAccess, LevelEvent, Ticks, data};
    use std::{cell::RefCell, path::PathBuf};

    #[derive(Clone)]
    struct Level {
        chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
        random: LegacyRandom,
        reads: RefCell<Vec<BlockPos>>,
        events: Vec<LevelEvent>,
        time: i64,
        speed: i32,
    }
    impl LevelAccess for Level {
        fn block_state(&self, pos: BlockPos) -> BlockStateId {
            self.reads.borrow_mut().push(pos);
            self.chunks
                .get(&(pos.x >> 4, pos.z >> 4))
                .map_or(BlockStateId(0), |c| c.block(pos.x, pos.y, pos.z))
        }
        fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
            let chunk = self.chunks.get_mut(&(pos.x >> 4, pos.z >> 4))?;
            Some(BlockStateId(
                Arc::make_mut(chunk).set(pos.x, pos.y, pos.z, state.0),
            ))
        }
        fn is_loaded(&self, pos: BlockPos) -> bool {
            self.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
        }
        fn random(&mut self) -> &mut LegacyRandom {
            &mut self.random
        }
        fn game_time(&self) -> i64 {
            self.time
        }
        fn game_rules(&self) -> horizon_world::level::GameRules {
            horizon_world::level::GameRules {
                random_tick_speed: self.speed,
                ..Default::default()
            }
        }
        fn emit(&mut self, event: LevelEvent) {
            self.events.push(event);
        }
    }

    /// The level answering `may_random_tick` from the cache's positions.
    struct Hinted<'b> {
        level: Level,
        cache: RandomTickSections,
        behaviours: &'b horizon_world::level::Behaviours,
    }
    impl LevelAccess for Hinted<'_> {
        fn block_state(&self, pos: BlockPos) -> BlockStateId {
            self.level.block_state(pos)
        }
        fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
            self.level.set_raw(pos, state)
        }
        fn is_loaded(&self, pos: BlockPos) -> bool {
            self.level.is_loaded(pos)
        }
        fn random(&mut self) -> &mut LegacyRandom {
            self.level.random()
        }
        fn game_time(&self) -> i64 {
            self.level.game_time()
        }
        fn game_rules(&self) -> horizon_world::level::GameRules {
            self.level.game_rules()
        }
        fn emit(&mut self, event: LevelEvent) {
            self.level.emit(event);
        }
        fn may_random_tick(&mut self, pos: BlockPos) -> bool {
            let key = (pos.x >> 4, pos.z >> 4);
            let Some(chunk) = self.level.chunks.get(&key) else {
                return true;
            };
            let b = self.behaviours;
            self.cache.may_tick(key, chunk, pos.x, pos.y, pos.z, |id| {
                b.takes_random_tick(BlockStateId(id))
            })
        }
    }

    #[test]
    fn actual_random_ticks_match_original_scan() {
        let root = std::env::var_os("HORIZON_RESOURCES")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let local = manifest
                    .join("../../.local/horizon-client")
                    .join(horizon_core::VANILLA_VERSION);
                if local.join("gameplay-facts.json").exists() {
                    local
                } else {
                    // The branch worktree lives below the main checkout.
                    manifest
                        .join("../../../../.local/horizon-client")
                        .join(horizon_core::VANILLA_VERSION)
                }
            });
        if !root.join("gameplay-facts.json").exists() {
            eprintln!(
                "skipping resource-backed random-tick parity: {}",
                root.display()
            );
            return;
        }
        let registry = data::load_registry(&root).unwrap();
        let (mut facts, behaviours) = BlockFacts::load_pair(&root, &registry).unwrap();
        let grass = registry.default_state("minecraft:grass_block").unwrap();
        let chunk = |x| {
            let mut c = Chunk::empty(x, 0);
            c.lit = true;
            // Every sampled block can run real behaviour and change to dirt.
            for i in [4, 7] {
                Arc::make_mut(&mut c.sections[i]).blocks =
                    crate::chunk::Section::from_blocks(&[grass.0; 4096]);
            }
            Arc::new(c)
        };
        let mut original = Level {
            chunks: [
                ((-1, 0), chunk(-1)),
                ((0, 0), chunk(0)),
                ((1, 0), Arc::new(Chunk::empty(1, 0))),
            ]
            .into_iter()
            .collect(),
            random: LegacyRandom::new(42),
            reads: RefCell::default(),
            events: Vec::new(),
            time: 0,
            speed: 3,
        };
        Arc::make_mut(original.chunks.get_mut(&(1, 0)).unwrap()).lit = true; // precipitation-only chunk
        let mut cached = original.clone();
        let mut cache = RandomTickSections::default();
        let mut hinted = Hinted {
            level: original.clone(),
            cache: RandomTickSections::default(),
            behaviours: &behaviours,
        };
        let (mut a, mut b, mut c) = (Ticks::new(12345), Ticks::new(12345), Ticks::new(12345));
        for x in -1..=1 {
            a.add_empty_chunk(ChunkPos::new(x, 0)).unwrap();
            b.add_empty_chunk(ChunkPos::new(x, 0)).unwrap();
            c.add_empty_chunk(ChunkPos::new(x, 0)).unwrap();
        }
        let (mut reads, mut hinted_reads) = (0, 0);
        for step in 0..128 {
            for level in [&mut original, &mut cached, &mut hinted.level] {
                level.time = step;
                level.speed = [0, 1, 3, 9][step as usize % 4];
                level.reads.borrow_mut().clear();
                level.events.clear();
                match step {
                    12 => {
                        level.chunks.remove(&(-1, 0));
                    }
                    13 => {
                        level.chunks.insert((-1, 0), chunk(-1));
                    }
                    30 => {
                        Arc::make_mut(level.chunks.get_mut(&(0, 0)).unwrap()).sections[7] =
                            Arc::new(crate::chunk::Section::default());
                    }
                    40 | 41 => {
                        Arc::make_mut(level.chunks.get_mut(&(0, 0)).unwrap()).lit = step == 41;
                    }
                    _ => {}
                }
            }
            if step == 12 {
                cache.remove(&(-1, 0));
                hinted.cache.remove(&(-1, 0));
            }
            if step == 50 || step == 60 {
                facts.states[grass.0 as usize].random_ticks = step == 60;
                cache.clear();
            }
            let old: Vec<_> = original
                .chunks
                .iter()
                .filter(|(_, c)| c.lit)
                .map(|(&key, c)| {
                    let sections: Vec<_> = c
                        .sections
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| {
                            s.blocks
                                .maybe_has(|id| facts.get(BlockStateId(id)).random_ticks)
                        })
                        .map(|(i, _)| MIN_Y + i as i32 * 16)
                        .collect();
                    (key, sections)
                })
                .collect();
            let new: Vec<_> = cached
                .chunks
                .iter()
                .filter(|(_, c)| c.lit)
                .map(|(&key, c)| (key, cache.get(key, c, &facts)))
                .collect();
            assert_eq!(
                old,
                new.iter()
                    .map(|(k, s)| (*k, s.collect::<Vec<_>>()))
                    .collect::<Vec<_>>()
            );
            // Stand in for an inside-block/scheduled write AFTER eligibility capture.
            if step == 20 {
                for level in [&mut original, &mut cached, &mut hinted.level] {
                    level.set_raw(BlockPos::new(0, MIN_Y + 16, 0), grass);
                }
            }
            for ((x, z), sections) in old {
                behaviours.tick_chunk_random(&mut original, &mut a, ChunkPos::new(x, z), sections);
            }
            for &((x, z), sections) in &new {
                behaviours.tick_chunk_random(&mut cached, &mut b, ChunkPos::new(x, z), sections);
                // Skipping the reads of blocks that do nothing changes nothing else.
                behaviours.tick_chunk_random(&mut hinted, &mut c, ChunkPos::new(x, z), sections);
            }
            assert_eq!(original.reads, cached.reads, "ordered reads at tick {step}");
            reads += original.reads.borrow().len();
            hinted_reads += hinted.level.reads.borrow().len();
            assert_eq!(original.events, cached.events, "events at tick {step}");
            assert_eq!(
                original.events, hinted.level.events,
                "hinted events at tick {step}"
            );
            assert_eq!(
                a.random_tick_value, b.random_tick_value,
                "position RNG at tick {step}"
            );
            assert_eq!(
                a.random_tick_value, c.random_tick_value,
                "hinted position RNG at tick {step}"
            );
            let next = original.random.next_long();
            assert_eq!(next, cached.random.next_long(), "level RNG at tick {step}");
            assert_eq!(
                next,
                hinted.level.random.next_long(),
                "hinted RNG at tick {step}"
            );
        }
        let dirt = registry.default_state("minecraft:dirt").unwrap();
        assert!(
            original
                .chunks
                .values()
                .any(|c| c.sections[4].blocks.maybe_has(|id| id == dirt.0))
        );
        for (key, c) in &original.chunks {
            for (s, t) in c.sections.iter().zip(&cached.chunks[key].sections) {
                assert_eq!(s.blocks, t.blocks);
            }
            for (s, t) in c.sections.iter().zip(&hinted.level.chunks[key].sections) {
                assert_eq!(s.blocks, t.blocks);
            }
        }
        // Blocks that do nothing went unread.
        assert!(hinted_reads < reads, "{hinted_reads} of {reads}");
    }
}
