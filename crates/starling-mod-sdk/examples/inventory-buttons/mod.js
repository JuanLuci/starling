// Inventory Buttons: Quark's chest buttons and inventory sorting, done as Inventory Profiles Next
// does them: with the clicks a mouse makes, so any server takes them. On a storage container's
// window (a chest, a double chest, a barrel, an ender chest, a shulker box): Take all and Sort in
// its title row, Deposit all and Sort in the inventory's. On the inventory, and
// on the creative screen's inventory tab: Sort. The game places them from the slots
// (widgets.add-window-button), so they sit right on each of those windows, as Quark's sit from
// its slots.
//
// Take all Shift-clicks every stack of the container into the inventory; Deposit all every stack
// of the inventory (not the hotbar) into the container, as Quark's Extract and Insert. Sort stacks
// what can stack and orders the stacks as the creative inventory lists items (Inventory Profiles
// Next's default order), the hotbar left as it is. A sort too long for one tick's clicks goes on
// over the next ticks, planned again from the menu as it then is.
import { getMenu, click } from "starling:mods/slots@0.1.0";
import { getItemInfo, getItemOrder } from "starling:mods/inventory@0.1.0";
import { addWindowButton } from "starling:mods/widgets@0.1.0";
import { write } from "starling:mods/log@0.1.0";

// The menus with a container to take from and put in: vanilla's storage menus of 27 slots or
// more (chests, double chests, barrels, ender chests, shulker boxes), as Quark's buttons show on
// those only (a dispenser's centred title has no room beside its slots).
const STORAGE = [
  "minecraft:generic_9x3", "minecraft:generic_9x4", "minecraft:generic_9x5",
  "minecraft:generic_9x6", "minecraft:shulker_box",
];
const INVENTORY = "minecraft:inventory";
// The buttons: 12 pixels, right-aligned in the row above a part's slots, 14 apart.
const SIZE = 12, STEP = 14;
// Clicks a tick: a call may make 64.
const CLICKS = 60;
// Ticks a sort may take before it gives up (a server refusing its clicks).
const PATIENCE = 40;

// A call into the game that may refuse: logged, not fatal.
function act(f) {
  try {
    f();
    return true;
  } catch (e) {
    write("debug", `${e}`);
    return false;
  }
}

// The menu slots of a part: what was opened, or the player's inventory without the hotbar.
function part(m, which) {
  const own = m.kind === INVENTORY ? 0 : m.slots.length - 36;
  const out = [];
  if (which === "container") {
    for (let i = 0; i < own; i++) out.push(i);
  } else {
    // The inventory menu's 27 start at 9 (after the result, the grid and the armour).
    const first = m.kind === INVENTORY ? 9 : own;
    for (let i = first; i < first + 27; i++) out.push(i);
  }
  return out;
}

// Shift-clicks on every stack of `from`: into the other part, as the game moves them.
function moveAll(from) {
  const m = getMenu();
  if (!m || m.carried) return;
  for (const i of part(m, from)) {
    if (m.slots[i]) act(() => click(i, 0, "quick-move"));
  }
}

// Sorting.
const key = (s) => `${s.id}\u0000${s.components}`;
const same = (a, b) => !!a && !!b && key(a) === key(b);
const equal = (a, b) => (!a && !b) || (same(a, b) && a.count === b.count);
const maxOf = (s) => {
  const info = getItemInfo(s.id);
  return info ? Math.max(1, info.maxStackSize) : 64;
};
// The creative inventory's order (unlisted items after it, by id), then the bigger stack first.
const places = new Map();
function place(s) {
  const k = key(s);
  if (!places.has(k)) {
    const p = getItemOrder(s);
    places.set(k, p === undefined || p === null ? Infinity : p);
  }
  return places.get(k);
}
function compare(a, b) {
  const pa = place(a), pb = place(b);
  if (pa !== pb) return pa < pb ? -1 : 1;
  if (a.id !== b.id) return a.id < b.id ? -1 : 1;
  if (a.components !== b.components) return a.components < b.components ? -1 : 1;
  return b.count - a.count;
}

// A left click on a slot as the game does it (AbstractContainerMenu's PICKUP), on a copy.
function leftClick(state, i) {
  const here = state.slots[i], hand = state.carried;
  if (!hand) {
    state.carried = here;
    state.slots[i] = null;
  } else if (!here) {
    state.slots[i] = hand;
    state.carried = null;
  } else if (same(here, hand)) {
    const put = Math.min(maxOf(here) - here.count, hand.count);
    state.slots[i] = Object.assign({}, here, { count: here.count + put });
    state.carried = hand.count - put > 0 ? Object.assign({}, hand, { count: hand.count - put }) : null;
  } else {
    state.slots[i] = hand;
    state.carried = here;
  }
}

// The clicks that sort `slots` of menu `m`, in groups that each leave the hand empty: stacks of
// one item merged first, then each place given its stack.
function plan(m, slots) {
  const state = { slots: m.slots.slice(), carried: null };
  const groups = [];
  const run = (clicks) => {
    for (const i of clicks) leftClick(state, i);
    groups.push(clicks);
  };
  // Merge: the last part-filled stack of an item into the others, what is left back where it was.
  const partial = new Map();
  for (const i of slots) {
    const s = state.slots[i];
    if (s && s.count < maxOf(s)) {
      if (!partial.has(key(s))) partial.set(key(s), []);
      partial.get(key(s)).push(i);
    }
  }
  for (const list of partial.values()) {
    while (list.length > 1) {
      const from = list.pop();
      const clicks = [from];
      state.carried = state.slots[from];
      state.slots[from] = null;
      for (const i of list) {
        if (!state.carried) break;
        clicks.push(i);
        leftClick(state, i);
      }
      if (state.carried) {
        clicks.push(from);
        leftClick(state, from);
        list.push(from);
      }
      groups.push(clicks);
      // The slots it filled are part-filled no more.
      for (let k = list.length - 1; k >= 0; k--) {
        const s = state.slots[list[k]];
        if (!s || s.count >= maxOf(s)) list.splice(k, 1);
      }
    }
  }
  // Place: the sorted stacks from the first slot on, each swapped in from where it is.
  const target = slots.map((i) => state.slots[i]).filter(Boolean).sort(compare);
  slots.forEach((i, n) => {
    const want = target[n];
    if (!want || equal(state.slots[i], want)) return;
    const j = slots.slice(n + 1).find((k) => equal(state.slots[k], want));
    if (j === undefined) return;
    run(state.slots[i] ? [j, i, j] : [j, i]);
  });
  return groups;
}

// The sort running: which part, how many ticks it has had.
let sorting = null;
function sortStep() {
  const m = getMenu();
  // The player holds a stack, or the screen went: stop.
  if (!m || m.carried || (m.kind !== INVENTORY && !STORAGE.includes(m.kind))) {
    sorting = null;
    return;
  }
  const groups = plan(m, part(m, sorting.part));
  let used = 0;
  for (const g of groups) {
    if (used + g.length > CLICKS) break;
    for (const i of g) act(() => click(i, 0, "pickup"));
    used += g.length;
  }
  sorting.ticks++;
  if (used === 0 || used === groups.reduce((n, g) => n + g.length, 0) || sorting.ticks > PATIENCE) {
    sorting = null;
  }
}
function sort(which) {
  sorting = { part: which, ticks: 0 };
  sortStep();
}

function button(id, which, menus, x, icon, label, tooltip, onPress) {
  act(() => addWindowButton({
    id, part: which, anchor: "top-right", x, y: -(SIZE + 1), width: SIZE, height: SIZE,
    icon: `inventory_buttons:${icon}`, label, tooltip, menus,
  }, onPress));
}

export const events = {
  init() {
    button("sort_container", "container", STORAGE, -1, "sort", "S",
      ["Sort", "§7Stacks and orders what is inside"], () => sort("container"));
    button("take_all", "container", STORAGE, -1 - STEP, "take_all", "T",
      ["Take all", "§7Everything inside into your inventory"], () => moveAll("container"));
    button("sort_inventory", "inventory", [INVENTORY, ...STORAGE], -1, "sort", "S",
      ["Sort inventory", "§7Stacks and orders it, the hotbar kept"], () => sort("inventory"));
    button("deposit_all", "inventory", STORAGE, -1 - STEP, "deposit_all", "D",
      ["Deposit all", "§7Your inventory (not the hotbar) inside"], () => moveAll("inventory"));
    return {
      commands: [], tick: true, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  tick() {
    if (sorting) sortStep();
  },
};
