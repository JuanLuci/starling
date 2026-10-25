// EMI: every item beside the screens with a window (the inventory, a chest, a crafting table, a
// furnace, the creative screen), and the recipes making each and taking it in.
//
// - The index, right of the window: the creative inventory's items in its order, a page at a
//   time (the arrows over it, or the wheel), searched as the creative search searches by the
//   box under the window (a tap on a phone brings up the keyboard).
// - Favourites, left of the window: A over an item adds it, A again takes it away, as does the
//   star on its recipes screen (a phone has no A); kept.
// - R over an item, or a click (a tap) on one in the index or a recipe: its recipes. U, or a right
//   click: its uses. Over the game's slots too.
// - The recipes screen: a tab for each station there are recipes at (crafting table, furnaces,
//   campfire, stonecutter, smithing table, brewing stand) with how many, the recipes laid out as
//   their station's window lays them, a slot's choices shown in turn each second; Recipes and Uses
//   switch over, Backspace or Back goes back. The + beside a crafting or furnace recipe puts its
//   ingredients in the open grid or furnace, as the recipe book does; ++ (or Shift +) as many as
//   can be. Every key has a button, for a phone.
import { button, item, itemId, itemTooltip, fill, panel, sprite, text, textWidth, tooltip }
  from "starling:mods/draw@0.1.0";
import { getTabs, getItems } from "starling:mods/creative@0.1.0";
import { countRecipes, getRecipes, fill as fillRecipe } from "starling:mods/recipes@0.1.0";
import { getHovered } from "starling:mods/inventory@0.1.0";
import { open, close, getWindow } from "starling:mods/screens@0.1.0";
import { keyboard, hasTouch } from "starling:mods/mobile@0.1.0";
import { showOverlay } from "starling:mods/chat@0.1.0";
import { get, set } from "starling:mods/storage@0.1.0";
import { write } from "starling:mods/log@0.1.0";

const SLOT = 18;
// The game screens with a window the index goes beside.
const WINDOWED = ["inventory", "container", "creative"];
const RECIPES = "emi:recipes";
// Where each kind of recipe is made, and its name.
const KINDS = {
  crafting: ["minecraft:crafting_table", "Crafting"],
  smelting: ["minecraft:furnace", "Smelting"],
  blasting: ["minecraft:blast_furnace", "Blasting"],
  smoking: ["minecraft:smoker", "Smoking"],
  "campfire-cooking": ["minecraft:campfire", "Campfire Cooking"],
  stonecutting: ["minecraft:stonecutter", "Stonecutting"],
  smithing: ["minecraft:smithing_table", "Smithing"],
  brewing: ["minecraft:brewing_stand", "Brewing"],
};
// The kinds the recipe book's + fills.
const FILLED = ["crafting", "smelting", "blasting", "smoking"];

// A call into the game that may refuse: logged, not fatal.
function act(f, or) {
  try {
    return f();
  } catch (e) {
    write("debug", `${e.payload === undefined ? e : e.payload}`);
    return or;
  }
}
const key = (s) => `${s.id}\u0000${s.components}`;
const inside = (r, x, y) => x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];

// ---- the index ----

let searchTab = null;
let search = "";
let searching = false;
let page = 0;
// The search's pages as asked: (search, start, count) -> {items, total}.
let pages = new Map();
function indexPage(start, count) {
  if (searchTab === null) {
    const tab = act(() => getTabs(), []).find((t) => t.kind === "search");
    searchTab = tab ? tab.id : "";
  }
  const k = `${search}\u0000${start},${count}`;
  let got = pages.get(k);
  if (!got) {
    if (pages.size > 64) pages = new Map();
    got = act(() => getItems(searchTab, search, start, count), { items: [], total: 0 });
    pages.set(k, got);
  }
  return got;
}

// ---- favourites, kept ----

let favourites = [];
const saved = get("favourites");
if (saved) {
  favourites = act(() => JSON.parse(String.fromCharCode(...saved)), []);
}
function saveFavourites() {
  const json = JSON.stringify(favourites);
  const bytes = new Uint8Array(json.length);
  for (let i = 0; i < json.length; i++) bytes[i] = json.charCodeAt(i) & 0xFF;
  act(() => set("favourites", bytes));
}
function toggleFavourite(s) {
  const at = favourites.findIndex((f) => key(f) === key(s));
  if (at >= 0) {
    favourites.splice(at, 1);
  } else {
    favourites.push({ id: s.id, count: 1, damage: 0, maxDamage: s.maxDamage, name: s.name, components: s.components });
  }
  saveFavourites();
}

// ---- the layout beside a window ----

// The index's and the favourites' grids, the search box and the page arrows for a GUI and the
// window on it (the creative screen's tabs 28 pixels under it). On a touch screen the game's
// close button takes the top right corner: the grids start under it, the arrows go under them.
let below = 0;
function layout(width, height, win) {
  const [wx, wy, ww, wh0] = win || [width / 2 - 88, height / 2 - 83, 176, 166];
  const wh = wh0 + below;
  const touch = hasTouch();
  const top = touch ? 40 : 16, bottom = height - 24;
  const arrows = touch ? height - 20 : 2;
  const rows = Math.max(0, Math.floor((bottom - top) / SLOT));
  const right = width - 2, left = wx + ww + 4;
  const cols = Math.max(0, Math.floor((right - left) / SLOT));
  const favCols = Math.max(0, Math.floor((wx - 6) / SLOT));
  const boxWidth = Math.min(160, width - 8);
  return {
    index: { x: right - cols * SLOT, y: top, cols, rows },
    favourites: { x: 2, y: top, cols: favCols, rows },
    box: [Math.round(width / 2 - boxWidth / 2), Math.min(height - 20, wy + wh + 4), boxWidth, 16],
    back: [right - cols * SLOT, arrows, 12, 12],
    next: [right - 12, arrows, 12, 12],
  };
}
// The slot of a grid at a point: its number, or -1.
function cell(g, x, y) {
  const c = Math.floor((x - g.x) / SLOT), r = Math.floor((y - g.y) / SLOT);
  if (c < 0 || c >= g.cols || r < 0 || r >= g.rows) return -1;
  return r * g.cols + c;
}

// The stacks under the mouse on a game screen now, drawn there last frame.
let mouse = [0, 0];
let shown = { index: [], favourites: [] };

function drawGrid(g, stacks, mx, my) {
  let hovered = null;
  for (let i = 0; i < stacks.length; i++) {
    const x = g.x + (i % g.cols) * SLOT, y = g.y + Math.floor(i / g.cols) * SLOT;
    if (isSelected(stacks[i], [x, y, SLOT, SLOT])) {
      // The touch inventory's pick: a white frame and a veil.
      fill(x, y, SLOT, 1, 0xFFFFFFFF);
      fill(x, y + SLOT - 1, SLOT, 1, 0xFFFFFFFF);
      fill(x, y + 1, 1, SLOT - 2, 0xFFFFFFFF);
      fill(x + SLOT - 1, y + 1, 1, SLOT - 2, 0xFFFFFFFF);
      fill(x + 1, y + 1, SLOT - 2, SLOT - 2, 0x3CFFFFFF);
    } else if (!hasTouch() && inside([x, y, SLOT, SLOT], mx, my)) {
      fill(x, y, SLOT, SLOT, 0x80FFFFFF);
      hovered = stacks[i];
    }
    item(x + 1, y + 1, stacks[i]);
  }
  return hovered;
}

// The index and the favourites beside a game screen's window (or the recipes screen), and their
// hovered or picked stack's tooltip (the recipes screen draws its own).
function drawBeside(context, win, tooltips = true) {
  const L = layout(context.width, context.height, win);
  const per = L.index.cols * L.index.rows;
  mouse = [context.mouseX, context.mouseY];
  let hovered = null;
  if (per > 0) {
    const got = indexPage(page * per, per);
    const pagesTotal = Math.max(1, Math.ceil(got.total / per));
    if (page >= pagesTotal) page = pagesTotal - 1;
    shown.index = got.items;
    hovered = drawGrid(L.index, got.items, ...mouse);
    sprite(...L.back, inside(L.back, ...mouse) ? "recipe_book/page_backward_highlighted" : "recipe_book/page_backward");
    sprite(...L.next, inside(L.next, ...mouse) ? "recipe_book/page_forward_highlighted" : "recipe_book/page_forward");
    const label = `${page + 1}/${pagesTotal}`;
    const mid = L.index.x + (L.index.cols * SLOT) / 2;
    text(Math.round(mid - textWidth(label) / 2), L.back[1] + 2, label, 0xFFFFFF, true);
  }
  if (L.favourites.cols > 0) {
    shown.favourites = favourites.slice(0, L.favourites.cols * L.favourites.rows);
    hovered = drawGrid(L.favourites, shown.favourites, ...mouse) || hovered;
  }
  // The search box, as the game's text fields.
  const [bx, by, bw, bh] = L.box;
  fill(bx, by, bw, bh, searching ? 0xFFFFFFFF : 0xFFA0A0A0);
  fill(bx + 1, by + 1, bw - 2, bh - 2, 0xFF000000);
  const shownText = search === "" && !searching ? "§7Search" : `${search}${searching ? "_" : ""}`;
  text(bx + 4, by + 4, shownText, 0xE0E0E0, true);
  if (!tooltips) return;
  if (hovered) {
    itemTooltip(...mouse, hovered);
  } else if (selected) {
    const r = selected.rect;
    itemTooltip(r[0] + r[2] - 4, r[1] + 4, selected.stack);
  }
}

// What a press on a game screen does beside its window: true when it was the mod's.
function pressBeside(context, win, x, y, buttonId) {
  const L = layout(context.width, context.height, win);
  if (inside(L.box, x, y)) {
    searching = true;
    if (hasTouch()) keyboard(true);
    return true;
  }
  if (searching) stopSearching();
  if (inside(L.back, x, y)) {
    page = Math.max(0, page - 1);
    return true;
  }
  if (inside(L.next, x, y)) {
    page++;
    return true;
  }
  const at = (g, n) => [g.x + (n % g.cols) * SLOT, g.y + Math.floor(n / g.cols) * SLOT, SLOT, SLOT];
  const i = cell(L.index, x, y);
  if (i >= 0 && shown.index[i]) {
    pressStack(shown.index[i], at(L.index, i), buttonId);
    return true;
  }
  const f = cell(L.favourites, x, y);
  if (f >= 0 && shown.favourites[f]) {
    pressStack(shown.favourites[f], at(L.favourites, f), buttonId);
    return true;
  }
  return false;
}
function stopSearching() {
  searching = false;
  if (hasTouch()) keyboard(false);
}
// The stack under the mouse beside the window, else in the game's slot under it.
function hoveredBeside(context, win) {
  const L = layout(context.width, context.height, win);
  const i = cell(L.index, ...mouse);
  if (i >= 0 && shown.index[i]) return shown.index[i];
  const f = cell(L.favourites, ...mouse);
  if (f >= 0 && shown.favourites[f]) return shown.favourites[f];
  return act(() => getHovered(), null);
}

// ---- what the recipes screen shows ----

// What it shows: the stack, recipes or uses, the tabs (kind, how many), the tab and page and its
// recipes, and where it came from. The recipes are fetched when the view changes, in the hook of
// the click or key that changed it: a page of them can be hundreds of stacks (a slot taking any
// log lists them all), more than a frame's drawing may spend.
let view = null;
const history = [];
function lookUp(stack, lookup) {
  const tabs = act(() => countRecipes(stack, lookup), []);
  return { stack, lookup, tabs, tab: 0, page: 0, list: [] };
}
// The view's page of recipes, for the recipes screen's size.
function load(v) {
  const tab = v.tabs[v.tab];
  if (!tab) {
    v.list = [];
    return;
  }
  const L = screenLayout(lastContext.width, lastContext.height);
  const per = perPage(v, L.area);
  const pages = Math.max(1, Math.ceil(tab[1] / per));
  v.page = ((v.page % pages) + pages) % pages;
  v.list = act(() => getRecipes(v.stack, v.lookup, tab[0], v.page * per, per), []);
}
// Opens the recipes (or uses) of a stack, the view before it kept to go back to.
function show(held, lookup) {
  selected = null;
  // The stack itself, not how many of it.
  const stack = { ...held, count: 1 };
  const next = lookUp(stack, lookup);
  if (next.tabs.length === 0) {
    act(() => showOverlay(`No ${lookup} for ${stack.name}`));
    return;
  }
  if (view) history.push(view);
  if (history.length > 32) history.shift();
  view = next;
  load(view);
  open("recipes");
}

// ---- drawing as the game's windows do ----

const BLACK = 0xFF000000, WHITE = 0xFFFFFFFF, FACE = 0xFFC6C6C6, SHADE = 0xFF555555;
const SLOT_DARK = 0xFF373737, SLOT_FACE = 0xFF8B8B8B;

// A window's panel as vanilla's container textures draw it: light grey, white over its top and
// left, dark grey under its bottom and right, a black edge with rounded corners.
function windowPanel(x, y, w, h, face) {
  fill(x + 1, y + 2, w - 2, h - 4, face);
  fill(x + 2, y + 1, w - 5, 1, WHITE);
  fill(x + 1, y + 2, w - 4, 1, WHITE);
  fill(x + 1, y + 3, 2, h - 6, WHITE);
  fill(x + 3, y + 3, 1, 1, WHITE);
  fill(x + w - 3, y + 3, 2, h - 6, SHADE);
  fill(x + 3, y + h - 3, w - 4, 1, SHADE);
  fill(x + 3, y + h - 2, w - 5, 1, SHADE);
  fill(x + w - 4, y + h - 4, 1, 1, SHADE);
  fill(x + 2, y, w - 5, 1, BLACK);
  fill(x + 3, y + h - 1, w - 5, 1, BLACK);
  fill(x, y + 2, 1, h - 5, BLACK);
  fill(x + w - 1, y + 3, 1, h - 5, BLACK);
  for (const [px, py] of [[1, 1], [w - 3, 1], [w - 2, 2], [1, h - 3], [2, h - 2], [w - 2, h - 2]]) {
    fill(x + px, y + py, 1, 1, BLACK);
  }
}
// A sunken box, as a slot's frame: dark over its top and left, white under its bottom and right.
function sunken(x, y, w, h) {
  fill(x, y, w, h, SLOT_DARK);
  fill(x + 1, y + 1, w - 1, h - 1, WHITE);
  fill(x + 1, y + 1, w - 2, h - 2, SLOT_FACE);
}
// The crafting table's arrow (22 by 15), and in white the part of it `done` (0 to 1) of the way.
function arrow(x, y, done) {
  const shape = (color, width) => {
    fill(x, y + 6, Math.min(14, width), 3, color);
    for (let i = 0; i < 8 && 14 + i < width; i++) fill(x + 14 + i, y + i, 1, 15 - 2 * i, color);
  };
  shape(SLOT_FACE, 22);
  if (done > 0) shape(WHITE, Math.round(22 * done));
}
// A small vanilla button with its label centred to the pixel ("+", "++"), or an arrow drawn in
// pixels ("<", ">"), shadowed as the font is.
function iconButton(r, icon, hovered) {
  button(r[0], r[1], r[2], r[3], "", hovered, true);
  if (icon !== "<" && icon !== ">") {
    // The font's widths count a pixel of space after each character.
    text(r[0] + Math.round((r[2] - textWidth(icon) + 1) / 2), r[1] + Math.round((r[3] - 8) / 2) + 1,
      icon, 0xFFFFFF, true);
    return;
  }
  const cx = r[0] + Math.floor(r[2] / 2), cy = r[1] + Math.floor(r[3] / 2);
  const rows = [];
  for (let i = 0; i < 7; i++) {
    const off = Math.abs(3 - i);
    rows.push([icon === "<" ? cx - 3 + off : cx - 1 - off, cy - 4 + i]);
  }
  for (const [x, y] of rows) fill(x + 1, y + 1, 2, 1, 0xFF3F3F3F);
  for (const [x, y] of rows) fill(x, y, 2, 1, WHITE);
}
// A station's tab over the panel, the one shown joined to it.
function tab(x, y, h, shown) {
  windowPanel(x, y, 24, h + 4, shown ? FACE : 0xFFA8A8A8);
  if (shown) {
    // Open into the panel: its face over the panel's edge, its sides running on.
    fill(x + 3, y + h, 18, 4, FACE);
    fill(x + 1, y + h, 2, 4, WHITE);
    fill(x + 21, y + h, 2, 4, SHADE);
  }
}

// ---- the recipes screen ----

// A recipe as EMI lays it out: its size, its slots (x, y from its corner, the stacks, large),
// its arrow and flame.
function recipeDisplay(r) {
  const slots = [];
  const inputs = r.inputs;
  switch (r.kind) {
    case "crafting": {
      // The whole 3 by 3 grid; a shape in its corner, a shapeless recipe's ingredients in turn
      // in rows of 3 (of 2 for up to 4); the result in a large slot.
      const ww = r.width || (inputs.length > 4 ? 3 : inputs.length > 1 ? 2 : 1);
      const cells = new Array(9).fill(null);
      for (let i = 0; i < inputs.length; i++) cells[Math.floor(i / ww) * 3 + (i % ww)] = inputs[i];
      for (let i = 0; i < 9; i++) slots.push([(i % 3) * SLOT, Math.floor(i / 3) * SLOT, cells[i] || [], false]);
      slots.push([92, 14, r.outputs, true]);
      return { width: 118, height: 54, slots, arrow: [62, 19], shapeless: r.width === 0 };
    }
    case "smithing":
    case "brewing": {
      for (let i = 0; i < inputs.length; i++) slots.push([i * SLOT, 0, inputs[i], false]);
      const ax = inputs.length * SLOT + 8;
      slots.push([ax + 32, 0, r.outputs, false]);
      return { width: ax + 50, height: SLOT, slots, arrow: [ax, 1] };
    }
    case "stonecutting":
      slots.push([0, 0, inputs[0] || [], false]);
      slots.push([58, 0, r.outputs, false]);
      return { width: 76, height: SLOT, slots, arrow: [27, 1] };
    default:
      // Cooking: the input over the flame, the arrow filling over the cooking time, a large
      // result, the experience under the arrow.
      slots.push([0, 4, inputs[0] || [], false]);
      slots.push([56, 0, r.outputs, true]);
      return { width: 82, height: 38, slots, arrow: [24, 5], flame: [2, 24], cooking: true };
  }
}
const RECIPE_HEIGHTS = { crafting: 54, smithing: SLOT, brewing: SLOT, stonecutting: SLOT };
const recipeHeight = (kind) => RECIPE_HEIGHTS[kind] || 38;
const RECIPE_PADDING = 10;

// The recipes screen's parts on a GUI, as EMI places them: the panel 176 wide, the stations'
// tabs over it, a bar for the station and one for the page, the recipes, the stations' strip on
// its left; and a row of buttons for what a phone has no keys for.
function screenLayout(width, height) {
  const w = 176, h = Math.min(256, Math.max(120, height - 52));
  const x = Math.round((width - w) / 2), y = Math.round((height - h) / 2) + 1;
  const foot = y + h - 23;
  return {
    panel: [x, y, w, h],
    strip: [x - 23, y + 4, 28, 28],
    station: [x - 18, y + 9, SLOT, SLOT],
    tabs: [x + 16, y - 24],
    tabsShown: 6,
    tabBack: [x + 2, y - 18, 12, 12],
    tabNext: [x + w - 14, y - 18, 12, 12],
    kindBack: [x + 5, y + 5, 12, 12],
    kindBar: [x + 19, y + 5, w - 38, 12],
    kindNext: [x + w - 17, y + 5, 12, 12],
    pageBack: [x + 5, y + 19, 12, 12],
    pageBar: [x + 19, y + 19, w - 38, 12],
    pageNext: [x + w - 17, y + 19, 12, 12],
    area: [x + 8, y + 37, w - 16, foot - 4 - (y + 37)],
    back: [x + 8, foot + 1, 32, 16],
    subject: [x + 43, foot, SLOT, SLOT],
    recipesButton: [x + 63, foot + 1, 44, 16],
    usesButton: [x + 109, foot + 1, 36, 16],
    star: [x + 148, foot + 1, 20, 16],
    // The window the index and the favourites go beside.
    window: [x - 23, y, w + 23, h],
  };
}
function perPage(v, area) {
  const tab = v.tabs[v.tab];
  if (!tab) return 1;
  const rh = recipeHeight(tab[0]) + RECIPE_PADDING;
  return Math.max(1, Math.min(16, Math.floor((area[3] + RECIPE_PADDING) / rh)));
}

let seconds = 0;
// What the recipes screen drew last: stacks by box, the + and ++ buttons by recipe (and whether
// as many as can be), the tabs.
let hits = { stacks: [], fills: [], tabs: [] };
// On a touch screen a tap picks a stack, showing its tooltip and making it what the buttons
// under the recipes act on; a second tap on it shows its recipes. { stack, rect } or null.
let selected = null;
const isSelected = (s, r) => selected && key(selected.stack) === key(s) && selected.rect.join() === r.join();
// The stack the buttons under the recipes act on: the one picked, else the one looked up.
const subject = () => (selected ? selected.stack : view && view.stack);

// A slot with its stack (the choice of this second), hovered or picked as the game shows it.
function drawSlot(at, s, large, mx, my) {
  if (large) sunken(at[0], at[1], 26, 26);
  else sprite(at[0], at[1], SLOT, SLOT, "container/slot");
  if (!s) return null;
  const ix = at[0] + (large ? 5 : 1), iy = at[1] + (large ? 5 : 1);
  item(ix, iy, s);
  hits.stacks.push([at, s]);
  if (selected && isSelected(s, at)) {
    // The touch inventory's pick: a white frame and a veil.
    fill(ix - 1, iy - 1, 18, 1, WHITE);
    fill(ix - 1, iy + 16, 18, 1, WHITE);
    fill(ix - 1, iy, 1, 16, WHITE);
    fill(ix + 16, iy, 1, 16, WHITE);
    fill(ix, iy, 16, 16, 0x3CFFFFFF);
  } else if (!hasTouch() && inside(at, mx, my)) {
    fill(ix, iy, 16, 16, 0x80FFFFFF);
    return s;
  }
  return null;
}

function drawRecipes(context) {
  hits = { stacks: [], fills: [], tabs: [] };
  if (!view) return;
  const L = screenLayout(context.width, context.height);
  const [px, py, pw, ph] = L.panel;
  const mx = hasTouch() ? -1 : context.mouseX, my = hasTouch() ? -1 : context.mouseY;
  let hovered = null, hint = null;
  const tabs = view.tabs;
  const tabPage = Math.floor(view.tab / L.tabsShown);
  // The tabs not shown behind the panel, the shown one joined to it over it.
  const tabAt = (i) => [L.tabs[0] + (i - tabPage * L.tabsShown) * 24, L.tabs[1], 24, 24];
  const first = tabPage * L.tabsShown, last = Math.min(tabs.length, first + L.tabsShown);
  for (let i = first; i < last; i++) {
    const r = tabAt(i);
    if (i !== view.tab) tab(r[0], r[1], 27, false);
    hits.tabs.push([r, i]);
    if (inside(r, mx, my)) {
      const [kind, n] = tabs[i];
      hint = [KINDS[kind][1], `§7${n} ${n === 1 ? "recipe" : "recipes"}`];
    }
  }
  // The stations' strip, then the panel over its edge.
  windowPanel(...L.strip, FACE);
  windowPanel(px, py, pw, ph, FACE);
  const shownTab = tabAt(view.tab);
  if (view.tab >= first && view.tab < last) tab(shownTab[0], shownTab[1] - 2, 29, true);
  for (let i = first; i < last; i++) {
    const r = tabAt(i);
    itemId(r[0] + 4, r[1] + 4 - (i === view.tab ? 2 : 0), KINDS[tabs[i][0]][0]);
  }
  if (tabs.length > L.tabsShown) {
    iconButton(L.tabBack, "<", inside(L.tabBack, mx, my));
    iconButton(L.tabNext, ">", inside(L.tabNext, mx, my));
  }
  const tabNow = tabs[view.tab];
  if (!tabNow) return;
  const [kind, total] = tabNow;
  const per = perPage(view, L.area);
  const pagesTotal = Math.max(1, Math.ceil(total / per));
  // The station's bar and the page's, their arrows beside them.
  const bar = (r, label) => {
    sunken(...r);
    text(Math.round(r[0] + r[2] / 2 - textWidth(label) / 2), r[1] + 2, label, 0xFFFFFF, true);
  };
  bar(L.kindBar, KINDS[kind][1]);
  bar(L.pageBar, `${view.page + 1}/${pagesTotal}`);
  if (tabs.length > 1) {
    iconButton(L.kindBack, "<", inside(L.kindBack, mx, my));
    iconButton(L.kindNext, ">", inside(L.kindNext, mx, my));
  }
  if (pagesTotal > 1) {
    iconButton(L.pageBack, "<", inside(L.pageBack, mx, my));
    iconButton(L.pageNext, ">", inside(L.pageNext, mx, my));
  }
  // The station in the strip.
  hovered = drawSlot(L.station, { id: KINDS[kind][0], count: 1, damage: 0, maxDamage: 0,
    name: KINDS[kind][1], components: "" }, false, mx, my) || hovered;
  // The recipes, each centred with its buttons right of it.
  const turn = Math.floor(seconds);
  let y = L.area[1];
  for (const r of view.list) {
    const d = recipeDisplay(r);
    const fills = FILLED.includes(r.kind);
    const right = fills ? 21 : 0;
    const x = Math.round(L.area[0] + Math.min((L.area[2] - d.width) / 2, L.area[2] - d.width - right));
    for (const [sx, sy, stacks, large] of d.slots) {
      const s = stacks.length ? stacks[turn % stacks.length] : null;
      hovered = drawSlot([x + sx, y + sy, large ? 26 : SLOT, large ? 26 : SLOT], s, large, mx, my) || hovered;
    }
    const a = [x + d.arrow[0], y + d.arrow[1], 22, 15];
    // A cooking recipe's arrow fills over its cooking time, as the furnace's does.
    arrow(a[0], a[1], d.cooking && r.time > 0 ? (seconds * 20 % r.time) / r.time : 0);
    if (inside(a, mx, my)) {
      hint = d.shapeless ? ["Shapeless", "§7The ingredients go anywhere"] : [KINDS[r.kind][1]];
      if (r.time > 0) hint = [...hint, `§7${(r.time / 20).toFixed(r.time % 20 ? 1 : 0)} seconds`];
    }
    if (d.flame) sprite(x + d.flame[0], y + d.flame[1], 14, 14, "container/furnace/lit_progress");
    if (d.cooking) {
      const t = `${(r.time / 20).toFixed(r.time % 20 ? 1 : 0)}s, ${r.experience} XP`;
      text(x + 24, y + 28, t, 0x404040, false);
    }
    if (fills) {
      // EMI's buttons: from the bottom right of the recipe up.
      const bx = x + d.width + 5, by = y + d.height - 12 - (d.height >= 30 ? 4 : 0);
      const once = [bx, by, 16, 12], all = [bx, by - 14, 16, 12];
      iconButton(once, "+", inside(once, mx, my));
      iconButton(all, "++", inside(all, mx, my));
      hits.fills.push([once, r, false], [all, r, true]);
      if (inside(once, mx, my)) hint = ["Fill the grid", "§7Shift: as many as can be"];
      if (inside(all, mx, my)) hint = ["Fill the grid", "§7As many as can be"];
    }
    y += d.height + RECIPE_PADDING;
  }
  // The buttons a phone has instead of keys, for the stack picked or looked up.
  const s = subject();
  button(...L.back, "Back", inside(L.back, mx, my), history.length > 0);
  hovered = drawSlot(L.subject, s, false, mx, my) || hovered;
  const same = !selected;
  button(...L.recipesButton, "Recipes", inside(L.recipesButton, mx, my), !(same && view.lookup === "recipes"));
  button(...L.usesButton, "Uses", inside(L.usesButton, mx, my), !(same && view.lookup === "uses"));
  const starred = favourites.some((f) => key(f) === key(s));
  const [sx, sy, sw, sh] = L.star;
  if (starred) fill(sx - 1, sy - 1, sw + 2, sh + 2, 0xFFFFD700);
  button(sx, sy, sw, sh, "", inside(L.star, mx, my), true);
  itemId(sx + 2, sy, "minecraft:nether_star");
  if (inside(L.star, mx, my)) {
    hint = starred ? ["Favourite", "§7Click: take it away"] : ["Add to the favourites"];
  }
  // The index and the favourites beside, as EMI keeps them.
  below = 0;
  drawBeside(context, L.window, false);
  if (hovered) {
    itemTooltip(mx, my, hovered);
  } else if (hint) {
    tooltip(mx, my, hint);
  } else if (selected) {
    const r = selected.rect;
    itemTooltip(r[0] + r[2] - 4, r[1] + 4, selected.stack);
  }
}

// A tap or click on a stack: on a touch screen the first picks it, the second shows its
// recipes; a mouse shows them at once (its uses for the right button).
function pressStack(s, r, buttonId) {
  if (hasTouch() && !isSelected(s, r)) {
    selected = { stack: s, rect: r };
    return;
  }
  selected = null;
  show(s, buttonId === 1 ? "uses" : "recipes");
}

function pressRecipes(event) {
  if (!view) return;
  const L = screenLayout(lastContext.width, lastContext.height);
  const { x, y } = event;
  const tabs = view.tabs;
  const turnTab = (to) => {
    view.tab = ((to % tabs.length) + tabs.length) % tabs.length;
    view.page = 0;
    load(view);
  };
  const turnPage = (by) => {
    view.page += by;
    load(view);
  };
  if (tabs.length > L.tabsShown && inside(L.tabBack, x, y)) return turnTab(view.tab - L.tabsShown);
  if (tabs.length > L.tabsShown && inside(L.tabNext, x, y)) return turnTab(view.tab + L.tabsShown);
  const t = hits.tabs.find(([r]) => inside(r, x, y));
  if (t) return turnTab(t[1]);
  if (tabs.length > 1 && inside(L.kindBack, x, y)) return turnTab(view.tab - 1);
  if (tabs.length > 1 && inside(L.kindNext, x, y)) return turnTab(view.tab + 1);
  if (inside(L.pageBack, x, y)) return turnPage(-1);
  if (inside(L.pageNext, x, y)) return turnPage(1);
  if (history.length > 0 && inside(L.back, x, y)) return back();
  const s = subject();
  if (inside(L.recipesButton, x, y) && !(!selected && view.lookup === "recipes")) {
    selected = null;
    return show(s, "recipes");
  }
  if (inside(L.usesButton, x, y) && !(!selected && view.lookup === "uses")) {
    selected = null;
    return show(s, "uses");
  }
  if (inside(L.star, x, y)) return toggleFavourite(s);
  const f = hits.fills.find(([r]) => inside(r, x, y));
  if (f) {
    if (act(() => fillRecipe(f[1].id, f[2] || shiftHeld), false)) {
      close();
    } else {
      act(() => showOverlay(`Open a ${KINDS[f[1].kind][1].toLowerCase()} grid to fill`));
    }
    return;
  }
  const hit = hits.stacks.find(([r]) => inside(r, x, y));
  if (hit) return pressStack(hit[1], hit[0], event.button);
  if (pressBeside(lastContext, L.window, x, y, event.button)) return;
  // A tap on nothing lets the pick go.
  selected = null;
}

function back() {
  if (history.length === 0) {
    close();
    return;
  }
  view = history.pop();
  load(view);
}

let shiftHeld = false;
// The last game screen's size and the mouse on the recipes screen, for the hooks without a
// render context.
let lastContext = { width: 0, height: 0 };
const mouseIn = { x: 0, y: 0 };

// A key typed into the search box: true when it took it.
function typeSearch(event) {
  switch (event.key) {
    case "key.keyboard.escape":
    case "key.keyboard.enter":
    case "key.keyboard.keypad.enter":
      stopSearching();
      return true;
    case "key.keyboard.backspace":
      search = search.slice(0, -1);
      page = 0;
      return true;
    default:
      if (event.text) {
        search = (search + event.text).slice(0, 50);
        page = 0;
      }
      return true;
  }
}
// R, U and A over a stack: its recipes, its uses, the favourites.
function lookUpKey(k, s) {
  if (!s || (k !== "key.keyboard.r" && k !== "key.keyboard.u" && k !== "key.keyboard.a")) return false;
  if (k === "key.keyboard.a") {
    toggleFavourite(s);
  } else {
    show(s, k === "key.keyboard.u" ? "uses" : "recipes");
  }
  return true;
}

export const events = {
  init() {
    return {
      commands: [], tick: false, frame: true, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: true, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  frame(delta) {
    seconds += delta;
  },
  screenOpened(screen) {
    if (WINDOWED.includes(screen)) history.length = 0;
    selected = null;
  },
  screenClosed(screen) {
    if (screen === RECIPES) view = null;
    selected = null;
    if ((screen === RECIPES || WINDOWED.includes(screen)) && searching) stopSearching();
  },
  screenRender(screen, context) {
    if (screen === RECIPES) {
      mouseIn.x = context.mouseX;
      mouseIn.y = context.mouseY;
      lastContext = context;
      drawRecipes(context);
      return;
    }
    if (!WINDOWED.includes(screen)) return;
    lastContext = context;
    below = screen === "creative" ? 28 : 0;
    const win = act(() => getWindow(), null);
    if (win) drawBeside(context, win);
  },
  screenMouse(screen, event) {
    if (screen === RECIPES) {
      if (event.action === "press") pressRecipes(event);
      if (event.action === "scroll" && view && event.scroll !== 0) {
        const L = screenLayout(lastContext.width, lastContext.height);
        if (cell(layout(lastContext.width, lastContext.height, L.window).index, event.x, event.y) >= 0) {
          page = Math.max(0, page - Math.sign(event.scroll));
        } else {
          view.page -= Math.sign(event.scroll);
          load(view);
        }
      }
      return true;
    }
    if (!WINDOWED.includes(screen)) return false;
    const win = act(() => getWindow(), null);
    if (!win) return false;
    if (event.action === "press") {
      if (pressBeside(lastContext, win, event.x, event.y, event.button)) return true;
      selected = null;
      return false;
    }
    if (event.action === "scroll") {
      const L = layout(lastContext.width, lastContext.height, win);
      if (cell(L.index, event.x, event.y) >= 0) {
        page = Math.max(0, page - Math.sign(event.scroll));
        return true;
      }
    }
    return false;
  },
  screenKey(screen, event) {
    if (event.key === "key.keyboard.left.shift" || event.key === "key.keyboard.right.shift") {
      shiftHeld = event.pressed;
      return false;
    }
    if (!event.pressed) return false;
    shiftHeld = event.shift;
    if (screen === RECIPES) {
      if (searching) return typeSearch(event);
      if (!view) return false;
      switch (event.key) {
        case "key.keyboard.backspace":
          back();
          return true;
        case "key.keyboard.left":
        case "key.keyboard.right":
          view.page += event.key === "key.keyboard.left" ? -1 : 1;
          load(view);
          return true;
      }
      const L = screenLayout(lastContext.width, lastContext.height);
      const hovered = hits.stacks.find(([r]) => inside(r, mouseIn.x, mouseIn.y));
      return lookUpKey(event.key, hovered ? hovered[1] : hoveredBeside(lastContext, L.window));
    }
    if (!WINDOWED.includes(screen)) return false;
    const win = act(() => getWindow(), null);
    if (!win) return false;
    if (searching) return typeSearch(event);
    return lookUpKey(event.key, hoveredBeside(lastContext, win));
  },
};
