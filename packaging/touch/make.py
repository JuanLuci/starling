"""Starling's touch buttons in a Bedrock-like look, drawn from scratch: a 20x20 grey tile with a
bevel (light top and left, dark bottom and right) and a white pixel icon; pressed, the bevel turns
over and the icon sinks a pixel. The sprites go to crates/horizon-client/assets/touch, which the
game carries as `starling:touch/<name>` (hud.rs `TOUCH_SPRITES`; a resource pack's own win).

    python packaging/touch/make.py                     # the sprites
    python packaging/touch/make.py SCREENSHOT OUTDIR   # and a sheet and a preview, to look at
"""
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "horizon-client" / "assets" / "touch"
T = 20  # tile size

# Palette (RGBA): outline, highlight, fill, shadow; pressed fill; icon and its shadow.
OUTLINE = (24, 24, 24, 235)
LIGHT = (200, 200, 200, 210)
FILL = (128, 128, 128, 170)
SHADE = (78, 78, 78, 210)
FILL_DOWN = (104, 104, 104, 190)
ICON = (255, 255, 255, 255)
ICON_SHADOW = (40, 40, 40, 200)

ICONS = {
    "jump": """
        .......##.......
        ......####......
        .....######.....
        ....########....
        ...##########...
        ..############..
        .......##.......
        .......##.......
        .......##.......
        .......##.......
        ................
        ..############..
        ..############..
    """,
    "sneak": """
        ..############..
        ..############..
        ................
        .......##.......
        .......##.......
        .......##.......
        .......##.......
        ..############..
        ...##########...
        ....########....
        .....######.....
        ......####......
        .......##.......
    """,
    "attack": """
        ...........###
        ..........####
        .........####.
        ........####..
        .......####...
        ......####....
        .#...####.....
        .##.####......
        ..#####.......
        ...###........
        ..##.##.......
        .##...##......
        ##............
        #.............
    """,
    "use": """
        .....####.....
        ...##....##...
        ..#........#..
        .#....##....#.
        .#....##....#.
        #...######...#
        #...######...#
        .#....##....#.
        .#....##....#.
        ..#........#..
        ...##....##...
        .....####.....
    """,
    "inventory": """
        ................
        ................
        ................
        .##....##....##.
        .##....##....##.
        ................
    """,
    "chat": """
        .############.
        #............#
        #..########..#
        #............#
        #..######....#
        #............#
        .####..######.
        ....#.#.......
        ....##........
        ....#.........
    """,
    "pause": """
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
        ...###..###...
    """,
    "drop": """
        ..########....
        ..#......#....
        ..#..##..#....
        ..#..##..#....
        ..########....
        ......##......
        ......##......
        ......##......
        ...########...
        ....######....
        .....####.....
        ......##......
    """,
    "offhand": """
        .....#........
        ....##........
        ...##########.
        ....##.......#
        .....#.......#
        .#.......#....
        .#.......##...
        #..........##.
        .##########...
        ........##....
        ........#.....
    """,
    "camera": """
        ....######....
        ..##......##..
        .#....##....#.
        #....####....#
        #....####....#
        .#....##....#.
        ..##......##..
        ....######....
    """,
    "command": """
        ..........##
        .........##.
        ........##..
        .......##...
        ......##....
        .....##.....
        ....##......
        ...##.......
        ..##........
        .##.........
    """,
    "up": """
        .......##.......
        ......####......
        .....######.....
        ....########....
        ...##########...
        ..############..
        .##############.
    """,
}


def icon_bitmap(name):
    rows = [r.strip() for r in ICONS[name].strip("\n").splitlines() if r.strip()]
    width = max(len(r) for r in rows)
    return [r.ljust(width, ".") for r in rows]


def tile(name, pressed):
    img = Image.new("RGBA", (T, T), (0, 0, 0, 0))
    px = img.load()
    for y in range(T):
        for x in range(T):
            edge = x in (0, T - 1) or y in (0, T - 1)
            corner = x in (0, T - 1) and y in (0, T - 1)
            if corner:
                continue
            if edge:
                px[x, y] = OUTLINE
            elif x == 1 or y == 1:
                px[x, y] = SHADE if pressed else LIGHT
            elif x == T - 2 or y == T - 2:
                px[x, y] = LIGHT if pressed else SHADE
            else:
                px[x, y] = FILL_DOWN if pressed else FILL
    if name:
        rows = icon_bitmap(name)
        h, w = len(rows), len(rows[0])
        ox, oy = (T - w) // 2, (T - h) // 2 + (1 if pressed else 0)
        for dy, row in enumerate(rows):
            for dx, ch in enumerate(row):
                if ch == "#":
                    sx, sy = ox + dx + 1, oy + dy + 1
                    if 1 < sx < T - 2 and 1 < sy < T - 2 and px[sx, sy] != ICON:
                        px[sx, sy] = ICON_SHADOW
        for dy, row in enumerate(rows):
            for dx, ch in enumerate(row):
                if ch == "#":
                    px[ox + dx, oy + dy] = ICON
    return img


def dpad(direction, pressed):
    """A D-pad cell: the arrow turned to its direction."""
    img = tile("up", pressed)
    turn = {"up": None, "down": Image.Transpose.ROTATE_180, "left": Image.Transpose.ROTATE_90,
            "right": Image.Transpose.ROTATE_270}[direction]
    if turn is None:
        return img
    base = tile(None, pressed)
    arrow = tile("up", pressed)
    # Turn only the icon, keep the bevel's light where it is.
    icon_only = Image.new("RGBA", (T, T))
    for y in range(T):
        for x in range(T):
            if arrow.getpixel((x, y)) in (ICON, ICON_SHADOW):
                icon_only.putpixel((x, y), arrow.getpixel((x, y)))
    base.alpha_composite(icon_only.transpose(turn))
    return base


def stick(size, fill=(96, 96, 96, 110)):
    """The movement stick's base: a ring of pixels, lit at the top left, around a dimmed plate
    (with `fill` as the plate's, the knob)."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    c = (size - 1) / 2
    for y in range(size):
        for x in range(size):
            d = ((x - c) ** 2 + (y - c) ** 2) ** 0.5
            r = size / 2
            if r - 1 <= d < r:
                img.putpixel((x, y), OUTLINE)
            elif r - 2 <= d < r - 1:
                img.putpixel((x, y), LIGHT if (x + y) < size else SHADE)
            elif d < r - 2:
                img.putpixel((x, y), fill)
    return img


def build():
    OUT.mkdir(parents=True, exist_ok=True)
    sprites = {}
    for name in ["jump", "sneak", "attack", "use", "inventory", "chat", "pause", "drop", "offhand",
                 "camera", "command"]:
        sprites[name] = tile(name, False)
        sprites[name + "_pressed"] = tile(name, True)
    for d in ["up", "down", "left", "right"]:
        sprites["dpad_" + d] = dpad(d, False)
        sprites["dpad_" + d + "_pressed"] = dpad(d, True)
    sprites["dpad_middle"] = tile(None, False)
    sprites["stick"] = stick(40)
    sprites["stick_knob"] = stick(18, FILL)
    for name, img in sprites.items():
        img.save(OUT / f"{name}.png")
    return sprites


def preview(sprites, background, out):
    bg = Image.open(background).convert("RGBA")
    W, H = bg.size
    s = H / 200  # one sprite pixel in screen pixels, as a phone's GUI scale draws it
    def put(img, cx, cy, scale=1.0):
        k = s * scale
        big = img.resize((max(1, round(img.width * k)), max(1, round(img.height * k))),
                         Image.Resampling.NEAREST)
        bg.alpha_composite(big, (round(cx - big.width / 2), round(cy - big.height / 2)))
    u = 22 * s  # one tile with its gap
    # D-pad, bottom left.
    cx, cy = 3.3 * u, H - 3.2 * u
    put(sprites["dpad_up"], cx, cy - u)
    put(sprites["dpad_down_pressed"], cx, cy + u)
    put(sprites["dpad_left"], cx - u, cy)
    put(sprites["dpad_right"], cx + u, cy)
    put(sprites["sneak"], cx, cy)
    # Right side: attack, use, jump, sneak.
    rx = W - 2.2 * u
    put(sprites["jump"], rx, H - 2.0 * u, 1.3)
    put(sprites["attack_pressed"], rx - 1.6 * u, H - 3.4 * u, 1.3)
    put(sprites["use"], rx, H - 4.2 * u, 1.3)
    put(sprites["drop"], rx - 2.9 * u, H - 1.3 * u)
    put(sprites["offhand"], rx - 3.9 * u, H - 1.3 * u)
    # Top row.
    for i, name in enumerate(["pause", "chat", "command", "camera", "inventory"]):
        put(sprites[name], W / 2 - 2 * u + i * u, 1.0 * u)
    # The stick, beside, for the other movement mode.
    put(sprites["stick"], 8.0 * u, H - 2.8 * u, 1.6)
    put(sprites["stick_knob"], 8.3 * u, H - 3.1 * u, 1.0)
    bg.convert("RGB").save(out, quality=92)


def sheet(sprites, out):
    names = list(sprites)
    cols = 8
    cell = T * 6 + 12
    rows = (len(names) + cols - 1) // cols
    img = Image.new("RGBA", (cols * cell, rows * cell), (60, 110, 60, 255))
    for i, name in enumerate(names):
        sp = sprites[name]
        big = sp.resize((sp.width * 6 if sp.width <= T else sp.width * 3,) * 2, Image.Resampling.NEAREST)
        img.alpha_composite(big, ((i % cols) * cell + 6, (i // cols) * cell + 6))
    img.convert("RGB").save(out)


if __name__ == "__main__":
    sprites = build()
    if len(sys.argv) > 2:
        look = Path(sys.argv[2])
        look.mkdir(parents=True, exist_ok=True)
        sheet(sprites, look / "sheet.png")
        preview(sprites, sys.argv[1], look / "preview.jpg")
    print("ok", len(sprites))
