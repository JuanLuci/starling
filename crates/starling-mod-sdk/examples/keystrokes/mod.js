// Keystrokes: W A S D, both mouse buttons with their clicks per second and the jump bar, lit
// while held, in a top corner of the HUD. The controls are the game's, so a controller's stick
// or the touch stick lights them too. Its Config button on the Mods page (Mod Menu's) opens a
// screen to hide the clicks per second and pick the corner, kept with storage.
import { isControlDown, presses } from "starling:mods/keys@0.1.0";
import { button, fill, panel, text, textWidth } from "starling:mods/draw@0.1.0";
import { close, setConfig } from "starling:mods/screens@0.1.0";
import { get, set } from "starling:mods/storage@0.1.0";

const WIDTH = 70, MARGIN = 4;
// Each box: its control, label and rectangle (x, y, width, height) from the top left.
const BOXES = [
  { control: "key.forward", label: "W", rect: [24, 0, 22, 22] },
  { control: "key.left", label: "A", rect: [0, 24, 22, 22] },
  { control: "key.back", label: "S", rect: [24, 24, 22, 22] },
  { control: "key.right", label: "D", rect: [48, 24, 22, 22] },
  { control: "key.attack", label: "LMB", rect: [0, 48, 34, 22], cps: true },
  { control: "key.use", label: "RMB", rect: [36, 48, 34, 22], cps: true },
  { control: "key.jump", label: "", rect: [0, 72, 70, 9], bar: true },
];

// The settings: clicks per second shown, and the corner (the right one, else the left).
const saved = get("settings");
const settings = { cps: !saved || saved[0] === 1, right: !saved || saved[1] === 1 };
function save() {
  try {
    set("settings", new Uint8Array([settings.cps ? 1 : 0, settings.right ? 1 : 0]));
  } catch (_) {
    // A full storage keeps the old settings.
  }
}

// Seconds since the mod started; for each counted button, the presses seen and when they came.
let now = 0;
const clicks = { "key.attack": { seen: undefined, at: [] }, "key.use": { seen: undefined, at: [] } };

function cps(control) {
  const c = clicks[control];
  const total = Number(presses(control));
  // The first look only learns the count.
  if (c.seen === undefined) c.seen = total;
  for (; c.seen < total; c.seen++) c.at.push(now);
  while (c.at.length && c.at[0] <= now - 1) c.at.shift();
  return c.at.length;
}

// `label` centred in a box.
function centred(label, x, y, width, colour) {
  text(x + Math.round((width - textWidth(label) + 1) / 2), y, label, colour, false);
}

// The settings screen: its panel and buttons, centred in the GUI.
const SETTINGS = "keystrokes:settings";
function layout(context) {
  const x = Math.round(context.width / 2 - 100), y = Math.round(context.height / 2 - 50);
  return {
    panel: [x, y, 200, 100],
    buttons: [
      { rect: [x + 10, y + 24, 180, 20], label: `Clicks per second: ${settings.cps ? "Shown" : "Hidden"}`,
        press: () => { settings.cps = !settings.cps; save(); } },
      { rect: [x + 10, y + 48, 180, 20], label: `Corner: ${settings.right ? "Top Right" : "Top Left"}`,
        press: () => { settings.right = !settings.right; save(); } },
      { rect: [x + 10, y + 72, 180, 20], label: "Done", press: () => close() },
    ],
  };
}
const inside = ([x, y, w, h], px, py) => px >= x && px < x + w && py >= y && py < y + h;
let size = { width: 0, height: 0 };

export const events = {
  init() {
    setConfig("settings");
    return {
      commands: [], tick: false, frame: true, chat: false, chatSend: false,
      hud: true, hudElements: [], screens: true, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  frame(seconds) {
    now += seconds;
  },
  hudRender(context) {
    const left = settings.right ? context.width - WIDTH - MARGIN : MARGIN;
    for (const box of BOXES) {
      const [bx, by, width, height] = box.rect;
      const x = left + bx, y = MARGIN + by;
      const held = isControlDown(box.control);
      const ink = held ? 0x000000 : 0xFFFFFF;
      fill(x, y, width, height, held ? 0xCCFFFFFF : 0x80000000);
      if (box.bar) {
        fill(x + width / 4, y + 4, width / 2, 1, held ? 0xFF000000 : 0xFFFFFFFF);
      } else if (box.cps && settings.cps) {
        centred(box.label, x, y + 3, width, ink);
        centred(`${cps(box.control)} CPS`, x, y + 12, width, ink);
      } else {
        centred(box.label, x, y + 7, width, ink);
      }
    }
  },
  screenRender(screen, context) {
    if (screen !== SETTINGS) return;
    size = context;
    const { panel: [x, y, w, h], buttons } = layout(context);
    panel(x, y, w, h);
    const title = "Keystrokes";
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
};
