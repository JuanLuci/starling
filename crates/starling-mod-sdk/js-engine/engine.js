// Starling's shared JavaScript engine: one component every JavaScript mod runs in. The game
// gives it the mod's source (js-source); the mod is an ES module importing starling:mods
// interfaces by name and exporting `events`, as a componentized mod would.
import * as mobile from "starling:mods/mobile@0.1.0";
import * as log from "starling:mods/log@0.1.0";
import * as game from "starling:mods/game@0.1.0";
import * as level from "starling:mods/level@0.1.0";
import * as inventory from "starling:mods/inventory@0.1.0";
import * as chat from "starling:mods/chat@0.1.0";
import * as chatSend from "starling:mods/chat-send@0.1.0";
import * as draw from "starling:mods/draw@0.1.0";
import * as hud from "starling:mods/hud@0.1.0";
import * as screens from "starling:mods/screens@0.1.0";
import * as widgetsApi from "starling:mods/widgets@0.1.0";
import * as keys from "starling:mods/keys@0.1.0";
import * as input from "starling:mods/input@0.1.0";
import * as sound from "starling:mods/sound@0.1.0";
import * as net from "starling:mods/net@0.1.0";
import * as storage from "starling:mods/storage@0.1.0";
import { get as source, snapshot as snapshotNow } from "starling:mods/js-source@0.1.0";
import { held as heldNow } from "starling:mods/js-keys@0.1.0";
import {
  fill as fillBits, gradient as gradientBits, batch,
} from "starling:mods/js-draw@0.1.0";

// A render hook's drawing reaches the game in one call when the hook returns: each call from
// JavaScript into the game costs microseconds (the engine's interpreter crossing into native
// code), and a HUD mod makes dozens a frame. Fills, gradients, texts, sprites, pushes and pops
// are queued in order (their numbers in 32-bit slots, colours as their bits, so an alpha of 0x80
// or more passes right: componentize-js passes such a u32 wrong); the other calls send the queue
// first. A text's width, which `text` returns, comes from a cache of the game's measures. The
// queue uses no builtin functions (push, Map, String...) but a join and an encode a frame: in
// this interpreter each costs about as much as a call into the game, and a string passed to the
// game costs a fifth of a microsecond a character where bytes pass whole, so the frame's texts
// go joined, as UTF-8.
const SLOTS = 8192;
const buffer = new ArrayBuffer(SLOTS * 4);
const floats = new Float32Array(buffer);
const ints = new Int32Array(buffer);
let used = 0;
let texts = [];
let count = 0;
let rendering = false;
let widths = Object.create(null);
let measured = 0;
// Measured again now and then: the player can change the font (Force Unicode).
let flushes = 0;
const utf8 = new TextEncoder();
let textBytes = new Uint8Array(16384);

function flush() {
  if (++flushes >= 1200) {
    widths = Object.create(null);
    measured = 0;
    flushes = 0;
  }
  if (used > 0) {
    let written = 0;
    if (count > 0) {
      const joined = texts.join("\u0000");
      // Up to three bytes a UTF-16 unit.
      if (joined.length * 3 > textBytes.length) textBytes = new Uint8Array(joined.length * 3);
      written = utf8.encodeInto(joined, textBytes).written;
    }
    batch(new Uint8Array(buffer, 0, used * 4), count, new Uint8Array(textBytes.buffer, 0, written));
    used = 0;
    texts = [];
    count = 0;
  }
}
function width(text) {
  let w = widths[text];
  if (w === undefined) {
    if (measured >= 1024) {
      widths = Object.create(null);
      measured = 0;
    }
    measured++;
    w = widths[text] = draw.textWidth(text);
  }
  return w;
}
// The calls not queued: the queue goes first, so that the order is kept.
const after = (f) => (...args) => {
  flush();
  return f(...args);
};
const drawing = {
  ...draw,
  fill(x, y, width, height, colour) {
    if (!rendering) return fillBits(x, y, width, height, colour | 0);
    if (used + 6 > SLOTS) flush();
    const at = used;
    ints[at] = 1;
    floats[at + 1] = x; floats[at + 2] = y; floats[at + 3] = width; floats[at + 4] = height;
    ints[at + 5] = colour | 0;
    used = at + 6;
  },
  gradient(x, y, width, height, top, bottom) {
    if (!rendering) return gradientBits(x, y, width, height, top | 0, bottom | 0);
    if (used + 7 > SLOTS) flush();
    const at = used;
    ints[at] = 2;
    floats[at + 1] = x; floats[at + 2] = y; floats[at + 3] = width; floats[at + 4] = height;
    ints[at + 5] = top | 0; ints[at + 6] = bottom | 0;
    used = at + 7;
  },
  text(x, y, text, colour, shadow) {
    if (!rendering) return draw.text(x, y, text, colour, shadow);
    if (typeof text !== "string") text = `${text}`;
    if (used + 6 > SLOTS) flush();
    const at = used;
    ints[at] = 3;
    floats[at + 1] = x; floats[at + 2] = y;
    ints[at + 3] = colour | 0; ints[at + 4] = shadow ? 1 : 0; ints[at + 5] = count;
    texts[count++] = text;
    used = at + 6;
    return width(text);
  },
  textWidth: (text) => width(typeof text === "string" ? text : `${text}`),
  sprite(x, y, width, height, sprite) {
    if (!rendering) return draw.sprite(x, y, width, height, sprite);
    if (used + 6 > SLOTS) flush();
    const at = used;
    ints[at] = 4;
    floats[at + 1] = x; floats[at + 2] = y; floats[at + 3] = width; floats[at + 4] = height;
    ints[at + 5] = count;
    texts[count++] = typeof sprite === "string" ? sprite : `${sprite}`;
    used = at + 6;
  },
  push(x, y, scale) {
    if (!rendering) return draw.push(x, y, scale);
    if (used + 4 > SLOTS) flush();
    const at = used;
    ints[at] = 5;
    floats[at + 1] = x; floats[at + 2] = y; floats[at + 3] = scale;
    used = at + 4;
  },
  pop() {
    if (!rendering) return draw.pop();
    if (used + 1 > SLOTS) flush();
    ints[used] = 6;
    used += 1;
  },
  item: after(draw.item),
  itemId: after(draw.itemId),
  tooltip: after(draw.tooltip),
  button: after(draw.button),
  panel: after(draw.panel),
};
// A render hook: its drawing queued, then sent when it returns.
function render(name) {
  const call = hook(name);
  return (...args) => {
    rendering = true;
    try {
      return call(...args);
    } finally {
      rendering = false;
      flush();
    }
  };
}

// What mods read of the game (game, level, inventory) changes once a tick, and frames come
// faster; a read costs microseconds a field (the player, 33 fields: 37 us). Each answer is kept
// until the game's snapshot changes, which is asked at most once a hook (it cannot change during
// one), and handed out as a shallow copy, so a mod changing it changes only its own.
let epoch = 0;
let checked = -1;
let generation = -1;
let kept = Object.create(null);
const NONE = {};
function current() {
  if (checked !== epoch) {
    checked = epoch;
    const g = snapshotNow();
    if (g !== generation) {
      generation = g;
      kept = Object.create(null);
    }
  }
}
function copied(v) {
  if (v === null || typeof v !== "object") return v;
  return Array.isArray(v) ? v.slice() : Object.assign({}, v);
}
// `read` kept by its name and its arguments' key.
function keep(name, read, key) {
  return (...args) => {
    current();
    const k = key ? name + key(...args) : name;
    let v = kept[k];
    if (v === undefined) {
      v = read(...args);
      kept[k] = v === undefined ? NONE : v;
    } else if (v === NONE) {
      return undefined;
    }
    return copied(v);
  };
}
const pos = (p) => `${p.x},${p.y},${p.z}`;
const one = (a) => `${a}`;
const reading = {
  game: {
    ...game,
    getPlayer: keep("player", game.getPlayer),
    getEffects: keep("effects", game.getEffects),
    getClock: keep("clock", game.getClock),
    getWorldInfo: keep("world", game.getWorldInfo),
    getTarget: keep("target", game.getTarget),
    getScreen: keep("screen", game.getScreen),
    getGui: keep("gui", game.getGui),
    getFps: keep("fps", game.getFps),
    getMining: keep("mining", game.getMining),
  },
  level: {
    ...level,
    getBlock: keep("block", level.getBlock, pos),
    getBlockInfo: keep("blockInfo", level.getBlockInfo, pos),
    getEntityInfo: keep("entityInfo", level.getEntityInfo, one),
    getBiome: keep("biome", level.getBiome, pos),
    getLight: keep("light", level.getLight, pos),
    getEntities: keep("entities", level.getEntities, one),
  },
  inventory: {
    ...inventory,
    getSlot: keep("slot", inventory.getSlot, one),
    getHeld: keep("held", inventory.getHeld, one),
    getCursor: keep("cursor", inventory.getCursor),
    getHovered: keep("hovered", inventory.getHovered),
    getContainer: keep("container", inventory.getContainer),
    getItemInfo: keep("itemInfo", inventory.getItemInfo, one),
  },
};

// The controls held, read once a hook (they change between frames, not during a hook) instead of
// a call a control.
const CONTROLS = Object.assign(Object.create(null), {
  "key.forward": 1, "key.back": 2, "key.left": 4, "key.right": 8, "key.jump": 16,
  "key.sneak": 32, "key.sprint": 64, "key.attack": 128, "key.use": 256,
});
let heldEpoch = -1;
let heldBits = 0;
const controls = {
  ...keys,
  isControlDown(control) {
    const bit = CONTROLS[control];
    if (bit === undefined) return false;
    if (heldEpoch !== epoch) {
      heldEpoch = epoch;
      heldBits = heldNow();
    }
    return (heldBits & bit) !== 0;
  },
};

// Buttons as Fabric's ButtonWidget.builder(label, onPress) makes them: the fields left out take
// their usual values, and `onPress` runs when the button is pressed (the mod's `key` hook still
// gets the press and the release, by the button's id).
const presses = new Map();
function withPress(add, defaults) {
  return (button, onPress) => {
    const full = { ...defaults, ...button };
    add(full);
    if (typeof onPress === "function") {
      presses.set(full.id, onPress);
    } else {
      presses.delete(full.id);
    }
  };
}
const widgets = {
  ...widgetsApi,
  addScreenButton: withPress(widgetsApi.addScreenButton, {
    anchor: "top-left", x: 0, y: 0, width: 100, height: 20, label: "", icon: "", tooltip: [],
    active: true,
  }),
  addTouchButton: withPress(widgetsApi.addTouchButton, {
    label: "", icon: "", shape: "round", x: 0.5, y: 0.5, size: 0.5,
  }),
  remove: (id) => {
    presses.delete(id);
    widgetsApi.remove(id);
  },
};

// `types` holds only types: nothing to import from it.
const API = {
  types: {}, mobile, log, game: reading.game, level: reading.level, inventory: reading.inventory,
  chat, "chat-send": chatSend, draw: drawing, hud,
  screens, widgets, keys: controls, input, sound, net, storage,
};

// The interface a module name imports: starling:mods/<interface>@0.1.x.
function api(name) {
  const m = /^starling:mods\/([a-z-]+)@0\.1\.\d+$/.exec(name);
  const ns = m && API[m[1]];
  if (!ns) {
    throw new Error(`imports ${name}, which Starling does not offer`);
  }
  return ns;
}

// The module's imports become constants and its exports properties of `exports`, so it runs
// as a function body.
function asFunctionBody(code) {
  return code
    .replace(/^[ \t]*import\s+\*\s+as\s+([\w$]+)\s+from\s+(["'][^"']+["'])\s*;?/gm,
      (_, ns, from) => `const ${ns} = __api(${from});`)
    .replace(/^[ \t]*import\s*\{([^}]*)\}\s*from\s+(["'][^"']+["'])\s*;?/gm,
      (_, names, from) => `const {${names.replace(/\s+as\s+/g, ": ")}} = __api(${from});`)
    .replace(/^[ \t]*export\s+(const|let|var)\s+([\w$]+)\s*=/gm,
      (_, kind, name) => `${kind} ${name} = __exports.${name} =`)
    .replace(/^[ \t]*export\s+(async\s+)?function\s*\*?\s*([\w$]+)/gm,
      (whole, _a, name) => { exported.push(name); return whole.replace(/^[ \t]*export\s+/, ""); })
    .replace(/^[ \t]*export\s*\{([^}]*)\}\s*;?/gm, (_, list) => list.split(",")
      .map((part) => part.trim()).filter(Boolean)
      .map((part) => {
        const [local, as] = part.split(/\s+as\s+/);
        return `__exports.${as || local} = ${local};`;
      }).join("\n"));
}
const exported = [];

let mod;
function load() {
  const exports = {};
  const body = asFunctionBody(source());
  const tail = exported.map((name) => `__exports.${name} = ${name};`).join("\n");
  new Function("__api", "__exports", `${body}\n${tail}`)(api, exports);
  if (!exports.events) {
    throw new Error("the mod exports no events");
  }
  return exports.events;
}

// A hook the mod leaves out answers as an unhandled one would. An error a hook throws stops the
// mod, and only `init` and `command` can say why, so the others log it (with where it was) first.
function hook(name, fallback, says = false) {
  return (...args) => {
    // A new hook: the game's snapshot may have changed since the last.
    epoch++;
    if (!mod[name]) return fallback;
    try {
      return mod[name](...args);
    } catch (e) {
      if (!says) log.write("error", `${name}: ${e}${e && e.stack ? `\n${e.stack}` : ""}`);
      throw e;
    }
  };
}

const keyHook = hook("key");

export const events = {
  init() {
    try {
      mod = load();
    } catch (e) {
      throw `${e}`;
    }
    return mod.init();
  },
  tick: hook("tick"),
  frame: hook("frame"),
  command: hook("command", undefined, true),
  chatReceived: hook("chatReceived", { tag: "keep" }),
  chatSending: hook("chatSending", { tag: "send" }),
  hudRender: render("hudRender"),
  renderHudElement: render("renderHudElement"),
  screenOpened: hook("screenOpened"),
  screenClosed: hook("screenClosed"),
  screenRender: render("screenRender"),
  screenMouse: hook("screenMouse", false),
  screenKey: hook("screenKey", false),
  tooltip: hook("tooltip", []),
  key(id, pressed) {
    // A press comes with a fresh snapshot, and onPress reads before the key hook does.
    epoch++;
    const onPress = presses.get(id);
    if (pressed && onPress) {
      try {
        onPress();
      } catch (e) {
        log.write("error", `button ${id}: ${e}${e && e.stack ? `\n${e.stack}` : ""}`);
        throw e;
      }
    }
    return keyHook(id, pressed);
  },
  payload: hook("payload"),
  joined: hook("joined"),
  left: hook("left"),
  dimensionChanged: hook("dimensionChanged"),
  useItem: hook("useItem", true),
  useBlock: hook("useBlock", true),
  attackEntity: hook("attackEntity", true),
  attackBlock: hook("attackBlock", true),
  blockBroken: hook("blockBroken"),
  playerHurt: hook("playerHurt"),
};
