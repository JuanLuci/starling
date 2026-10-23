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
import { get as source } from "starling:mods/js-source@0.1.0";
import { fill as fillBits, gradient as gradientBits } from "starling:mods/js-draw@0.1.0";

// componentize-js passes a u32 of 2^31 or more (an ARGB colour with an alpha of 0x80 or more,
// such as 0xF0100010) wrong, as the low bits of the number's double: fill and gradient go through
// js-draw, which takes their colours as s32, the same bits.
const drawing = {
  ...draw,
  fill: (x, y, width, height, colour) => fillBits(x, y, width, height, colour | 0),
  gradient: (x, y, width, height, top, bottom) =>
    gradientBits(x, y, width, height, top | 0, bottom | 0),
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
  types: {}, mobile, log, game, level, inventory, chat, "chat-send": chatSend, draw: drawing, hud,
  screens, widgets, keys, input, sound, net, storage,
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
  hudRender: hook("hudRender"),
  renderHudElement: hook("renderHudElement"),
  screenOpened: hook("screenOpened"),
  screenClosed: hook("screenClosed"),
  screenRender: hook("screenRender"),
  screenMouse: hook("screenMouse", false),
  screenKey: hook("screenKey", false),
  tooltip: hook("tooltip", []),
  key(id, pressed) {
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
