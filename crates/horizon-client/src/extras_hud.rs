//! HUD boxes in the corners: the minimap's place and mods' panels, small
//! boxes in GUI pixels stacked per corner (`docs/extras.md`).
use crate::{
    extras::Corner,
    hud::{HudAtlas, HudBatch, Rect, rgba},
};

/// The gap between an overlay and the screen edge or the next overlay.
const MARGIN: f32 = 2.0;
const BACKGROUND: u32 = 0x7800_0000;

/// Where the next overlay goes in each corner.
pub struct Corners {
    width: f32,
    height: f32,
    /// Distance from the corner's top or bottom edge, per `Corner::ALL`.
    used: [f32; 4],
}
impl Corners {
    /// `top_right` reserves the status effect icons' rows.
    pub fn new(width: f32, height: f32, top_right: f32) -> Self {
        Self {
            width,
            height,
            used: [0.0, top_right, 0.0, 0.0],
        }
    }
    /// The top-left position of a `w` x `h` box in `corner`, below (or
    /// above) what the corner already holds.
    pub fn place(&mut self, corner: Corner, w: f32, h: f32) -> (f32, f32) {
        let i = Corner::ALL.iter().position(|c| *c == corner).unwrap_or(0);
        let edge = self.used[i] + MARGIN;
        self.used[i] = edge + h;
        let x = if corner.left() {
            MARGIN
        } else {
            self.width - MARGIN - w
        };
        let y = if corner.top() {
            edge
        } else {
            self.height - edge - h
        };
        (x, y)
    }
}

/// How the minimap shows: its corner and side in GUI pixels, its image (an
/// atlas rect), the player arrow (the map item's decoration sprite) turned
/// `arrow` degrees clockwise from up, and the rim's N toward `north` when the
/// map turns.
pub struct MinimapLook {
    pub corner: Corner,
    pub size: f32,
    pub image: Rect,
    pub arrow: f32,
    pub north: Option<(f32, f32)>,
}
pub fn draw_minimap(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    corners: &mut Corners,
    look: &MinimapLook,
) {
    let MinimapLook {
        corner,
        size,
        image,
        arrow,
        north,
    } = *look;
    let (x, y) = corners.place(corner, size, size);
    batch.rect_uv(x, y, size, size, image, u32::MAX);
    let (cx, cy) = (x + size / 2.0, y + size / 2.0);
    if let Some(r) = atlas.sprites.get("map/decorations/player").copied() {
        // MapRenderer's decorations draw the sprite flipped: its arrow
        // points down the texture, so up on screen is half a turn.
        let (s, c) = (arrow + 180.0).to_radians().sin_cos();
        let p = |dx: f32, dy: f32| [cx + dx * c - dy * s, cy + dx * s + dy * c];
        let h = 4.0;
        batch.quad(
            [p(-h, h), p(h, h), p(h, -h), p(-h, -h)],
            [[r.u0, r.v0], [r.u1, r.v0], [r.u1, r.v1], [r.u0, r.v1]],
            u32::MAX,
        );
    }
    if let Some((nx, ny)) = north {
        let reach = size / 2.0 - 6.0;
        let tw = HudBatch::text_width(atlas, "N");
        let (tx, ty) = (cx + nx * reach - tw / 2.0, cy + ny * reach - 4.0);
        batch.text(
            atlas,
            "N",
            tx.round(),
            ty.round(),
            rgba(255, 255, 255, 255),
            true,
        );
    }
}

/// A box of text lines (a mod's HUD panel): `(text, 0xRRGGBB)` each.
pub fn draw_lines(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    corners: &mut Corners,
    corner: Corner,
    lines: &[(&str, u32)],
) {
    if lines.is_empty() {
        return;
    }
    let w = lines
        .iter()
        .map(|(text, _)| HudBatch::text_width(atlas, text))
        .fold(0.0, f32::max)
        + 4.0;
    let h = 10.0 * lines.len() as f32 + 2.0;
    let (x, y) = corners.place(corner, w, h);
    batch.fill(atlas, x, y, w, h, BACKGROUND);
    for (i, (text, color)) in lines.iter().enumerate() {
        let [_, r, g, b] = color.to_be_bytes();
        let y = y + 2.0 + 10.0 * i as f32;
        batch.text(atlas, text, x + 2.0, y, rgba(r, g, b, 255), true);
    }
}

pub struct VoltPerfHudData<'a> {
    pub full: bool,
    pub corner: Corner,
    pub fps: u32,
    pub low_1pct: u32,
    pub frame_ms: f32,
    pub ram_mib: u64,
    pub scale_pct: u32,
    pub entities_drawn: u32,
    pub entities_total: u32,
    pub particles: u32,
    pub chunks: u32,
    pub active_boosts: usize,
    pub samples_ms: &'a [f32; 48],
    pub sample_head: usize,
}

pub fn draw_volt_perf_hud(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    corners: &mut Corners,
    d: &VoltPerfHudData<'_>,
) {
    let fps_col = if d.fps >= 55 {
        rgba(74, 222, 128, 255)
    } else if d.fps >= 30 {
        rgba(250, 204, 21, 255)
    } else {
        rgba(248, 113, 113, 255)
    };
    if !d.full {
        let line = if d.ram_mib > 0 {
            format!("VOLT {} FPS | 1% {} | {}M", d.fps, d.low_1pct, d.ram_mib)
        } else {
            format!("VOLT {} FPS | 1% {}", d.fps, d.low_1pct)
        };
        let w = HudBatch::text_width(atlas, &line) + 8.0;
        let h = 14.0;
        let (x, y) = corners.place(d.corner, w, h);
        batch.fill(atlas, x, y, w, h, rgba(220, 20, 60, 210));
        batch.fill(atlas, x + 1.0, y + 1.0, w - 2.0, h - 2.0, rgba(11, 11, 16, 232));
        batch.text(atlas, &line, x + 4.0, y + 3.0, fps_col, true);
        return;
    }

    let l1 = format!(
        "VOLT BOOST  {} FPS  1% {}  {:.1}ms",
        d.fps, d.low_1pct, d.frame_ms
    );
    let l2 = format!(
        "ENT {}/{}  PRT {}  CHK {}",
        d.entities_drawn, d.entities_total, d.particles, d.chunks
    );
    let l3 = format!(
        "RAM {}M  RES {}%  {} ON",
        d.ram_mib, d.scale_pct, d.active_boosts
    );
    let text_w = [&l1, &l2, &l3]
        .into_iter()
        .map(|s| HudBatch::text_width(atlas, s))
        .fold(104.0f32, f32::max);
    let w = text_w + 10.0;
    let graph_h = 18.0;
    let h = 34.0 + graph_h + 6.0;
    let (x, y) = corners.place(d.corner, w, h);

    batch.fill(atlas, x, y, w, h, rgba(220, 20, 60, 220));
    batch.fill(atlas, x + 1.0, y + 1.0, w - 2.0, h - 2.0, rgba(10, 10, 15, 236));
    batch.text(atlas, &l1, x + 5.0, y + 4.0, fps_col, true);
    batch.text(atlas, &l2, x + 5.0, y + 14.0, rgba(228, 228, 238, 255), true);
    batch.text(atlas, &l3, x + 5.0, y + 24.0, rgba(176, 176, 196, 255), true);

    let gx = x + 5.0;
    let gy = y + 35.0;
    let gw = w - 10.0;
    batch.fill(atlas, gx, gy, gw, graph_h, rgba(22, 22, 32, 240));
    // 16.67ms (60 FPS) horizontal reference line
    let target_y = gy + graph_h - (16.67 / 40.0 * graph_h).clamp(1.0, graph_h - 1.0);
    batch.fill(atlas, gx, target_y, gw, 1.0, rgba(220, 20, 60, 110));

    let bar_step = (gw / 48.0).max(1.0);
    for i in 0..48 {
        let ms = d.samples_ms[(d.sample_head + i) % 48];
        if ms <= 0.01 {
            continue;
        }
        let bh = ((ms / 40.0) * graph_h).clamp(1.0, graph_h);
        let col = if ms <= 17.5 {
            rgba(74, 222, 128, 220)
        } else if ms <= 28.0 {
            rgba(250, 204, 21, 220)
        } else {
            rgba(244, 63, 94, 235)
        };
        let bx = gx + i as f32 * bar_step;
        if bx + bar_step <= gx + gw + 0.5 {
            batch.fill(atlas, bx, gy + graph_h - bh, (bar_step - 0.5).max(1.0), bh, col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_stack_away_from_their_edge() {
        let mut c = Corners::new(400.0, 300.0, 26.0);
        assert_eq!(c.place(Corner::TopLeft, 50.0, 12.0), (2.0, 2.0));
        assert_eq!(c.place(Corner::TopLeft, 70.0, 20.0), (2.0, 16.0));
        // Top right starts under the effect icons.
        assert_eq!(c.place(Corner::TopRight, 50.0, 12.0), (348.0, 28.0));
        assert_eq!(c.place(Corner::BottomRight, 50.0, 12.0), (348.0, 286.0));
        assert_eq!(c.place(Corner::BottomRight, 50.0, 10.0), (348.0, 274.0));
        assert_eq!(c.place(Corner::BottomLeft, 10.0, 10.0), (2.0, 288.0));
    }
}
