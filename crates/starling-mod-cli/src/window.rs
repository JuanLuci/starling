//! The game's screens with slots, pretended: `container` (a chest of the scenario menu's rows)
//! and `inventory`, laid out as vanilla's ContainerScreen and InventoryScreen are, the menu's
//! stacks in their slots and the mod's window buttons (`widgets.add-window-button`) placed from
//! the slots as the game places them. The windows are plain panels, not the game's textures.
use horizon_mods::{Anchor, DrawOp, Gui, Menu, Mods, WindowButton, WindowPart};

/// A slotted screen's window on the GUI.
pub struct Window {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// The chest's rows; none for the inventory.
    rows: Option<usize>,
    /// The menu's kind, as `slots.get-menu` names it.
    pub kind: String,
}

impl Window {
    /// Screen `screen`'s window for `menu`, if it is one with slots.
    pub fn of(screen: &str, menu: Option<&Menu>, gui: &Gui) -> Option<Window> {
        let (rows, kind) = match screen {
            "inventory" => (None, "minecraft:inventory".to_owned()),
            "container" => {
                let own = menu.map_or(27, |m| m.slots.len().saturating_sub(36));
                let rows = own.div_ceil(9).clamp(1, 6);
                let kind = menu
                    .map(|m| m.kind.clone())
                    .filter(|k| k.starts_with("minecraft:generic_9x"))
                    .unwrap_or_else(|| format!("minecraft:generic_9x{rows}"));
                (Some(rows), kind)
            }
            _ => return None,
        };
        let (w, h) = (176.0, rows.map_or(166.0, |r| 114.0 + 18.0 * r as f32));
        Some(Window {
            x: ((gui.width as f32 - w) / 2.0).floor(),
            y: ((gui.height as f32 - h) / 2.0).floor(),
            w,
            h,
            rows,
            kind,
        })
    }

    /// Where menu slot `i` is drawn (its top left, in GUI pixels), as vanilla's menus put it.
    fn slot(&self, i: usize) -> Option<(f32, f32)> {
        let grid = |c: usize, r: usize, x: f32, y: f32| (x + c as f32 * 18.0, y + r as f32 * 18.0);
        let (x, y) = match self.rows {
            // ChestMenu: the container's rows, then the inventory 13 lower and the hotbar.
            Some(rows) => {
                let own = rows * 9;
                let below = (rows as f32 - 4.0) * 18.0;
                match i {
                    _ if i < own => grid(i % 9, i / 9, 8.0, 18.0),
                    _ if i < own + 27 => grid((i - own) % 9, (i - own) / 9, 8.0, 103.0 + below),
                    _ if i < own + 36 => grid(i - own - 27, 0, 8.0, 161.0 + below),
                    _ => return None,
                }
            }
            // InventoryMenu: the result, the grid, the armour, the inventory, the hotbar, the
            // off hand.
            None => match i {
                0 => (154.0, 28.0),
                1..=4 => grid((i - 1) % 2, (i - 1) / 2, 98.0, 18.0),
                5..=8 => (8.0, 8.0 + (i - 5) as f32 * 18.0),
                9..=35 => grid((i - 9) % 9, (i - 9) / 9, 8.0, 84.0),
                36..=44 => grid(i - 36, 0, 8.0, 142.0),
                45 => (77.0, 62.0),
                _ => return None,
            },
        };
        Some((self.x + x, self.y + y))
    }

    /// The menu slots of a part: the container's, or the player's inventory and hotbar.
    fn part_slots(&self, part: WindowPart) -> std::ops::Range<usize> {
        let own = self.rows.map_or(0, |r| r * 9);
        match (part, self.rows) {
            (WindowPart::Container, Some(_)) => 0..own,
            (WindowPart::Container, None) => 0..0,
            (WindowPart::Inventory, Some(_)) => own..own + 36,
            (WindowPart::Inventory, None) => 9..45,
        }
    }

    /// The box around a part's slots' cells (x, y, width, height), as the game's window
    /// buttons are placed from.
    pub fn part(&self, part: WindowPart) -> Option<[f32; 4]> {
        let cells: Vec<(f32, f32)> = self.part_slots(part).filter_map(|i| self.slot(i)).collect();
        let x0 = cells.iter().map(|c| c.0).fold(f32::MAX, f32::min) - 1.0;
        let y0 = cells.iter().map(|c| c.1).fold(f32::MAX, f32::min) - 1.0;
        let x1 = cells.iter().map(|c| c.0).fold(f32::MIN, f32::max) + 17.0;
        let y1 = cells.iter().map(|c| c.1).fold(f32::MIN, f32::max) + 17.0;
        (!cells.is_empty()).then_some([x0, y0, x1 - x0, y1 - y0])
    }

    /// The window: a panel, the titles, each slot's cell and stack.
    pub fn ops(&self, menu: Option<&Menu>) -> Vec<DrawOp> {
        let mut ops = vec![
            DrawOp::Gradient {
                x: 0.0,
                y: 0.0,
                w: 4096.0,
                h: 4096.0,
                top: 0xC010_1010,
                bottom: 0xD010_1010,
            },
            DrawOp::Panel {
                x: self.x,
                y: self.y,
                w: self.w,
                h: self.h,
            },
        ];
        let text = |x: f32, y: f32, t: &str| DrawOp::Text {
            x: self.x + x,
            y: self.y + y,
            text: t.into(),
            color: 0x40_4040,
            shadow: false,
        };
        match self.rows {
            Some(rows) => {
                ops.push(text(8.0, 6.0, "Chest"));
                ops.push(text(8.0, 18.0 * rows as f32 + 20.0, "Inventory"));
            }
            None => ops.push(text(97.0, 8.0, "Crafting")),
        }
        let count = menu.map_or(0, |m| m.slots.len());
        for i in 0..count.max(self.part_slots(WindowPart::Inventory).end) {
            let Some((x, y)) = self.slot(i) else {
                continue;
            };
            let fill =
                |x: f32, y: f32, w: f32, h: f32, color: u32| DrawOp::Fill { x, y, w, h, color };
            ops.push(fill(x - 1.0, y - 1.0, 18.0, 18.0, 0xFF37_3737));
            ops.push(fill(x, y, 17.0, 17.0, 0xFFFF_FFFF));
            ops.push(fill(x, y, 16.0, 16.0, 0xFF8B_8B8B));
            if let Some(stack) = menu.and_then(|m| m.slots.get(i)).cloned().flatten() {
                ops.push(DrawOp::Item { x, y, stack });
            }
        }
        ops
    }
}

/// An anchor as the share of the width and height it is at.
fn anchor_point(anchor: Anchor) -> (f32, f32) {
    match anchor {
        Anchor::TopLeft => (0.0, 0.0),
        Anchor::TopMiddle => (0.5, 0.0),
        Anchor::TopRight => (1.0, 0.0),
        Anchor::LeftMiddle => (0.0, 0.5),
        Anchor::Center => (0.5, 0.5),
        Anchor::RightMiddle => (1.0, 0.5),
        Anchor::BottomLeft => (0.0, 1.0),
        Anchor::BottomMiddle => (0.5, 1.0),
        Anchor::BottomRight => (1.0, 1.0),
    }
}

/// The mod's window buttons on `window`: owner, button and box, as the game places them.
pub fn buttons(mods: &Mods, window: &Window) -> Vec<(String, WindowButton, [f32; 4])> {
    mods.widgets()
        .flat_map(|(id, w)| w.window.iter().map(move |b| (id.to_owned(), b.clone())))
        .filter(|(_, b)| b.menus.is_empty() || b.menus.contains(&window.kind))
        .filter_map(|(id, b)| {
            let area = window.part(b.part)?;
            let (fx, fy) = anchor_point(b.anchor);
            let (w, h) = (b.width.floor(), b.height.floor());
            let r = [
                (area[0] + (area[2] - w) * fx + b.x).floor(),
                (area[1] + (area[3] - h) * fy + b.y).floor(),
                w,
                h,
            ];
            Some((id, b, r))
        })
        .collect()
}

/// The window buttons as the game draws them: the icon as the whole button (the highlighted one
/// under the mouse), else a plain button with the label.
pub fn button_ops(mods: &Mods, window: &Window, mouse: (f32, f32)) -> Vec<DrawOp> {
    let mut ops = Vec::new();
    let mut tooltip = None;
    for (_, b, r) in buttons(mods, window) {
        let over =
            mouse.0 >= r[0] && mouse.0 < r[0] + r[2] && mouse.1 >= r[1] && mouse.1 < r[1] + r[3];
        if b.icon.is_empty() {
            ops.push(DrawOp::Button {
                x: r[0],
                y: r[1],
                w: r[2],
                h: r[3],
                label: b.label.clone(),
                hovered: over,
                active: true,
            });
        } else {
            ops.push(DrawOp::Sprite {
                x: r[0],
                y: r[1],
                w: r[2],
                h: r[3],
                sprite: if over {
                    format!("{}_highlighted", b.icon)
                } else {
                    b.icon.clone()
                },
            });
        }
        if over && !b.tooltip.is_empty() {
            tooltip = Some(b.tooltip.clone());
        }
    }
    if let Some(lines) = tooltip {
        ops.push(DrawOp::Tooltip {
            x: mouse.0,
            y: mouse.1,
            lines,
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_where_vanillas_menus_put_the_slots() {
        let gui = Gui {
            width: 427,
            height: 240,
            scale: 3.0,
        };
        // A 3-row chest: 176 by 168 in the middle; its slots from (8, 18), the inventory's from
        // (8, 85) to the hotbar at 143.
        let chest = Window::of("container", None, &gui).unwrap();
        assert_eq!(
            (chest.x, chest.y, chest.kind.as_str()),
            (125.0, 36.0, "minecraft:generic_9x3")
        );
        assert_eq!(
            chest.part(WindowPart::Container),
            Some([132.0, 53.0, 162.0, 54.0])
        );
        assert_eq!(
            chest.part(WindowPart::Inventory),
            Some([132.0, 120.0, 162.0, 76.0])
        );
        // The inventory: no container; its 27 and the hotbar from (8, 84).
        let inventory = Window::of("inventory", None, &gui).unwrap();
        assert_eq!(inventory.part(WindowPart::Container), None);
        assert_eq!(
            inventory.part(WindowPart::Inventory),
            Some([132.0, 120.0, 162.0, 76.0])
        );
        assert!(Window::of("pause", None, &gui).is_none());
    }
}
