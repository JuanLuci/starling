//! SynchedEntityData values (ClientboundSetEntityDataPacket): an entry index,
//! the EntityDataSerializers id, and the serializer's value, ending with 0xFF.
//! Every 26.3 serializer is decoded; particles and resolvable profiles are
//! kept as the exact bytes they were sent as. A serializer id, particle type
//! or vibration position source type past the 26.3 registries has no known
//! length, so it ends the list early; the entries before it are kept.
use crate::{
    Result,
    buf::{Codec, Reader, Writer},
    item::{Slot, Stack, skip_profile, skip_template},
};
use horizon_world::nbt::Tag;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(i8),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    Text(Tag),
    OptText(Option<Tag>),
    Item(Option<Stack>),
    Bool(bool),
    Floats(Vec<f32>),
    Pos([i32; 3]),
    OptPos(Option<[i32; 3]>),
    OptUuid(Option<u128>),
    /// Any VarInt-valued serializer (direction, block state, pose, variants ...).
    Id(i32),
    /// Several VarInts (VillagerData).
    Ids(Vec<i32>),
    /// Optional GlobalPos: the dimension Identifier and the block.
    OptGlobalPos(Option<(String, [i32; 3])>),
    /// Exactly the bytes the serializer's codec consumed, re-sent as is
    /// (PARTICLE, PARTICLES, RESOLVABLE_PROFILE).
    Raw(Vec<u8>),
}
/// Serializer ids in EntityDataSerializers registration order.
pub mod serializer {
    pub const BYTE: i32 = 0;
    pub const INT: i32 = 1;
    pub const LONG: i32 = 2;
    pub const FLOAT: i32 = 3;
    pub const STRING: i32 = 4;
    pub const COMPONENT: i32 = 5;
    pub const OPTIONAL_COMPONENT: i32 = 6;
    pub const ITEM_STACK: i32 = 7;
    pub const BOOLEAN: i32 = 8;
    pub const ROTATIONS: i32 = 9;
    pub const BLOCK_POS: i32 = 10;
    pub const OPTIONAL_BLOCK_POS: i32 = 11;
    pub const DIRECTION: i32 = 12;
    pub const OPTIONAL_LIVING_ENTITY_REFERENCE: i32 = 13;
    pub const BLOCK_STATE: i32 = 14;
    pub const OPTIONAL_BLOCK_STATE: i32 = 15;
    pub const PARTICLE: i32 = 16;
    pub const PARTICLES: i32 = 17;
    pub const VILLAGER_DATA: i32 = 18;
    pub const OPTIONAL_UNSIGNED_INT: i32 = 19;
    pub const POSE: i32 = 20;
    pub const CAT_VARIANT: i32 = 21;
    pub const CAT_SOUND_VARIANT: i32 = 22;
    pub const COW_VARIANT: i32 = 23;
    pub const COW_SOUND_VARIANT: i32 = 24;
    pub const WOLF_VARIANT: i32 = 25;
    pub const WOLF_SOUND_VARIANT: i32 = 26;
    pub const FROG_VARIANT: i32 = 27;
    pub const PIG_VARIANT: i32 = 28;
    pub const PIG_SOUND_VARIANT: i32 = 29;
    pub const CHICKEN_VARIANT: i32 = 30;
    pub const CHICKEN_SOUND_VARIANT: i32 = 31;
    pub const ZOMBIE_NAUTILUS_VARIANT: i32 = 32;
    pub const OPTIONAL_GLOBAL_POS: i32 = 33;
    pub const PAINTING_VARIANT: i32 = 34;
    pub const SNIFFER_STATE: i32 = 35;
    pub const ARMADILLO_STATE: i32 = 36;
    pub const COPPER_GOLEM_STATE: i32 = 37;
    pub const WEATHERING_COPPER_STATE: i32 = 38;
    pub const VECTOR3: i32 = 39;
    pub const QUATERNION: i32 = 40;
    pub const RESOLVABLE_PROFILE: i32 = 41;
    pub const HUMANOID_ARM: i32 = 42;
    pub const DYE_COLOR: i32 = 43;
}
/// minecraft:particle_type registry ids (26.3) of the types with options;
/// the other ids below COUNT are SimpleParticleTypes.
mod particle_type {
    pub const BLOCK: i32 = 1;
    pub const BLOCK_MARKER: i32 = 2;
    pub const GEYSER: i32 = 7;
    pub const GEYSER_BASE: i32 = 8;
    pub const GEYSER_POOF: i32 = 9;
    pub const GEYSER_PLUME: i32 = 10;
    pub const DRAGON_BREATH: i32 = 15;
    pub const DUST: i32 = 21;
    pub const DUST_COLOR_TRANSITION: i32 = 22;
    pub const EFFECT: i32 = 23;
    pub const ENTITY_EFFECT: i32 = 28;
    pub const FALLING_DUST: i32 = 36;
    pub const TINTED_LEAVES: i32 = 46;
    pub const SCULK_CHARGE: i32 = 48;
    pub const FLASH: i32 = 52;
    pub const INSTANT_EFFECT: i32 = 56;
    pub const ITEM: i32 = 57;
    pub const VIBRATION: i32 = 58;
    pub const TRAIL: i32 = 59;
    pub const SHRIEK: i32 = 115;
    pub const DUST_PILLAR: i32 = 121;
    pub const BLOCK_CRUMBLE: i32 = 125;
    pub const COUNT: i32 = 128;
}
/// ParticleTypes.STREAM_CODEC: the particle_type id, then that type's
/// options. False when the type (or a vibration's position source type) is
/// unknown, so the value's length is too.
fn particle(r: &mut Reader) -> Result<bool> {
    use particle_type::*;
    match r.varint()? {
        // BlockParticleOption: a block state id.
        BLOCK | BLOCK_MARKER | FALLING_DUST | DUST_PILLAR | BLOCK_CRUMBLE => {
            r.varint().map(drop)?
        }
        // GeyserParticleOptions: waterBlocks.
        GEYSER | GEYSER_PLUME => r.i32().map(drop)?,
        // GeyserBaseParticleOptions: waterBlocks, burstImpulseBase.
        GEYSER_BASE | GEYSER_POOF => {
            r.i32()?;
            r.f32()?;
        }
        // PowerParticleOption: power. SculkChargeParticleOptions: roll.
        DRAGON_BREATH | SCULK_CHARGE => r.f32().map(drop)?,
        // DustParticleOptions: color, scale. SpellParticleOption: color, power.
        DUST | EFFECT | INSTANT_EFFECT => {
            r.i32()?;
            r.f32()?;
        }
        // DustColorTransitionOptions: fromColor, toColor, scale.
        DUST_COLOR_TRANSITION => {
            r.i32()?;
            r.i32()?;
            r.f32()?;
        }
        // ColorParticleOption: an ARGB int.
        ENTITY_EFFECT | TINTED_LEAVES | FLASH => r.i32().map(drop)?,
        // ItemParticleOption: an ItemStackTemplate.
        ITEM => skip_template(r)?,
        // VibrationParticleOption: PositionSource.STREAM_CODEC (the
        // position_source_type id, then BlockPositionSource's BlockPos or
        // EntityPositionSource's entity id and yOffset), arrivalInTicks.
        VIBRATION => {
            match r.varint()? {
                0 => r.i64().map(drop)?,
                1 => {
                    r.varint()?;
                    r.f32()?;
                }
                _ => return Ok(false),
            }
            r.varint()?;
        }
        // TrailParticleOption: target (Vec3.STREAM_CODEC), color, duration.
        TRAIL => {
            (0..3).try_for_each(|_| r.f64().map(drop))?;
            r.i32()?;
            r.varint()?;
        }
        // ShriekParticleOption: delay.
        SHRIEK => r.varint().map(drop)?,
        // SimpleParticleType: StreamCodec.unit.
        0..COUNT => {}
        _ => return Ok(false),
    }
    Ok(true)
}
/// ParticleTypes.STREAM_CODEC.apply(ByteBufCodecs.list()).
fn particles(r: &mut Reader) -> Result<bool> {
    for _ in 0..r.count(r.remaining())? {
        if !particle(r)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn read_value(kind: i32, r: &mut Reader) -> Result<Option<Value>> {
    use serializer::*;
    Ok(Some(match kind {
        BYTE => Value::Byte(r.i8()?),
        INT => Value::Int(r.varint()?),
        LONG => Value::Long(r.varlong()?),
        FLOAT => Value::Float(r.f32()?),
        STRING => Value::String(r.string(32767)?),
        COMPONENT => Value::Text(r.nbt()?),
        OPTIONAL_COMPONENT => Value::OptText(if r.bool()? { Some(r.nbt()?) } else { None }),
        ITEM_STACK => Value::Item(Slot::read(r)?),
        BOOLEAN => Value::Bool(r.bool()?),
        ROTATIONS | VECTOR3 => Value::Floats(vec![r.f32()?, r.f32()?, r.f32()?]),
        QUATERNION => Value::Floats(vec![r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
        BLOCK_POS => Value::Pos(crate::buf::Pos::read(r)?),
        OPTIONAL_BLOCK_POS => Value::OptPos(if r.bool()? {
            Some(crate::buf::Pos::read(r)?)
        } else {
            None
        }),
        OPTIONAL_LIVING_ENTITY_REFERENCE => {
            Value::OptUuid(if r.bool()? { Some(r.uuid()?) } else { None })
        }
        // VillagerData: type, profession and level.
        VILLAGER_DATA => Value::Ids(vec![r.varint()?, r.varint()?, r.varint()?]),
        // GlobalPos.STREAM_CODEC: the dimension ResourceKey, then a BlockPos.
        OPTIONAL_GLOBAL_POS => Value::OptGlobalPos(if r.bool()? {
            Some((r.string(32767)?, crate::buf::Pos::read(r)?))
        } else {
            None
        }),
        // Measured by their codecs, then kept as the bytes they took.
        PARTICLE | PARTICLES | RESOLVABLE_PROFILE => {
            let start = r.position();
            let known = match kind {
                PARTICLE => particle(r)?,
                PARTICLES => particles(r)?,
                _ => {
                    skip_profile(r)?;
                    true
                }
            };
            if !known {
                return Ok(None);
            }
            Value::Raw(r.since(start).to_vec())
        }
        // VarInt-encoded: direction, block states, optional ints, pose, the
        // holder-registry variants (cat to zombie nautilus, painting) and the
        // enum states (sniffer to weathering copper), arm, dye.
        DIRECTION
        | BLOCK_STATE
        | OPTIONAL_BLOCK_STATE
        | OPTIONAL_UNSIGNED_INT
        | POSE
        | CAT_VARIANT..=ZOMBIE_NAUTILUS_VARIANT
        | PAINTING_VARIANT..=WEATHERING_COPPER_STATE
        | HUMANOID_ARM
        | DYE_COLOR => Value::Id(r.varint()?),
        _ => return Ok(None),
    }))
}
fn write_value(kind: i32, v: &Value, w: &mut Writer) {
    match v {
        Value::Byte(b) => w.i8(*b),
        Value::Int(i) | Value::Id(i) => w.varint(*i),
        Value::Ids(ids) => ids.iter().for_each(|i| w.varint(*i)),
        Value::Long(l) => w.varlong(*l),
        Value::Float(f) => w.f32(*f),
        Value::String(s) => w.string(s),
        Value::Text(t) => w.nbt(t),
        Value::OptText(t) => {
            w.bool(t.is_some());
            if let Some(t) = t {
                w.nbt(t);
            }
        }
        Value::Item(s) => Slot::write(s, w),
        Value::Bool(b) => w.bool(*b),
        Value::Floats(f) => f.iter().for_each(|f| w.f32(*f)),
        Value::Pos(p) => crate::buf::Pos::write(p, w),
        Value::OptPos(p) => {
            w.bool(p.is_some());
            if let Some(p) = p {
                crate::buf::Pos::write(p, w);
            }
        }
        Value::OptUuid(u) => {
            w.bool(u.is_some());
            if let Some(u) = u {
                w.uuid(*u);
            }
        }
        Value::OptGlobalPos(p) => {
            w.bool(p.is_some());
            if let Some((dimension, pos)) = p {
                w.string(dimension);
                crate::buf::Pos::write(pos, w);
            }
        }
        Value::Raw(bytes) => w.bytes(bytes),
    }
    let _ = kind;
}

/// (index, serializer id, value) entries.
pub struct EntityData;
impl Codec<Vec<(u8, i32, Value)>> for EntityData {
    fn read(r: &mut Reader) -> Result<Vec<(u8, i32, Value)>> {
        let mut out = Vec::new();
        loop {
            let index = r.u8()?;
            if index == 0xFF {
                return Ok(out);
            }
            let kind = r.varint()?;
            match read_value(kind, r)? {
                Some(v) => out.push((index, kind, v)),
                None => {
                    r.rest();
                    return Ok(out);
                }
            }
        }
    }
    fn write(v: &Vec<(u8, i32, Value)>, w: &mut Writer) {
        for (index, kind, value) in v {
            w.u8(*index);
            w.varint(*kind);
            write_value(*kind, value, w);
        }
        w.u8(0xFF);
    }
}

/// ItemEntity.DATA_ITEM (after the eight Entity data entries).
pub const ITEM_ENTITY_ITEM: u8 = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_stops_at_unknown_serializers() {
        let entries = vec![
            (0u8, serializer::BYTE, Value::Byte(0x02)),
            (1, serializer::INT, Value::Int(300)),
            (2, serializer::OPTIONAL_COMPONENT, Value::OptText(None)),
            (6, serializer::POSE, Value::Id(5)),
            (
                8,
                serializer::ITEM_STACK,
                Value::Item(Some(Stack::new(3, 12))),
            ),
        ];
        let mut w = Writer::new();
        EntityData::write(&entries, &mut w);
        assert_eq!(*w.data.last().unwrap(), 0xFF);
        assert_eq!(
            EntityData::read(&mut Reader::new(&w.data)).unwrap(),
            entries
        );
        // A serializer past the registry (99) cannot be skipped: keep what
        // came before.
        let mut bytes = vec![0, 0, 7, 9, 99, 1, 2, 3];
        bytes.push(0xFF);
        let read = EntityData::read(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(read, vec![(0, 0, Value::Byte(7))]);
    }

    /// One `kind` entry (index 10) holding `value`, then a Byte entry (11).
    fn with_next(kind: i32, value: &[u8]) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(10);
        w.varint(kind);
        w.bytes(value);
        w.u8(11);
        w.varint(serializer::BYTE);
        w.i8(-3);
        w.u8(0xFF);
        w.data
    }
    /// `value` reads back as exactly its bytes, the entry after it still
    /// decodes, and the list writes back byte for byte.
    fn raw_round_trip(kind: i32, value: &[u8]) {
        let bytes = with_next(kind, value);
        let read = EntityData::read(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(
            read,
            vec![
                (10, kind, Value::Raw(value.to_vec())),
                (11, serializer::BYTE, Value::Byte(-3)),
            ]
        );
        let mut w = Writer::new();
        EntityData::write(&read, &mut w);
        assert_eq!(w.data, bytes);
    }
    /// A particle with its options, as ParticleTypes.STREAM_CODEC writes it.
    fn particle_bytes(id: i32, options: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(id);
        options(&mut w);
        w.data
    }

    #[test]
    fn dust_particle() {
        // DustParticleOptions: color, scale.
        let dust = particle_bytes(particle_type::DUST, |w| {
            w.i32(0x00FF_3300);
            w.f32(1.5);
        });
        raw_round_trip(serializer::PARTICLE, &dust);
    }

    #[test]
    fn block_particle() {
        // BlockParticleOption: a (multi-byte VarInt) block state id.
        let block = particle_bytes(particle_type::BLOCK, |w| w.varint(13_337));
        raw_round_trip(serializer::PARTICLE, &block);
    }

    #[test]
    fn item_particle() {
        // ItemParticleOption: ItemStackTemplate (item, count, one added
        // damage component, one removed item name).
        let item = particle_bytes(particle_type::ITEM, |w| {
            w.varint(812);
            w.varint(1);
            w.varint(1);
            w.varint(1);
            w.varint(crate::item::component::DAMAGE);
            w.varint(40);
            w.varint(crate::item::component::ITEM_NAME);
        });
        raw_round_trip(serializer::PARTICLE, &item);
    }

    #[test]
    fn vibration_particle_with_an_entity_source() {
        // PositionSource: entity (1), entity id, yOffset; then arrival ticks.
        let vibration = particle_bytes(particle_type::VIBRATION, |w| {
            w.varint(1);
            w.varint(4242);
            w.f32(0.5);
            w.varint(20);
        });
        raw_round_trip(serializer::PARTICLE, &vibration);
        // ... and a block (0) source: a BlockPos.
        let vibration = particle_bytes(particle_type::VIBRATION, |w| {
            w.varint(0);
            crate::buf::Pos::write(&[-5, 70, 12], w);
            w.varint(7);
        });
        raw_round_trip(serializer::PARTICLE, &vibration);
    }

    #[test]
    fn particles_list() {
        // LivingEntity.DATA_EFFECT_PARTICLES: entity effect, a simple flame,
        // a trail and a dust colour transition.
        let mut list = vec![4];
        list.extend(particle_bytes(particle_type::ENTITY_EFFECT, |w| {
            w.i32(0x80FF_0000u32 as i32)
        }));
        list.extend(particle_bytes(39, |_| {}));
        list.extend(particle_bytes(particle_type::TRAIL, |w| {
            w.f64(1.0);
            w.f64(-2.5);
            w.f64(64.0);
            w.i32(0x00AB_CDEF);
            w.varint(30);
        }));
        list.extend(particle_bytes(particle_type::DUST_COLOR_TRANSITION, |w| {
            w.i32(0x0000_00FF);
            w.i32(0x00FF_FF00);
            w.f32(2.0);
        }));
        raw_round_trip(serializer::PARTICLES, &list);
        raw_round_trip(serializer::PARTICLES, &[0]);
    }

    #[test]
    fn optional_global_pos() {
        for value in [
            Some(("minecraft:the_nether".to_owned(), [-300, -64, 1_000_000])),
            None,
        ] {
            let mut w = Writer::new();
            w.bool(value.is_some());
            if let Some((dimension, pos)) = &value {
                w.string(dimension);
                crate::buf::Pos::write(pos, &mut w);
            }
            let bytes = with_next(serializer::OPTIONAL_GLOBAL_POS, &w.data);
            let read = EntityData::read(&mut Reader::new(&bytes)).unwrap();
            assert_eq!(
                read,
                vec![
                    (
                        10,
                        serializer::OPTIONAL_GLOBAL_POS,
                        Value::OptGlobalPos(value)
                    ),
                    (11, serializer::BYTE, Value::Byte(-3)),
                ]
            );
            let mut back = Writer::new();
            EntityData::write(&read, &mut back);
            assert_eq!(back.data, bytes);
        }
    }

    #[test]
    fn resolvable_profile() {
        // Full: GAME_PROFILE (id, name, a signed property), then a
        // PlayerSkin.Patch with a body texture and the slim model.
        let mut w = Writer::new();
        w.bool(true);
        w.uuid(0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF);
        w.string("Steve");
        w.varint(1);
        w.string("textures");
        w.string("eyJ0ZXh0dXJlcyI6e319");
        w.bool(true);
        w.string("c2lnbmF0dXJl");
        w.bool(true);
        w.string("minecraft:entity/player/wide/steve");
        w.bool(false);
        w.bool(false);
        w.bool(true);
        w.bool(true);
        raw_round_trip(serializer::RESOLVABLE_PROFILE, &w.data);
        // Partial: a name, no id, no properties, an empty patch.
        let mut w = Writer::new();
        w.bool(false);
        w.bool(true);
        w.string("Alex");
        w.bool(false);
        w.varint(0);
        (0..4).for_each(|_| w.bool(false));
        raw_round_trip(serializer::RESOLVABLE_PROFILE, &w.data);
    }

    #[test]
    fn unknown_particle_types_end_the_list() {
        let byte = [0u8, serializer::BYTE as u8, 7];
        // A particle type past the registry, a vibration from an unknown
        // position source type, and a list holding an unknown type.
        let values = [
            (serializer::PARTICLE, particle_bytes(200, |_| {})),
            (
                serializer::PARTICLE,
                particle_bytes(particle_type::VIBRATION, |w| w.varint(5)),
            ),
            (serializer::PARTICLES, {
                let mut list = vec![2];
                list.extend(particle_bytes(39, |_| {}));
                list.extend(particle_bytes(particle_type::COUNT, |_| {}));
                list
            }),
        ];
        for (kind, value) in values {
            let mut bytes = byte.to_vec();
            bytes.extend(with_next(kind, &value));
            let read = EntityData::read(&mut Reader::new(&bytes)).unwrap();
            assert_eq!(read, vec![(0, serializer::BYTE, Value::Byte(7))]);
        }
    }
}
