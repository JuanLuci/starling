// Peek, in the spirit of Jade and WTHIT: what the crosshair is on, in a tooltip at the top of
// the screen. A block shows its item, its name, a crop's growth, the tool it takes (green when
// breaking it with what you hold makes it drop) and the breaking progress; a creature its name,
// hearts and armor. The last line says whose it is: Minecraft, or a content mod's namespace.
import { getTarget, getMining } from "starling:mods/game@0.1.0";
import { getBlockInfo, getEntityInfo } from "starling:mods/level@0.1.0";
import { fill, gradient, text, textWidth, itemId, sprite } from "starling:mods/draw@0.1.0";

// The age each crop is ripe at.
const RIPE = {
  "minecraft:wheat": 7, "minecraft:carrots": 7, "minecraft:potatoes": 7, "minecraft:beetroots": 3,
  "minecraft:nether_wart": 3, "minecraft:cocoa": 2, "minecraft:sweet_berry_bush": 3,
  "minecraft:torchflower_crop": 1, "minecraft:pitcher_crop": 4, "minecraft:melon_stem": 7,
  "minecraft:pumpkin_stem": 7,
};
const WHITE = 0xFFFFFF, GRAY = 0xAAAAAA, GREEN = 0x55FF55, RED = 0xFF5555;
// Hearts and armor points a row.
const PER_ROW = 10;

const titled = (words) => words.split(/[_ ]/).map((w) => w && w[0].toUpperCase() + w.slice(1)).join(" ");
// Whose a block or creature is: `minecraft:stone` is Minecraft's.
const origin = (id) => ({ text: `§9§o${titled(id.split(":")[0])}`, color: WHITE });
const number = (n) => (Number.isInteger(n) ? `${n}` : n.toFixed(1));

// A tooltip for the block at `hit`: its item and lines; none for air.
function block(hit) {
  const info = getBlockInfo(hit.pos);
  if (!info || /^minecraft:(cave_|void_)?air\b/.test(info.block)) return null;
  const id = info.block.split("[")[0];
  const lines = [{ text: info.name, color: WHITE }];
  const age = /[[,]age=(\d+)/.exec(info.block);
  if (age && RIPE[id]) {
    const growth = Math.min(100, Math.round((100 * Number(age[1])) / RIPE[id]));
    lines.push(growth === 100 ? { text: "Mature", color: GREEN } : { text: `Growth: ${growth}%`, color: GRAY });
  }
  if (info.hardness < 0) {
    lines.push({ text: "Unbreakable", color: GRAY });
  } else if (info.tool) {
    lines.push({ text: `Tool: ${titled(info.tool)}`, color: info.harvestable ? GREEN : RED });
  }
  const mining = getMining();
  if (mining && ["x", "y", "z"].every((a) => mining.pos[a] === hit.pos[a])) {
    lines.push({ bar: mining.progress });
  }
  lines.push(origin(id));
  return { icon: info.item, lines };
}

// A tooltip for the creature `hit` is.
function creature(hit) {
  const info = getEntityInfo(hit.id);
  const lines = [{ text: info ? info.name : hit.kind, color: WHITE }];
  if (info && info.baby) lines.push({ text: "Baby", color: GRAY });
  if (info && info.health !== undefined && info.maxHealth) {
    lines.push(info.maxHealth <= 2 * PER_ROW * 2
      ? { hearts: info.health, max: info.maxHealth }
      : { text: `Health: ${number(info.health)} / ${number(info.maxHealth)}`, color: RED });
  }
  if (info && info.armor > 0) lines.push({ armor: info.armor });
  lines.push(origin(hit.kind));
  return { icon: undefined, lines };
}

// Rows of icons `points` take, two points an icon.
const rows = (points) => Math.max(1, Math.ceil(Math.ceil(points / 2) / PER_ROW));
function size(line) {
  if (line.bar !== undefined) return [60, 4];
  if (line.hearts !== undefined) return [Math.min(PER_ROW, Math.ceil(line.max / 2)) * 8 + 1, rows(line.max) * 10];
  if (line.armor !== undefined) return [Math.min(PER_ROW, Math.ceil(line.armor / 2)) * 8 + 1, rows(line.armor) * 10];
  return [textWidth(line.text) - 1, 10];
}

// Two points an icon: full, half or none over `under` (if any), in rows.
function icons(x, y, points, max, full, half, under) {
  for (let i = 0; i < Math.ceil(max / 2); i++) {
    const ix = x + (i % PER_ROW) * 8, iy = y + Math.floor(i / PER_ROW) * 10;
    if (under) sprite(ix, iy, 9, 9, under);
    if (points >= 2 * i + 2) sprite(ix, iy, 9, 9, full);
    else if (points >= 2 * i + 1) sprite(ix, iy, 9, 9, half);
  }
}

function draw(line, x, y, width) {
  if (line.bar !== undefined) {
    fill(x, y, width, 3, 0xFF404040);
    fill(x, y, Math.round(width * Math.min(1, line.bar)), 3, 0xFFFFFFFF);
  } else if (line.hearts !== undefined) {
    icons(x, y, line.hearts, line.max, "hud/heart/full", "hud/heart/half", "hud/heart/container");
  } else if (line.armor !== undefined) {
    icons(x, y, line.armor, line.armor, "hud/armor_full", "hud/armor_half");
  } else {
    text(x, y, line.text, line.color, true);
  }
}

// Vanilla's tooltip box around a `w` by `h` area.
function box(x, y, w, h) {
  const back = 0xF0100010, top = 0x505000FF, bottom = 0x5028007F;
  fill(x - 3, y - 4, w + 6, 1, back);
  fill(x - 3, y + h + 3, w + 6, 1, back);
  fill(x - 3, y - 3, w + 6, h + 6, back);
  fill(x - 4, y - 3, 1, h + 6, back);
  fill(x + w + 3, y - 3, 1, h + 6, back);
  gradient(x - 3, y - 2, 1, h + 4, top, bottom);
  gradient(x + w + 2, y - 2, 1, h + 4, top, bottom);
  fill(x - 3, y - 3, w + 6, 1, top);
  fill(x - 3, y + h + 2, w + 6, 1, bottom);
}

export const events = {
  init() {
    return {
      commands: [], tick: false, frame: false, chat: false, chatSend: false,
      hud: true, hudElements: [], screens: false, tooltips: false, keybinds: [], channels: [],
      connection: false, interaction: false, damage: false,
    };
  },
  hudRender(context) {
    const target = getTarget();
    const shown = target.tag === "block" ? block(target.val)
      : target.tag === "entity" ? creature(target.val) : null;
    if (!shown) return;
    const sizes = shown.lines.map(size);
    const linesWidth = Math.max(...sizes.map(([w]) => w));
    const linesHeight = sizes.reduce((sum, [, h]) => sum + h, 0) - 2;
    const iconWidth = shown.icon ? 20 : 0;
    const width = iconWidth + linesWidth, height = Math.max(shown.icon ? 16 : 0, linesHeight);
    const x = Math.round((context.width - width) / 2), y = 6;
    box(x, y, width, height);
    if (shown.icon) itemId(x, y + Math.round((height - 16) / 2), shown.icon);
    let lineY = y;
    shown.lines.forEach((line, i) => {
      draw(line, x + iconWidth, lineY, linesWidth);
      lineY += sizes[i][1];
    });
  },
};
