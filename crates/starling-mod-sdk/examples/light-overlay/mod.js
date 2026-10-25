// Light Overlay: where monsters can spawn around the player, as Light Overlay (shedaniel's) and
// Lighty show it. Every place within reach where a monster has room to spawn (the game's own
// SpawnPlacements check, level.get-spawn-spots) gets a mark in Lighty's colours: green where block
// light keeps them off (shown unless turned off), orange where they spawn only at night (sky light
// above 7), red where they spawn at any time. The mark is one of four modes:
//
// - Cross: Light Overlay's cross, a line corner to corner each way on the ground;
// - Number: Lighty's numbers lying on the ground, the block light over the sky light;
// - Tile: Lighty's carpet, a see-through tile a pixel thick;
// - Marker: a see-through block standing on the place.
//
// F7 turns it on and off (Light Overlay's key), as does its touch button; F8, or Config on the
// Mods page, opens its settings: the mode, the green marks, the reach. They are kept.
import { getPlayer } from "starling:mods/game@0.1.0";
import { getSpawnSpots, getBiome } from "starling:mods/level@0.1.0";
import { setLayer, removeLayer } from "starling:mods/world-draw@0.1.0";
import { addTouchButton } from "starling:mods/widgets@0.1.0";
import { button, panel, text, textWidth } from "starling:mods/draw@0.1.0";
import { open, close, setConfig } from "starling:mods/screens@0.1.0";
import { get, set } from "starling:mods/storage@0.1.0";
import { write } from "starling:mods/log@0.1.0";

// Lighty's thresholds and colours: block light above 0 is safe, sky light above 7 only at night.
const BLOCK_THRESHOLD = 0, SKY_THRESHOLD = 7;
const GREEN = 0x00FF00, ORANGE = 0xFF6600, RED = 0xFF0000;
const MODES = ["Cross", "Number", "Tile", "Marker"];
const REACHES = [8, 12, 16];
// Light Overlay's cross, a little wider to be seen on a phone's screen.
const CROSS_WIDTH = 2;
// Lighty's carpet: half see-through, a pixel thick; the marker a block, more see-through. A box
// shows two faces over what is behind it (its top and its bottom on the ground, say), so each
// face is a little more see-through: 1 - (1 - a)^2 is the whole.
const TILE_ALPHA = 0x4B, MARKER_ALPHA = 0x36;
const PIXEL = 1 / 16, LIFT = 0.002;
// Lighty's numbers: a quarter block high (its font's 8 pixels, a name tag's 1/40 block each at
// scale 1), the block light's middle 3 pixels beyond the block's, the sky light's 3 before, as
// the game turns them to be read from where the player looks (along the nearest direction).
const NUMBER_SCALE = 1.25, NUMBER_STEP = 3 * (1 / 16);
const MUSHROOM = "minecraft:mushroom_fields";
// A look at the blocks around (a few milliseconds on a phone) when the player gets to another
// block; else every second, and two ticks after the player places or breaks one (its light
// settled), as Light Overlay's cache looks again where blocks change.
const EVERY = 20, SOON = 2;

// The settings, kept: the mode, the green marks shown, the reach.
const settings = { mode: 0, safe: true, reach: 1 };
const saved = get("settings");
if (saved && saved.length >= 3) {
  settings.mode = Math.min(saved[0], MODES.length - 1);
  settings.safe = saved[1] === 1;
  settings.reach = Math.min(saved[2], REACHES.length - 1);
}
function save() {
  try {
    set("settings", new Uint8Array([settings.mode, settings.safe ? 1 : 0, settings.reach]));
  } catch (_) {
    // A full storage keeps the old settings.
  }
}

let enabled = false;
let ticks = 0;
// When the next look is due, and the block (and the numbers' direction) of the last.
let next = 0;
let last = "";

// The nearest of the four directions to where the player looks (x, z), as the game turns text
// lying on the ground: yaw 0 looks south.
function ahead(p) {
  const fx = -Math.sin(p.yaw * Math.PI / 180), fz = Math.cos(p.yaw * Math.PI / 180);
  return Math.abs(fx) > Math.abs(fz) ? [Math.sign(fx), 0] : [0, fz < 0 ? -1 : 1];
}
// What a look depends on: the block, and the numbers' direction in the number mode.
function where(p) {
  const block = `${Math.floor(p.x)},${Math.floor(p.y)},${Math.floor(p.z)}`;
  return MODES[settings.mode] === "Number" ? `${block},${ahead(p)}` : block;
}

// Lighty's LightyColors.getARGB, 0 for a safe place when those are hidden.
function color(blockLight, skyLight) {
  if (blockLight > BLOCK_THRESHOLD) return settings.safe ? GREEN : 0;
  return skyLight > SKY_THRESHOLD ? ORANGE : RED;
}

// Whether a place is in mushroom fields, by its biome cell (4 blocks a side, as the game keeps
// biomes), kept: a biome costs a call, a cell's stays. A cell's key is a number (the cells of
// one place 8 million blocks apart share one, never both near).
const mushroom = new Map();
function inMushroomFields(x, y, z) {
  const key = (((x >> 2) & 0x1FFFFF) * 512 + ((y >> 2) & 511)) * 0x200000 + ((z >> 2) & 0x1FFFFF);
  let m = mushroom.get(key);
  if (m === undefined) {
    if (mushroom.size > 4096) mushroom.clear();
    m = getBiome({ x, y, z }) === MUSHROOM;
    mushroom.set(key, m);
  }
  return m;
}

// The marks around the player, from its block.
function draw(p) {
  const at = { x: Math.floor(p.x), y: Math.floor(p.y), z: Math.floor(p.z) };
  last = where(p);
  const [ax, az] = ahead(p);
  next = ticks + EVERY;
  const reach = REACHES[settings.reach];
  const spots = getSpawnSpots(
    { x: at.x - reach, y: at.y - reach, z: at.z - reach },
    { x: at.x + reach, y: at.y + reach, z: at.z + reach },
  );
  const pos = spots.positions, block = spots.blockLight, sky = spots.skyLight, tops = spots.tops;
  const n = block.length;
  const mode = MODES[settings.mode];
  // What a mode draws a place with: lines, boxes, texts (two a place).
  const ends = new Float32Array(mode === "Cross" ? n * 12 : mode === "Number" ? 0 : n * 24);
  const lineColors = new Uint32Array(mode === "Cross" ? n * 2 : mode === "Number" ? 0 : n * 4);
  const corners = new Float32Array(mode === "Tile" || mode === "Marker" ? n * 6 : 0);
  const boxColors = new Uint32Array(corners.length / 6);
  const places = new Float32Array(mode === "Number" ? n * 6 : 0);
  const numbers = [];
  const textColors = new Uint32Array(places.length / 3);
  let lines = 0, boxes = 0, texts = 0;
  const line = (x1, y1, z1, x2, y2, z2, c) => {
    const e = lines * 6;
    ends[e] = x1; ends[e + 1] = y1; ends[e + 2] = z1;
    ends[e + 3] = x2; ends[e + 4] = y2; ends[e + 5] = z2;
    lineColors[lines++] = c;
  };
  for (let i = 0; i < n; i++) {
    const x = pos[i * 3], y = pos[i * 3 + 1], z = pos[i * 3 + 2];
    const dx = x - at.x, dy = y - at.y, dz = z - at.z;
    // BlockPos.closerThan(playerPos, reach).
    if (dx * dx + dy * dy + dz * dz >= reach * reach) continue;
    const rgb = color(block[i], sky[i]);
    if (rgb === 0 || inMushroomFields(x, y, z)) continue;
    // The ground: a snow layer's top, say.
    const g = dy + tops[i];
    if (mode === "Cross") {
      // LightOverlayRenderer.renderCross: a hair inside the block, a hair above the ground.
      const c = 0xFF000000 | rgb, h = g + 0.005;
      line(dx + 0.01, h, dz + 0.01, dx + 0.99, h, dz + 0.99, c);
      line(dx + 0.99, h, dz + 0.01, dx + 0.01, h, dz + 0.99, c);
    } else if (mode === "Number") {
      const t = texts * 3, h = g + 0.005, sx = ax * NUMBER_STEP, sz = az * NUMBER_STEP;
      places[t] = dx + 0.5 + sx; places[t + 1] = h; places[t + 2] = dz + 0.5 + sz;
      places[t + 3] = dx + 0.5 - sx; places[t + 4] = h; places[t + 5] = dz + 0.5 - sz;
      numbers.push(`${block[i]}`, `${sky[i]}`);
      textColors[texts++] = 0xFF000000 | rgb;
      textColors[texts++] = 0xFF000000 | rgb;
    } else {
      const tall = mode === "Tile" ? PIXEL : 1;
      const alpha = mode === "Tile" ? TILE_ALPHA : MARKER_ALPHA;
      // A hair above the ground, which its bottom would fight with.
      const b = boxes * 6, bottom = g + LIFT;
      corners[b] = dx; corners[b + 1] = bottom; corners[b + 2] = dz;
      corners[b + 3] = dx + 1; corners[b + 4] = bottom + tall; corners[b + 5] = dz + 1;
      boxColors[boxes++] = (alpha << 24 | rgb) >>> 0;
      // Its top's edges, solid.
      const c = 0xFF000000 | rgb, h = bottom + tall + LIFT;
      line(dx, h, dz, dx + 1, h, dz, c);
      line(dx + 1, h, dz, dx + 1, h, dz + 1, c);
      line(dx + 1, h, dz + 1, dx, h, dz + 1, c);
      line(dx, h, dz + 1, dx, h, dz, c);
    }
  }
  const layer = { lines: [], boxes: [], texts: [] };
  if (lines > 0) {
    layer.lines.push({
      origin: at, ends: ends.subarray(0, lines * 6), colors: lineColors.subarray(0, lines),
      width: mode === "Cross" ? CROSS_WIDTH : 1,
    });
  }
  if (boxes > 0) {
    layer.boxes.push({
      origin: at, corners: corners.subarray(0, boxes * 6), colors: boxColors.subarray(0, boxes),
    });
  }
  if (texts > 0) {
    layer.texts.push({
      origin: at, places: places.subarray(0, texts * 3), lines: numbers,
      colors: textColors.subarray(0, texts), scale: NUMBER_SCALE, facing: "up",
    });
  }
  try {
    setLayer("marks", layer);
  } catch (e) {
    write("warn", `${e.payload === undefined ? e : e.payload}`);
  }
}

function refresh() {
  const p = enabled && getPlayer();
  if (p) draw(p);
}

function toggle() {
  enabled = !enabled;
  if (enabled) {
    refresh();
  } else {
    removeLayer("marks");
  }
}

// A block placed or broken: look again soon.
function changed() {
  if (enabled) next = Math.min(next, ticks + SOON);
}

// The settings screen: its panel and buttons, centred in the GUI.
const SETTINGS = "light_overlay:settings";
function layout(context) {
  const x = Math.round(context.width / 2 - 100), y = Math.round(context.height / 2 - 62);
  const change = (f) => () => {
    f();
    save();
    refresh();
  };
  return {
    panel: [x, y, 200, 124],
    buttons: [
      { rect: [x + 10, y + 24, 180, 20], label: `Mode: ${MODES[settings.mode]}`,
        press: change(() => { settings.mode = (settings.mode + 1) % MODES.length; }) },
      { rect: [x + 10, y + 48, 180, 20], label: `Lit places (green): ${settings.safe ? "Shown" : "Hidden"}`,
        press: change(() => { settings.safe = !settings.safe; }) },
      { rect: [x + 10, y + 72, 180, 20], label: `Reach: ${REACHES[settings.reach]} blocks`,
        press: change(() => { settings.reach = (settings.reach + 1) % REACHES.length; }) },
      { rect: [x + 10, y + 96, 180, 20], label: "Done", press: () => close() },
    ],
  };
}
const inside = ([x, y, w, h], px, py) => px >= x && px < x + w && py >= y && py < y + h;
let size = { width: 0, height: 0 };

export const events = {
  init() {
    setConfig("settings");
    // At the left, under Waypoints' compass and over the stick: the top right is the HUD's
    // (Keystrokes' panel, the effects).
    addTouchButton({
      id: "toggle", label: "Light", icon: "item/torch", shape: "round", x: 0.045, y: 0.5, size: 0.42,
    });
    return {
      commands: [], tick: true, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: true, tooltips: false,
      keybinds: [
        { id: "toggle", name: "Toggle Light Overlay", key: "key.keyboard.f7" },
        { id: "settings", name: "Light Overlay Settings", key: "key.keyboard.f8" },
      ],
      channels: [], connection: false, interaction: true, damage: false,
    };
  },
  tick() {
    ticks++;
    if (!enabled) return;
    const p = getPlayer();
    if (p && (ticks >= next || where(p) !== last)) draw(p);
  },
  key(id, pressed) {
    if (!pressed) return;
    if (id === "toggle") toggle();
    if (id === "settings") open("settings");
  },
  screenRender(screen, context) {
    if (screen !== SETTINGS) return;
    size = context;
    const { panel: [x, y, w, h], buttons } = layout(context);
    panel(x, y, w, h);
    const title = "Light Overlay";
    text(x + Math.round((w - textWidth(title) + 1) / 2), y + 8, title, 0xFFFFFF, true);
    for (const b of buttons) {
      button(...b.rect, b.label, inside(b.rect, context.mouseX, context.mouseY), true);
    }
  },
  screenMouse(screen, event) {
    if (screen !== SETTINGS) return false;
    if (event.action === "press" && event.button === 0) {
      const hit = layout(size).buttons.find((b) => inside(b.rect, event.x, event.y));
      if (hit) hit.press();
    }
    return true;
  },
  useItem() {
    return true;
  },
  useBlock() {
    changed();
    return true;
  },
  attackEntity() {
    return true;
  },
  attackBlock() {
    return true;
  },
  blockBroken() {
    changed();
  },
};
