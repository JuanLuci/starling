//! Starling's capability `recipes` (docs/mods.md): the game's recipes as a recipe viewer shows
//! them (the recipe book's displays of the crafting and furnace recipes, campfire cooking,
//! stonecutting, smithing and brewing), found by what they make and what they take; and the
//! recipe book's placing of one in the open crafting grid or furnace (`recipes.fill`). Horizon's
//! capability `recipes`, its tables, is native_recipes.rs.
use super::Game;
use horizon_client::{
    crafting::Ingredient,
    items::ItemStack,
    recipe_book::{Book, BookType, Category, Display},
    stations::Smithing,
};
use horizon_mods::{Crafting, Lookup, Recipe, RecipeCatalog, RecipeKind};
use std::{collections::HashMap, sync::Arc};

/// A recipe kept with its stacks as numbers into [`Catalog::stacks`].
struct Kept {
    id: String,
    kind: RecipeKind,
    width: u32,
    height: u32,
    /// The grid a shapeless recipe fills: its inputs, and 0.
    shape: (u32, u32),
    inputs: Vec<Vec<usize>>,
    outputs: Vec<usize>,
    station: &'static str,
    time: u32,
    experience: f32,
}

/// The recipes for mods, their stacks made mods' once (names in the player's language).
pub(super) struct Catalog {
    all: Vec<Kept>,
    stacks: Vec<horizon_mods::ItemStack>,
    /// The recipes making and taking each item, in order.
    makes: HashMap<String, Vec<usize>>,
    takes: HashMap<String, Vec<usize>>,
    by_id: HashMap<String, usize>,
}

/// The kinds in a viewer's order (the enum's).
const KINDS: [RecipeKind; 8] = [
    RecipeKind::Crafting,
    RecipeKind::Smelting,
    RecipeKind::Blasting,
    RecipeKind::Smoking,
    RecipeKind::CampfireCooking,
    RecipeKind::Stonecutting,
    RecipeKind::Smithing,
    RecipeKind::Brewing,
];

impl Catalog {
    /// The recipes `lookup` finds for `stack`: by its item, and its components when it has any.
    fn found(&self, stack: &horizon_mods::ItemStack, lookup: Lookup) -> Vec<usize> {
        let index = match lookup {
            Lookup::Recipes => &self.makes,
            Lookup::Uses => &self.takes,
        };
        let Some(all) = index.get(&stack.id) else {
            return Vec::new();
        };
        let same = |&s: &usize| {
            let s = &self.stacks[s];
            s.id == stack.id && (stack.components.is_empty() || s.components == stack.components)
        };
        all.iter()
            .copied()
            .filter(|&r| {
                let r = &self.all[r];
                match lookup {
                    Lookup::Recipes => r.outputs.iter().any(same),
                    Lookup::Uses => r.inputs.iter().flatten().any(same),
                }
            })
            .collect()
    }
}

impl RecipeCatalog for Catalog {
    fn count(&self, stack: &horizon_mods::ItemStack, lookup: Lookup) -> Vec<(RecipeKind, u32)> {
        let found = self.found(stack, lookup);
        KINDS
            .iter()
            .map(|&k| {
                (
                    k,
                    found.iter().filter(|&&r| self.all[r].kind == k).count() as u32,
                )
            })
            .filter(|(_, n)| *n > 0)
            .collect()
    }
    fn recipes(
        &self,
        stack: &horizon_mods::ItemStack,
        lookup: Lookup,
        kind: RecipeKind,
        start: usize,
        count: usize,
    ) -> Vec<Recipe> {
        let stack_of = |&s: &usize| self.stacks[s].clone();
        self.found(stack, lookup)
            .into_iter()
            .map(|r| &self.all[r])
            .filter(|r| r.kind == kind)
            .skip(start)
            .take(count)
            .map(|r| Recipe {
                id: r.id.clone(),
                kind: r.kind,
                width: r.width,
                height: r.height,
                inputs: r
                    .inputs
                    .iter()
                    .map(|i| i.iter().map(stack_of).collect())
                    .collect(),
                outputs: r.outputs.iter().map(stack_of).collect(),
                station: r.station.to_owned(),
                time: r.time,
                experience: r.experience,
            })
            .collect()
    }
    fn shape(&self, id: &str) -> Option<(RecipeKind, u32, u32)> {
        let r = &self.all[*self.by_id.get(id)?];
        Some((r.kind, r.shape.0, r.shape.1))
    }
}

/// Recipes as they are made: game stacks, numbered as they come.
struct Making<'g, 'a> {
    game: &'g Game<'a>,
    all: Vec<Kept>,
    stacks: Vec<horizon_mods::ItemStack>,
    numbers: HashMap<(Arc<str>, u32, String), usize>,
}
impl Making<'_, '_> {
    fn number(&mut self, s: &ItemStack) -> usize {
        let made = self.game.mod_stack(s);
        let key = (s.item.clone(), s.count, made.components.clone());
        *self.numbers.entry(key).or_insert_with(|| {
            self.stacks.push(made);
            self.stacks.len() - 1
        })
    }
    fn items(&mut self, ingredient: &Ingredient) -> Vec<usize> {
        let stacks: Vec<ItemStack> = ingredient.1.iter().map(|i| ItemStack::new(i, 1)).collect();
        stacks.iter().map(|s| self.number(s)).collect()
    }
    fn stacks(&mut self, stacks: &[ItemStack]) -> Vec<usize> {
        stacks.iter().map(|s| self.number(s)).collect()
    }
}

impl Game<'_> {
    /// Starling's `recipes` for mods, made once for the language their names are in.
    pub(super) fn mod_recipe_catalog(&mut self) -> Arc<dyn RecipeCatalog> {
        let language = horizon_client::lang::code();
        if let Some((made_for, recipes)) = &self.mods.recipes
            && *made_for == language
        {
            return recipes.clone();
        }
        let recipes: Arc<dyn RecipeCatalog> = Arc::new(self.make_mod_recipe_catalog());
        self.mods.recipes = Some((language, recipes.clone()));
        recipes
    }
    fn make_mod_recipe_catalog(&self) -> Catalog {
        let mut m = Making {
            game: self,
            all: Vec::new(),
            stacks: Vec::new(),
            numbers: HashMap::new(),
        };
        let data = self.res.recipes();
        let cooking: HashMap<String, (u32, f32)> = data
            .cooking
            .iter()
            .map(|c| (format!("minecraft:{}", c.id), (c.time, c.experience)))
            .collect();
        // The recipe book's displays: the crafting recipes and the furnaces'.
        let book = Book::build(data, self.res.items());
        for e in &book.entries {
            let (kind, station) = match e.category {
                Category::CraftingBuilding
                | Category::CraftingRedstone
                | Category::CraftingEquipment
                | Category::CraftingMisc => (RecipeKind::Crafting, "minecraft:crafting_table"),
                Category::FurnaceFood | Category::FurnaceBlocks | Category::FurnaceMisc => {
                    (RecipeKind::Smelting, "minecraft:furnace")
                }
                Category::BlastBlocks | Category::BlastMisc => {
                    (RecipeKind::Blasting, "minecraft:blast_furnace")
                }
                Category::SmokerFood => (RecipeKind::Smoking, "minecraft:smoker"),
            };
            // The stacks some requirements show instead of their items (by their place among
            // the inputs that are not empty).
            let shown = |k: usize| e.stacks.iter().find(|(i, _)| *i == k).map(|(_, s)| s);
            let (width, height, shape, inputs) = match &e.display {
                Display::Shaped {
                    width,
                    height,
                    cells,
                } => {
                    let mut k = 0;
                    let inputs = cells
                        .iter()
                        .map(|c| match c {
                            Some(ingredient) => {
                                k += 1;
                                match shown(k - 1) {
                                    Some(stacks) => m.stacks(stacks),
                                    None => m.items(ingredient),
                                }
                            }
                            None => Vec::new(),
                        })
                        .collect();
                    let (w, h) = (*width as u32, *height as u32);
                    (w, h, (w, h), inputs)
                }
                Display::Shapeless(ingredients) => {
                    let inputs = ingredients
                        .iter()
                        .enumerate()
                        .map(|(k, i)| match shown(k) {
                            Some(stacks) => m.stacks(stacks),
                            None => m.items(i),
                        })
                        .collect();
                    (0, 0, (ingredients.len() as u32, 0), inputs)
                }
                Display::Furnace(ingredient) => (0, 0, (1, 1), vec![m.items(ingredient)]),
            };
            let outputs = m.stacks(e.results());
            let (time, experience) = cooking.get(&e.id).copied().unwrap_or_default();
            m.all.push(Kept {
                id: e.id.clone(),
                kind,
                width,
                height,
                shape,
                inputs,
                outputs,
                station,
                time,
                experience,
            });
        }
        // Campfire cooking, which no recipe book shows.
        for c in data
            .cooking
            .iter()
            .filter(|c| c.kind == "minecraft:campfire_cooking")
        {
            let inputs = vec![m.items(&c.ingredient)];
            let outputs = m.stacks(std::slice::from_ref(&c.result));
            m.all.push(Kept {
                id: format!("minecraft:{}", c.id),
                kind: RecipeKind::CampfireCooking,
                width: 0,
                height: 0,
                shape: (1, 1),
                inputs,
                outputs,
                station: "minecraft:campfire",
                time: c.time,
                experience: c.experience,
            });
        }
        let stations = self.res.stations();
        for r in &stations.recipes.stonecutting {
            let inputs = vec![m.items(&r.input)];
            let outputs = m.stacks(std::slice::from_ref(&r.result));
            m.all.push(kept(
                &r.id,
                RecipeKind::Stonecutting,
                "minecraft:stonecutter",
                inputs,
                outputs,
            ));
        }
        for r in &stations.recipes.smithing {
            let (id, template, base, addition) = match r {
                Smithing::Transform {
                    id,
                    template,
                    base,
                    addition,
                    ..
                }
                | Smithing::Trim {
                    id,
                    template,
                    base,
                    addition,
                    ..
                } => (id, template, base, addition),
            };
            // An output for each base with each addition, as a viewer cycling them together
            // shows them: the n-th with each input's n-th.
            let firsts = |i: &Ingredient| i.1.iter().map(|s| ItemStack::new(s, 1)).collect();
            let (t, b, a): (Vec<ItemStack>, Vec<ItemStack>, Vec<ItemStack>) =
                (firsts(template), firsts(base), firsts(addition));
            let n = lcm(lcm(t.len().max(1), b.len().max(1)), a.len().max(1)).min(256);
            let outputs: Vec<ItemStack> = (0..n)
                .filter_map(|k| {
                    stations.recipes.smithing_result(
                        &stations.enchantments,
                        t.get(k % t.len().max(1)),
                        b.get(k % b.len().max(1)),
                        a.get(k % a.len().max(1)),
                    )
                })
                .collect();
            let inputs = vec![m.stacks(&t), m.stacks(&b), m.stacks(&a)];
            let outputs = m.stacks(&outputs);
            m.all.push(kept(
                id,
                RecipeKind::Smithing,
                "minecraft:smithing_table",
                inputs,
                outputs,
            ));
        }
        for r in &stations.brewing.recipes {
            let stacks = |i: &horizon_client::brewing::PotionIngredient| -> Vec<ItemStack> {
                let mut items: Vec<&String> = i.items.iter().collect();
                items.sort();
                let mut potions: Vec<Option<&String>> = match &i.potions {
                    Some(p) => p.iter().map(Some).collect(),
                    None => vec![None],
                };
                potions.sort();
                items
                    .iter()
                    .flat_map(|item| {
                        potions.iter().map(move |p| {
                            let s = ItemStack::new(item, 1);
                            match p {
                                Some(p) => s.with_potion(p),
                                None => s,
                            }
                        })
                    })
                    .collect()
            };
            let inputs = vec![m.stacks(&stacks(&r.input)), m.stacks(&stacks(&r.reagent))];
            let outputs = m.stacks(std::slice::from_ref(&r.output));
            m.all.push(kept(
                &r.id,
                RecipeKind::Brewing,
                "minecraft:brewing_stand",
                inputs,
                outputs,
            ));
        }
        let mut makes: HashMap<String, Vec<usize>> = HashMap::new();
        let mut takes: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_id = HashMap::new();
        for (r, kept) in m.all.iter().enumerate() {
            by_id.entry(kept.id.clone()).or_insert(r);
            let add = |index: &mut HashMap<String, Vec<usize>>, s: &usize| {
                let list = index.entry(m.stacks[*s].id.clone()).or_default();
                if list.last() != Some(&r) {
                    list.push(r);
                }
            };
            for s in &kept.outputs {
                add(&mut makes, s);
            }
            for s in kept.inputs.iter().flatten() {
                add(&mut takes, s);
            }
        }
        Catalog {
            all: m.all,
            stacks: m.stacks,
            makes,
            takes,
            by_id,
        }
    }
    /// What the open screen's recipe book places, for `recipes.fill`.
    pub(super) fn mod_crafting(&self) -> Option<Crafting> {
        Some(match self.active_book()? {
            BookType::Crafting => Crafting::Grid(self.book_grid().0 as u32),
            BookType::Furnace => Crafting::Furnace(RecipeKind::Smelting),
            BookType::BlastFurnace => Crafting::Furnace(RecipeKind::Blasting),
            BookType::Smoker => Crafting::Furnace(RecipeKind::Smoking),
        })
    }
}

/// A recipe of one kind's own layout (no grid).
fn kept(
    id: &str,
    kind: RecipeKind,
    station: &'static str,
    inputs: Vec<Vec<usize>>,
    outputs: Vec<usize>,
) -> Kept {
    Kept {
        id: id.to_owned(),
        kind,
        width: 0,
        height: 0,
        shape: (inputs.len() as u32, 0),
        inputs,
        outputs,
        station,
        time: 0,
        experience: 0.0,
    }
}

fn lcm(a: usize, b: usize) -> usize {
    let gcd = |mut a: usize, mut b: usize| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    a / gcd(a, b) * b
}
