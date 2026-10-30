//! A mod's HUD and screens drawn to a PNG without the game: its draw calls on a canvas the GUI's
//! size times its scale, with the font and sprites of the player's own prepared resources
//! (`--resources`, the folder Starling reads), else plain shapes. Nothing of the game is bundled.
use horizon_mods::{DrawOp, Measure};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

/// An RGBA image.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

impl Image {
    pub(crate) fn new(width: u32, height: u32, fill: [u8; 4]) -> Self {
        let pixels = fill.repeat((width * height) as usize);
        Image {
            width,
            height,
            pixels,
        }
    }
    pub(crate) fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].try_into().expect("four bytes")
    }
    /// `color` over the pixel at `x`, `y` (alpha blending), when inside.
    pub(crate) fn blend(&mut self, x: i64, y: i64, color: [u8; 4]) {
        if x < 0
            || y < 0
            || x >= i64::from(self.width)
            || y >= i64::from(self.height)
            || color[3] == 0
        {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let a = u32::from(color[3]);
        for c in 0..3 {
            let under = u32::from(self.pixels[i + c]);
            self.pixels[i + c] = ((u32::from(color[c]) * a + under * (255 - a)) / 255) as u8;
        }
        self.pixels[i + 3] = 255;
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&self.pixels)
            .map_err(|e| e.to_string())
    }
}

fn read_png(path: &Path) -> Option<Image> {
    png_image(std::io::BufReader::new(std::fs::File::open(path).ok()?))
}

fn png_image(source: impl std::io::BufRead + std::io::Seek) -> Option<Image> {
    let mut decoder = png::Decoder::new(source);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf[..(w * h * 4) as usize].to_vec(),
        png::ColorType::Rgb => buf[..(w * h * 3) as usize]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..(w * h * 2) as usize]
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..(w * h) as usize]
            .iter()
            .flat_map(|&g| [g, g, g, 255])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Image {
        width: w,
        height: h,
        pixels,
    })
}

/// A sprite's nine-slice border, from its `.mcmeta`.
fn nine_slice(png: &Path) -> Option<u32> {
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(png.with_extension("png.mcmeta")).ok()?)
            .ok()?;
    let scaling = &meta["gui"]["scaling"];
    (scaling["type"] == "nine_slice").then(|| scaling["border"].as_u64().unwrap_or(0) as u32)
}

/// The player's prepared resources: the font and the textures, loaded as asked for.
pub struct Resources {
    root: Option<PathBuf>,
    /// The mod's folder or `.hmod`: its own textures (its GUI sprites), as the game loads them.
    own: Option<PathBuf>,
    glyphs: Option<Rc<Image>>,
    textures: HashMap<String, Option<Rc<(Image, Option<u32>)>>>,
}

impl Resources {
    /// `root` is a prepared resources folder (holding `assets/minecraft`), or none.
    pub fn new(root: Option<PathBuf>) -> Self {
        let glyphs = root
            .as_ref()
            .and_then(|r| read_png(&r.join("assets/minecraft/textures/font/ascii.png")))
            .map(Rc::new);
        Resources {
            root,
            own: None,
            glyphs,
            textures: HashMap::new(),
        }
    }
    /// The mod at `path` (a folder or a `.hmod`) brings its own textures.
    pub fn with_mod(mut self, path: &Path) -> Self {
        self.own = Some(path.to_owned());
        self
    }
    pub fn has_font(&self) -> bool {
        self.glyphs.is_some()
    }
    /// Text widths in the font, as the game measures them for mods; none without the font.
    pub fn measure(&self) -> Option<Measure> {
        self.glyphs.as_ref()?;
        let advances: Vec<f32> = (0..128u8)
            .map(|c| self.glyph_width(c as char) as f32 + 1.0)
            .collect();
        Some(Arc::new(move |text: &str| {
            spans(text, 0)
                .iter()
                .map(|&(c, _)| advances.get(c as usize).copied().unwrap_or(6.0))
                .sum()
        }))
    }
    /// `assets/<ns>/textures/<path>.png` and its nine-slice border: the resources', else the
    /// mod's own.
    pub(crate) fn texture(&mut self, id: &str) -> Option<Rc<(Image, Option<u32>)>> {
        let (root, own) = (self.root.clone(), self.own.clone());
        self.textures
            .entry(id.to_owned())
            .or_insert_with(|| {
                let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
                let name = format!("assets/{ns}/textures/{path}.png");
                if let Some(root) = root {
                    let file = root.join(&name);
                    if let Some(image) = read_png(&file) {
                        return Some(Rc::new((image, nine_slice(&file))));
                    }
                }
                let own = own.filter(|_| ns != "minecraft")?;
                if own.is_dir() {
                    return read_png(&own.join(&name)).map(|image| Rc::new((image, None)));
                }
                let mut zip = zip::ZipArchive::new(std::fs::File::open(&own).ok()?).ok()?;
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut zip.by_name(&name).ok()?, &mut bytes).ok()?;
                png_image(std::io::Cursor::new(bytes)).map(|image| Rc::new((image, None)))
            })
            .clone()
    }
    /// An ascii.png glyph's width (its last coloured column), the space 3.
    fn glyph_width(&self, c: char) -> u32 {
        let Some(font) = &self.glyphs else {
            return 5;
        };
        let code = if c.is_ascii() {
            c as u32
        } else {
            u32::from(b'?')
        };
        if code == u32::from(b' ') {
            return 3;
        }
        let cell = font.width / 16;
        let (cx, cy) = ((code % 16) * cell, (code / 16) * cell);
        (0..cell)
            .rev()
            .find(|&x| (0..cell).any(|y| font.get(cx + x, cy + y)[3] > 0))
            .map_or(0, |x| (x + 1) * 8 / cell)
    }
}

/// The 16 chat colours, by `§` code.
const COLOURS: [u32; 16] = [
    0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xFFAA00, 0xAAAAAA, 0x555555,
    0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
];

/// A rich text's characters with their colours: a JSON text component, or text with `§`
/// codes; `colour` where nothing sets one.
fn spans(text: &str, colour: u32) -> Vec<(char, u32)> {
    let mut out = Vec::new();
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(json) if json.is_object() || json.is_array() => {
            component(&json, colour, &mut out);
            return out;
        }
        _ => {}
    }
    let mut current = colour;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            // A colour, or `r` back to none; bold, italic and the like keep the colour.
            match chars.next().map(|code| code.to_ascii_lowercase()) {
                Some('r') => current = colour,
                Some(code) => {
                    if let Some(i) = code.to_digit(16) {
                        current = COLOURS[i as usize];
                    }
                }
                None => {}
            }
            continue;
        }
        out.push((c, current));
    }
    out
}

fn component(json: &serde_json::Value, colour: u32, out: &mut Vec<(char, u32)>) {
    match json {
        serde_json::Value::String(s) => out.extend(spans(s, colour)),
        serde_json::Value::Array(parts) => parts.iter().for_each(|p| component(p, colour, out)),
        serde_json::Value::Object(o) => {
            let colour = o
                .get("color")
                .and_then(|c| c.as_str())
                .map_or(colour, |name| {
                    const NAMES: [&str; 16] = [
                        "black",
                        "dark_blue",
                        "dark_green",
                        "dark_aqua",
                        "dark_red",
                        "dark_purple",
                        "gold",
                        "gray",
                        "dark_gray",
                        "blue",
                        "green",
                        "aqua",
                        "red",
                        "light_purple",
                        "yellow",
                        "white",
                    ];
                    NAMES
                        .iter()
                        .position(|n| *n == name)
                        .map(|i| COLOURS[i])
                        .or_else(|| u32::from_str_radix(name.trim_start_matches('#'), 16).ok())
                        .unwrap_or(colour)
                });
            if let Some(text) = o.get("text").and_then(|t| t.as_str()) {
                out.extend(spans(text, colour));
            } else if let Some(key) = o.get("translate").and_then(|t| t.as_str()) {
                out.extend(key.chars().map(|c| (c, colour)));
            }
            if let Some(extra) = o.get("extra") {
                component(extra, colour, out);
            }
        }
        _ => {}
    }
}

/// Drawing at a pose: GUI pixels to image pixels.
struct Canvas<'a> {
    image: Image,
    res: &'a mut Resources,
    poses: Vec<(f32, f32, f32)>,
}

impl Canvas<'_> {
    fn pose(&self) -> (f32, f32, f32) {
        *self.poses.last().expect("a pose")
    }
    /// A rectangle in GUI pixels, filled.
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, argb: u32) {
        self.gradient(x, y, w, h, argb, argb);
    }
    fn gradient(&mut self, x: f32, y: f32, w: f32, h: f32, top: u32, bottom: u32) {
        let (tx, ty, s) = self.pose();
        let (x0, y0) = ((tx + x * s).round() as i64, (ty + y * s).round() as i64);
        let (x1, y1) = (
            (tx + (x + w) * s).round() as i64,
            (ty + (y + h) * s).round() as i64,
        );
        let rows = (y1 - y0).max(1);
        for py in y0..y1 {
            let t = (py - y0) as f32 / rows as f32;
            let lerp = |shift: u32| {
                let a = ((top >> shift) & 0xFF) as f32;
                let b = ((bottom >> shift) & 0xFF) as f32;
                (a + (b - a) * t).round() as u8
            };
            let color = [lerp(16), lerp(8), lerp(0), lerp(24)];
            for px in x0..x1 {
                self.image.blend(px, py, color);
            }
        }
    }
    /// `src`'s rectangle scaled into a GUI rectangle (nearest pixel).
    fn blit(
        &mut self,
        src: &Image,
        sx: u32,
        sy: u32,
        sw: u32,
        sh: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        let (tx, ty, s) = self.pose();
        let (x0, y0) = ((tx + x * s).round() as i64, (ty + y * s).round() as i64);
        let (x1, y1) = (
            (tx + (x + w) * s).round() as i64,
            (ty + (y + h) * s).round() as i64,
        );
        let (dw, dh) = ((x1 - x0).max(1), (y1 - y0).max(1));
        for py in y0..y1 {
            for px in x0..x1 {
                let u = sx + (((px - x0) as u64 * u64::from(sw)) / dw as u64) as u32;
                let v = sy + (((py - y0) as u64 * u64::from(sh)) / dh as u64) as u32;
                if u < src.width && v < src.height {
                    self.image.blend(px, py, src.get(u, v));
                }
            }
        }
    }
    /// A sprite (`gui/sprites/...`), nine-sliced when its `.mcmeta` says so; a frame without it.
    fn sprite(&mut self, id: &str, x: f32, y: f32, w: f32, h: f32) {
        let Some(texture) = self.res.texture(id) else {
            self.fill(x, y, w, 1.0, 0xFFFF00FF);
            self.fill(x, y + h - 1.0, w, 1.0, 0xFFFF00FF);
            self.fill(x, y, 1.0, h, 0xFFFF00FF);
            self.fill(x + w - 1.0, y, 1.0, h, 0xFFFF00FF);
            return;
        };
        let (image, border) = (&texture.0, texture.1);
        // An animated sprite's first frame.
        let frame = image.width.min(image.height);
        let (iw, ih) = (
            image.width,
            if image.height > image.width {
                frame
            } else {
                image.height
            },
        );
        let Some(b) = border.filter(|&b| b > 0 && 2 * b < iw && 2 * b < ih) else {
            self.blit(image, 0, 0, iw, ih, x, y, w, h);
            return;
        };
        let bf = b as f32;
        let (cols, rows) = (
            [
                (0, b, 0.0, bf),
                (b, iw - 2 * b, bf, w - 2.0 * bf),
                (iw - b, b, w - bf, bf),
            ],
            [
                (0, b, 0.0, bf),
                (b, ih - 2 * b, bf, h - 2.0 * bf),
                (ih - b, b, h - bf, bf),
            ],
        );
        for (sx, sw, dx, dw) in cols {
            for (sy, sh, dy, dh) in rows {
                if dw > 0.0 && dh > 0.0 {
                    self.blit(image, sx, sy, sw, sh, x + dx, y + dy, dw, dh);
                }
            }
        }
    }
    fn text_width(&self, text: &str) -> f32 {
        spans(text, 0)
            .iter()
            .map(|&(c, _)| self.res.glyph_width(c) as f32 + 1.0)
            .sum()
    }
    /// Text in the game's font (8 pixels high), its shadow first when asked; boxes without the
    /// font. Returns the width drawn.
    fn text(&mut self, x: f32, y: f32, text: &str, rgb: u32, shadow: bool) -> f32 {
        let spans = spans(text, rgb);
        if shadow {
            self.glyphs(x + 1.0, y + 1.0, &spans, true);
        }
        self.glyphs(x, y, &spans, false)
    }
    fn glyphs(&mut self, x: f32, y: f32, spans: &[(char, u32)], shadow: bool) -> f32 {
        let mut pen = x;
        for &(c, rgb) in spans {
            let rgb = if shadow { (rgb & 0xFC_FC_FC) >> 2 } else { rgb };
            let advance = self.res.glyph_width(c) as f32 + 1.0;
            match self.res.glyphs.clone() {
                Some(font) => {
                    let code = if c.is_ascii() {
                        c as u32
                    } else {
                        u32::from(b'?')
                    };
                    let cell = font.width / 16;
                    let (cx, cy) = ((code % 16) * cell, (code / 16) * cell);
                    let (tx, ty, s) = self.pose();
                    let px_per_cell = 8.0 * s / cell as f32;
                    for gy in 0..cell {
                        for gx in 0..cell {
                            let a = font.get(cx + gx, cy + gy)[3];
                            if a == 0 {
                                continue;
                            }
                            let x0 = (tx + pen * s + gx as f32 * px_per_cell).round() as i64;
                            let y0 = (ty + y * s + gy as f32 * px_per_cell).round() as i64;
                            let x1 = (tx + pen * s + (gx + 1) as f32 * px_per_cell).round() as i64;
                            let y1 = (ty + y * s + (gy + 1) as f32 * px_per_cell).round() as i64;
                            let color = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a];
                            for py in y0..y1 {
                                for px in x0..x1 {
                                    self.image.blend(px, py, color);
                                }
                            }
                        }
                    }
                }
                None if !c.is_whitespace() => {
                    self.fill(pen, y + 1.0, advance - 2.0, 6.0, 0xFF00_0000 | rgb)
                }
                None => {}
            }
            pen += advance;
        }
        pen - x
    }
    /// An item's icon: its item texture, else its block's, else a placeholder.
    fn item(&mut self, id: &str, x: f32, y: f32) {
        let path = id.split_once(':').map_or(id, |(_, p)| p);
        let ns = id.split_once(':').map_or("minecraft", |(n, _)| n);
        for kind in ["item", "block"] {
            let key = format!("{ns}:{kind}/{path}");
            if let Some(texture) = self.res.texture(&key) {
                let side = texture.0.width.min(texture.0.height);
                self.blit(&texture.0, 0, 0, side, side, x, y, 16.0, 16.0);
                return;
            }
        }
        self.fill(x, y, 8.0, 8.0, 0xFFF800F8);
        self.fill(x + 8.0, y + 8.0, 8.0, 8.0, 0xFFF800F8);
        self.fill(x + 8.0, y, 8.0, 8.0, 0xFF000000);
        self.fill(x, y + 8.0, 8.0, 8.0, 0xFF000000);
    }
    /// Vanilla's tooltip box at the mouse.
    fn tooltip(&mut self, x: f32, y: f32, lines: &[String]) {
        let width = lines.iter().map(|l| self.text_width(l)).fold(0.0, f32::max) - 1.0;
        let height =
            8.0 + 10.0 * (lines.len() as f32 - 1.0) + if lines.len() > 1 { 2.0 } else { 0.0 };
        let (bx, by) = (x + 12.0, y - 12.0);
        let back = 0xF010_0010;
        self.fill(bx - 3.0, by - 4.0, width + 6.0, 1.0, back);
        self.fill(bx - 3.0, by + height + 3.0, width + 6.0, 1.0, back);
        self.fill(bx - 3.0, by - 3.0, width + 6.0, height + 6.0, back);
        self.fill(bx - 4.0, by - 3.0, 1.0, height + 6.0, back);
        self.fill(bx + width + 3.0, by - 3.0, 1.0, height + 6.0, back);
        self.gradient(
            bx - 3.0,
            by - 2.0,
            1.0,
            height + 4.0,
            0x5050_00FF,
            0x5028_007F,
        );
        self.gradient(
            bx + width + 2.0,
            by - 2.0,
            1.0,
            height + 4.0,
            0x5050_00FF,
            0x5028_007F,
        );
        self.fill(bx - 3.0, by - 3.0, width + 6.0, 1.0, 0x5050_00FF);
        self.fill(bx - 3.0, by + height + 2.0, width + 6.0, 1.0, 0x5028_007F);
        let mut ly = by;
        for (i, line) in lines.iter().enumerate() {
            self.text(bx, ly, line, 0xFFFFFF, true);
            ly += if i == 0 { 12.0 } else { 10.0 };
        }
    }
}

/// `text` in the game's font (8 pixels high), shadowed, on `image` at `x`, `y`.
pub(crate) fn label(image: Image, res: &mut Resources, x: f32, y: f32, text: &str) -> Image {
    let mut canvas = Canvas {
        image,
        res,
        poses: vec![(0.0, 0.0, 1.0)],
    };
    canvas.text(x, y, text, 0xFFFFFF, true);
    canvas.image
}

/// `ops` drawn on a `width` by `height` GUI at `scale`, over a dark backdrop.
pub fn render(ops: &[DrawOp], width: u32, height: u32, scale: u32, res: &mut Resources) -> Image {
    let mut canvas = Canvas {
        image: Image::new(width * scale, height * scale, [0x2B, 0x2D, 0x31, 0xFF]),
        res,
        poses: vec![(0.0, 0.0, scale as f32)],
    };
    for op in ops {
        match op {
            DrawOp::Fill { x, y, w, h, color } => canvas.fill(*x, *y, *w, *h, *color),
            DrawOp::Gradient {
                x,
                y,
                w,
                h,
                top,
                bottom,
            } => canvas.gradient(*x, *y, *w, *h, *top, *bottom),
            DrawOp::Text {
                x,
                y,
                text,
                color,
                shadow,
            } => {
                canvas.text(*x, *y, text, *color, *shadow);
            }
            DrawOp::Item { x, y, stack } => {
                canvas.item(&stack.id, *x, *y);
                // The count, as an inventory shows it.
                if stack.count > 1 {
                    let count = stack.count.to_string();
                    let w = canvas.text_width(&count) - 1.0;
                    canvas.text(x + 17.0 - w - 1.0, y + 9.0, &count, 0xFFFFFF, true);
                }
            }
            DrawOp::ItemId { x, y, id } => canvas.item(id, *x, *y),
            DrawOp::Sprite { x, y, w, h, sprite } => {
                // As the game looks it up: a GUI sprite, else a texture (`item/compass_16`).
                let (ns, path) = sprite.split_once(':').unwrap_or(("minecraft", sprite));
                let gui = format!("{ns}:gui/sprites/{path}");
                let id = if canvas.res.texture(&gui).is_none()
                    && canvas.res.texture(&format!("{ns}:{path}")).is_some()
                {
                    format!("{ns}:{path}")
                } else {
                    gui
                };
                canvas.sprite(&id, *x, *y, *w, *h);
            }
            DrawOp::Tooltip { x, y, lines } => canvas.tooltip(*x, *y, lines),
            DrawOp::Button {
                x,
                y,
                w,
                h,
                label,
                hovered,
                active,
            } => {
                let sprite = match (active, hovered) {
                    (false, _) => "widget/button_disabled",
                    (true, true) => "widget/button_highlighted",
                    (true, false) => "widget/button",
                };
                canvas.sprite(&format!("minecraft:gui/sprites/{sprite}"), *x, *y, *w, *h);
                let tw = canvas.text_width(label) - 1.0;
                let colour = if *active { 0xFFFFFF } else { 0xA0A0A0 };
                canvas.text(x + (w - tw) / 2.0, y + (h - 8.0) / 2.0, label, colour, true);
            }
            DrawOp::Panel { x, y, w, h } => {
                canvas.sprite("minecraft:gui/sprites/popup/background", *x, *y, *w, *h)
            }
            DrawOp::Push { x, y, scale } => {
                let (tx, ty, s) = canvas.pose();
                canvas.poses.push((tx + x * s, ty + y * s, s * scale));
            }
            DrawOp::Pop => {
                if canvas.poses.len() > 1 {
                    canvas.poses.pop();
                }
            }
            // The game draws the player's model: here a figure of its size, standing in the box.
            DrawOp::Player {
                x, y, w, h, size, ..
            } => {
                let (cx, bottom) = (x + w / 2.0, y + h / 2.0 + size * 0.9625);
                let px = size / 16.0;
                let part = |c: &mut Canvas, dx: f32, top: f32, pw: f32, ph: f32, colour: u32| {
                    c.fill(cx + dx * px, bottom - top * px, pw * px, ph * px, colour);
                };
                part(&mut canvas, -4.0, 32.0, 8.0, 8.0, 0xFFB5_8D6E);
                part(&mut canvas, -4.0, 24.0, 8.0, 12.0, 0xFF2F_8FA8);
                part(&mut canvas, -8.0, 24.0, 4.0, 12.0, 0xFFB5_8D6E);
                part(&mut canvas, 4.0, 24.0, 4.0, 12.0, 0xFFB5_8D6E);
                part(&mut canvas, -4.0, 12.0, 8.0, 12.0, 0xFF3B_3B8F);
            }
            DrawOp::ItemTooltip { x, y, stack } => {
                canvas.tooltip(*x, *y, std::slice::from_ref(&stack.name));
            }
        }
    }
    canvas.image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_takes_section_codes_and_json_colours() {
        let colours = |text: &str| {
            spans(text, 0xFFFFFF)
                .into_iter()
                .map(|(_, c)| c)
                .collect::<Vec<_>>()
        };
        assert_eq!(colours("a§cb§rc"), [0xFFFFFF, 0xFF5555, 0xFFFFFF]);
        assert_eq!(colours("§9§oa"), [0x5555FF]);
        assert_eq!(
            colours(r##"{"text":"a","color":"gold","extra":[{"text":"b","color":"#123456"}]}"##),
            [0xFFAA00, 0x123456]
        );
    }

    #[test]
    fn fills_land_at_the_gui_scale_and_inside_a_pushed_pose() {
        let ops = [
            DrawOp::Fill {
                x: 1.0,
                y: 1.0,
                w: 1.0,
                h: 1.0,
                color: 0xFFFF0000,
            },
            DrawOp::Push {
                x: 4.0,
                y: 0.0,
                scale: 0.5,
            },
            DrawOp::Fill {
                x: 0.0,
                y: 0.0,
                w: 2.0,
                h: 2.0,
                color: 0xFF00FF00,
            },
            DrawOp::Pop,
        ];
        let image = render(&ops, 8, 4, 2, &mut Resources::new(None));
        assert_eq!((image.width, image.height), (16, 8));
        assert_eq!(image.get(2, 2), [0xFF, 0, 0, 0xFF]);
        assert_eq!(image.get(3, 3), [0xFF, 0, 0, 0xFF]);
        assert_ne!(image.get(4, 4), [0xFF, 0, 0, 0xFF]);
        // The pose moved 4 GUI pixels (8 image pixels) and halved the scale.
        assert_eq!(image.get(8, 0), [0, 0xFF, 0, 0xFF]);
        assert_eq!(image.get(9, 1), [0, 0xFF, 0, 0xFF]);
        assert_ne!(image.get(10, 2), [0, 0xFF, 0, 0xFF]);
    }
}
