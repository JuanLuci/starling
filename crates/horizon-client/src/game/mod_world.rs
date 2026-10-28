//! The mods' drawing in the world (`world-draw`): their layers' lines among the frame's debug
//! lines (gizmos), depth tested or over everything; their boxes as unlit see-through quads, and
//! their texts and labels as name tags are drawn, in the world or seen through it; their icons as
//! GUI sprites facing the camera, with their texts as labels'.
use super::{Game, mods::rich_text, names::TextAxes, view_vector};
use horizon_client::{block_entity_render::vertex_light, gizmos::Line, mesher::Vertex, text::Text};
use horizon_mods::{Facing, WorldIcon};

/// Labels' texts kept at most.
const LABEL_TEXTS: usize = 1024;

/// ARGB as a vertex's RGBA8 (alpha on top).
fn rgba8(argb: u32) -> u32 {
    argb & 0xFF00_FF00 | (argb & 0xFF) << 16 | (argb >> 16) & 0xFF
}

impl Game<'_> {
    /// The mods' lines: into `lines`, or `on_top` for a layer seen through walls.
    pub(super) fn mod_world_lines(&self, lines: &mut Vec<Line>, on_top: &mut Vec<Line>) {
        for (_, layers) in self.mods.mods.world_layers() {
            for (_, layer) in layers {
                let out = if layer.through_walls {
                    &mut *on_top
                } else {
                    &mut *lines
                };
                for l in &layer.lines {
                    let o = [l.origin.x, l.origin.y, l.origin.z];
                    let at = |e: &[f32]| std::array::from_fn(|i| o[i] + f64::from(e[i]));
                    for (i, e) in l.ends.as_chunks::<6>().0.iter().enumerate() {
                        out.push(Line {
                            start: at(&e[..3]),
                            end: at(&e[3..]),
                            color: l.colors.get(i).or(l.colors.first()).copied().unwrap_or(0),
                            width: l.width,
                        });
                    }
                }
            }
        }
    }
    /// The mods' boxes, texts and labels, `camera` the eye, the frame `fov` degrees high in a window
    /// `size` pixels: in the world into `out`, or seen through it into `see_through`.
    pub(super) fn mod_world_geometry(
        &self,
        camera: glam::DVec3,
        fov: f32,
        size: [u32; 2],
        out: &mut Vec<Vertex>,
        see_through: &mut Vec<Vertex>,
    ) {
        let forward = glam::DVec3::from_array(view_vector(self.yaw, self.pitch));
        let facing_camera = self.camera_text_axes();
        // Lying face up, its top away from the camera along the nearest of the four
        // directions, a background below it.
        let ahead = if forward.x.abs() > forward.z.abs() {
            glam::DVec3::new(forward.x.signum(), 0.0, 0.0)
        } else {
            glam::DVec3::new(0.0, 0.0, if forward.z < 0.0 { -1.0 } else { 1.0 })
        };
        let facing_up = TextAxes {
            right: ahead.cross(glam::DVec3::Y).normalize_or_zero().as_vec3(),
            up: ahead.as_vec3(),
            away: glam::Vec3::NEG_Y * 0.002,
        };
        let gui = horizon_client::hud::HudBatch::new(size).scale;
        // Blocks a GUI pixel spans one block away along the view.
        let pixel = 2.0 * (f64::from(fov).to_radians() / 2.0).tan() * f64::from(gui)
            / f64::from(size[1].max(1));
        // The middle of the font's white texels: an edge samples the next ones on some pixels.
        let white = self.gui.atlas.font.white();
        let uv = [(white.u0 + white.u1) / 2.0, (white.v0 + white.v1) / 2.0];
        for (_, layers) in self.mods.mods.world_layers() {
            for (_, layer) in layers {
                let through = layer.through_walls;
                let out = if through {
                    &mut *see_through
                } else {
                    &mut *out
                };
                for b in &layer.boxes {
                    let o = glam::DVec3::new(b.origin.x, b.origin.y, b.origin.z) - camera;
                    for (i, c) in b.corners.as_chunks::<6>().0.iter().enumerate() {
                        let color = b.colors.get(i).or(b.colors.first()).copied().unwrap_or(0);
                        box_quads(o, c, rgba8(color), uv, out);
                    }
                }
                let (layer_index, light) = if through {
                    (2, 0)
                } else {
                    (0, vertex_light(0xFF))
                };
                for t in &layer.texts {
                    let o = glam::DVec3::new(t.origin.x, t.origin.y, t.origin.z) - camera;
                    let (axes, lift) = match t.facing {
                        Facing::Camera => (facing_camera, 8.0),
                        Facing::Up => (facing_up, 4.0),
                    };
                    let scale = 0.025 * t.scale;
                    let lift = axes.up.as_dvec3() * f64::from(scale * lift);
                    let background = (t.background >> 24 != 0).then(|| rgba8(t.background));
                    let places = t.places.as_chunks::<3>().0.iter();
                    for (i, (p, line)) in places.zip(t.lines.split('\n')).enumerate() {
                        let color = t.colors.get(i).or(t.colors.first()).copied().unwrap_or(0);
                        let mut text = Text::legacy(line);
                        for span in &mut text.spans {
                            span.style.color.get_or_insert(color & 0xFF_FFFF);
                        }
                        let at = o
                            + glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
                            + lift;
                        self.text_quads_along(
                            at,
                            &[(text, 0.0)],
                            axes,
                            scale,
                            color >> 24,
                            background,
                            layer_index,
                            light,
                            out,
                        );
                    }
                }
                for b in &layer.labels {
                    let o = glam::DVec3::new(b.at.x, b.at.y, b.at.z) - camera;
                    let scale = if b.fixed_size {
                        let depth = o.dot(forward);
                        if depth <= 0.05 {
                            continue;
                        }
                        (pixel * depth) as f32 * b.scale
                    } else {
                        0.025 * b.scale
                    };
                    let (axes, lift) = match b.facing {
                        // The text's bottom on the point.
                        Facing::Camera => (facing_camera, 8.0),
                        // Its middle.
                        Facing::Up => (facing_up, 4.0),
                    };
                    let o = o + axes.up.as_dvec3() * f64::from(scale * lift);
                    let background = (b.background >> 24 != 0).then(|| rgba8(b.background));
                    let lines = [(self.mod_rich_text(&b.text), 0.0)];
                    self.text_quads_along(
                        o,
                        &lines,
                        axes,
                        scale,
                        0xFF,
                        background,
                        layer_index,
                        light,
                        out,
                    );
                }
            }
        }
        let mut icons: Vec<(&WorldIcon, f64)> = self
            .mods
            .mods
            .world_icons()
            .flat_map(|(_, layers)| layers.iter().flat_map(|(_, icons)| icons))
            .map(|i| {
                let at = glam::DVec3::new(i.at.x, i.at.y, i.at.z);
                (i, at.distance_squared(camera))
            })
            .collect();
        // The farthest first: a nearer one is drawn over it (and a layer's in its order).
        icons.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (icon, _) in icons {
            let out = if icon.through_walls {
                &mut *see_through
            } else {
                &mut *out
            };
            self.mod_icon_quads(icon, camera, forward, pixel, facing_camera, out);
        }
    }
    /// An icon's sprite and texts facing the camera (`axes`), `pixel` the blocks a GUI pixel spans
    /// one block away along the view `forward`.
    fn mod_icon_quads(
        &self,
        icon: &WorldIcon,
        camera: glam::DVec3,
        forward: glam::DVec3,
        pixel: f64,
        axes: TextAxes,
        out: &mut Vec<Vertex>,
    ) {
        let o = glam::DVec3::new(icon.at.x, icon.at.y, icon.at.z) - camera;
        // The sprite's half side and a pixel of the texts' font, in blocks.
        let (half, text) = if icon.fixed_size {
            let depth = o.dot(forward);
            if depth <= 0.05 {
                return;
            }
            let unit = (pixel * depth) as f32;
            (icon.size * unit / 2.0, icon.text_scale * unit)
        } else {
            (icon.size / 2.0, 0.025 * icon.text_scale)
        };
        let sprite = (!icon.sprite.is_empty())
            .then(|| self.mod_sprite(&icon.sprite))
            .flatten()
            .and_then(|k| self.gui.atlas.sprites.get(&k).copied());
        if let Some(r) = sprite {
            let (c, right, up) = (o.as_vec3(), axes.right * half, axes.up * half);
            let color = rgba8(icon.color);
            let corners = [
                (c - right - up, [r.u0, r.v1]),
                (c + right - up, [r.u1, r.v1]),
                (c + right + up, [r.u1, r.v0]),
                (c - right + up, [r.u0, r.v0]),
            ];
            for (pos, uv) in corners {
                out.push(Vertex {
                    normal: [0; 4],
                    pos: pos.to_array(),
                    uv,
                    layer: 2,
                    color,
                    light: 0,
                    glint_uv: [0.0, 0.0],
                });
            }
        }
        let (layer, light) = if icon.through_walls {
            (2, 0)
        } else {
            (0, vertex_light(0xFF))
        };
        let background = (icon.background >> 24 != 0).then(|| rgba8(icon.background));
        // Two of the font's pixels from the sprite; a text is 8 high, drawn down from its top.
        let up = axes.up.as_dvec3();
        let gap = half + 2.0 * text;
        let texts = [
            (&icon.above, o + up * f64::from(gap + 8.0 * text)),
            (&icon.below, o - up * f64::from(gap)),
        ];
        for (line, at) in texts {
            if line.is_empty() {
                continue;
            }
            let lines = [(self.mod_rich_text(line), 0.0)];
            self.text_quads_along(at, &lines, axes, text, 0xFF, background, layer, light, out);
        }
    }
    /// A label's or an icon's text, read once.
    fn mod_rich_text(&self, text: &str) -> Text {
        let mut texts = self.mods.label_texts.borrow_mut();
        if let Some(t) = texts.get(text) {
            return t.clone();
        }
        if texts.len() >= LABEL_TEXTS {
            texts.clear();
        }
        let t = rich_text(text);
        texts.insert(text.to_owned(), t.clone());
        t
    }
}

/// A box's six faces, `o` its origin from the camera and `c` its corners from there: unlit
/// (overlay 2), unshaded, in `color` over the white texel at `uv`.
fn box_quads(o: glam::DVec3, c: &[f32], color: u32, uv: [f32; 2], out: &mut Vec<Vertex>) {
    let p = |x: usize, y: usize, z: usize| {
        (o + glam::DVec3::new(f64::from(c[x]), f64::from(c[y]), f64::from(c[z]))).as_vec3()
    };
    // Least (0, 1, 2) and greatest (3, 4, 5) corners' coordinates.
    let (x0, y0, z0, x1, y1, z1) = (0, 1, 2, 3, 4, 5);
    let faces = [
        [p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)],
        [p(x0, y1, z0), p(x0, y1, z1), p(x1, y1, z1), p(x1, y1, z0)],
        [p(x0, y0, z0), p(x0, y1, z0), p(x1, y1, z0), p(x1, y0, z0)],
        [p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)],
        [p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1), p(x0, y1, z0)],
        [p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)],
    ];
    for face in faces {
        for pos in face {
            out.push(Vertex {
                normal: [0; 4],
                pos: pos.to_array(),
                uv,
                layer: 2,
                color,
                light: 0,
                glint_uv: [0.0, 0.0],
            });
        }
    }
}
