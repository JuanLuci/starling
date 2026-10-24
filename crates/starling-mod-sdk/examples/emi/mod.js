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
// window on it (the creative screen's tabs 28 pixels under it).
let below = 0;
function layout(width, height, win) {
  const [wx, wy, ww, wh0] = win || [width / 2 - 88, height / 2 - 83, 176, 166];
  const wh = wh0 + below;
  const top = 16, bottom = height - 24;
  const rows = Math.max(0, Math.floor((bottom - top) / SLOT));
  const right = width - 2, left = wx + ww + 4;
  const cols = Math.max(0, Math.floor((right - left) / SLOT));
  const favCols = Math.max(0, Math.floor((wx - 6) / SLOT));
  const boxWidth = Math.min(160, width - 8);
  return {
    index: { x: right - cols * SLOT, y: top, cols, rows },
    favourites: { x: 2, y: top, cols: favCols, rows },
    box: [Math.round(width / 2 - boxWidth / 2), Math.min(height - 20, wy + wh + 4), boxWidth, 16],
    back: [right - cols * SLOT, 2, 12, 12],
    next: [right - 12, 2, 12, 12],
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
    if (inside([x, y, SLOT, SLOT], mx, my)) {
      fill(x, y, SLOT, SLOT, 0x80FFFFFF);
      hovered = stacks[i];
    }
    item(x + 1, y + 1, stacks[i]);
  }
  return hovered;
}

// The index and the favourites beside a game screen's window, and their hovered stack's tooltip.
function drawBeside(context, win) {
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
    text(Math.round(mid - textWidth(label) / 2), 4, label, 0xFFFFFF, true);
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
  if (hovered) itemTooltip(...mouse, hovered);
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
  const i = cell(L.index, x, y);
  if (i >= 0 && shown.index[i]) {
    show(shown.index[i], buttonId === 1 ? "uses" : "recipes");
    return true;
  }
  const f = cell(L.favourites, x, y);
  if (f >= 0 && shown.favourites[f]) {
    show(shown.favourites[f], buttonId === 1 ? "uses" : "recipes");
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

// ---- the recipes screen ----

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

// A recipe's height and its slots (x, y from its corner, the stacks, an output) as its station
// lays them out.
function recipeSlots(r) {
  const slots = [];
  const inputs = r.inputs;
  switch (r.kind) {
    case "crafting": {
      // The whole 3 by 3 grid, as EMI draws it; a shape in its corner, a shapeless recipe's
      // ingredients in turn in rows of 3 (of 2 for up to 4).
      const ww = r.width || (inputs.length > 4 ? 3 : inputs.length > 1 ? 2 : 1);
      const cells = new Array(9).fill(null);
      for (let i = 0; i < inputs.length; i++) cells[Math.floor(i / ww) * 3 + (i % ww)] = inputs[i];
      for (let i = 0; i < 9; i++) slots.push([(i % 3) * SLOT, Math.floor(i / 3) * SLOT, cells[i] || [], false]);
      slots.push([84, 18, r.outputs, true]);
      return { height: 3 * SLOT, slots, arrow: [58, 19], shapeless: r.width === 0 };
    }
    case "smithing":
    case "brewing":
      for (let i = 0; i < inputs.length; i++) slots.push([i * SLOT, 0, inputs[i], false]);
      slots.push([inputs.length * SLOT + 30, 0, r.outputs, true]);
      return { height: SLOT, slots, arrow: [inputs.length * SLOT + 4, 1] };
    default:
      // Cooking and stonecutting: one input, the arrow, the output.
      slots.push([0, 0, inputs[0] || [], false]);
      slots.push([54, 0, r.outputs, true]);
      return { height: SLOT, slots, arrow: [24, 1], flame: r.kind !== "stonecutting" };
  }
}
function recipeHeight(kind) {
  return kind === "crafting" ? 3 * SLOT + 8 : SLOT + 16;
}

// The recipes screen's panel and parts on a GUI.
function screenLayout(width, height) {
  const w = Math.min(width - 8, 190), h = Math.min(height - 8, 236);
  const x = Math.round((width - w) / 2), y = Math.round((height - h) / 2);
  const tabs = [x + 6, y + 26];
  const area = [x + 8, y + 52, w - 16, h - 76];
  return {
    panel: [x, y, w, h],
    tabs,
    area,
    recipesButton: [x + w - 92, y + 6, 48, 14],
    usesButton: [x + w - 42, y + 6, 36, 14],
    back: [x + 8, y + h - 22, 12, 17],
    next: [x + w - 20, y + h - 22, 12, 17],
    pageLabel: [x + w / 2, y + h - 17],
    // Back to the view before (Backspace), and the star adding the stack to the favourites (A).
    history: [x + 24, y + h - 23, 34, 18],
    star: [x + w - 46, y + h - 23, 20, 18],
  };
}
function perPage(v, area) {
  const tab = v.tabs[v.tab];
  return tab ? Math.max(1, Math.min(16, Math.floor(area[3] / recipeHeight(tab[0])))) : 1;
}

let seconds = 0;
// What the recipes screen drew last: stacks by box, the + and ++ buttons by recipe (and whether
// as many as can be).
let hits = { stacks: [], fills: [], tabs: [] };

function drawRecipes(context) {
  hits = { stacks: [], fills: [], tabs: [] };
  if (!view) return;
  let tabHint = null;
  const L = screenLayout(context.width, context.height);
  const [px, py, pw, ph] = L.panel;
  const mx = context.mouseX, my = context.mouseY;
  panel(px, py, pw, ph);
  // The stack looked up and which way.
  item(px + 6, py + 6, view.stack);
  // Its name, cut to the room before the buttons.
  const room = L.recipesButton[0] - (px + 26) - 4;
  let name = view.stack.name;
  while (name.length > 1 && textWidth(name) > room) name = name.slice(0, -1);
  if (name !== view.stack.name) name = `${name.slice(0, -1)}…`;
  text(px + 26, py + 10, name, 0xFFFFFF, true);
  hits.stacks.push([[px + 5, py + 5, SLOT, SLOT], view.stack]);
  button(...L.recipesButton, "Recipes", inside(L.recipesButton, mx, my), view.lookup !== "recipes");
  button(...L.usesButton, "Uses", inside(L.usesButton, mx, my), view.lookup !== "uses");
  // The tabs: a station each, with how many.
  let tx = L.tabs[0];
  view.tabs.forEach(([kind, n], i) => {
    const label = `${n}`;
    const w = 20 + textWidth(label) + 2;
    const r = [tx, L.tabs[1], w, 20];
    if (tx + w > px + pw - 6) return;
    fill(...r, i === view.tab ? 0xFFFFFFFF : inside(r, mx, my) ? 0xFFA0A0A0 : 0xFF6F6F6F);
    fill(r[0] + 1, r[1] + 1, r[2] - 2, r[3] - 2, i === view.tab ? 0xFF5A5A5A : 0xFF2B2B2B);
    itemId(tx + 2, L.tabs[1] + 2, KINDS[kind][0]);
    text(tx + 19, L.tabs[1] + 6, label, i === view.tab ? 0xFFFFFF : 0xA0A0A0, true);
    hits.tabs.push([r, i]);
    if (inside(r, mx, my)) tabHint = [KINDS[kind][1], `§7${n} ${n === 1 ? "recipe" : "recipes"}`];
    tx += w + 2;
  });
  const tab = view.tabs[view.tab];
  if (!tab) return;
  const [kind, total] = tab;
  const per = perPage(view, L.area);
  const pagesTotal = Math.max(1, Math.ceil(total / per));
  const list = view.list;
  let y = L.area[1];
  let hovered = null, hint = null;
  const turn = Math.floor(seconds);
  for (const r of list) {
    const lay = recipeSlots(r);
    const x = L.area[0] + 4;
    for (const [sx, sy, stacks, output] of lay.slots) {
      const at = [x + sx, y + sy, SLOT, SLOT];
      sprite(...at, "container/slot");
      const s = stacks.length ? stacks[turn % stacks.length] : null;
      if (s) {
        item(at[0] + 1, at[1] + 1, s);
        hits.stacks.push([at, s]);
        if (inside(at, mx, my)) {
          fill(at[0] + 1, at[1] + 1, 16, 16, 0x80FFFFFF);
          hovered = s;
        }
      }
    }
    const arrow = [x + lay.arrow[0], y + lay.arrow[1], 24, 16];
    sprite(...arrow, "container/furnace/burn_progress");
    if (inside(arrow, mx, my)) {
      hint = lay.shapeless ? ["Shapeless", "§7The ingredients go anywhere"] : [KINDS[r.kind][1]];
    }
    if (lay.flame) sprite(x + 2, y + SLOT + 1, 14, 14, "container/furnace/lit_progress");
    if (r.time > 0) {
      const t = `${(r.time / 20).toFixed(r.time % 20 ? 1 : 0)}s · ${r.experience} xp`;
      text(x + 20, y + SLOT + 4, t, 0xA0A0A0, false);
    }
    if (FILLED.includes(r.kind)) {
      const once = [L.area[0] + L.area[2] - 20, y + 2, 18, 12];
      const all = [once[0], y + 16, 18, 12];
      button(...once, "+", inside(once, mx, my), true);
      button(...all, "++", inside(all, mx, my), true);
      hits.fills.push([once, r, false], [all, r, true]);
      if (inside(once, mx, my)) hint = ["Fill the grid", "§7Shift: as many as can be"];
      if (inside(all, mx, my)) hint = ["Fill the grid", "§7As many as can be"];
    }
    y += recipeHeight(kind);
  }
  if (pagesTotal > 1) {
    sprite(...L.back, inside(L.back, mx, my) ? "recipe_book/page_backward_highlighted" : "recipe_book/page_backward");
    sprite(...L.next, inside(L.next, mx, my) ? "recipe_book/page_forward_highlighted" : "recipe_book/page_forward");
  }
  const label = `${view.page + 1}/${pagesTotal}`;
  text(Math.round(L.pageLabel[0] - textWidth(label) / 2), L.pageLabel[1], label, 0xFFFFFF, true);
  if (history.length > 0) button(...L.history, "Back", inside(L.history, mx, my), true);
  // The star, framed in gold while the stack is a favourite.
  const starred = favourites.some((f) => key(f) === key(view.stack));
  const [sx, sy, sw, sh] = L.star;
  if (starred) fill(sx - 1, sy - 1, sw + 2, sh + 2, 0xFFFFD700);
  button(sx, sy, sw, sh, "", inside(L.star, mx, my), true);
  itemId(sx + 2, sy + 1, "minecraft:nether_star");
  if (inside(L.star, mx, my)) {
    hint = starred ? ["Favourite", "§7Click: take it away"] : ["Add to the favourites"];
  }
  if (hovered) {
    itemTooltip(mx, my, hovered);
  } else if (hint || tabHint) {
    tooltip(mx, my, hint || tabHint);
  }
}

function pressRecipes(event) {
  if (!view) return;
  const L = screenLayout(lastContext.width, lastContext.height);
  const { x, y } = event;
  if (inside(L.recipesButton, x, y) && view.lookup !== "recipes") return show(view.stack, "recipes");
  if (inside(L.usesButton, x, y) && view.lookup !== "uses") return show(view.stack, "uses");
  const tab = hits.tabs.find(([r]) => inside(r, x, y));
  if (tab) {
    view.tab = tab[1];
    view.page = 0;
    load(view);
    return;
  }
  if (inside(L.back, x, y)) {
    view.page--;
    load(view);
    return;
  }
  if (inside(L.next, x, y)) {
    view.page++;
    load(view);
    return;
  }
  if (history.length > 0 && inside(L.history, x, y)) {
    back();
    return;
  }
  if (inside(L.star, x, y)) {
    toggleFavourite(view.stack);
    return;
  }
  const f = hits.fills.find(([r]) => inside(r, x, y));
  if (f) {
    if (act(() => fillRecipe(f[1].id, f[2] || shiftHeld), false)) {
      close();
    } else {
      act(() => showOverlay(`Open a ${KINDS[f[1].kind][1].toLowerCase()} grid to fill`));
    }
    return;
  }
  const s = hits.stacks.find(([r]) => inside(r, x, y));
  if (s) show(s[1], event.button === 1 ? "uses" : "recipes");
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
  },
  screenClosed(screen) {
    if (screen === RECIPES) view = null;
    if (WINDOWED.includes(screen) && searching) stopSearching();
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
        view.page -= Math.sign(event.scroll);
        load(view);
      }
      return true;
    }
    if (!WINDOWED.includes(screen)) return false;
    const win = act(() => getWindow(), null);
    if (!win) return false;
    if (event.action === "press") {
      return pressBeside(lastContext, win, event.x, event.y, event.button);
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
      if (event.key === "key.keyboard.backspace") {
        back();
        return true;
      }
      const hovered = hits.stacks.find(([r]) => inside(r, mouseIn.x, mouseIn.y));
      if (hovered && (event.key === "key.keyboard.r" || event.key === "key.keyboard.u")) {
        show(hovered[1], event.key === "key.keyboard.u" ? "uses" : "recipes");
        return true;
      }
      return false;
    }
    if (!WINDOWED.includes(screen)) return false;
    const win = act(() => getWindow(), null);
    if (!win) return false;
    if (searching) {
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
    const k = event.key;
    if (k !== "key.keyboard.r" && k !== "key.keyboard.u" && k !== "key.keyboard.a") return false;
    const s = hoveredBeside(lastContext, win);
    if (!s) return false;
    if (k === "key.keyboard.a") {
      toggleFavourite(s);
    } else {
      show(s, k === "key.keyboard.u" ? "uses" : "recipes");
    }
    return true;
  },
};

