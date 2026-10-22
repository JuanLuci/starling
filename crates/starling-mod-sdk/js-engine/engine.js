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
import * as keys from "starling:mods/keys@0.1.0";
import * as sound from "starling:mods/sound@0.1.0";
import * as net from "starling:mods/net@0.1.0";
import * as storage from "starling:mods/storage@0.1.0";
import { get as source } from "starling:mods/js-source@0.1.0";

// `types` holds only types: nothing to import from it.
const API = {
  types: {}, mobile, log, game, level, inventory, chat, "chat-send": chatSend, draw, hud, screens,
  keys, sound, net, storage,
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

// A hook the mod leaves out answers as an unhandled one would.
function hook(name, fallback) {
  return (...args) => (mod[name] ? mod[name](...args) : fallback);
}

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
  command: hook("command"),
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
  key: hook("key"),
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
