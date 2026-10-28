#!/usr/bin/env python3
"""Starling's low-budget skin, made here: what the menus are drawn with while the game files are
being set up on a phone (the game's own are what is missing). Chalk on graphite paper; buttons
drawn without a ruler and scribbled in with a rough pencil that goes over the edges; our own
handwriting (hand.py), each letter written several times; our own words for the labels
(strings.json). The game picks a take of each letter and of each button by where it stands
(font.rs `glyph_varied`, ui.rs `button_take`), so no two look alike.

    python3 packaging/lowbudget/make.py      # writes packaging/lowbudget/pack (needs Pillow)
"""
import json
import math
import random
import shutil
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

import hand

HERE = Path(__file__).resolve().parent
OUT = HERE / "pack"
# Texture pixels to a GUI unit: sharp on a phone at any GUI scale.
K = 4
PAPER = (40, 41, 46)
LEAD = (206, 206, 212)
CHALK = (178, 178, 186)
WHITE = (232, 232, 236)
# Takes of each button and of each letter.
BUTTON_TAKES = 4
LETTER_TAKES = 5
# The writing on a pressed button (its chalk is whiter).
PRESSED_LABEL = "202126"


def pencil(draw, a, b, rnd, alpha=210, width=0.65, passes=2, color=LEAD):
    """A line from a to b gone over `passes` times, each a little off, past the ends or short."""
    for p in range(passes):
        ja = (a[0] + rnd.gauss(0, 0.5) * K, a[1] + rnd.gauss(0, 0.35) * K)
        jb = (b[0] + rnd.gauss(0, 0.5) * K, b[1] + rnd.gauss(0, 0.35) * K)
        dx, dy = jb[0] - ja[0], jb[1] - ja[1]
        length = math.hypot(dx, dy) or 1
        ux, uy = dx / length, dy / length
        ja = (ja[0] - ux * rnd.uniform(-0.3, 1.2) * K, ja[1] - uy * rnd.uniform(-0.3, 1.2) * K)
        jb = (jb[0] + ux * rnd.uniform(-0.3, 1.4) * K, jb[1] + uy * rnd.uniform(-0.3, 1.4) * K)
        bow = rnd.gauss(0, 0.3) * K
        n = max(4, int(length / (10 * K)))
        pts = []
        for i in range(n + 1):
            t = i / n
            off = math.sin(t * math.pi) * bow + rnd.gauss(0, 0.12) * K
            pts.append((ja[0] + dx * t - uy * off, ja[1] + dy * t + ux * off))
        a_ = int(alpha * (1.0 if p == 0 else rnd.uniform(0.45, 0.75)))
        draw.line(pts, fill=color + (a_,), width=max(1, round(width * K * rnd.uniform(0.85, 1.1))), joint="curve")


def corners(x, y, w, h, rnd, tilt=1.0, turn=0.0):
    """A box drawn without a ruler, turned `turn` degrees about its middle."""
    j = lambda s: rnd.gauss(0, s) * K
    pts = [(x + j(0.6 * tilt), y + j(0.45 * tilt)), (x + w + j(0.6 * tilt), y + j(0.45 * tilt)),
           (x + w + j(0.6 * tilt), y + h + j(0.45 * tilt)), (x + j(0.6 * tilt), y + h + j(0.45 * tilt))]
    if turn:
        a = math.radians(turn)
        cx, cy = x + w / 2, y + h / 2
        pts = [(cx + (px - cx) * math.cos(a) - (py - cy) * math.sin(a),
                cy + (px - cx) * math.sin(a) + (py - cy) * math.cos(a)) for px, py in pts]
    return pts


def tooth(size, seed, pressed=False):
    """The paper's grain: where a rough pencil leaves chalk and where it skips, in clumps."""
    random.seed(seed)
    w, h = size
    fine = Image.effect_noise((max(1, w // 2), max(1, h // 2)), 70).convert("L").resize((w, h), Image.BILINEAR)
    coarse = Image.effect_noise((max(1, w // 6), max(1, h // 6)), 50).convert("L").resize((w, h), Image.BILINEAR)
    grain = ImageChops.add(fine.point(lambda v: v // 2), coarse.point(lambda v: v // 2))
    if pressed:
        # Pressed harder, the chalk fills more of the tooth.
        return grain.point(lambda v: 255 if v >= 138 else 215 if v >= 112 else 150)
    return grain.point(lambda v: 255 if v > 138 else (150 if v > 124 else (60 if v > 112 else 0)))


def scribble(mask, x, y, w, h, rnd, strength=1.0, passes=2):
    """A quick back-and-forth fill over the box, past its edges, not all the way across."""
    d = ImageDraw.Draw(mask)
    leans = [rnd.uniform(-0.25, 0.25), rnd.uniform(0.35, 0.7), rnd.uniform(-0.7, -0.35)]
    for k in range(passes):
        lean, weight = leans[k % 3], 1.0 if k == 0 else 0.6
        start = x + w * rnd.uniform(0.0, 0.05) - 1.2 * K
        end = x + w * rnd.uniform(0.88, 1.0) + 1.2 * K
        over = rnd.uniform(0.6, 1.6) * K
        pts = []
        px = start
        up = rnd.random() < 0.5
        while px < end:
            yy = (y - over * rnd.uniform(0.0, 1.3)) if up else (y + h + over * rnd.uniform(0.0, 1.3))
            pts.append((px + (yy - y - h / 2) * lean + rnd.gauss(0, 0.5) * K, yy))
            px += rnd.uniform(0.9, 2.0) * K
            up = not up
        for a, b in zip(pts, pts[1:]):
            if rnd.random() < 0.06:
                continue
            value = int(255 * strength * weight * rnd.uniform(0.5, 1.0))
            mid = ((a[0] + b[0]) / 2 + rnd.gauss(0, 0.6) * K, (a[1] + b[1]) / 2)
            d.line([a, mid, b], fill=value, width=round(rnd.uniform(1.8, 2.8) * K), joint="curve")


def widget(units, seed, state="normal", fill=1.0, inset=(2.6, 2.4)):
    """A widget's texture `units` GUI units large: a scribbled box, as `state` draws it."""
    rnd = random.Random(seed)
    w, h = units[0] * K, units[1] * K
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    x0, y0 = inset[0] * K, inset[1] * K
    bw, bh = w - 2 * x0, h - 2 * y0
    mask = Image.new("L", (w, h), 0)
    pressed = state == "pressed"
    scribble(mask, x0, y0, bw, bh, rnd, strength=1.0, passes=3 if pressed else 2)
    grain = tooth((w, h), seed, pressed)
    level = {"normal": fill, "pressed": 0.93, "disabled": fill * 0.4}[state]
    chalk = WHITE if pressed else CHALK
    alpha = ImageChops.multiply(mask, grain).point(lambda v: int(v * level))
    img = Image.composite(Image.new("RGBA", (w, h), chalk + (255,)), img, alpha)
    draw = ImageDraw.Draw(img, "RGBA")
    turn = rnd.choice((-1, 1)) * rnd.uniform(0.9, 1.4) if pressed else 0.0
    box = corners(x0, y0, bw, bh, rnd, tilt=2.2 if pressed else 1.0, turn=turn)
    line_alpha = 110 if state == "disabled" else 210
    for i in range(4):
        pencil(draw, box[i], box[(i + 1) % 4], rnd, alpha=line_alpha, width=0.8 if pressed else 0.65)
    return img


def nine_slice(path, units, border):
    path.with_name(path.name + ".mcmeta").write_text(json.dumps({"gui": {"scaling": {
        "type": "nine_slice", "width": units[0], "height": units[1],
        "border": {"left": border[0], "top": border[1], "right": border[0], "bottom": border[1]},
        "stretch_inner": True}}}, indent=2) + "\n", encoding="utf-8")


def save(img, rel):
    path = OUT / "assets" / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, optimize=True)
    return path


def paper(size, seed):
    """Graphite paper, seamless when tiled: a grain mirrored into a square four times its size."""
    random.seed(seed)
    half = size // 2
    noise = Image.effect_noise((half, half), 18).convert("L").filter(ImageFilter.GaussianBlur(0.6))
    tile = Image.new("L", (size, size))
    tile.paste(noise, (0, 0))
    tile.paste(noise.transpose(Image.FLIP_LEFT_RIGHT), (half, 0))
    tile.paste(noise.transpose(Image.FLIP_TOP_BOTTOM), (0, half))
    tile.paste(noise.transpose(Image.ROTATE_180), (half, half))
    base = Image.new("RGB", (size, size), PAPER)
    light = Image.new("RGB", (size, size), tuple(c + 7 for c in PAPER))
    return Image.composite(light, base, tile.point(lambda v: 255 if v > 140 else 0)).convert("RGBA")


def separator(seed):
    """A chalk line 32 GUI units long, the same height at both ends so it tiles."""
    rnd = random.Random(seed)
    w, h = 32 * K, 2 * K
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img, "RGBA")
    pts = [(x, h / 2 + math.sin(x / w * math.tau) * 0.3 * K + rnd.gauss(0, 0.08) * K) for x in range(0, w + 1, 4)]
    d.line(pts, fill=LEAD + (190,), width=max(1, round(0.55 * K)))
    return img


def letters(take):
    """One take of every ASCII letter: a sheet of 16 by 6 cells, 8 by 10 GUI units each, white."""
    px = 6  # sheet pixels to a GUI unit
    cw, ch = 8 * px, 10 * px
    chars = [chr(c) for c in range(32, 128)]
    rows = ["".join(chars[i:i + 16]).replace("\x7f", "\u0000") for i in range(0, 96, 16)]
    sheet = Image.new("RGBA", (16 * cw, 6 * ch), (0, 0, 0, 0))
    for i, c in enumerate(chars):
        if c in (" ", "\x7f"):
            continue
        # Each letter in a cell of its own, cut at its edges: a stroke that strays out of it
        # would widen the next letter (the game takes a letter's width from its last column).
        cell = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
        draw = ImageDraw.Draw(cell, "RGBA")
        seed = take * 1000 + ord(c)
        hand.write(draw, c, 0.6 * px, 8 * px, 7.2 * px, ink=(255, 255, 255, 255), seed=seed,
                   width=0.78 * px, slant=(2.5, 2.0))
        sheet.alpha_composite(cell, ((i % 16) * cw, (i // 16) * ch))
    return sheet, rows


def main():
    if OUT.exists():
        shutil.rmtree(OUT)
    (OUT / "assets").mkdir(parents=True)
    (OUT / "pack.mcmeta").write_text(json.dumps({"pack": {
        "pack_format": 97, "description": "Starling's low-budget skin"}}, indent=2) + "\n", encoding="utf-8")
    sprites = "minecraft/textures/gui/sprites/widget/"
    # Buttons: a take of each state per take number; the plain names are take 0.
    for take in range(BUTTON_TAKES):
        suffix = "" if take == 0 else f"_{take}"
        for state, name in (("normal", "button"), ("pressed", "button_highlighted"), ("disabled", "button_disabled")):
            p = save(widget((200, 20), 100 + take * 10 + len(name), state), f"{sprites}{name}{suffix}.png")
            nine_slice(p, (200, 20), (6, 5))
    for state, name in (("normal", "slider"), ("pressed", "slider_highlighted")):
        p = save(widget((200, 20), 300 + len(name), state, fill=0.55), f"{sprites}{name}.png")
        nine_slice(p, (200, 20), (6, 5))
    for state, name in (("normal", "slider_handle"), ("pressed", "slider_handle_highlighted")):
        p = save(widget((8, 20), 400 + len(name), state, fill=0.9, inset=(1.2, 1.6)), f"{sprites}{name}.png")
        nine_slice(p, (8, 20), (2, 5))
    for state, name in (("normal", "text_field"), ("pressed", "text_field_highlighted")):
        p = save(widget((200, 20), 500 + len(name), state, fill=0.35), f"{sprites}{name}.png")
        nine_slice(p, (200, 20), (6, 5))
    for state, name in (("normal", "scroller"), ("pressed", "scroller_background")):
        p = save(widget((6, 32), 600 + len(name), "normal", fill=0.9 if name == "scroller" else 0.25,
                        inset=(0.8, 1.2)), f"{sprites}{name}.png")
        nine_slice(p, (6, 32), (2, 4))
    for name in ("menu_background", "menu_list_background"):
        save(paper(256, 700 + len(name)), f"minecraft/textures/gui/{name}.png")
    for i, name in enumerate(("header_separator", "footer_separator")):
        save(separator(800 + i), f"minecraft/textures/gui/{name}.png")
    # The letters: take 0 is the default font, the others fonts the game picks among.
    takes = []
    for take in range(LETTER_TAKES):
        sheet, rows = letters(take)
        save(sheet, f"minecraft/textures/font/hand_{take}.png")
        provider = {"type": "bitmap", "file": f"minecraft:font/hand_{take}.png", "height": 10, "ascent": 8,
                    "chars": rows}
        if take == 0:
            font = {"providers": [{"type": "space", "advances": {" ": 4}}, provider]}
        else:
            font = {"providers": [{"type": "space", "advances": {" ": 4}}, provider]}
            path = OUT / "assets" / "starling" / "font" / f"hand_{take}.json"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(font, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
            takes.append(f"starling:hand_{take}")
    default = {"providers": [{"type": "space", "advances": {" ": 4}},
                             {"type": "bitmap", "file": "minecraft:font/hand_0.png", "height": 10, "ascent": 8,
                              "chars": letters(0)[1]}],
               "starling:variants": takes}
    path = OUT / "assets" / "minecraft" / "font" / "default.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(default, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    strings = json.loads((HERE / "strings.json").read_text(encoding="utf-8"))
    path = OUT / "assets" / "minecraft" / "lang" / "en_us.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(strings, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    path = OUT / "assets" / "starling" / "skin.json"
    path.write_text(json.dumps({"button_takes": BUTTON_TAKES, "pressed_label": PRESSED_LABEL}, indent=1) + "\n",
                    encoding="utf-8")
    print(OUT)


if __name__ == "__main__":
    main()
