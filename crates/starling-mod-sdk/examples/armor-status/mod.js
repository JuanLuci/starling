// Armor Status (ArmorStatusHUD's idea): the armour worn, head to feet, then the items in both
// hands, each with what is left of its durability as a number or a percent, from green through
// yellow to red, in a corner of the HUD (an item without durability is its icon alone). Its
// Config button on the Mods page opens a screen for the corner, the number and the held items,
// kept with storage.
import { getSlot, getHeld } from "starling:mods/inventory@0.1.0";
import { button, item, panel, text, textWidth } from "starling:mods/draw@0.1.0";
import { close, setConfig } from "starling:mods/screens@0.1.0";
import { get, set } from "starling:mods/storage@0.1.0";

const ROW = 18, MARGIN = 4, ICON = 16, GAP = 3;
// The armour slots, head to feet.
const ARMOUR = [39, 38, 37, 36];
const CORNERS = ["Top Left", "Top Right", "Bottom Left", "Bottom Right"];

// The settings: the corner (index into CORNERS), percent (else uses left), held items shown.
const saved = get("settings");
const settings = {
  corner: saved && saved[0] < CORNERS.length ? saved[0] : 0,
  percent: !!saved && saved[1] === 1,
  held: !saved || saved[2] === 1,
};
function save() {
  try {
    set("settings", new Uint8Array([settings.corner, settings.percent ? 1 : 0, settings.held ? 1 : 0]));
  } catch (_) {
    // A full storage keeps the old settings.
  }
}

// What is left of a stack's durability, 0 to 1; undefined without durability.
function left(stack) {
  return stack.maxDamage > 0 ? Math.max(0, stack.maxDamage - stack.damage) / stack.maxDamage : undefined;
}

// Green at full, yellow at half, red when nearly broken: the durability bar's own colour
// (ItemStack.getBarColor, the hue a third of what is left, full saturation and value).
function colour(fraction) {
  const sector = fraction * 2;
  const [r, g] = sector >= 1 ? [2 - sector, 1] : [1, sector];
  return (Math.round(r * 255) << 16) | (Math.round(g * 255) << 8);
}

function label(stack) {
  const fraction = left(stack);
  // Without durability the icon says it all (its count is drawn on it).
  if (fraction === undefined) return ["", 0xFFFFFF];
  const shown = settings.percent
    ? `${Math.round(fraction * 100)}%`
    : `${stack.maxDamage - stack.damage}`;
  return [shown, colour(fraction)];
}

// The rows: each stack worth showing, head to feet, then the main hand and the offhand.
function rows() {
  const out = [];
  for (const slot of ARMOUR) {
    const stack = getSlot(slot);
    if (stack) out.push(stack);
  }
  if (settings.held) {
    for (const hand of ["main-hand", "off-hand"]) {
      const stack = getHeld(hand);
      if (stack) out.push(stack);
    }
  }
  return out;
}

// The settings screen: its panel and buttons, centred in the GUI.
const SETTINGS = "armor_status:settings";
function layout(context) {
  const x = Math.round(context.width / 2 - 100), y = Math.round(context.height / 2 - 62);
  return {
    panel: [x, y, 200, 124],
    buttons: [
      { rect: [x + 10, y + 24, 180, 20], label: `Corner: ${CORNERS[settings.corner]}`,
        press: () => { settings.corner = (settings.corner + 1) % CORNERS.length; save(); } },
      { rect: [x + 10, y + 48, 180, 20], label: `Durability: ${settings.percent ? "Percent" : "Uses left"}`,
        press: () => { settings.percent = !settings.percent; save(); } },
      { rect: [x + 10, y + 72, 180, 20], label: `Held items: ${settings.held ? "Shown" : "Hidden"}`,
        press: () => { settings.held = !settings.held; save(); } },
      { rect: [x + 10, y + 96, 180, 20], label: "Done", press: () => close() },
    ],
  };
}
const inside = ([x, y, w, h], px, py) => px >= x && px < x + w && py >= y && py < y + h;
let size = { width: 0, height: 0 };

export const events = {
  init() {
    setConfig("settings");
    return {
      commands: [], tick: false, frame: false, chat: false, chatSend: false,
      hud: true, hudElements: [], screens: true, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  hudRender(context) {
    const shown = rows().map((stack) => [stack, ...label(stack)]);
    if (!shown.length) return;
    const widest = Math.max(...shown.map(([, t]) => (t ? textWidth(t) : 0)));
    const width = ICON + GAP + widest;
    const right = settings.corner % 2 === 1, bottom = settings.corner >= 2;
    // Above the hotbar's height at the bottom, so the rows never cover it on a narrow screen.
    const top = bottom ? context.height - MARGIN - 24 - shown.length * ROW : MARGIN;
    const x = right ? context.width - MARGIN - width : MARGIN;
    shown.forEach(([stack, t, c], i) => {
      const y = top + i * ROW;
      // On the right the numbers come first, so the icons line up along the edge.
      const iconX = right ? x + width - ICON : x;
      item(iconX, y, stack);
      if (t) {
        const tx = right ? iconX - GAP - textWidth(t) : x + ICON + GAP;
        text(tx, y + 4, t, c, true);
      }
    });
  },
  screenRender(screen, context) {
    if (screen !== SETTINGS) return;
    size = context;
    const { panel: [x, y, w, h], buttons } = layout(context);
    panel(x, y, w, h);
    const title = "Armor Status";
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
