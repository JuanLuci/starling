//! Lazy block collision selection in vanilla cursor/query order. Providers own
//! chunk availability, state metadata, suffocation predicates and context dispatch.
use crate::{collision_context::CollisionContext, registry::BlockStateId};
use horizon_core::{
    geometry::{Aabb, Collider, MovedBlock, ShapeError, ShapeOp, Vec3, VoxelShape},
    position::{BlockPos, ChunkPos},
};

#[derive(Clone, Copy, Debug)]
pub struct CollisionBlock {
    pub state: BlockStateId,
    pub large_collision_shape: bool,
    pub moving_piston: bool,
    /// The shape is empty under every context (air, plants): a scan skips
    /// the block without asking for its shape. Providers whose shapes come
    /// from elsewhere leave it false.
    pub empty_shape: bool,
    /// The shape is the unit cube under every context (stone, dirt): a scan
    /// collides with it without asking for its shape. Providers whose shapes
    /// come from elsewhere leave it false.
    pub full_block: bool,
}

pub trait BlockCollisionWorld {
    type Error;
    type Chunk: Copy;
    /// None is a missing collision chunk; native iteration retries missing chunks.
    fn collision_chunk(&mut self, position: ChunkPos) -> Result<Option<Self::Chunk>, Self::Error>;
    fn collision_block(
        &mut self,
        chunk: Self::Chunk,
        position: BlockPos,
    ) -> Result<CollisionBlock, Self::Error>;
    fn is_suffocating(
        &mut self,
        chunk: Self::Chunk,
        block: CollisionBlock,
        position: BlockPos,
    ) -> Result<bool, Self::Error>;
    /// Whether the 16-block section `section` (`y >> 4`) of `chunk` may hold
    /// a block that collides outside its own cell: a large collision shape
    /// or a moving piston. The cells a scan visits only for those skip their
    /// block reads where none can be. A provider that can't tell keeps true.
    fn section_may_reach(
        &mut self,
        _chunk: Self::Chunk,
        _section: i32,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
    /// Query against the global collision world, not the cached chunk provider.
    /// Shapes must preserve canonical block identity (including imported shapes).
    fn context_shape(
        &mut self,
        block: CollisionBlock,
        position: BlockPos,
        context: &CollisionContext<'_>,
    ) -> Result<VoxelShape, Self::Error>;
}

#[derive(Debug)]
pub enum CollisionScanError<E> {
    InvalidBounds,
    TooLarge,
    VisitBudget,
    World(E),
    Shape(ShapeError),
}
impl<E: std::fmt::Display> std::fmt::Display for CollisionScanError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBounds => f.write_str("collision scan bounds must be finite"),
            Self::TooLarge => {
                f.write_str("collision scan exceeds the positive Java-int cursor domain")
            }
            Self::VisitBudget => f.write_str("collision scan candidate budget exhausted"),
            Self::World(error) => error.fmt(f),
            Self::Shape(error) => error.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for CollisionScanError<E> {}

#[derive(Debug)]
pub struct BlockCollisionHit {
    pub position: BlockPos,
    pub shape: VoxelShape,
}

pub struct BlockCollisionScan<'w, 'c, W: BlockCollisionWorld> {
    world: &'w mut W,
    context: CollisionContext<'c>,
    bounds: Aabb,
    /// `Shapes.create(box)`, built the first time a block's shape is not
    /// the unit cube (finite bounds always make one).
    entity_shape: Option<VoxelShape>,
    only_suffocating: bool,
    start: BlockPos,
    sizes: [usize; 3],
    index: usize,
    /// The cell at `index`, relative to `start` (x fastest, then y, then z).
    cursor: [usize; 3],
    count: usize,
    budget: usize,
    chunk: Option<(ChunkPos, W::Chunk)>,
    /// The last section asked [`BlockCollisionWorld::section_may_reach`].
    section: Option<(ChunkPos, i32, bool)>,
    failed: bool,
}
impl<'w, 'c, W: BlockCollisionWorld> BlockCollisionScan<'w, 'c, W> {
    /// Finite bounds and a positive-Java-int candidate volume are supported.
    /// Budget counts every cursor cell, including skipped corners. After an error
    /// iteration terminates; already performed queries are not rolled back.
    pub fn new(
        world: &'w mut W,
        context: CollisionContext<'c>,
        bounds: Aabb,
        only_suffocating: bool,
        candidate_budget: usize,
    ) -> Result<Self, CollisionScanError<W::Error>> {
        let min = bounds.min_position();
        let max = bounds.max_position();
        if !min.is_finite() || !max.is_finite() {
            return Err(CollisionScanError::InvalidBounds);
        }
        let start = BlockPos::containing(Vec3::new(min.x - 1.0e-7, min.y - 1.0e-7, min.z - 1.0e-7))
            .offset(-1, -1, -1);
        let end = BlockPos::containing(Vec3::new(max.x + 1.0e-7, max.y + 1.0e-7, max.z + 1.0e-7))
            .offset(1, 1, 1);
        let sizes = [
            end.x.wrapping_sub(start.x).wrapping_add(1),
            end.y.wrapping_sub(start.y).wrapping_add(1),
            end.z.wrapping_sub(start.z).wrapping_add(1),
        ];
        if sizes.iter().any(|size| *size <= 0) {
            return Err(CollisionScanError::TooLarge);
        }
        let count = sizes
            .into_iter()
            .try_fold(1u64, |n, size| {
                n.checked_mul(size as u64).filter(|n| *n <= i32::MAX as u64)
            })
            .ok_or(CollisionScanError::TooLarge)? as usize;
        Ok(Self {
            world,
            context,
            bounds,
            entity_shape: None,
            only_suffocating,
            start,
            sizes: sizes.map(|s| s as usize),
            index: 0,
            cursor: [0; 3],
            count,
            budget: candidate_budget,
            chunk: None,
            section: None,
            failed: false,
        })
    }

    /// The next hit as a collider: a full block stays a [`MovedBlock`]
    /// instead of the moved shape the iterator builds for it.
    pub fn next_collider(
        &mut self,
    ) -> Option<Result<(BlockPos, Collider), CollisionScanError<W::Error>>> {
        if self.failed {
            return None;
        }
        match self.find_next() {
            Ok(hit) => hit.map(Ok),
            Err(error) => {
                self.failed = true;
                Some(Err(error))
            }
        }
    }
    fn find_next(&mut self) -> Result<Option<(BlockPos, Collider)>, CollisionScanError<W::Error>> {
        while self.index < self.count {
            if self.index >= self.budget {
                return Err(CollisionScanError::VisitBudget);
            }
            let [width, height, _] = self.sizes;
            let relative = self.cursor;
            self.index += 1;
            self.cursor[0] += 1;
            if self.cursor[0] == width {
                self.cursor[0] = 0;
                self.cursor[1] += 1;
                if self.cursor[1] == height {
                    self.cursor[1] = 0;
                    self.cursor[2] += 1;
                }
            }
            let kind = relative
                .into_iter()
                .zip(self.sizes)
                .filter(|(p, size)| *p == 0 || *p == size - 1)
                .count();
            if kind == 3 {
                continue;
            }
            let p = self
                .start
                .offset(relative[0] as i32, relative[1] as i32, relative[2] as i32);
            let chunk_position = p.chunk();
            let chunk = match self.chunk {
                Some((at, chunk)) if at == chunk_position => Some(chunk),
                _ => {
                    let chunk = self
                        .world
                        .collision_chunk(chunk_position)
                        .map_err(CollisionScanError::World)?;
                    self.chunk = chunk.map(|chunk| (chunk_position, chunk));
                    chunk
                }
            };
            let Some(chunk) = chunk else {
                continue;
            };
            // Off the box's own cells only blocks reaching out of theirs
            // collide (below); a section with none needs no reads there.
            if kind != 0 {
                let section = p.y >> 4;
                let reaches = match self.section {
                    Some((at, y, reaches)) if at == chunk_position && y == section => reaches,
                    _ => {
                        let reaches = self
                            .world
                            .section_may_reach(chunk, section)
                            .map_err(CollisionScanError::World)?;
                        self.section = Some((chunk_position, section, reaches));
                        reaches
                    }
                };
                if !reaches {
                    continue;
                }
            }
            let block = self
                .world
                .collision_block(chunk, p)
                .map_err(CollisionScanError::World)?;
            if self.only_suffocating
                && !self
                    .world
                    .is_suffocating(chunk, block, p)
                    .map_err(CollisionScanError::World)?
            {
                continue;
            }
            if kind == 1 && !block.large_collision_shape || kind == 2 && !block.moving_piston {
                continue;
            }
            if block.empty_shape {
                continue;
            }
            let shape = if block.full_block {
                None
            } else {
                let shape = self
                    .world
                    .context_shape(block, p, &self.context)
                    .map_err(CollisionScanError::World)?;
                (!shape.is_block()).then_some(shape)
            };
            let offset = Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z));
            let moved = if let Some(shape) = shape {
                // Shapes whose bounds miss the box share no volume with it:
                // joinIsNotEmpty(AND) is false without moving them.
                if !shape
                    .bounds()
                    .is_some_and(|b| b.move_by(offset).intersects(self.bounds))
                {
                    continue;
                }
                let moved = shape.moved(offset);
                let entity_shape = match &self.entity_shape {
                    Some(shape) => shape,
                    None => self.entity_shape.insert(
                        VoxelShape::from_box(self.bounds).map_err(CollisionScanError::Shape)?,
                    ),
                };
                if moved.is_empty()
                    || !moved
                        .join_is_not_empty(entity_shape, ShapeOp::AND)
                        .map_err(CollisionScanError::Shape)?
                {
                    continue;
                }
                Collider::Shape(moved)
            } else {
                // Native adds one in floating point, including at i32::MAX.
                let top = Vec3::new(offset.x + 1.0, offset.y + 1.0, offset.z + 1.0);
                if !self.bounds.intersects_corners(offset, top) {
                    continue;
                }
                Collider::Block(MovedBlock::new(offset))
            };
            return Ok(Some((p, moved)));
        }
        Ok(None)
    }
}
impl<W: BlockCollisionWorld> Iterator for BlockCollisionScan<'_, '_, W> {
    type Item = Result<BlockCollisionHit, CollisionScanError<W::Error>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_collider().map(|hit| {
            hit.map(|(position, collider)| BlockCollisionHit {
                position,
                shape: collider.into_shape(),
            })
        })
    }
}
impl<W: BlockCollisionWorld> std::iter::FusedIterator for BlockCollisionScan<'_, '_, W> {}
