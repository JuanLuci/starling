//! The stream codec of every data component type (DataComponents: the
//! `networkSynchronized` codec, else DataComponentType.Builder's fallback
//! `ByteBufCodecs.fromCodecWithRegistries`, an NBT tag), parsed only far
//! enough to know where each value ends, so components Horizon does not model
//! can be kept as the exact bytes they were sent as.
use super::component::*;
use crate::{Error, Result, buf::Reader};
use horizon_world::nbt::Tag;

/// Item stacks, component predicates and hidden effects nested deeper than
/// this are refused, so a hostile packet cannot exhaust the stack.
pub(super) const MAX_DEPTH: u32 = 64;

/// Consume one `kind` component value (DataComponentPatch.STREAM_CODEC's
/// `type.streamCodec()`); `depth` counts the item stacks around it.
pub(super) fn skip(kind: i32, r: &mut Reader, depth: u32) -> Result<()> {
    if depth >= MAX_DEPTH {
        return Err(Error::new("item components nested too deep"));
    }
    // Only the components holding stacks (or components) recurse; the rest
    // are in `flat`, whose frame then stays off the recursion.
    match kind {
        // AdventureModePredicate: a list of BlockPredicate.
        CAN_PLACE_ON | CAN_BREAK => list(r, usize::MAX, |r| block_predicate(r, depth)),
        // UseRemainder, SulfurCubeContent: one ItemStackTemplate.
        USE_REMAINDER | SULFUR_CUBE_CONTENT => template(r, depth),
        // ChargedProjectiles: list(1024) of ItemStackTemplate.
        CHARGED_PROJECTILES => list(r, 1024, |r| template(r, depth)),
        // BundleContents: a list of ItemStackTemplate.
        BUNDLE_CONTENTS => list(r, usize::MAX, |r| template(r, depth)),
        // PotDecorations: back, left, right, front.
        POT_DECORATIONS => (0..4).try_for_each(|_| optional(r, |r| template(r, depth))),
        // ItemContainerContents: list(256) of optional ItemStackTemplate.
        CONTAINER => list(r, 256, |r| optional(r, |r| template(r, depth))),
        _ => flat(kind, r),
    }
}

/// Consume one ItemStackTemplate.STREAM_CODEC value on its own (as in
/// ItemParticleOption).
pub(crate) fn skip_template(r: &mut Reader) -> Result<()> {
    template(r, 0)
}
/// Consume one ResolvableProfile.STREAM_CODEC value on its own (as in the
/// RESOLVABLE_PROFILE entity data serializer).
pub(crate) fn skip_profile(r: &mut Reader) -> Result<()> {
    profile(r)
}

/// The components that hold no item stacks or other components.
#[inline(never)]
fn flat(kind: i32, r: &mut Reader) -> Result<()> {
    match kind {
        // Unit.STREAM_CODEC
        UNBREAKABLE | CREATIVE_SLOT_LOCK | GLIDER | WAXED => Ok(()),
        // ByteBufCodecs.VAR_INT, holderRegistry ids and idMapper enums.
        MAX_STACK_SIZE
        | MAX_DAMAGE
        | DAMAGE
        | DAMAGE_TYPE
        | RARITY
        | REPAIR_COST
        | ENCHANTABLE
        | ADDITIONAL_TRADE_COST
        | BLOCK_TRANSFORMER
        | VILLAGER_FOOD
        | DYE
        | MAP_ID
        | MAP_POST_PROCESSING
        | OMINOUS_BOTTLE_AMPLIFIER
        | BASE_COLOR
        | VILLAGER_VARIANT
        | WOLF_VARIANT
        | WOLF_SOUND_VARIANT
        | WOLF_COLLAR
        | FOX_VARIANT
        | SALMON_SIZE
        | PARROT_VARIANT
        | TROPICAL_FISH_PATTERN
        | TROPICAL_FISH_BASE_COLOR
        | TROPICAL_FISH_PATTERN_COLOR
        | MOOSHROOM_VARIANT
        | RABBIT_VARIANT
        | PIG_VARIANT
        | PIG_SOUND_VARIANT
        | COW_VARIANT
        | COW_SOUND_VARIANT
        | CHICKEN_VARIANT
        | CHICKEN_SOUND_VARIANT
        | ZOMBIE_NAUTILUS_VARIANT
        | FROG_VARIANT
        | HORSE_VARIANT
        | LLAMA_VARIANT
        | AXOLOTL_VARIANT
        | CAT_VARIANT
        | CAT_SOUND_VARIANT
        | CAT_COLLAR
        | SHEEP_COLOR
        | SHULKER_COLOR
        | PROVIDES_POTTERY_PATTERN
        | CUSHION_COLOR => varint(r),
        // ByteBufCodecs.FLOAT
        MINIMUM_ATTACK_CHARGE | POTION_DURATION_SCALE => float(r),
        // DyedItemColor.STREAM_CODEC: ByteBufCodecs.INT
        DYED_COLOR => int(r),
        ENCHANTMENT_GLINT_OVERRIDE => bool(r),
        // Identifier.STREAM_CODEC
        ITEM_MODEL | TOOLTIP_STYLE | NOTE_BLOCK_SOUND => ident(r),
        // ComponentSerialization.STREAM_CODEC
        CUSTOM_NAME | ITEM_NAME => tag(r),
        // Persistent-only types: the fromCodecWithRegistries fallback.
        CUSTOM_DATA
        | INTANGIBLE_PROJECTILE
        | MAP_DECORATIONS
        | DEBUG_STICK_STATE
        | RECIPES
        | LOCK
        | CONTAINER_LOOT => tag(r),
        // CustomData.STREAM_CODEC
        BUCKET_ENTITY_DATA => compound(r),
        // TypedEntityData.streamCodec: the (block) entity type, then the tag.
        ENTITY_DATA | BLOCK_ENTITY_DATA => {
            varint(r)?;
            compound(r)
        }
        // UseEffects: canSprint, interactVibrations, speedMultiplier.
        USE_EFFECTS => {
            bool(r)?;
            bool(r)?;
            float(r)
        }
        // ItemLore: list(256) of components.
        LORE => list(r, 256, tag),
        // ItemEnchantments: map of Enchantment holder ids to levels.
        ENCHANTMENTS | STORED_ENCHANTMENTS => list(r, usize::MAX, |r| {
            varint(r)?;
            varint(r)
        }),
        // ItemAttributeModifiers: a list of Entry.
        ATTRIBUTE_MODIFIERS => list(r, usize::MAX, attribute_modifier),
        // CustomModelData: floats, flags, strings, colors.
        CUSTOM_MODEL_DATA => {
            list(r, usize::MAX, float)?;
            list(r, usize::MAX, bool)?;
            list(r, usize::MAX, utf8)?;
            list(r, usize::MAX, int)
        }
        // TooltipDisplay: hideTooltip, hidden component types.
        TOOLTIP_DISPLAY => {
            bool(r)?;
            list(r, usize::MAX, |r| component_type(r).map(drop))
        }
        // FoodProperties.DIRECT_STREAM_CODEC: nutrition, saturation, canAlwaysEat.
        FOOD => {
            varint(r)?;
            float(r)?;
            bool(r)
        }
        // Consumable: seconds, ItemUseAnimation, sound, particles, effects.
        CONSUMABLE => {
            float(r)?;
            varint(r)?;
            sound(r)?;
            bool(r)?;
            list(r, usize::MAX, consume_effect)
        }
        // UseCooldown: seconds, optional cooldown group.
        USE_COOLDOWN => {
            float(r)?;
            optional(r, ident)
        }
        // DamageResistant, Repairable, PROVIDES_BANNER_PATTERNS: a holderSet.
        DAMAGE_RESISTANT | REPAIRABLE | PROVIDES_BANNER_PATTERNS => holder_set(r),
        // Tool: rules, defaultMiningSpeed, damagePerBlock, canDestroyBlocksInCreative.
        TOOL => {
            list(r, usize::MAX, |r| {
                holder_set(r)?;
                optional(r, float)?;
                optional(r, bool)
            })?;
            float(r)?;
            varint(r)?;
            bool(r)
        }
        // Weapon: itemDamagePerAttack, disableBlockingForSeconds.
        WEAPON => {
            varint(r)?;
            float(r)
        }
        // AttackRange: min/max reach, min/max creative reach, hitbox margin, mob factor.
        ATTACK_RANGE => (0..6).try_for_each(|_| float(r)),
        // Equippable
        EQUIPPABLE => {
            varint(r)?; // EquipmentSlot
            sound(r)?; // equipSound
            optional(r, ident)?; // assetId (ResourceKey)
            optional(r, ident)?; // cameraOverlay
            optional(r, holder_set)?; // allowedEntities
            (0..5).try_for_each(|_| bool(r))?; // dispensable .. canBeSheared
            sound(r) // shearingSound
        }
        // DeathProtection: deathEffects.
        DEATH_PROTECTION => list(r, usize::MAX, consume_effect),
        // BlocksAttacks
        BLOCKS_ATTACKS => {
            float(r)?; // blockDelaySeconds
            float(r)?; // disableCooldownScale
            list(r, usize::MAX, |r| {
                // DamageReduction: angle, type, base, factor.
                float(r)?;
                optional(r, holder_set)?;
                float(r)?;
                float(r)
            })?;
            (0..3).try_for_each(|_| float(r))?; // ItemDamageFunction
            optional(r, holder_set)?; // bypassedBy
            optional(r, sound)?; // blockSound
            optional(r, sound) // disableSound
        }
        // PiercingWeapon: dealsKnockback, dismounts, sound, hitSound.
        PIERCING_WEAPON => {
            bool(r)?;
            bool(r)?;
            optional(r, sound)?;
            optional(r, sound)
        }
        // KineticWeapon
        KINETIC_WEAPON => {
            varint(r)?; // contactCooldownTicks
            varint(r)?; // delayTicks
            // dismount, knockback and damage Conditions.
            (0..3).try_for_each(|_| {
                optional(r, |r| {
                    varint(r)?;
                    float(r)?;
                    float(r)
                })
            })?;
            float(r)?; // forwardMovement
            float(r)?; // damageMultiplier
            optional(r, sound)?;
            optional(r, sound)
        }
        // SwingAnimation: SwingAnimationType, duration.
        ATTACK_ANIMATION | INTERACT_ANIMATION => {
            varint(r)?;
            varint(r)
        }
        // PotionContents: potion, customColor, customEffects, customName.
        POTION_CONTENTS => {
            optional(r, varint)?;
            optional(r, int)?;
            list(r, usize::MAX, effect)?;
            optional(r, utf8)
        }
        // SuspiciousStewEffects: a list of (MobEffect, duration).
        SUSPICIOUS_STEW_EFFECTS => list(r, usize::MAX, |r| {
            varint(r)?;
            varint(r)
        }),
        // WritableBookContent: list(100) of Filterable<stringUtf8(1024)>.
        WRITABLE_BOOK_CONTENT => list(r, 100, |r| filterable(r, |r| string(r, 1024))),
        // WrittenBookContent: title, author, generation, pages, resolved.
        WRITTEN_BOOK_CONTENT => {
            filterable(r, |r| string(r, 32))?;
            utf8(r)?;
            varint(r)?;
            list(r, usize::MAX, |r| filterable(r, tag))?;
            bool(r)
        }
        // ArmorTrim: TrimMaterial and TrimPattern holders.
        TRIM => {
            holder(r, trim_material)?;
            holder(r, |r| {
                // TrimPattern.DIRECT_STREAM_CODEC: assetId, description, decal.
                ident(r)?;
                tag(r)?;
                bool(r)
            })
        }
        // InstrumentComponent: Instrument.STREAM_CODEC.
        INSTRUMENT => holder(r, |r| {
            // Instrument.DIRECT_STREAM_CODEC: sound, useDuration, range,
            // durabilityDamage, description.
            sound(r)?;
            float(r)?;
            float(r)?;
            varint(r)?;
            tag(r)
        }),
        // TrimMaterial.STREAM_CODEC
        PROVIDES_TRIM_MATERIAL => holder(r, trim_material),
        // JukeboxPlayable: JukeboxSong.STREAM_CODEC.
        JUKEBOX_PLAYABLE => holder(r, |r| {
            // JukeboxSong.DIRECT_STREAM_CODEC: sound, description, length, comparator.
            sound(r)?;
            tag(r)?;
            float(r)?;
            varint(r)
        }),
        // LodestoneTracker: optional GlobalPos (dimension key, BlockPos), tracked.
        LODESTONE_TRACKER => {
            optional(r, |r| {
                ident(r)?;
                r.i64().map(drop)
            })?;
            bool(r)
        }
        FIREWORK_EXPLOSION => firework_explosion(r),
        // Fireworks: flightDuration, list(256) of explosions.
        FIREWORKS => {
            varint(r)?;
            list(r, 256, firework_explosion)
        }
        PROFILE => profile(r),
        // BannerPatternLayers: a list of (BannerPattern holder, DyeColor).
        BANNER_PATTERNS => list(r, usize::MAX, |r| {
            holder(r, |r| {
                // BannerPattern.DIRECT_STREAM_CODEC: assetId, translationKey.
                ident(r)?;
                utf8(r)
            })?;
            varint(r)
        }),
        // BlockItemStateProperties: a map of property names to values.
        BLOCK_STATE => list(r, usize::MAX, |r| {
            utf8(r)?;
            utf8(r)
        }),
        // Bees: a list of BeehiveBlockEntity.Occupant.
        BEES => list(r, usize::MAX, |r| {
            // TypedEntityData, ticksInHive, minTicksInHive.
            varint(r)?;
            compound(r)?;
            varint(r)?;
            varint(r)
        }),
        // SoundEvent.STREAM_CODEC
        BREAK_SOUND => sound(r),
        // Compostable: ResolvableInt layers.
        COMPOSTABLE => resolvable(r, int),
        // CookingFuel, BrewingFuel: a ResolvableInt then a ResolvableFloat.
        COOKING_FUEL | BREWING_FUEL => {
            resolvable(r, int)?;
            resolvable(r, float)
        }
        // MobVisibility: targetingEntityTypes, visibility.
        MOB_VISIBILITY => {
            holder_set(r)?;
            float(r)
        }
        // PaintingVariant.STREAM_CODEC
        PAINTING_VARIANT => holder(r, |r| {
            // DIRECT_STREAM_CODEC: width, height, assetId, title, author.
            varint(r)?;
            varint(r)?;
            ident(r)?;
            optional(r, tag)?;
            optional(r, tag)
        }),
        // SignText: four messages, optional filtered messages, color, glowing.
        SIGN_TEXT_FRONT | SIGN_TEXT_BACK => {
            (0..4).try_for_each(|_| tag(r))?;
            optional(r, |r| (0..4).try_for_each(|_| tag(r)))?;
            varint(r)?;
            bool(r)
        }
        other => Err(Error::new(format!("unsupported item component {other}"))),
    }
}

fn varint(r: &mut Reader) -> Result<()> {
    r.varint().map(drop)
}
fn int(r: &mut Reader) -> Result<()> {
    r.i32().map(drop)
}
fn float(r: &mut Reader) -> Result<()> {
    r.f32().map(drop)
}
fn bool(r: &mut Reader) -> Result<()> {
    r.bool().map(drop)
}
fn string(r: &mut Reader, max: usize) -> Result<()> {
    r.string(max).map(drop)
}
/// ByteBufCodecs.STRING_UTF8
fn utf8(r: &mut Reader) -> Result<()> {
    string(r, 32767)
}
/// Identifier.STREAM_CODEC (and ResourceKey.streamCodec): STRING_UTF8.
fn ident(r: &mut Reader) -> Result<()> {
    utf8(r)
}
/// ByteBufCodecs.TAG, as in fromCodec / fromCodecWithRegistries (and so
/// ComponentSerialization.STREAM_CODEC): any tag but End.
fn tag(r: &mut Reader) -> Result<()> {
    match r.nbt()? {
        Tag::End => Err(Error::new("expected an NBT tag")),
        _ => Ok(()),
    }
}
/// ByteBufCodecs.COMPOUND_TAG
fn compound(r: &mut Reader) -> Result<()> {
    match r.nbt()? {
        Tag::Compound(_) => Ok(()),
        _ => Err(Error::new("expected a compound tag")),
    }
}
/// ByteBufCodecs.optional: a presence flag.
fn optional(r: &mut Reader, value: impl FnOnce(&mut Reader) -> Result<()>) -> Result<()> {
    if r.bool()? { value(r) } else { Ok(()) }
}
/// ByteBufCodecs.either: true for the left codec.
fn either(
    r: &mut Reader,
    left: impl FnOnce(&mut Reader) -> Result<()>,
    right: impl FnOnce(&mut Reader) -> Result<()>,
) -> Result<()> {
    if r.bool()? { left(r) } else { right(r) }
}
/// ByteBufCodecs.list / collection / map: a VarInt count, at most `max`.
/// Every element takes at least a byte, so the count is also capped by what
/// is left.
fn list(
    r: &mut Reader,
    max: usize,
    mut element: impl FnMut(&mut Reader) -> Result<()>,
) -> Result<()> {
    let n = r.count(max.min(r.remaining()))?;
    (0..n).try_for_each(|_| element(r))
}
/// ByteBufCodecs.holder: 0 then an inline value, else a registry id + 1.
fn holder(r: &mut Reader, direct: impl FnOnce(&mut Reader) -> Result<()>) -> Result<()> {
    if r.varint()? == 0 { direct(r) } else { Ok(()) }
}
/// ByteBufCodecs.holderSet: 0 then a tag Identifier, else n + 1 then n ids.
fn holder_set(r: &mut Reader) -> Result<()> {
    match r.varint()? {
        0 => ident(r),
        n if n < 0 => Err(Error::new(format!("negative holder set size {n}"))),
        n => (1..n).try_for_each(|_| varint(r)),
    }
}
/// SoundEvent.STREAM_CODEC: a holder of (location, optional fixed range).
fn sound(r: &mut Reader) -> Result<()> {
    holder(r, |r| {
        ident(r)?;
        optional(r, float)
    })
}
/// DataComponentType.STREAM_CODEC: a registry id.
fn component_type(r: &mut Reader) -> Result<i32> {
    let kind = r.varint()?;
    if (0..COUNT).contains(&kind) {
        Ok(kind)
    } else {
        Err(Error::new(format!("unsupported item component {kind}")))
    }
}
/// ItemStackTemplate.STREAM_CODEC: item, count, DataComponentPatch.
fn template(r: &mut Reader, depth: u32) -> Result<()> {
    varint(r)?;
    varint(r)?;
    let added = r.count(r.remaining())?;
    let removed = r.count(r.remaining())?;
    for _ in 0..added {
        let kind = r.varint()?;
        skip(kind, r, depth + 1)?;
    }
    (0..removed).try_for_each(|_| component_type(r).map(drop))
}
/// Filterable.streamCodec: the raw value, then an optional filtered one.
fn filterable(r: &mut Reader, mut value: impl FnMut(&mut Reader) -> Result<()>) -> Result<()> {
    value(r)?;
    optional(r, value)
}
/// ResolvableInt / ResolvableFloat: a constant, else a provider ResourceKey.
fn resolvable(r: &mut Reader, constant: fn(&mut Reader) -> Result<()>) -> Result<()> {
    either(r, constant, ident)
}
/// TrimMaterial.DIRECT_STREAM_CODEC: paletteId, description.
fn trim_material(r: &mut Reader) -> Result<()> {
    ident(r)?;
    tag(r)
}
/// MobEffectInstance.STREAM_CODEC: the MobEffect id, then Details.
fn effect(r: &mut Reader) -> Result<()> {
    varint(r)?;
    effect_details(r, 0)
}
/// MobEffectInstance.Details: amplifier, duration, ambient, showParticles,
/// showIcon, and the (recursive) hidden effect.
fn effect_details(r: &mut Reader, depth: u32) -> Result<()> {
    if depth >= MAX_DEPTH {
        return Err(Error::new("hidden effects nested too deep"));
    }
    varint(r)?;
    varint(r)?;
    (0..3).try_for_each(|_| bool(r))?;
    optional(r, |r| effect_details(r, depth + 1))
}
/// ConsumeEffect.STREAM_CODEC: dispatched on the consume_effect_type id.
fn consume_effect(r: &mut Reader) -> Result<()> {
    match r.varint()? {
        // apply_effects: effects, probability.
        0 => {
            list(r, usize::MAX, effect)?;
            float(r)
        }
        // remove_effects: a MobEffect holderSet.
        1 => holder_set(r),
        // clear_all_effects
        2 => Ok(()),
        // teleport_randomly: diameter, directionalParticles.
        3 => {
            float(r)?;
            bool(r)
        }
        // play_sound
        4 => sound(r),
        other => Err(Error::new(format!("unknown consume effect type {other}"))),
    }
}
/// ItemAttributeModifiers.Entry: attribute, AttributeModifier (id, amount,
/// operation), EquipmentSlotGroup, Display.
fn attribute_modifier(r: &mut Reader) -> Result<()> {
    varint(r)?;
    ident(r)?;
    r.f64()?;
    varint(r)?;
    varint(r)?;
    // Display.Type ids out of range map to DEFAULT; only OVERRIDE (2) carries text.
    if r.varint()? == 2 { tag(r) } else { Ok(()) }
}
/// FireworkExplosion: shape, colors, fadeColors, hasTrail, hasTwinkle.
fn firework_explosion(r: &mut Reader) -> Result<()> {
    varint(r)?;
    list(r, usize::MAX, int)?;
    list(r, usize::MAX, int)?;
    bool(r)?;
    bool(r)
}
/// ResolvableProfile.STREAM_CODEC: a full GameProfile or a Partial one, then
/// the PlayerSkin.Patch.
fn profile(r: &mut Reader) -> Result<()> {
    either(
        r,
        |r| {
            // ByteBufCodecs.GAME_PROFILE: id, name, properties.
            r.uuid()?;
            string(r, 16)?;
            profile_properties(r)
        },
        |r| {
            // ResolvableProfile.Partial: optional name, optional id, properties.
            optional(r, |r| string(r, 16))?;
            optional(r, |r| r.uuid().map(drop))?;
            profile_properties(r)
        },
    )?;
    // PlayerSkin.Patch: body, cape, elytra textures, then PlayerModelType (slim).
    (0..3).try_for_each(|_| optional(r, ident))?;
    optional(r, bool)
}
/// ByteBufCodecs.GAME_PROFILE_PROPERTIES: at most 16 (name, value, signature).
fn profile_properties(r: &mut Reader) -> Result<()> {
    list(r, 16, |r| {
        string(r, 64)?;
        string(r, 32767)?;
        optional(r, |r| string(r, 1024))
    })
}
/// advancements.predicates.BlockPredicate: blocks, properties, nbt, components.
fn block_predicate(r: &mut Reader, depth: u32) -> Result<()> {
    optional(r, holder_set)?;
    // StatePropertiesPredicate: (name, exact value or min/max range) matchers.
    optional(r, |r| {
        list(r, usize::MAX, |r| {
            utf8(r)?;
            either(r, utf8, |r| {
                optional(r, utf8)?;
                optional(r, utf8)
            })
        })
    })?;
    // NbtPredicate
    optional(r, compound)?;
    // DataComponentMatchers: DataComponentExactPredicate (TypedDataComponents) ...
    list(r, usize::MAX, |r| {
        let kind = component_type(r)?;
        skip(kind, r, depth + 1)
    })?;
    // ... then list(64) of DataComponentPredicate.Single: a predicate or
    // component type (either), then its fromCodecWithRegistries tag.
    list(r, 64, |r| {
        bool(r)?;
        varint(r)?;
        tag(r)
    })
}
