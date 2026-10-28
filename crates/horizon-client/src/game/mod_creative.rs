//! The mods' `creative` interface and their screens standing in for the creative screen
//! (`screens.replace`): the creative tabs and their items as mods see them, the creative screen's
//! clicks on a tab's items (`creative::click_display`), its slot and cursor sets
//! (`MultiPlayerGameMode.handleCreativeModeItemAdd`), and the swap between the game's creative
//! screen and a mod's.
use super::{Game, Screen};
use horizon_client::{
    creative::{Creative, DisplayClick, Names, click_display},
    items::ItemStack,
};
use horizon_mods::{CreativeCatalog, CreativeClick, ItemPage, Tab, TabKind};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

/// The game screen a mod's screen can stand in for.
pub(super) const CREATIVE: &str = "creative";

/// The creative tabs as mods read them (`creative.get-tabs`, `get-items`): the game's tabs, each
/// stack made a mod's once, and the creative search for the search tab.
pub(super) struct CreativeTabs {
    tabs: Vec<Tab>,
    /// Each tab's stacks, by its place in `tabs` (the search tab's: every stack it searches).
    items: Vec<Vec<horizon_mods::ItemStack>>,
    creative: Creative,
    names: Arc<Names>,
    /// The last search and its matches, by their index in the search tab's stacks.
    found: Mutex<Option<(String, Arc<Vec<usize>>)>>,
    /// Each stack's first place in the search tab, by its item and components and by its item
    /// alone (`inventory.get-item-order`), made when first asked.
    places: OnceLock<Places>,
}

/// The search tab's places: by (id, components), and by id.
type Places = (HashMap<(String, String), u32>, HashMap<String, u32>);

impl CreativeCatalog for CreativeTabs {
    fn tabs(&self) -> Vec<Tab> {
        self.tabs.clone()
    }
    fn items(&self, tab: &str, search: &str, start: usize, count: usize) -> ItemPage {
        let empty = ItemPage {
            total: 0,
            items: Vec::new(),
        };
        let Some(i) = self.tabs.iter().position(|t| t.id == tab) else {
            return empty;
        };
        let all = &self.items[i];
        if self.tabs[i].kind != TabKind::Search {
            return ItemPage {
                total: all.len() as u32,
                items: all.iter().skip(start).take(count).cloned().collect(),
            };
        }
        let found = {
            let mut last = self.found.lock().unwrap_or_else(|e| e.into_inner());
            match last.as_ref() {
                Some((term, found)) if term == search => found.clone(),
                _ => {
                    let found = Arc::new(self.creative.search_results(search, &self.names));
                    *last = Some((search.to_owned(), found.clone()));
                    found
                }
            }
        };
        ItemPage {
            total: found.len() as u32,
            items: found
                .iter()
                .skip(start)
                .take(count)
                .filter_map(|&j| all.get(j).cloned())
                .collect(),
        }
    }
    fn order(&self, stack: &horizon_mods::ItemStack) -> Option<u32> {
        let (exact, item) = self.places.get_or_init(|| {
            let mut places: Places = Default::default();
            if let Some(i) = self.tabs.iter().position(|t| t.kind == TabKind::Search) {
                for (place, s) in self.items[i].iter().enumerate() {
                    let place = place as u32;
                    places
                        .0
                        .entry((s.id.clone(), s.components.clone()))
                        .or_insert(place);
                    places.1.entry(s.id.clone()).or_insert(place);
                }
            }
            places
        });
        exact
            .get(&(stack.id.clone(), stack.components.clone()))
            .or_else(|| item.get(&stack.id))
            .copied()
    }
}

impl Game<'_> {
    /// The creative tabs for mods, made once for the language their names are in.
    pub(super) fn mod_creative_tabs(&mut self) -> Arc<dyn CreativeCatalog> {
        let language = horizon_client::lang::code();
        if let Some((made_for, tabs)) = &self.mods.creative
            && *made_for == language
        {
            return tabs.clone();
        }
        let creative = self.creative_screen.clone();
        let made = |game: &Self, stacks: &[ItemStack]| -> Vec<horizon_mods::ItemStack> {
            stacks.iter().map(|s| game.mod_stack(s)).collect()
        };
        let mut tabs = Vec::with_capacity(creative.tabs.len());
        let mut items = Vec::with_capacity(creative.tabs.len());
        for t in &creative.tabs {
            let kind = if t.search {
                TabKind::Search
            } else if t.inventory {
                TabKind::Inventory
            } else {
                TabKind::Category
            };
            let stacks = if t.search {
                made(self, creative.search_items())
            } else {
                made(self, &t.items)
            };
            tabs.push(Tab {
                id: t.id.clone(),
                title: t.title.clone(),
                icon: t.icon.clone(),
                kind,
                top: t.top,
                column: t.column,
                size: stacks.len() as u32,
            });
            items.push(stacks);
        }
        let tabs: Arc<dyn CreativeCatalog> = Arc::new(CreativeTabs {
            tabs,
            items,
            creative,
            names: self.pack.names.clone(),
            found: Mutex::new(None),
            places: OnceLock::new(),
        });
        self.mods.creative = Some((language, tabs.clone()));
        tabs
    }

    /// A creative screen is open, the game's or a mod's standing in for it: the player's
    /// inventory menu is clicked here, each slot it changes sent to a server as the creative
    /// screen sends them (SetCreativeModeSlot).
    pub(super) fn creative_menu_open(&self) -> bool {
        self.screen == Screen::Creative || self.mods.standing_in()
    }

    /// `creative.click`: CreativeModeInventoryScreen.slotClicked on a tab's slot showing item
    /// `index` of what `tab` lists for `search`.
    pub(super) fn mod_creative_click(
        &mut self,
        tab: &str,
        search: &str,
        index: u32,
        button: u8,
        kind: CreativeClick,
    ) {
        use horizon_client::menu::Input;
        if !self.creative {
            return;
        }
        let input = match kind {
            CreativeClick::Pickup => Input::Pickup,
            CreativeClick::QuickMove => Input::QuickMove,
            CreativeClick::Swap => Input::Swap,
            CreativeClick::Clone => Input::Clone,
            CreativeClick::Throw => Input::Throw,
        };
        let shown = self
            .creative_screen
            .listed(tab, search, index as usize, &self.pack.names)
            .cloned();
        let catalog = self.res.items();
        let max = |s: &ItemStack| catalog.get(&s.item).map_or(64, |i| i.max_stack);
        let effect = click_display(
            &mut self.carried,
            shown.as_ref(),
            max,
            i32::from(button),
            input,
        );
        self.creative_display_effect(effect);
    }

    /// What a click on a creative tab's slot does besides the carried stack: a hotbar slot (or
    /// the off hand, button 40: `Inventory.setItem`) filled, a copy dropped.
    pub(super) fn creative_display_effect(&mut self, effect: DisplayClick) {
        match effect {
            DisplayClick::Hotbar(i, stack) => {
                let before = self.player_equipment();
                match i {
                    0..9 => self.inventory[i] = Some(stack),
                    40 => self.offhand = Some(stack),
                    _ => return,
                }
                self.player_equipment_changed(&before);
                self.apply_equipment();
            }
            DisplayClick::Offhand(stack) => {
                self.creative_display_effect(DisplayClick::Hotbar(40, stack))
            }
            DisplayClick::Drop(stack) => self.creative_drop(stack),
            DisplayClick::Nothing => {}
        }
    }

    /// `creative.set-slot`: `handleCreativeModeItemAdd` on the player's inventory menu's slot
    /// `slot` (1 to 45): set here, and sent to a server at once.
    pub(super) fn mod_creative_slot(&mut self, slot: u32, stack: Option<&horizon_mods::ItemStack>) {
        if !self.creative {
            return;
        }
        let stack = stack.and_then(|s| self.game_stack(s));
        // The inventory menu's slot as the game's (InventoryMenu's layout).
        let game = match slot {
            // The 2x2 grid is the inventory's only while no crafting table or container is open.
            1..=4 if self.craft_size == 2 && self.container.is_none() && self.station.is_none() => {
                100 + slot as usize - 1
            }
            5..=8 => 500 + slot as usize - 5,
            9..=35 => slot as usize,
            36..=44 => slot as usize - 36,
            45 => 504,
            _ => return,
        };
        let before = self.player_equipment();
        *self.slot_mut(game) = stack.clone();
        self.player_equipment_changed(&before);
        self.apply_equipment();
        self.remote_creative_slot(slot as i16, game, &stack);
    }

    /// `creative.set-carried`: the creative screen's cursor (`menu.setCarried`).
    pub(super) fn mod_creative_carried(&mut self, stack: Option<&horizon_mods::ItemStack>) {
        if self.creative {
            self.carried = stack.and_then(|s| self.game_stack(s));
        }
    }

    /// Once a frame, before drawing: the game's creative screen gives way to the mod screen
    /// standing in for it (`screens.replace`), and comes back when no mod stands in any more;
    /// outside creative mode the stand-in closes for the survival inventory
    /// (CreativeModeInventoryScreen.containerTick).
    pub(super) fn swap_creative_screen(&mut self) {
        let replacement = self.mods.mods.replacement(CREATIVE);
        if self.mods.replacing {
            let open = self.mods.screen.clone();
            if open.is_none() {
                self.end_creative_stand_in(false);
                return;
            }
            // Out of the world, or another of the game's screens came up under it (death, a
            // container a server opened): the stand-in gives way.
            if self.player.is_none() || self.title.is_some() || self.screen != Screen::Playing {
                self.close_mod_screen();
                return;
            }
            if !self.creative {
                self.close_mod_screen();
                self.screen = Screen::Inventory;
                self.craft_size = 2;
                self.capture(false);
                return;
            }
            if replacement != open {
                // Given back: the game's own screen in its place, the cursor's stack kept.
                self.end_creative_stand_in(true);
                self.mods.screen = None;
                self.mods.held = None;
                self.mods.keyboard = false;
                self.screen = Screen::Creative;
            }
            return;
        }
        if self.screen == Screen::Creative
            && self.title.is_none()
            && self.mods.screen.is_none()
            && let Some(name) = replacement
        {
            self.open_mod_screen(name);
        }
    }

    /// The mod screen opening is the creative screen's stand-in: in creative mode, the one
    /// `screens.replace` names. The game's creative screen gives way to it (its cursor kept).
    pub(super) fn begin_creative_stand_in(&mut self, screen: &str) {
        let standing = self.creative
            && self.player.is_some()
            && self.mods.mods.replacement(CREATIVE).as_deref() == Some(screen);
        if self.mods.replacing && !standing {
            self.end_creative_stand_in(false);
        }
        if !standing {
            return;
        }
        if self.screen == Screen::Creative {
            self.screen = Screen::Playing;
        } else if self.screen == Screen::Inventory {
            self.close_container();
            self.screen = Screen::Playing;
        }
        // The player's own menu: its 2x2 grid.
        if self.container.is_none() && self.station.is_none() {
            self.craft_size = 2;
        }
        self.mods.replacing = true;
    }

    /// The stand-in closes as the creative screen does: the cursor's stack destroyed (unless it
    /// goes on to the game's creative screen), the grid's items back in the inventory, a server
    /// told of the last slots changed and of the menu closing.
    pub(super) fn end_creative_stand_in(&mut self, keep_carried: bool) {
        if !self.mods.replacing {
            return;
        }
        self.mods.replacing = false;
        let carried = self.carried.take().filter(|_| keep_carried);
        // The slots changed since the last tick, before the server's menu closes.
        self.remote_creative_menu_sync();
        self.close_container();
        self.carried = carried;
    }
}
