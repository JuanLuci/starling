// Elytra Swap, as the Elytra Swapper mods do: R (or the elytra on the touch controls) puts on the
// elytra from the inventory in place of the chestplate worn, or the best chestplate in place of
// the elytra, with three clicks on the player's inventory menu (no screen needs to be open: a
// server sees vanilla's clicks). Slot ids are vanilla's menu's: 6 the chest, 9 to 44 the
// inventory and the hotbar.
import { getMenu, click } from "starling:mods/slots@0.1.0";
import { showOverlay } from "starling:mods/chat@0.1.0";
import { addTouchButton } from "starling:mods/widgets@0.1.0";

const CHEST = 6;
const elytra = (s) => s && s.id === "minecraft:elytra";
const chestplate = (s) => s && s.id.endsWith("_chestplate");
// The better chestplate first.
const RANK = ["netherite", "diamond", "iron", "chainmail", "golden", "copper", "leather"];
const rank = (s) => {
  const i = RANK.indexOf(s.id.slice("minecraft:".length, -"_chestplate".length));
  return i < 0 ? RANK.length : i;
};

function swap() {
  const menu = getMenu();
  // Only on the player's own menu (no screen, or the inventory), with nothing on the cursor.
  if (!menu || menu.kind !== "minecraft:inventory" || menu.carried) return;
  const worn = menu.slots[CHEST];
  const wanted = elytra(worn) ? chestplate : elytra;
  let from = -1;
  for (let i = 9; i <= 44; i++) {
    const s = menu.slots[i];
    if (wanted(s) && (from < 0 || (wanted === chestplate && rank(s) < rank(menu.slots[from])))) {
      from = i;
    }
  }
  if (from < 0) {
    showOverlay(wanted === elytra ? "No elytra in your inventory" : "No chestplate in your inventory");
    return;
  }
  // Pick it up, put it on (picking up what was worn), put that where it was.
  click(from, 0, "pickup");
  click(CHEST, 0, "pickup");
  click(from, 0, "pickup");
  showOverlay(wanted === elytra ? "Elytra on" : "Chestplate on");
}

export const events = {
  init() {
    addTouchButton(
      { id: "swap", label: "Elytra", icon: "item/elytra", shape: "square", x: 0.72, y: 0.7, size: 0.42 },
    );
    return {
      commands: [], tick: false, frame: false, chat: false, chatSend: false, hud: false,
      hudElements: [], screens: false, tooltips: false,
      keybinds: [{ id: "swap", name: "Swap Elytra", key: "key.keyboard.r" }], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  key(id, pressed) {
    if (id === "swap" && pressed) swap();
  },
};
