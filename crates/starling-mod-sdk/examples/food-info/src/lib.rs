//! Example Horizon mod in the spirit of AppleSkin (`docs/mods.md`), drawn around the vanilla
//! hunger bar with HUD elements:
//!
//! - the player's saturation, outlined in gold over the hunger icons (2 per icon);
//! - what the food in hand would restore, blinking on the icons it would fill;
//! - the exhaustion building toward the next lost hunger point, as a light band behind the bar
//!   (single player only: servers do not send it);
//! - nutrition and saturation in food tooltips.
use starling_mod_sdk::{
    Hand, HudPart, ItemStack, Mod, Placement, Registration, RenderContext, draw, game,
    inventory::{self, Food},
};

struct FoodInfo {
    /// Seconds drawn, for the blinking.
    time: f32,
}

/// Gui.extractFood: the ten icons run leftwards from `width / 2 + 91`, 8 pixels apart, 39
/// pixels above the bottom, 9 pixels square.
fn icon_x(context: &RenderContext, i: u32) -> f32 {
    (context.width / 2 + 91) as f32 - i as f32 * 8.0 - 9.0
}
fn bar_y(context: &RenderContext) -> f32 {
    context.height as f32 - 39.0
}

/// The food in the main hand, else in the off hand.
fn held_food() -> Option<Food> {
    [Hand::MainHand, Hand::OffHand]
        .into_iter()
        .find_map(|hand| {
            let stack = inventory::get_held(hand)?;
            inventory::get_item_info(&stack.id)?.food
        })
}

/// A gold outline around the icon at `x, y`; `part` (0 to 1) of it for a partly full icon,
/// from its right edge, as the icons empty.
fn outline(x: f32, y: f32, part: f32) {
    const GOLD: u32 = 0xFF_E0A51E;
    let w = 9.0 * part;
    let left = x + 9.0 - w;
    draw::fill(left, y, w, 1.0, GOLD);
    draw::fill(left, y + 8.0, w, 1.0, GOLD);
    draw::fill(x + 8.0, y, 1.0, 9.0, GOLD);
    if part >= 1.0 {
        draw::fill(x, y, 1.0, 9.0, GOLD);
    }
}

impl Mod for FoodInfo {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new()
            .hud_element("exhaustion", HudPart::Food, Placement::Before)
            .hud_element("saturation", HudPart::Food, Placement::After)
            .on_tooltips();
        Ok((FoodInfo { time: 0.0 }, hooks))
    }

    fn hud_element(&mut self, id: &str, context: RenderContext) {
        let Some(player) = game::get_player() else {
            return;
        };
        match id {
            "exhaustion" => {
                // FoodData: 4 exhaustion take one point of hunger (or saturation).
                let width = 81.0 * (player.exhaustion / 4.0).clamp(0.0, 1.0);
                if width > 0.0 {
                    let right = icon_x(&context, 0) + 9.0;
                    draw::fill(right - width, bar_y(&context), width, 9.0, 0x50_FFFFFF);
                }
            }
            "saturation" => {
                self.time += context.delta;
                for i in 0..10 {
                    let part = (player.saturation - i as f32 * 2.0).clamp(0.0, 2.0) / 2.0;
                    if part <= 0.0 {
                        break;
                    }
                    outline(icon_x(&context, i), bar_y(&context), part);
                }
                // The icons the held food would fill, blinking twice a second.
                let Some(food) = held_food() else {
                    return;
                };
                if (self.time * 4.0) as u32 % 2 == 1 {
                    return;
                }
                let after = (player.food + food.nutrition).min(20);
                for i in 0..10u32 {
                    let (low, high) = (i * 2, i * 2 + 2);
                    if after <= low || player.food >= high {
                        continue;
                    }
                    let sprite = if after >= high {
                        "hud/food_full"
                    } else {
                        "hud/food_half"
                    };
                    draw::sprite(icon_x(&context, i), bar_y(&context), 9.0, 9.0, sprite);
                }
            }
            _ => {}
        }
    }

    fn tooltip(&mut self, stack: &ItemStack) -> Vec<String> {
        let Some(food) = inventory::get_item_info(&stack.id).and_then(|i| i.food) else {
            return Vec::new();
        };
        vec![format!(
            "{{\"text\":\"+{} hunger, +{:.1} saturation\",\"color\":\"gold\"}}",
            food.nutrition, food.saturation
        )]
    }
}

starling_mod_sdk::export_mod!(FoodInfo);
