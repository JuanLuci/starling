// ShulkerBoxTooltip: a shulker box's tooltip shows what it holds as slots on the box's colour, in
// place of the game's list of its first five stacks; so does any stack carrying a container's
// items (`minecraft:container`: a chest, a barrel, a dispenser picked with its contents). Each
// kind of item once with how many there are, as ShulkerBoxTooltip's compact preview; with Shift
// held, each slot where it is, as its full one. The stack's components come as SNBT, read here.
import { addGrid } from "starling:mods/tooltips@0.1.0";
import { hasTouch } from "starling:mods/mobile@0.1.0";
import { write } from "starling:mods/log@0.1.0";

// The colours of the boxes (DyeColor's texture diffuse colours; the plain box's purple).
const COLORS = {
  white: 0xF9FFFE, orange: 0xF9801D, magenta: 0xC74EBD, light_blue: 0x3AB3DA, yellow: 0xFED83D,
  lime: 0x80C71F, pink: 0xF38BAA, gray: 0x474F52, light_gray: 0x9D9D97, cyan: 0x169C9C,
  purple: 0x8932B8, blue: 0x3C44AA, brown: 0x835432, green: 0x5E7C16, red: 0xB02E26,
  black: 0x1D1D21,
};
const SHULKER = 0x976997;
// How many slots and columns a container has, by its item.
const SIZES = {
  "minecraft:dispenser": [9, 3], "minecraft:dropper": [9, 3], "minecraft:crafter": [9, 3],
  "minecraft:hopper": [5, 5], "minecraft:brewing_stand": [5, 5], "minecraft:furnace": [3, 3],
  "minecraft:blast_furnace": [3, 3], "minecraft:smoker": [3, 3], "minecraft:chiseled_bookshelf": [6, 3],
  "minecraft:decorated_pot": [1, 1], "minecraft:campfire": [4, 4], "minecraft:soul_campfire": [4, 4],
};
// The slots in the colour's full shade, as ShulkerBoxTooltip's.
const TINT = 0xFF000000;

// ---- SNBT, as the game writes components ----

// The value at `i` of `text`: [value, end]. Compounds are objects (each also keeps its own text,
// `$raw`), lists arrays, numbers numbers, strings strings.
function parse(text, i) {
  const skip = () => {
    while (i < text.length && " \n\t".includes(text[i])) i++;
  };
  skip();
  const c = text[i];
  if (c === "{") {
    const start = i;
    const out = {};
    i++;
    skip();
    while (text[i] !== "}") {
      let key;
      if (text[i] === "\"" || text[i] === "'") {
        [key, i] = parse(text, i);
        skip();
        i++; // :
      } else {
        // A bare key may hold ':' itself (minecraft:container): the last of the run is the one.
        let j = i;
        while (j < text.length && /[A-Za-z0-9_.+\-:]/.test(text[j])) j++;
        const colon = text.lastIndexOf(":", j - 1);
        key = text.slice(i, colon);
        i = colon + 1;
      }
      let value;
      [value, i] = parse(text, i);
      out[key] = value;
      skip();
      if (text[i] === ",") i++;
      skip();
    }
    Object.defineProperty(out, "$raw", { value: text.slice(start, i + 1) });
    return [out, i + 1];
  }
  if (c === "[") {
    i++;
    // Typed arrays: [B;1b,2b], [I;...], [L;...].
    if (/[BIL]/.test(text[i]) && text[i + 1] === ";") i += 2;
    const out = [];
    skip();
    while (text[i] !== "]") {
      let value;
      [value, i] = parse(text, i);
      out.push(value);
      skip();
      if (text[i] === ",") i++;
      skip();
    }
    return [out, i + 1];
  }
  if (c === "\"" || c === "'") {
    let out = "";
    i++;
    while (text[i] !== c) {
      if (text[i] === "\\") i++;
      out += text[i++];
    }
    return [out, i + 1];
  }
  let j = i;
  while (j < text.length && !",}] ".includes(text[j])) j++;
  const word = text.slice(i, j);
  const n = Number(word.replace(/[bslfdBSLFD]$/, ""));
  return [Number.isNaN(n) ? word : n, j];
}

// The stacks a container component holds: [slot, stack] in order.
function contents(stack) {
  if (!stack.components || !stack.components.includes("minecraft:container")) return [];
  let components;
  try {
    components = parse(stack.components, 0)[0];
  } catch (e) {
    write("debug", `unreadable components of ${stack.id}: ${e}`);
    return [];
  }
  const slots = components["minecraft:container"];
  if (!Array.isArray(slots)) return [];
  return slots
    .filter((s) => s && s.item && s.item.id)
    .map((s) => [s.slot | 0, {
      id: s.item.id, count: s.item.count || 1, damage: 0, maxDamage: 0, name: "",
      components: s.item.components ? s.item.components.$raw : "",
    }]);
}

// The box's colour, from its item.
function color(id) {
  if (id === "minecraft:shulker_box") return SHULKER;
  const m = /^minecraft:(.+)_shulker_box$/.exec(id);
  return m && COLORS[m[1]] !== undefined ? COLORS[m[1]] : 0;
}

let shift = false;

export const events = {
  init() {
    return {
      commands: [], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: true, tooltips: true, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  // Shift, held over any screen.
  screenKey(_screen, event) {
    if (event.key === "key.keyboard.left.shift" || event.key === "key.keyboard.right.shift") {
      shift = event.pressed;
    }
    return false;
  },
  screenClosed() {
    shift = false;
  },
  tooltip(stack) {
    const held = contents(stack);
    if (held.length === 0) return [];
    const tint = color(stack.id);
    const background = tint ? (TINT | tint) >>> 0 : 0;
    let grid;
    if (shift) {
      // Each slot where it is.
      const [size, columns] = SIZES[stack.id] || [27, 9];
      const last = held.reduce((m, [slot]) => Math.max(m, slot), 0);
      const slots = new Array(Math.min(64, Math.max(size, last + 1))).fill(null);
      for (const [slot, s] of held) if (slot < slots.length) slots[slot] = s;
      grid = { columns, stacks: slots, color: background };
    } else {
      // Each kind once, with how many, in the order first held.
      const kinds = [];
      const at = new Map();
      for (const [, s] of held) {
        const k = `${s.id}\u0000${s.components}`;
        if (at.has(k)) {
          kinds[at.get(k)].count += s.count;
        } else {
          at.set(k, kinds.length);
          kinds.push({ ...s });
        }
      }
      grid = { columns: Math.min(9, kinds.length), stacks: kinds.slice(0, 64), color: background };
    }
    try {
      addGrid(grid, true);
    } catch (e) {
      write("debug", `${e.payload === undefined ? e : e.payload}`);
      return [];
    }
    // ShulkerBoxTooltip's hint, where there is a Shift to hold.
    return shift || hasTouch() ? [] : ["§8Shift: each slot"];
  },
};
