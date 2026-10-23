// Keystrokes: W A S D, both mouse buttons with their clicks per second and the jump bar, lit
// while held, at the top right of the HUD. The controls are the game's, so a controller's stick
// or the touch stick lights them too.
import { isControlDown, presses } from "starling:mods/keys@0.1.0";
import { fill, text, textWidth } from "starling:mods/draw@0.1.0";

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

export const events = {
  init() {
    return {
      commands: [], tick: false, frame: true, chat: false, chatSend: false,
      hud: true, hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  frame(seconds) {
    now += seconds;
  },
  hudRender(context) {
    const left = context.width - WIDTH - MARGIN;
    for (const box of BOXES) {
      const [bx, by, width, height] = box.rect;
      const x = left + bx, y = MARGIN + by;
      const held = isControlDown(box.control);
      const ink = held ? 0x000000 : 0xFFFFFF;
      fill(x, y, width, height, held ? 0xCCFFFFFF : 0x80000000);
      if (box.bar) {
        fill(x + width / 4, y + 4, width / 2, 1, held ? 0xFF000000 : 0xFFFFFFFF);
      } else if (box.cps) {
        centred(box.label, x, y + 3, width, ink);
        centred(`${cps(box.control)} CPS`, x, y + 12, width, ink);
      } else {
        centred(box.label, x, y + 7, width, ink);
      }
    }
  },
};
