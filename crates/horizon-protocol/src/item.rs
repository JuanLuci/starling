//! Item stacks on the wire (ItemStack.OPTIONAL_STREAM_CODEC): a count, the
//! item's registry id and a data component patch, plus the hashed form the
//! client sends back in container clicks (HashedStack).
use crate::{
    Result,
    buf::{Codec, Reader, Writer},
};
use horizon_world::nbt::Tag;

mod codecs;

/// minecraft:data_component_type registry ids (26.3).
pub mod component {
    pub const CUSTOM_DATA: i32 = 0;
    pub const MAX_STACK_SIZE: i32 = 1;
    pub const MAX_DAMAGE: i32 = 2;
    pub const DAMAGE: i32 = 3;
    pub const UNBREAKABLE: i32 = 4;
    pub const USE_EFFECTS: i32 = 5;
    pub const CUSTOM_NAME: i32 = 6;
    pub const MINIMUM_ATTACK_CHARGE: i32 = 7;
    pub const DAMAGE_TYPE: i32 = 8;
    pub const ITEM_NAME: i32 = 9;
    pub const ITEM_MODEL: i32 = 10;
    pub const LORE: i32 = 11;
    pub const RARITY: i32 = 12;
    pub const ENCHANTMENTS: i32 = 13;
    pub const CAN_PLACE_ON: i32 = 14;
    pub const CAN_BREAK: i32 = 15;
    pub const ATTRIBUTE_MODIFIERS: i32 = 16;
    pub const CUSTOM_MODEL_DATA: i32 = 17;
    pub const TOOLTIP_DISPLAY: i32 = 18;
    pub const REPAIR_COST: i32 = 19;
    pub const CREATIVE_SLOT_LOCK: i32 = 20;
    pub const ENCHANTMENT_GLINT_OVERRIDE: i32 = 21;
    pub const INTANGIBLE_PROJECTILE: i32 = 22;
    pub const FOOD: i32 = 23;
    pub const CONSUMABLE: i32 = 24;
    pub const USE_REMAINDER: i32 = 25;
    pub const USE_COOLDOWN: i32 = 26;
    pub const DAMAGE_RESISTANT: i32 = 27;
    pub const TOOL: i32 = 28;
    pub const WEAPON: i32 = 29;
    pub const ATTACK_RANGE: i32 = 30;
    pub const ENCHANTABLE: i32 = 31;
    pub const EQUIPPABLE: i32 = 32;
    pub const REPAIRABLE: i32 = 33;
    pub const GLIDER: i32 = 34;
    pub const TOOLTIP_STYLE: i32 = 35;
    pub const DEATH_PROTECTION: i32 = 36;
    pub const BLOCKS_ATTACKS: i32 = 37;
    pub const PIERCING_WEAPON: i32 = 38;
    pub const KINETIC_WEAPON: i32 = 39;
    pub const ATTACK_ANIMATION: i32 = 40;
    pub const INTERACT_ANIMATION: i32 = 41;
    pub const ADDITIONAL_TRADE_COST: i32 = 42;
    pub const BLOCK_TRANSFORMER: i32 = 43;
    pub const VILLAGER_FOOD: i32 = 44;
    pub const STORED_ENCHANTMENTS: i32 = 45;
    pub const DYE: i32 = 46;
    pub const DYED_COLOR: i32 = 47;
    pub const MAP_ID: i32 = 48;
    pub const MAP_DECORATIONS: i32 = 49;
    pub const MAP_POST_PROCESSING: i32 = 50;
    pub const CHARGED_PROJECTILES: i32 = 51;
    pub const BUNDLE_CONTENTS: i32 = 52;
    pub const POTION_CONTENTS: i32 = 53;
    pub const POTION_DURATION_SCALE: i32 = 54;
    pub const SUSPICIOUS_STEW_EFFECTS: i32 = 55;
    pub const WRITABLE_BOOK_CONTENT: i32 = 56;
    pub const WRITTEN_BOOK_CONTENT: i32 = 57;
    pub const TRIM: i32 = 58;
    pub const DEBUG_STICK_STATE: i32 = 59;
    pub const ENTITY_DATA: i32 = 60;
    pub const BUCKET_ENTITY_DATA: i32 = 61;
    pub const BLOCK_ENTITY_DATA: i32 = 62;
    pub const INSTRUMENT: i32 = 63;
    pub const PROVIDES_TRIM_MATERIAL: i32 = 64;
    pub const OMINOUS_BOTTLE_AMPLIFIER: i32 = 65;
    pub const JUKEBOX_PLAYABLE: i32 = 66;
    pub const PROVIDES_BANNER_PATTERNS: i32 = 67;
    pub const RECIPES: i32 = 68;
    pub const LODESTONE_TRACKER: i32 = 69;
    pub const FIREWORK_EXPLOSION: i32 = 70;
    pub const FIREWORKS: i32 = 71;
    pub const PROFILE: i32 = 72;
    pub const NOTE_BLOCK_SOUND: i32 = 73;
    pub const BANNER_PATTERNS: i32 = 74;
    pub const BASE_COLOR: i32 = 75;
    pub const POT_DECORATIONS: i32 = 76;
    pub const CONTAINER: i32 = 77;
    pub const BLOCK_STATE: i32 = 78;
    pub const BEES: i32 = 79;
    pub const SULFUR_CUBE_CONTENT: i32 = 80;
    pub const LOCK: i32 = 81;
    pub const CONTAINER_LOOT: i32 = 82;
    pub const BREAK_SOUND: i32 = 83;
    pub const COMPOSTABLE: i32 = 84;
    pub const COOKING_FUEL: i32 = 85;
    pub const BREWING_FUEL: i32 = 86;
    pub const MOB_VISIBILITY: i32 = 87;
    pub const VILLAGER_VARIANT: i32 = 88;
    pub const WOLF_VARIANT: i32 = 89;
    pub const WOLF_SOUND_VARIANT: i32 = 90;
    pub const WOLF_COLLAR: i32 = 91;
    pub const FOX_VARIANT: i32 = 92;
    pub const SALMON_SIZE: i32 = 93;
    pub const PARROT_VARIANT: i32 = 94;
    pub const TROPICAL_FISH_PATTERN: i32 = 95;
    pub const TROPICAL_FISH_BASE_COLOR: i32 = 96;
    pub const TROPICAL_FISH_PATTERN_COLOR: i32 = 97;
    pub const MOOSHROOM_VARIANT: i32 = 98;
    pub const RABBIT_VARIANT: i32 = 99;
    pub const PIG_VARIANT: i32 = 100;
    pub const PIG_SOUND_VARIANT: i32 = 101;
    pub const COW_VARIANT: i32 = 102;
    pub const COW_SOUND_VARIANT: i32 = 103;
    pub const CHICKEN_VARIANT: i32 = 104;
    pub const CHICKEN_SOUND_VARIANT: i32 = 105;
    pub const ZOMBIE_NAUTILUS_VARIANT: i32 = 106;
    pub const FROG_VARIANT: i32 = 107;
    pub const HORSE_VARIANT: i32 = 108;
    pub const PAINTING_VARIANT: i32 = 109;
    pub const LLAMA_VARIANT: i32 = 110;
    pub const AXOLOTL_VARIANT: i32 = 111;
    pub const CAT_VARIANT: i32 = 112;
    pub const CAT_SOUND_VARIANT: i32 = 113;
    pub const CAT_COLLAR: i32 = 114;
    pub const SHEEP_COLOR: i32 = 115;
    pub const SHULKER_COLOR: i32 = 116;
    pub const PROVIDES_POTTERY_PATTERN: i32 = 117;
    pub const SIGN_TEXT_FRONT: i32 = 118;
    pub const SIGN_TEXT_BACK: i32 = 119;
    pub const WAXED: i32 = 120;
    pub const CUSHION_COLOR: i32 = 121;
    /// The number of component types; ids at or past it are refused.
    pub const COUNT: i32 = 122;
}

/// The value of one patched data component. Components Horizon does not model
/// are kept as `Raw` bytes; only an unknown component type makes the stack
/// (and packet) undecodable.
#[derive(Clone, Debug, PartialEq)]
pub enum Component {
    Int(i32),
    Unit,
    Bool(bool),
    Text(Tag),
    Nbt(Tag),
    Ident(String),
    Lore(Vec<Tag>),
    Enchantments(Vec<(i32, i32)>),
    CustomModelData {
        floats: Vec<f32>,
        flags: Vec<bool>,
        strings: Vec<String>,
        colors: Vec<i32>,
    },
    TooltipDisplay {
        hide: bool,
        hidden: Vec<i32>,
    },
    /// Exactly the bytes the component's stream codec consumed, re-sent as is.
    Raw(Vec<u8>),
}
impl Component {
    fn read(kind: i32, r: &mut Reader) -> Result<Self> {
        use component::*;
        Ok(match kind {
            MAX_STACK_SIZE | MAX_DAMAGE | DAMAGE | RARITY | REPAIR_COST | MAP_ID => {
                Self::Int(r.varint()?)
            }
            DYED_COLOR => Self::Int(r.i32()?),
            UNBREAKABLE | CREATIVE_SLOT_LOCK => Self::Unit,
            ENCHANTMENT_GLINT_OVERRIDE => Self::Bool(r.bool()?),
            CUSTOM_NAME | ITEM_NAME => Self::Text(r.nbt()?),
            CUSTOM_DATA => Self::Nbt(r.nbt()?),
            ITEM_MODEL => Self::Ident(r.string(32767)?),
            LORE => {
                let n = r.count(256)?;
                Self::Lore((0..n).map(|_| r.nbt()).collect::<Result<_>>()?)
            }
            ENCHANTMENTS | STORED_ENCHANTMENTS => {
                let n = r.count(r.remaining())?;
                Self::Enchantments(
                    (0..n)
                        .map(|_| Ok((r.varint()?, r.varint()?)))
                        .collect::<Result<_>>()?,
                )
            }
            CUSTOM_MODEL_DATA => {
                let n = r.count(r.remaining())?;
                let floats = (0..n).map(|_| r.f32()).collect::<Result<_>>()?;
                let n = r.count(r.remaining())?;
                let flags = (0..n).map(|_| r.bool()).collect::<Result<_>>()?;
                let n = r.count(r.remaining())?;
                let strings = (0..n).map(|_| r.string(32767)).collect::<Result<_>>()?;
                let n = r.count(r.remaining())?;
                let colors = (0..n).map(|_| r.i32()).collect::<Result<_>>()?;
                Self::CustomModelData {
                    floats,
                    flags,
                    strings,
                    colors,
                }
            }
            TOOLTIP_DISPLAY => {
                let hide = r.bool()?;
                let n = r.count(r.remaining())?;
                Self::TooltipDisplay {
                    hide,
                    hidden: (0..n).map(|_| r.varint()).collect::<Result<_>>()?,
                }
            }
            other => {
                let start = r.position();
                codecs::skip(other, r, 0)?;
                Self::Raw(r.since(start).to_vec())
            }
        })
    }
    fn write(&self, kind: i32, w: &mut Writer) {
        match self {
            Self::Int(v) if kind == component::DYED_COLOR => w.i32(*v),
            Self::Int(v) => w.varint(*v),
            Self::Unit => {}
            Self::Bool(v) => w.bool(*v),
            Self::Text(t) | Self::Nbt(t) => w.nbt(t),
            Self::Ident(s) => w.string(s),
            Self::Lore(lines) => {
                w.varint(lines.len() as i32);
                lines.iter().for_each(|l| w.nbt(l));
            }
            Self::Enchantments(list) => {
                w.varint(list.len() as i32);
                for (e, level) in list {
                    w.varint(*e);
                    w.varint(*level);
                }
            }
            Self::CustomModelData {
                floats,
                flags,
                strings,
                colors,
            } => {
                w.varint(floats.len() as i32);
                floats.iter().for_each(|f| w.f32(*f));
                w.varint(flags.len() as i32);
                flags.iter().for_each(|f| w.bool(*f));
                w.varint(strings.len() as i32);
                strings.iter().for_each(|s| w.string(s));
                w.varint(colors.len() as i32);
                colors.iter().for_each(|c| w.i32(*c));
            }
            Self::TooltipDisplay { hide, hidden } => {
                w.bool(*hide);
                w.varint(hidden.len() as i32);
                hidden.iter().for_each(|h| w.varint(*h));
            }
            Self::Raw(bytes) => w.bytes(bytes),
        }
    }
}

/// A non-empty item stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub item: i32,
    pub count: i32,
    pub added: Vec<(i32, Component)>,
    pub removed: Vec<i32>,
}
impl Stack {
    pub fn new(item: i32, count: i32) -> Self {
        Self {
            item,
            count,
            added: Vec::new(),
            removed: Vec::new(),
        }
    }
    pub fn component(&self, kind: i32) -> Option<&Component> {
        self.added.iter().find(|(k, _)| *k == kind).map(|(_, c)| c)
    }
    pub fn damage(&self) -> i32 {
        match self.component(component::DAMAGE) {
            Some(Component::Int(d)) => *d,
            _ => 0,
        }
    }
}
/// ItemStack.OPTIONAL_STREAM_CODEC: `None` is the empty stack (count 0).
pub struct Slot;
impl Codec<Option<Stack>> for Slot {
    fn read(r: &mut Reader) -> Result<Option<Stack>> {
        let count = r.varint()?;
        if count <= 0 {
            return Ok(None);
        }
        let item = r.varint()?;
        let positive = r.count(65536)?;
        let negative = r.count(65536)?;
        let mut added = Vec::with_capacity(positive.min(64));
        for _ in 0..positive {
            let kind = r.varint()?;
            added.push((kind, Component::read(kind, r)?));
        }
        let removed = (0..negative).map(|_| r.varint()).collect::<Result<_>>()?;
        Ok(Some(Stack {
            item,
            count,
            added,
            removed,
        }))
    }
    fn write(v: &Option<Stack>, w: &mut Writer) {
        let Some(s) = v else {
            w.varint(0);
            return;
        };
        w.varint(s.count);
        w.varint(s.item);
        w.varint(s.added.len() as i32);
        w.varint(s.removed.len() as i32);
        for (kind, c) in &s.added {
            w.varint(*kind);
            c.write(*kind, w);
        }
        for kind in &s.removed {
            w.varint(*kind);
        }
    }
}

/// ItemStackTemplate.STREAM_CODEC: the item, the count, then the patch.
pub struct Template;
impl Codec<Stack> for Template {
    fn read(r: &mut Reader) -> Result<Stack> {
        let item = r.varint()?;
        let count = r.varint()?;
        let positive = r.count(65536)?;
        let negative = r.count(65536)?;
        let mut added = Vec::with_capacity(positive.min(64));
        for _ in 0..positive {
            let kind = r.varint()?;
            added.push((kind, Component::read(kind, r)?));
        }
        let removed = (0..negative).map(|_| r.varint()).collect::<Result<_>>()?;
        Ok(Stack {
            item,
            count,
            added,
            removed,
        })
    }
    fn write(s: &Stack, w: &mut Writer) {
        w.varint(s.item);
        w.varint(s.count);
        w.varint(s.added.len() as i32);
        w.varint(s.removed.len() as i32);
        for (kind, c) in &s.added {
            w.varint(*kind);
            c.write(*kind, w);
        }
        for kind in &s.removed {
            w.varint(*kind);
        }
    }
}

/// HashedStack: what the client believes a slot holds. Component values are
/// sent as hashes; Horizon sends patches without hashes, so a server whose
/// stack carries components resynchronises that slot (as it does for any
/// mismatched prediction).
#[derive(Clone, Debug, PartialEq)]
pub struct Hashed {
    pub item: i32,
    pub count: i32,
    pub added: Vec<(i32, i32)>,
    pub removed: Vec<i32>,
}
impl Hashed {
    pub fn of(stack: &Stack) -> Self {
        Self {
            item: stack.item,
            count: stack.count,
            added: Vec::new(),
            removed: stack.removed.clone(),
        }
    }
}
pub struct HashedSlot;
impl Codec<Option<Hashed>> for HashedSlot {
    fn read(r: &mut Reader) -> Result<Option<Hashed>> {
        if !r.bool()? {
            return Ok(None);
        }
        let item = r.varint()?;
        let count = r.varint()?;
        let n = r.count(256)?;
        let added = (0..n)
            .map(|_| Ok((r.varint()?, r.i32()?)))
            .collect::<Result<_>>()?;
        let n = r.count(256)?;
        let removed = (0..n).map(|_| r.varint()).collect::<Result<_>>()?;
        Ok(Some(Hashed {
            item,
            count,
            added,
            removed,
        }))
    }
    fn write(v: &Option<Hashed>, w: &mut Writer) {
        w.bool(v.is_some());
        if let Some(h) = v {
            w.varint(h.item);
            w.varint(h.count);
            w.varint(h.added.len() as i32);
            for (k, hash) in &h.added {
                w.varint(*k);
                w.i32(*hash);
            }
            w.varint(h.removed.len() as i32);
            for k in &h.removed {
                w.varint(*k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::literal;

    fn round<T: PartialEq + std::fmt::Debug, C: Codec<T>>(v: T) {
        let mut w = Writer::new();
        C::write(&v, &mut w);
        let mut r = Reader::new(&w.data);
        assert_eq!(C::read(&mut r).unwrap(), v);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn stacks_round_trip() {
        round::<Option<Stack>, Slot>(None);
        round::<Option<Stack>, Slot>(Some(Stack::new(42, 64)));
        let mut sword = Stack::new(900, 1);
        sword.added.push((component::DAMAGE, Component::Int(17)));
        sword.added.push((
            component::CUSTOM_NAME,
            Component::Text(literal("Excalibur")),
        ));
        sword.added.push((
            component::ENCHANTMENTS,
            Component::Enchantments(vec![(5, 3), (12, 1)]),
        ));
        sword
            .added
            .push((component::DYED_COLOR, Component::Int(-6265536)));
        sword.added.push((
            component::LORE,
            Component::Lore(vec![literal("a"), literal("b")]),
        ));
        sword.removed.push(component::REPAIR_COST);
        assert_eq!(sword.damage(), 17);
        round::<Option<Stack>, Slot>(Some(sword.clone()));
        round::<Option<Hashed>, HashedSlot>(Some(Hashed::of(&sword)));
        round::<Option<Hashed>, HashedSlot>(None);
    }

    #[test]
    fn empty_stack_is_a_zero_count() {
        let mut w = Writer::new();
        Slot::write(&None, &mut w);
        assert_eq!(w.data, [0]);
        let mut w = Writer::new();
        Slot::write(&Some(Stack::new(1, 2)), &mut w);
        assert_eq!(w.data, [2, 1, 0, 0]);
    }

    /// A stack with one `kind` component, hand-encoded after its vanilla codec
    /// by `value`, then a plain stack.
    fn with_component(kind: i32, value: impl FnOnce(&mut Writer)) -> (Vec<u8>, Vec<u8>) {
        let mut v = Writer::new();
        value(&mut v);
        let mut w = Writer::new();
        for x in [1, 900, 1, 0, kind] {
            w.varint(x);
        }
        w.bytes(&v.data);
        (v.data, w.data)
    }
    /// The component must come back as exactly its bytes, re-encode
    /// unchanged, end where the codec ends (the next stack still decodes) and
    /// need every one of its bytes.
    fn raw_round_trip(kind: i32, value: impl FnOnce(&mut Writer)) {
        let (bytes, mut data) = with_component(kind, value);
        let end = data.len();
        let mut w = Writer::new();
        Slot::write(&Some(Stack::new(7, 3)), &mut w);
        data.extend_from_slice(&w.data);

        let mut r = Reader::new(&data);
        let stack = Slot::read(&mut r).unwrap().unwrap();
        assert_eq!(stack.added, [(kind, Component::Raw(bytes))]);
        assert_eq!(Slot::read(&mut r).unwrap(), Some(Stack::new(7, 3)));
        assert_eq!(r.remaining(), 0);
        let mut back = Writer::new();
        Slot::write(&Some(stack), &mut back);
        assert_eq!(back.data, data[..end]);
        assert!(Slot::read(&mut Reader::new(&data[..end - 1])).is_err());
    }
    fn refused(kind: i32, value: impl FnOnce(&mut Writer)) {
        let (_, data) = with_component(kind, value);
        assert!(Slot::read(&mut Reader::new(&data)).is_err(), "{kind}");
    }

    fn text(w: &mut Writer, s: &str) {
        w.nbt(&literal(s));
    }
    /// SoundEvent.STREAM_CODEC by registry id.
    fn sound_id(w: &mut Writer, id: i32) {
        w.varint(id + 1);
    }
    /// SoundEvent.STREAM_CODEC inline, with a fixed range.
    fn sound_inline(w: &mut Writer, location: &str) {
        w.varint(0);
        w.string(location);
        w.bool(true);
        w.f32(16.0);
    }
    /// MobEffectInstance.STREAM_CODEC hiding a weaker instance.
    fn effect(w: &mut Writer, id: i32) {
        w.varint(id);
        for details in [(1, 600, true), (0, 1200, false)] {
            w.varint(details.0);
            w.varint(details.1);
            w.bool(false);
            w.bool(true);
            w.bool(true);
            w.bool(details.2);
        }
    }
    /// ItemStackTemplate.STREAM_CODEC with an empty patch.
    fn template(w: &mut Writer, item: i32, count: i32) {
        for x in [item, count, 0, 0] {
            w.varint(x);
        }
    }
    fn explosion(w: &mut Writer) {
        w.varint(1);
        w.varint(2);
        w.i32(0xFF0000);
        w.i32(0x00FF00);
        w.varint(0);
        w.bool(true);
        w.bool(false);
    }

    #[test]
    fn attack_range_food_and_simple_values() {
        raw_round_trip(component::ATTACK_RANGE, |w| {
            for f in [0.0, 3.0, 0.0, 5.0, 0.3, 1.0] {
                w.f32(f);
            }
        });
        raw_round_trip(component::FOOD, |w| {
            w.varint(4);
            w.f32(2.4);
            w.bool(false);
        });
        raw_round_trip(component::USE_EFFECTS, |w| {
            w.bool(true);
            w.bool(false);
            w.f32(0.2);
        });
        raw_round_trip(component::WEAPON, |w| {
            w.varint(1);
            w.f32(5.0);
        });
        raw_round_trip(component::MINIMUM_ATTACK_CHARGE, |w| w.f32(0.9));
        raw_round_trip(component::BASE_COLOR, |w| w.varint(11));
        raw_round_trip(component::PAINTING_VARIANT, |w| w.varint(4));
        raw_round_trip(component::TOOLTIP_STYLE, |w| w.string("minecraft:fancy"));
        raw_round_trip(component::ATTACK_ANIMATION, |w| {
            w.varint(2);
            w.varint(10);
        });
        raw_round_trip(component::USE_COOLDOWN, |w| {
            w.f32(1.5);
            w.bool(true);
            w.string("minecraft:ender_pearl");
        });
        raw_round_trip(component::COOKING_FUEL, |w| {
            w.bool(true);
            w.i32(200);
            w.bool(false);
            w.string("minecraft:furnace_speed");
        });
    }

    #[test]
    fn profiles() {
        // A name only: a dynamic profile, default skin.
        raw_round_trip(component::PROFILE, |w| {
            w.bool(false);
            w.bool(true);
            w.string("Notch");
            w.bool(false);
            w.varint(0);
            (0..4).for_each(|_| w.bool(false));
        });
        // A full GameProfile with a signed textures property and a skin patch.
        raw_round_trip(component::PROFILE, |w| {
            w.bool(true);
            w.uuid(0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5);
            w.string("Notch");
            w.varint(1);
            w.string("textures");
            w.string("eyJ0ZXh0dXJlcyI6e319");
            w.bool(true);
            w.string("c2lnbmF0dXJl");
            w.bool(true);
            w.string("minecraft:entity/player/wide/steve");
            w.bool(false);
            w.bool(true);
            w.string("minecraft:entity/equipment/wings/elytra");
            w.bool(true);
            w.bool(true);
        });
        // A partial profile with an id and an unsigned property.
        raw_round_trip(component::PROFILE, |w| {
            w.bool(false);
            w.bool(false);
            w.bool(true);
            w.uuid(7);
            w.varint(1);
            w.string("textures");
            w.string("e30=");
            w.bool(false);
            (0..4).for_each(|_| w.bool(false));
        });
    }

    #[test]
    fn potion_contents_with_custom_effects() {
        raw_round_trip(component::POTION_CONTENTS, |w| {
            w.bool(true);
            w.varint(5);
            w.bool(true);
            w.i32(0xFF00FF);
            w.varint(2);
            effect(w, 1);
            effect(w, 10);
            w.bool(true);
            w.string("custom");
        });
        raw_round_trip(component::POTION_CONTENTS, |w| {
            w.bool(false);
            w.bool(false);
            w.varint(0);
            w.bool(false);
        });
        raw_round_trip(component::SUSPICIOUS_STEW_EFFECTS, |w| {
            w.varint(1);
            w.varint(15);
            w.varint(160);
        });
    }

    #[test]
    fn equippable_and_consumable() {
        raw_round_trip(component::EQUIPPABLE, |w| {
            w.varint(5);
            sound_id(w, 120);
            w.bool(true);
            w.string("minecraft:diamond");
            w.bool(false);
            w.bool(true);
            w.varint(0);
            w.string("minecraft:can_wear_horse_armor");
            for b in [true, true, true, false, false] {
                w.bool(b);
            }
            sound_inline(w, "minecraft:item.shears.snip");
        });
        raw_round_trip(component::CONSUMABLE, |w| {
            w.f32(1.6);
            w.varint(1);
            sound_id(w, 300);
            w.bool(true);
            w.varint(5);
            // apply_effects
            w.varint(0);
            w.varint(1);
            effect(w, 3);
            w.f32(0.5);
            // remove_effects: two ids
            w.varint(1);
            w.varint(3);
            w.varint(4);
            w.varint(9);
            // clear_all_effects
            w.varint(2);
            // teleport_randomly
            w.varint(3);
            w.f32(16.0);
            w.bool(true);
            // play_sound
            w.varint(4);
            sound_inline(w, "minecraft:entity.generic.burp");
        });
        raw_round_trip(component::DEATH_PROTECTION, |w| {
            w.varint(1);
            w.varint(2);
        });
    }

    #[test]
    fn tools_weapons_and_attributes() {
        raw_round_trip(component::TOOL, |w| {
            w.varint(2);
            // A block tag that denies drops.
            w.varint(0);
            w.string("minecraft:incorrect_for_wooden_tool");
            w.bool(false);
            w.bool(true);
            w.bool(false);
            // Two block ids, mined at 8 with drops.
            w.varint(3);
            w.varint(10);
            w.varint(20);
            w.bool(true);
            w.f32(8.0);
            w.bool(true);
            w.bool(true);
            w.f32(1.0);
            w.varint(1);
            w.bool(true);
        });
        raw_round_trip(component::ATTRIBUTE_MODIFIERS, |w| {
            w.varint(3);
            for (attribute, id, amount, display) in [
                (3, "minecraft:base_attack_damage", 5.0, 0),
                (5, "minecraft:base_attack_speed", -2.4, 1),
                (1, "example:armor", 0.5, 2),
            ] {
                w.varint(attribute);
                w.string(id);
                w.f64(amount);
                w.varint(0);
                w.varint(1);
                w.varint(display);
            }
            text(w, "+50% armor");
        });
        raw_round_trip(component::BLOCKS_ATTACKS, |w| {
            w.f32(0.25);
            w.f32(1.0);
            w.varint(1);
            w.f32(90.0);
            w.bool(true);
            w.varint(0);
            w.string("minecraft:is_projectile");
            w.f32(0.0);
            w.f32(1.0);
            [3.0, 1.0, 1.0].into_iter().for_each(|f| w.f32(f));
            w.bool(true);
            w.varint(0);
            w.string("minecraft:bypasses_shield");
            w.bool(true);
            sound_id(w, 1);
            w.bool(false);
        });
        raw_round_trip(component::KINETIC_WEAPON, |w| {
            w.varint(10);
            w.varint(5);
            w.bool(true);
            w.varint(100);
            w.f32(1.0);
            w.f32(0.5);
            w.bool(false);
            w.bool(true);
            w.varint(200);
            w.f32(2.0);
            w.f32(0.0);
            w.f32(0.38);
            w.f32(1.2);
            w.bool(true);
            sound_inline(w, "minecraft:item.spear.use");
            w.bool(false);
        });
        raw_round_trip(component::PIERCING_WEAPON, |w| {
            w.bool(true);
            w.bool(false);
            w.bool(false);
            w.bool(true);
            sound_id(w, 4);
        });
    }

    #[test]
    fn bundles_and_containers_hold_nested_stacks() {
        raw_round_trip(component::BUNDLE_CONTENTS, |w| {
            w.varint(2);
            template(w, 42, 3);
            // A damaged, named stack holding its own bundle, minus a component.
            for x in [43, 1, 3, 1, component::DAMAGE, 5, component::CUSTOM_NAME] {
                w.varint(x);
            }
            text(w, "inner");
            w.varint(component::BUNDLE_CONTENTS);
            w.varint(1);
            template(w, 1, 1);
            w.varint(component::REPAIR_COST);
        });
        raw_round_trip(component::CHARGED_PROJECTILES, |w| {
            w.varint(1);
            template(w, 800, 1);
        });
        raw_round_trip(component::CONTAINER, |w| {
            w.varint(3);
            w.bool(true);
            template(w, 1, 64);
            w.bool(false);
            w.bool(true);
            template(w, 2, 1);
        });
        raw_round_trip(component::POT_DECORATIONS, |w| {
            w.bool(true);
            template(w, 5, 1);
            w.bool(false);
            w.bool(false);
            w.bool(true);
            template(w, 6, 1);
        });
        raw_round_trip(component::USE_REMAINDER, |w| template(w, 9, 1));
    }

    #[test]
    fn books() {
        raw_round_trip(component::WRITTEN_BOOK_CONTENT, |w| {
            w.string("Title");
            w.bool(true);
            w.string("T");
            w.string("Steve");
            w.varint(0);
            w.varint(2);
            text(w, "page one");
            w.bool(false);
            text(w, "page two");
            w.bool(true);
            text(w, "page ***");
            w.bool(true);
        });
        raw_round_trip(component::WRITABLE_BOOK_CONTENT, |w| {
            w.varint(1);
            w.string("draft");
            w.bool(false);
        });
    }

    #[test]
    fn trims_fireworks_and_banners() {
        raw_round_trip(component::TRIM, |w| {
            w.varint(3);
            w.varint(0);
            w.string("minecraft:sentry");
            text(w, "Sentry Armor Trim");
            w.bool(false);
        });
        raw_round_trip(component::TRIM, |w| {
            w.varint(0);
            w.string("minecraft:amethyst");
            text(w, "Amethyst Material");
            w.varint(1);
        });
        raw_round_trip(component::FIREWORK_EXPLOSION, explosion);
        raw_round_trip(component::FIREWORKS, |w| {
            w.varint(2);
            w.varint(2);
            explosion(w);
            explosion(w);
        });
        raw_round_trip(component::BANNER_PATTERNS, |w| {
            w.varint(2);
            w.varint(6);
            w.varint(14);
            w.varint(0);
            w.string("minecraft:creeper");
            w.string("block.minecraft.banner.creeper");
            w.varint(0);
        });
        raw_round_trip(component::PROVIDES_BANNER_PATTERNS, |w| {
            w.varint(0);
            w.string("minecraft:pattern_item/globe");
        });
        raw_round_trip(component::INSTRUMENT, |w| {
            w.varint(0);
            sound_inline(w, "minecraft:item.goat_horn.sound.0");
            w.f32(7.0);
            w.f32(256.0);
            w.varint(0);
            text(w, "Ponder");
        });
        raw_round_trip(component::JUKEBOX_PLAYABLE, |w| w.varint(4));
    }

    #[test]
    fn adventure_mode_predicates() {
        raw_round_trip(component::CAN_BREAK, |w| {
            w.varint(2);
            // Blocks, state properties, NBT and component matchers.
            w.bool(true);
            w.varint(2);
            w.varint(1);
            w.bool(true);
            w.varint(2);
            w.string("facing");
            w.bool(true);
            w.string("north");
            w.string("age");
            w.bool(false);
            w.bool(true);
            w.string("1");
            w.bool(false);
            w.bool(true);
            w.nbt(&Tag::compound([("Lock", literal("key"))]));
            w.varint(2);
            w.varint(component::DAMAGE);
            w.varint(0);
            w.varint(component::CUSTOM_NAME);
            text(w, "named");
            w.varint(2);
            w.bool(true);
            w.varint(3);
            w.nbt(&Tag::compound([("min", Tag::Int(1))]));
            w.bool(false);
            w.varint(component::ENCHANTMENTS);
            w.nbt(&Tag::Compound(Default::default()));
            // A block tag alone.
            w.bool(true);
            w.varint(0);
            w.string("minecraft:logs");
            w.bool(false);
            w.bool(false);
            w.varint(0);
            w.varint(0);
        });
        raw_round_trip(component::CAN_PLACE_ON, |w| w.varint(0));
    }

    #[test]
    fn block_and_entity_data() {
        let tag = Tag::compound([("id", literal("minecraft:pig")), ("Age", Tag::Int(3))]);
        raw_round_trip(component::ENTITY_DATA, |w| {
            w.varint(12);
            w.nbt(&tag);
        });
        raw_round_trip(component::BLOCK_ENTITY_DATA, |w| {
            w.varint(7);
            w.nbt(&tag);
        });
        raw_round_trip(component::BUCKET_ENTITY_DATA, |w| w.nbt(&tag));
        raw_round_trip(component::BEES, |w| {
            w.varint(1);
            w.varint(9);
            w.nbt(&tag);
            w.varint(100);
            w.varint(600);
        });
        raw_round_trip(component::BLOCK_STATE, |w| {
            w.varint(2);
            for s in ["facing", "east", "lit", "true"] {
                w.string(s);
            }
        });
        raw_round_trip(component::SIGN_TEXT_FRONT, |w| {
            ["a", "b", "c", "d"].into_iter().for_each(|l| text(w, l));
            w.bool(true);
            ["a", "*", "c", "d"].into_iter().for_each(|l| text(w, l));
            w.varint(15);
            w.bool(true);
        });
        raw_round_trip(component::LODESTONE_TRACKER, |w| {
            w.bool(true);
            w.string("minecraft:overworld");
            w.i64(crate::buf::pack_block_pos([10, 64, -5]));
            w.bool(true);
        });
        // Persistent-only types go as their codec's NBT (fromCodecWithRegistries).
        raw_round_trip(component::LOCK, |w| w.nbt(&tag));
        raw_round_trip(component::CONTAINER_LOOT, |w| {
            w.nbt(&Tag::compound([(
                "loot_table",
                literal("minecraft:chests/x"),
            )]));
        });
    }

    #[test]
    fn modelled_components_skip_like_they_encode() {
        let samples = [
            (component::DAMAGE, Component::Int(3)),
            (component::DYED_COLOR, Component::Int(-6265536)),
            (component::UNBREAKABLE, Component::Unit),
            (component::ENCHANTMENT_GLINT_OVERRIDE, Component::Bool(true)),
            (component::CUSTOM_NAME, Component::Text(literal("x"))),
            (
                component::CUSTOM_DATA,
                Component::Nbt(Tag::compound([("a", Tag::Int(1))])),
            ),
            (
                component::ITEM_MODEL,
                Component::Ident("minecraft:stone".into()),
            ),
            (
                component::LORE,
                Component::Lore(vec![literal("a"), literal("b")]),
            ),
            (
                component::STORED_ENCHANTMENTS,
                Component::Enchantments(vec![(1, 2)]),
            ),
            (
                component::CUSTOM_MODEL_DATA,
                Component::CustomModelData {
                    floats: vec![1.0],
                    flags: vec![true, false],
                    strings: vec!["s".into()],
                    colors: vec![0xFFFFFF],
                },
            ),
            (
                component::TOOLTIP_DISPLAY,
                Component::TooltipDisplay {
                    hide: false,
                    hidden: vec![component::DAMAGE, component::LORE],
                },
            ),
        ];
        for (kind, c) in samples {
            let mut w = Writer::new();
            c.write(kind, &mut w);
            let mut r = Reader::new(&w.data);
            codecs::skip(kind, &mut r, 0).unwrap();
            assert_eq!(r.remaining(), 0, "{kind}");
        }
    }

    /// `n` bundles, each holding the next.
    fn nested_bundles(n: usize) -> Vec<u8> {
        let mut w = Writer::new();
        for x in [1, 900, 1, 0] {
            w.varint(x);
        }
        for i in 0..n {
            w.varint(component::BUNDLE_CONTENTS);
            w.varint(1);
            if i + 1 < n {
                [901, 1, 1, 0].into_iter().for_each(|x| w.varint(x));
            } else {
                template(&mut w, 901, 1);
            }
        }
        w.data
    }

    #[test]
    fn nesting_is_bounded() {
        let max = codecs::MAX_DEPTH as usize;
        // The deepest accepted nesting fits a small stack, even unoptimised.
        let deepest = nested_bundles(max);
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || Slot::read(&mut Reader::new(&deepest)).map(drop))
            .unwrap()
            .join()
            .unwrap()
            .unwrap();
        assert!(Slot::read(&mut Reader::new(&nested_bundles(max + 1))).is_err());
        // Hidden effects chain the same way.
        let chain = |n: usize| {
            move |w: &mut Writer| {
                w.bool(false);
                w.bool(false);
                w.varint(1);
                w.varint(1);
                for i in 0..n {
                    w.varint(0);
                    w.varint(20);
                    (0..3).for_each(|_| w.bool(true));
                    w.bool(i + 1 < n);
                }
                w.bool(false);
            }
        };
        raw_round_trip(component::POTION_CONTENTS, chain(max));
        refused(component::POTION_CONTENTS, chain(max + 1));
    }

    #[test]
    fn unknown_components_are_errors() {
        // Every id below COUNT has a codec; past it there is no type.
        for kind in [component::COUNT, 999, -1] {
            refused(kind, |w| w.varint(0));
        }
        // A nested template must also remove only real types.
        refused(component::BUNDLE_CONTENTS, |w| {
            [1, 901, 1, 0, 1, component::COUNT]
                .into_iter()
                .for_each(|x| w.varint(x));
        });
        // An unknown consume effect type.
        refused(component::DEATH_PROTECTION, |w| {
            w.varint(1);
            w.varint(5);
        });
        // Vanilla's tag codecs refuse End, COMPOUND_TAG anything but a compound.
        refused(component::LOCK, |w| w.u8(0));
        refused(component::ENTITY_DATA, |w| {
            w.varint(1);
            w.nbt(&Tag::Int(3));
        });
    }
}
