// Bedrock Inventory: Bedrock's creative inventory in place of the game's creative screen
// (screens.replace): the creative screen as the game draws it (its tabs above and below, the
// tab's items, the search, the scrollbar, the hotbar row) with its Survival Inventory tab beside
// it, as Bedrock puts the inventory beside the items: the player with the armour around it, the
// off hand, the inventory, the hotbar, the Destroy Item slot, and a 2x2 crafting grid in the room
// at the player's right. Both work at once. The Survival Inventory tab shows only that window,
// the tabs around it, as the game's screen does.
//
// The mouse clicks as on the game's own screens: left and right, Shift for a full stack or the
// other side, a double click gathers, the middle button clones, 1 to 9, F and Q over a slot,
// a click outside throws. Fingers work as the game's touch inventory: a tap picks a stack (the
// tabs' items too), a tap on a slot puts it there, a finger held on a stack picks half of it, a
// second quick tap sends it across (a tab's item: into the hotbar), a stack dragged goes where it
// is let go, a tap outside throws it. Java UI goes back to the game's screen; there, Bedrock UI
// comes back here. Every click is the game's: the creative screen's semantics, a server told
// as vanilla tells it. The windows are the game's own textures (a resource pack's too).
import { getTabs, getItems, click as take, setSlot, setCarried } from "starling:mods/creative@0.1.0";
import { getMenu, click } from "starling:mods/slots@0.1.0";
import {
  fill, text, textWidth, item, itemId, sprite, tooltip, itemTooltip, player, button, push, pop,
} from "starling:mods/draw@0.1.0";
import { replace } from "starling:mods/screens@0.1.0";
import { addScreenButton } from "starling:mods/widgets@0.1.0";
import { hasTouch, keyboard } from "starling:mods/mobile@0.1.0";
import { get, set } from "starling:mods/storage@0.1.0";
import { write } from "starling:mods/log@0.1.0";

const NAME = "inventory";
const OWN = "bedrock_inventory:inventory";

// The settings: only the inventory shown (the Survival Inventory tab), and the game's own screen
// instead of this one.
const saved = get("settings");
const settings = { only: !!saved && saved[0] === 1, java: !!saved && saved[1] === 1 };
function save() {
  try {
    set("settings", new Uint8Array([settings.only ? 1 : 0, settings.java ? 1 : 0]));
  } catch (_) {
    // A full storage keeps the old settings.
  }
}

// A call into the game that may refuse (outside creative mode, a full queue): logged, not fatal.
function act(f) {
  try {
    f();
    return true;
  } catch (e) {
    write("debug", `${e}`);
    return false;
  }
}

// The creative screen's window (CreativeModeInventoryScreen): its size, the grid of a tab's items
// (9 by 5 from 9, 18), its hotbar row, scrollbar, search box and title, and its textures (256 by
// 256 with the window at the top left, the rest clear: drawn whole they are the window).
const W = 195, H = 136, COLS = 9, ROWS = 5, CELL = 18;
const SCROLLER = [175, 18, 14, 112];
const SEARCH = [82, 6, 80, 9];
const ITEMS_TEXTURE = "gui/container/creative_inventory/tab_items";
const SEARCH_TEXTURE = "gui/container/creative_inventory/tab_item_search";
// The creative screen's Survival Inventory tab (its window is the creative screen's size) and its
// slots by the inventory menu's ids (slots.get-menu's), where selectTab puts them: the armour
// either side of the player (head and chest at its left, legs and feet at its right), the off
// hand, the inventory, the hotbar. The crafting grid (1 to 4) and its result (0) are this mod's,
// in the room at the player's right, the result over the Destroy Item slot.
const INV_W = W, INV_H = H;
const INVENTORY_TEXTURE = "gui/container/creative_inventory/tab_inventory";
const SLOTS = [];
for (let i = 0; i < 9; i++) SLOTS.push([36 + i, 9 + i * CELL, 112]);
for (let i = 0; i < 27; i++) SLOTS.push([9 + i, 9 + (i % 9) * CELL, 54 + Math.floor(i / 9) * CELL]);
SLOTS.push([5, 54, 6], [6, 54, 33], [7, 108, 6], [8, 108, 33], [45, 35, 20]);
const CRAFTING = [[1, 129, 7], [2, 147, 7], [3, 129, 25], [4, 147, 25], [0, 173, 16]];
SLOTS.push(...CRAFTING);
const EMPTY_SPRITES = {
  5: "container/slot/helmet", 6: "container/slot/chestplate", 7: "container/slot/leggings",
  8: "container/slot/boots", 45: "container/slot/shield",
};
const TRASH = [173, 112];
const DOLL = [73, 6, 32, 43];
const MARGIN = 4, GAP = 4;
// How soon a second tap is a double tap, and a second click a double click (seconds).
const DOUBLE_TAP = 0.35, DOUBLE_CLICK = 0.25;
// Degrees the player turns for each GUI pixel a finger drags across it.
const TURN = 2.5;
// How far (GUI pixels) a finger moves before it drags: a finger held still is let go with a move
// where it was.
const DRAG = 3;

// The screen's state.
let now = 0;
let size = { width: 0, height: 0, mouseX: 0, mouseY: 0 };
let selected = null;
let search = "";
let focused = false;
let scroll = 0;
let turn = 0;
let shift = false, control = false;
// Touch: the stack picked ({slot, half} or a tab's {index, tab, search}), the finger down, what
// the finger drags, and the leftover to settle once the game has done the clicks.
let pick = null;
let finger = null;
let pending = null;
// Mouse: the press waiting for its release, the last click (for double clicks), the scrollbar
// or the player held.
let down = null;
let last = null;
let holding = null;

const inside = (r, x, y) => x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
const at2 = (o, r) => [o.x + r[0], o.y + r[1], r[2], r[3]];

// The creative tabs and their items do not change while the screen is open: kept from its opening
// (the game's answers last a tick otherwise, and reading a page of items again costs as much as
// drawing the screen), the last few pages.
let tabsKept = null;
const pages = new Map();
function tabs() {
  if (!tabsKept) tabsKept = getTabs();
  return tabsKept;
}
function items(tab, term, start, count) {
  const key = `${tab}\u0000${term}\u0000${start}\u0000${count}`;
  let page = pages.get(key);
  if (!page) {
    page = getItems(tab, term, start, count);
    if (pages.size >= 16) pages.delete(pages.keys().next().value);
    pages.set(key, page);
  }
  return page;
}

// The tab whose items show (the Survival Inventory shows none).
function current(list) {
  const items = list.filter((t) => t.kind !== "inventory");
  return items.find((t) => t.id === selected) || items.find((t) => t.kind === "category") ||
    items[0] || null;
}
const searchOf = (tab) => (tab && tab.kind === "search" ? search : "");

// Where everything is for a GUI this size: drawn at `scale` (below 1 when it does not fit), in
// the GUI pixels of that scale. `frame` is the window the tabs sit on: the creative screen's, or
// with only the inventory, a window as wide around it.
function layout(width, height) {
  const list = tabs();
  const only = settings.only || list.length === 0;
  const needW = only ? W + GAP + 48 : W + GAP + INV_W;
  const needH = 28 + H + 28;
  const scale = Math.min(1, (width - 2 * MARGIN) / needW, (height - 2 * MARGIN) / needH);
  const vw = width / scale, vh = height / scale;
  const x0 = Math.floor((vw - needW) / 2);
  if (only) {
    const frame = { x: x0, y: Math.floor((vh - H) / 2), w: W, h: H };
    return {
      scale, list, only, frame, creative: null, inv: { x: frame.x, y: frame.y },
      java: [frame.x + W + GAP, frame.y, 48, 16],
    };
  }
  const frame = { x: x0, y: Math.floor((vh - H) / 2), w: W, h: H };
  const inv = { x: x0 + W + GAP, y: frame.y };
  return { scale, list, only, frame, creative: frame, inv, java: [inv.x + INV_W - 48, inv.y - 18, 48, 16] };
}
// A tab's button (CreativeModeInventoryScreen.getTabX/Y): the top row above the window, the
// bottom row below it, the last two columns at its right.
function tabRect(L, t) {
  const x = t.column >= 5 ? W - 27 * (7 - t.column) + 1 : t.column * 27;
  const y = t.top ? -28 : L.frame.h - 4;
  return [L.frame.x + x, L.frame.y + y, 26, 32];
}
function isSelected(L, t, tab) {
  return t.kind === "inventory" ? L.only : !L.only && !!tab && t.id === tab.id;
}

// The selected tab's items in view: the grid's first row, and the page from it.
function page(L) {
  const tab = current(L.list);
  if (!tab || !L.creative) return { tab, first: 0, total: 0, hidden: 0, items: [] };
  const total = items(tab.id, searchOf(tab), 0, 0).total;
  const hidden = Math.max(0, Math.ceil(total / COLS) - ROWS);
  scroll = clamp(scroll, 0, hidden);
  const first = Math.round(scroll);
  const shown = items(tab.id, searchOf(tab), first * COLS, ROWS * COLS);
  return { tab, first, total: shown.total, hidden, items: shown.items };
}

// What is at a point: a tab, a grid item, the search box, the scrollbar, an inventory slot (the
// creative window's hotbar row too), the Destroy Item slot, the player, the Java UI button, a
// window; null outside them all.
function at(L, x, y) {
  if (inside(L.java, x, y)) return { kind: "java" };
  for (const t of L.list) {
    if (inside(tabRect(L, t), x, y)) return { kind: "tab", tab: t };
  }
  const c = L.creative;
  if (c && inside([c.x, c.y, W, H], x, y)) {
    const tab = current(L.list);
    if (tab && tab.kind === "search" && inside(at2(c, SEARCH), x, y)) return { kind: "search" };
    if (inside(at2(c, SCROLLER), x, y)) return { kind: "scrollbar" };
    const col = Math.floor((x - c.x - 8) / CELL);
    if (col >= 0 && col < COLS) {
      const row = Math.floor((y - c.y - 17) / CELL);
      if (row >= 0 && row < ROWS) {
        return { kind: "item", cell: row * COLS + col, x: c.x + 9 + col * CELL, y: c.y + 18 + row * CELL };
      }
      if (y >= c.y + 111 && y < c.y + 129) {
        return { kind: "slot", slot: 36 + col, x: c.x + 9 + col * CELL, y: c.y + 112 };
      }
    }
    return { kind: "panel" };
  }
  const inv = L.inv;
  if (inside([inv.x, inv.y, INV_W, INV_H], x, y)) {
    for (const [slot, sx, sy] of SLOTS) {
      if (inside([inv.x + sx - 1, inv.y + sy - 1, CELL, CELL], x, y)) {
        return { kind: "slot", slot, x: inv.x + sx, y: inv.y + sy };
      }
    }
    if (inside([inv.x + TRASH[0] - 1, inv.y + TRASH[1] - 1, CELL, CELL], x, y)) return { kind: "trash" };
    if (inside([inv.x + DOLL[0], inv.y + DOLL[1], DOLL[2], DOLL[3]], x, y)) return { kind: "doll" };
    return { kind: "panel" };
  }
  return null;
}

// A slot as the game's windows draw them (for the crafting grid, which the window lacks).
function slotBox(x, y) {
  fill(x - 1, y - 1, 18, 18, 0xFF373737);
  fill(x, y, 17, 17, 0xFFFFFFFF);
  fill(x, y, 16, 16, 0xFF8B8B8B);
}
function frame(x, y, colour) {
  fill(x - 1, y - 1, 18, 1, colour);
  fill(x - 1, y + 16, 18, 1, colour);
  fill(x - 1, y, 1, 16, colour);
  fill(x + 16, y, 1, 16, colour);
}

function menu() {
  const m = getMenu();
  return m && m.kind === "minecraft:inventory" ? m : null;
}

// Clicks, as the mouse or a finger makes them.
function selectTab(tab) {
  pick = null;
  if (tab.kind === "inventory") {
    settings.only = true;
  } else {
    settings.only = false;
    if (tab.id !== selected) scroll = 0;
    selected = tab.id;
  }
  // The search tab takes the keys (selectTab focuses its box); on a phone only a tap on the box
  // brings up the keyboard.
  focus(tab.kind === "search" && !hasTouch());
  save();
}
function focus(on) {
  if (on && !focused) {
    const tab = tabs().find((t) => t.kind === "search");
    if (tab) {
      if (tab.id !== selected) scroll = 0;
      selected = tab.id;
      settings.only = false;
    }
  }
  if (on !== focused && hasTouch()) keyboard(on);
  focused = on;
}
function toJava() {
  settings.java = true;
  save();
  replace("creative", "");
}
function toBedrock() {
  settings.java = false;
  save();
  replace("creative", NAME);
}
// Shift on the Destroy Item slot: everything goes (CreativeModeInventoryScreen's).
function clearAll() {
  for (let slot = 1; slot <= 45; slot++) act(() => setSlot(slot, undefined));
}
// The scrollbar held at height `y` (CreativeModeInventoryScreen.mouseDragged).
function scrollTo(L, y) {
  const p = page(L);
  if (!L.creative) return;
  const top = L.creative.y + SCROLLER[1];
  scroll = clamp((y - top - 7.5) / (SCROLLER[3] - 15), 0, 1) * p.hidden;
}
function takeAt(L, t, button, kind) {
  const p = page(L);
  if (p.tab) act(() => take(p.tab.id, searchOf(p.tab), p.first * COLS + t.cell, button, kind));
}

// The mouse, as on the game's container screens: with nothing carried a press clicks; with a
// stack carried the release does (where it lands), a double click gathering.
function mousePress(L, e, x, y) {
  const t = at(L, x, y);
  // The search tab's box keeps the keys while the mouse works its window.
  if (!t || t.kind === "tab" || t.kind === "java" || t.kind === "slot" && !L.creative) {
    focus(false);
  } else if (t.kind === "search") {
    focus(true);
  }
  const m = menu();
  const carrying = !!(m && m.carried);
  down = null;
  if (!t) {
    if (carrying) down = { t, button: e.button };
    return;
  }
  const kind = e.button === 2 ? "clone" : shift ? "quick-move" : "pickup";
  switch (t.kind) {
    case "java": toJava(); return;
    case "tab": selectTab(t.tab); return;
    case "scrollbar": holding = "scrollbar"; scrollTo(L, y); return;
    case "doll": holding = { x }; return;
    case "trash":
      if (shift) clearAll();
      else if (carrying) act(() => setCarried(undefined));
      return;
    case "item": case "slot":
      if (carrying && e.button !== 2) {
        down = { t, button: e.button };
        return;
      }
      last = { t, at: now, button: e.button };
      if (t.kind === "slot") act(() => click(t.slot, e.button, kind));
      else takeAt(L, t, e.button, kind);
      return;
  }
}
function mouseRelease(L, e, x, y) {
  holding = null;
  const d = down;
  down = null;
  if (!d || d.button !== e.button) return;
  const t = at(L, x, y);
  if (!t) {
    act(() => click(-999, e.button, "pickup"));
    return;
  }
  const double = last && e.button === 0 && now - last.at < DOUBLE_CLICK && last.t.kind === t.kind &&
    (t.kind === "slot" ? last.t.slot === t.slot : last.t.cell === t.cell);
  last = null;
  if (t.kind === "slot") {
    act(() => click(t.slot, e.button, double ? "pickup-all" : shift ? "quick-move" : "pickup"));
  } else if (t.kind === "item") {
    takeAt(L, t, e.button, shift ? "quick-move" : "pickup");
  } else if (t.kind === "trash") {
    act(() => setCarried(undefined));
  }
}
// 1 to 9, F and Q over the slot or item under the mouse.
function hotkey(L, key) {
  const t = at(L, size.mouseX / L.scale, size.mouseY / L.scale);
  if (!t || (t.kind !== "slot" && t.kind !== "item")) return false;
  const digit = /^key\.keyboard\.([1-9])$/.exec(key);
  let button, kind;
  if (digit) [button, kind] = [Number(digit[1]) - 1, "swap"];
  else if (key === "key.keyboard.f") [button, kind] = [40, "swap"];
  else if (key === "key.keyboard.q") [button, kind] = [control ? 1 : 0, "throw"];
  else return false;
  if (t.kind === "slot") act(() => click(t.slot, button, kind));
  else takeAt(L, t, button, kind);
  return true;
}

// Fingers, as the game's touch inventory works them.
function stackAt(t, m, L) {
  if (t.kind === "slot") return m ? m.slots[t.slot] : undefined;
  if (t.kind === "item") return page(L).items[t.cell];
  return undefined;
}
function pickItem(L, t) {
  const p = page(L);
  if (!p.tab || !p.items[t.cell]) return null;
  return { index: p.first * COLS + t.cell, tab: p.tab.id, search: searchOf(p.tab), at: now, stack: p.items[t.cell] };
}
// The picked stack to slot `slot`: what the hand still holds after (a swapped stack, what did
// not fit) goes back where it came from, a tab's item away.
function moveTo(slot, m) {
  const p = pick;
  pick = null;
  if (p.index !== undefined) {
    act(() => take(p.tab, p.search, p.index, 0, "quick-move"));
    act(() => click(slot, 0, "pickup"));
    pending = { discard: true };
    return;
  }
  const there = m && m.slots[slot], mine = m && m.slots[p.slot];
  // Half a stack onto another item would swap it in: it stays.
  if (p.half && there && mine && (there.id !== mine.id || there.components !== mine.components)) return;
  act(() => click(p.slot, p.half ? 1 : 0, "pickup"));
  act(() => click(slot, 0, "pickup"));
  pending = { back: p.slot };
}
// A tab's item into the first empty hotbar slot, else the inventory's.
function intoInventory(p, m) {
  if (!m) return;
  const order = [];
  for (let s = 36; s <= 44; s++) order.push(s);
  for (let s = 9; s <= 35; s++) order.push(s);
  const free = order.find((s) => !m.slots[s]);
  if (free === undefined) return;
  act(() => take(p.tab, p.search, p.index, 0, "quick-move"));
  act(() => click(free, 0, "pickup"));
}
function throwPick() {
  const p = pick;
  pick = null;
  if (p.index !== undefined) {
    act(() => take(p.tab, p.search, p.index, 1, "throw"));
  } else if (p.half) {
    act(() => click(p.slot, 1, "pickup"));
    act(() => click(-999, 0, "pickup"));
  } else {
    act(() => click(p.slot, 1, "throw"));
  }
}
function tap(L, t, y) {
  const m = menu();
  if (!t) {
    if (pick) throwPick();
    return;
  }
  switch (t.kind) {
    case "java": toJava(); return;
    case "tab": selectTab(t.tab); return;
    case "search": focus(true); return;
    case "scrollbar": scrollTo(L, y); return;
    case "trash":
      if (pick && pick.slot !== undefined) act(() => setSlot(pick.slot, undefined));
      pick = null;
      return;
    case "item": {
      const p = pickItem(L, t);
      if (p && pick && pick.index === p.index && pick.tab === p.tab && pick.search === p.search &&
        now - pick.at < DOUBLE_TAP) {
        intoInventory(p, m);
        pick = null;
      } else {
        pick = p;
      }
      return;
    }
    case "slot": {
      if (!pick) {
        if (stackAt(t, m, L)) pick = { slot: t.slot, half: false, at: now };
        return;
      }
      if (pick.slot === t.slot) {
        if (!pick.half && now - pick.at < DOUBLE_TAP) act(() => click(t.slot, 0, "quick-move"));
        pick = null;
        return;
      }
      moveTo(t.slot, m);
      return;
    }
    default:
      pick = null;
  }
}
// A finger held still on a stack, then lifted: half of it picked (a tab's item: picked).
function hold(L, t, y) {
  if (t && t.kind === "slot" && stackAt(t, menu(), L)) {
    pick = { slot: t.slot, half: true, at: now };
  } else {
    tap(L, t, y);
  }
}
// A finger moving with a stack under it: the stack taken into the hand to go where it is let go.
function grab(L) {
  const t = finger.t;
  const m = menu();
  pick = null;
  if (t && t.kind === "slot" && stackAt(t, m, L)) {
    act(() => click(t.slot, 0, "pickup"));
    finger.grabbed = { slot: t.slot };
  } else if (t && t.kind === "item") {
    const p = pickItem(L, t);
    if (p) {
      act(() => take(p.tab, p.search, p.index, 0, "quick-move"));
      finger.grabbed = { item: true };
    }
  }
}
function drop(L, t) {
  const g = finger.grabbed;
  if (!t) {
    act(() => click(-999, 0, "pickup"));
  } else if (t.kind === "slot" && t.slot !== g.slot) {
    act(() => click(t.slot, 0, "pickup"));
    pending = g.item ? { discard: true } : { back: g.slot };
  } else if (t.kind === "trash" || g.item) {
    act(() => setCarried(undefined));
  } else {
    act(() => click(g.slot, 0, "pickup"));
  }
}
function touchMouse(L, e, x, y) {
  if (e.action === "press") {
    finger = { t: at(L, x, y), x, y, frames: 0, grabbed: null, moved: false };
    if (!(finger.t && finger.t.kind === "search")) focus(false);
    return;
  }
  if (!finger) return;
  if (e.action === "drag") {
    const t = finger.t;
    if (!finger.moved && Math.abs(x - finger.x) < DRAG && Math.abs(y - finger.y) < DRAG) return;
    if (t && t.kind === "doll") {
      turn += (x - finger.x) * TURN;
    } else if (t && t.kind === "scrollbar") {
      scrollTo(L, y);
    } else if (!finger.grabbed && !finger.moved) {
      grab(L);
    }
    finger.moved = true;
    finger.x = x;
    finger.y = y;
    return;
  }
  if (e.action === "release") {
    const t = at(L, x, y);
    if (finger.grabbed) drop(L, t);
    else if (finger.moved) { /* the player turned, the scrollbar dragged */ }
    else if (finger.frames === 0) tap(L, t, y);
    else hold(L, t, y);
    finger = null;
  }
}

// What the hand still holds once the game did a move: back where it came from, or away. True
// when it holds something until then.
function settle() {
  const p = pending;
  pending = null;
  const m = p && menu();
  if (!m || !m.carried) return false;
  if (p.back !== undefined) act(() => click(p.back, 0, "pickup"));
  else act(() => setCarried(undefined));
  return true;
}

// A slot's stack, hover and pick marks.
function drawSlot(x, y, slot, stack, touch, hovered) {
  if (stack) item(x, y, stack);
  else if (EMPTY_SPRITES[slot]) sprite(x, y, 16, 16, EMPTY_SPRITES[slot]);
  // The slot under the mouse (the hotbar shows twice: only the one it is over).
  if (!touch && hovered && hovered.kind === "slot" && hovered.x === x && hovered.y === y) {
    fill(x, y, 16, 16, 0x80FFFFFF);
  }
  if (pick && pick.slot === slot) {
    frame(x, y, 0xFFFFFFFF);
    if (pick.half) fill(x + 2, y + 14, 6, 2, 0xFFFFFFFF);
  }
}
// The tabs as CreativeModeInventoryScreen draws them: the others behind the window, the selected
// one in front, then their icons.
function drawTabs(L, tab, behind) {
  for (const t of L.list) {
    const on = isSelected(L, t, tab);
    if (on === behind) continue;
    const [x, y, w, h] = tabRect(L, t);
    const name = `container/creative_inventory/tab_${t.top ? "top" : "bottom"}_${on ? "selected" : "unselected"}_${t.column + 1}`;
    sprite(x, y, w, h, name);
  }
}
function drawTabIcons(L) {
  for (const t of L.list) {
    const [x, y] = tabRect(L, t);
    itemId(x + 5, y + (t.top ? 9 : 7), t.icon);
  }
}
function drawCreative(L, p, m, touch, hovered) {
  const c = L.creative;
  const searching = p.tab && p.tab.kind === "search";
  sprite(c.x, c.y, 256, 256, searching ? SEARCH_TEXTURE : ITEMS_TEXTURE);
  if (p.tab) text(c.x + 8, c.y + 6, p.tab.title, 0x404040, false);
  if (searching) {
    const caret = focused && Math.floor(now * 3) % 2 === 0 ? "_" : "";
    let shown = search;
    while (shown.length > 0 && textWidth(shown + caret) > SEARCH[2]) shown = shown.slice(1);
    text(c.x + SEARCH[0], c.y + SEARCH[1], shown + caret, 0xFFFFFF, true);
  }
  for (let i = 0; i < ROWS * COLS; i++) {
    const x = c.x + 9 + (i % COLS) * CELL, y = c.y + 18 + Math.floor(i / COLS) * CELL;
    const stack = p.items[i];
    if (stack) item(x, y, stack);
    if (!touch && hovered && hovered.kind === "item" && hovered.cell === i) fill(x, y, 16, 16, 0x80FFFFFF);
    if (pick && p.tab && pick.index === p.first * COLS + i && pick.tab === p.tab.id &&
      pick.search === searchOf(p.tab)) {
      frame(x, y, 0xFFFFFFFF);
    }
  }
  for (let i = 0; i < COLS; i++) drawSlot(c.x + 9 + i * CELL, c.y + 112, 36 + i, m && m.slots[36 + i], touch, hovered);
  const offset = p.hidden > 0 ? Math.floor((SCROLLER[3] - 17) * scroll / p.hidden) : 0;
  const bar = [c.x + SCROLLER[0], c.y + SCROLLER[1] + offset];
  sprite(bar[0], bar[1], 12, 15, `container/creative_inventory/scroller${p.hidden > 0 ? "" : "_disabled"}`);
}
function drawInventory(L, m, touch, hovered) {
  const { x: ox, y: oy } = L.inv;
  // The game's window, its Destroy Item slot in it.
  sprite(ox, oy, 256, 256, INVENTORY_TEXTURE);
  // The crafting grid, an arrow, its result: not in the game's window, drawn on it.
  for (const [, sx, sy] of CRAFTING) slotBox(ox + sx, oy + sy);
  fill(ox + 165, oy + 23, 3, 2, 0xFF8B8B8B);
  for (let i = 0; i < 3; i++) fill(ox + 168 + i, oy + 21 + i, 1, 6 - 2 * i, 0xFF8B8B8B);
  player(ox + DOLL[0], oy + DOLL[1], DOLL[2], DOLL[3], 20, turn);
  for (const [slot, sx, sy] of SLOTS) drawSlot(ox + sx, oy + sy, slot, m && m.slots[slot], touch, hovered);
  if (!touch && hovered && hovered.kind === "trash") fill(ox + TRASH[0], oy + TRASH[1], 16, 16, 0x80FFFFFF);
}

export const events = {
  init() {
    if (!settings.java) replace("creative", NAME);
    act(() => addScreenButton({
      id: "bedrock", screen: "creative", anchor: "top-left", x: 4, y: 4, width: 70, height: 20,
      label: "Bedrock UI", tooltip: ["The creative tabs beside your inventory (Bedrock Inventory)"],
    }, toBedrock));
    return {
      commands: [], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  screenOpened(screen) {
    if (screen !== OWN) return;
    pick = null; finger = null; pending = null; down = null; last = null; holding = null;
    tabsKept = null;
    pages.clear();
    search = "";
    scroll = 0;
    const tab = current(tabs());
    focused = !settings.only && !!tab && tab.kind === "search" && !hasTouch();
  },
  screenClosed(screen) {
    if (screen === OWN) focused = false;
  },
  screenRender(screen, context) {
    if (screen !== OWN) return;
    now += context.delta;
    size = context;
    if (finger) finger.frames++;
    const settling = settle();
    const L = layout(context.width, context.height);
    const touch = hasTouch();
    const mx = context.mouseX / L.scale, my = context.mouseY / L.scale;
    const m = menu();
    const hovered = touch ? null : at(L, mx, my);
    if (L.scale !== 1) push(0, 0, L.scale);
    const p = page(L);
    drawTabs(L, p.tab, true);
    if (L.creative) drawCreative(L, p, m, touch, hovered);
    drawInventory(L, m, touch, hovered);
    drawTabs(L, p.tab, false);
    drawTabIcons(L);
    button(L.java[0], L.java[1], L.java[2], L.java[3], "Java UI", !touch && inside(L.java, mx, my), true);
    // What the hand carries, at the mouse (or the finger dragging it).
    const carried = m && m.carried;
    if (carried && !settling && (!touch || (finger && finger.grabbed))) {
      item(mx - 8, my - 8, carried);
    } else if (touch && pick) {
      const stack = pick.index !== undefined ? pick.stack : m && m.slots[pick.slot];
      const place = pick.slot !== undefined && SLOTS.find((s) => s[0] === pick.slot);
      if (stack && place) itemTooltip(L.inv.x + place[1] + 8, L.inv.y + place[2] + 8, stack);
      else if (stack && L.creative) itemTooltip(L.creative.x + 8, L.creative.y + 24, stack);
    } else if (!touch && !carried && hovered) {
      if (hovered.kind === "slot" && m && m.slots[hovered.slot]) itemTooltip(mx, my, m.slots[hovered.slot]);
      else if (hovered.kind === "item" && p.items[hovered.cell]) itemTooltip(mx, my, p.items[hovered.cell]);
      else if (hovered.kind === "tab") tooltip(mx, my, [hovered.tab.title]);
      else if (hovered.kind === "trash") tooltip(mx, my, ["Destroy Item"]);
    }
    if (L.scale !== 1) pop();
  },
  screenMouse(screen, e) {
    if (screen !== OWN) return false;
    const L = layout(size.width, size.height);
    const x = e.x / L.scale, y = e.y / L.scale;
    if (e.action === "scroll") {
      if (L.creative) scroll = Math.max(0, scroll - e.scroll);
      return true;
    }
    if (hasTouch()) {
      touchMouse(L, e, x, y);
      return true;
    }
    if (e.action === "press") mousePress(L, e, x, y);
    else if (e.action === "release") mouseRelease(L, e, x, y);
    else if (e.action === "drag" && holding === "scrollbar") scrollTo(L, y);
    else if (e.action === "drag" && holding) {
      turn += (x - holding.x) * TURN;
      holding.x = x;
    }
    return true;
  },
  screenKey(screen, k) {
    if (screen !== OWN) return false;
    if (k.key === "key.keyboard.left.shift" || k.key === "key.keyboard.right.shift") shift = k.pressed;
    if (k.key === "key.keyboard.left.control" || k.key === "key.keyboard.right.control") control = k.pressed;
    if (!k.pressed) return false;
    if (focused) {
      if (k.key === "key.keyboard.backspace") {
        search = search.slice(0, -1);
        scroll = 0;
      } else if (k.key === "key.keyboard.enter" || k.key === "key.keyboard.keypad.enter") {
        if (hasTouch()) focus(false);
      } else if (k.text && !k.control && search.length < 50 && k.text >= " ") {
        search += k.text;
        scroll = 0;
      }
      // Typing keeps its keys (E types an e, it does not close the screen).
      return true;
    }
    // key.chat: the search tab, its box taking the keys (CreativeModeInventoryScreen.keyPressed).
    if (k.key === "key.keyboard.t") {
      focus(true);
      return true;
    }
    return hotkey(layout(size.width, size.height), k.key);
  },
};
