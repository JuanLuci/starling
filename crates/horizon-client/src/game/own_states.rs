//! A content mod's block with states of its own being placed (Starling; mod_content's `states`):
//! the properties its template does not place are set as Create places its blocks. An `axis`
//! follows the face clicked (RotatedPillarKineticBlock); a `facing` with up and down, the way
//! the player looks, turned round unless they sneak (DirectionalKineticBlock); a `facing` or
//! `horizontal_facing` of four, the way they face, turned round (HorizontalKineticBlock).
use super::Game;
use horizon_core::geometry::Direction;
use horizon_core::position::BlockPos;
use horizon_world::level::PlaceContext;
use horizon_world::registry::BlockStateId;

/// `Direction.orderedByNearest(entity)[0]`: the way a look at `yaw` and `pitch` (degrees) points
/// most along.
fn nearest_looking(yaw: f32, pitch: f32) -> [i32; 3] {
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let look = [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ];
    let axis = (0..3)
        .max_by(|a, b| look[*a].abs().total_cmp(&look[*b].abs()))
        .unwrap_or(1);
    let mut d = [0; 3];
    d[axis] = if look[axis] < 0.0 { -1 } else { 1 };
    d
}

fn name_of(d: [i32; 3]) -> &'static str {
    match d {
        [0, 1, 0] => "up",
        [0, -1, 0] => "down",
        [0, 0, -1] => "north",
        [0, 0, 1] => "south",
        [-1, 0, 0] => "west",
        _ => "east",
    }
}

impl Game<'_> {
    /// `state`, just placed at `pos` by `ctx`, with its own properties set (and so in the world):
    /// the state as it now is.
    pub(super) fn orient_own_states(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        ctx: &PlaceContext,
    ) -> BlockStateId {
        let name = self.res.facts().get(state).name.clone();
        let Some(props) = self
            .res
            .kinetics()
            .own_placement(&name)
            .map(<[String]>::to_vec)
        else {
            return state;
        };
        let registry = self.pack.assets.registry;
        let Some(mut s) = registry.state(state).cloned() else {
            return state;
        };
        let clicked: [i32; 3] = match ctx.face {
            Direction::Up => [0, 1, 0],
            Direction::Down => [0, -1, 0],
            Direction::North => [0, 0, -1],
            Direction::South => [0, 0, 1],
            Direction::West => [-1, 0, 0],
            Direction::East => [1, 0, 0],
        };
        let look = nearest_looking(ctx.yaw, ctx.pitch);
        let back = |d: [i32; 3]| d.map(|v| -v);
        // The way the player faces on the ground (yaw), turned round.
        let horizontal = {
            let mut flat = nearest_looking(ctx.yaw, 0.0);
            flat[1] = 0;
            back(flat)
        };
        for p in &props {
            let candidates: Vec<&str> = match p.as_str() {
                "axis" => {
                    let axis = clicked.iter().position(|v| *v != 0).unwrap_or(1);
                    vec![["x", "y", "z"][axis]]
                }
                "facing" => {
                    let looked = if ctx.sneaking { look } else { back(look) };
                    vec![name_of(looked), name_of(horizontal)]
                }
                "horizontal_facing" => vec![name_of(horizontal)],
                _ => continue,
            };
            // The first value the block has (a facing of four takes the horizontal one).
            for value in candidates {
                let mut t = s.clone();
                t.properties.insert(p.clone(), value.to_owned());
                if registry.id(&t).is_some() {
                    s = t;
                    break;
                }
            }
        }
        let Some(new) = registry.id(&s) else {
            return state;
        };
        if new != state {
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            self.with_level(|b, level, ticks| b.set_block(level, ticks, at, new, 3));
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_points_most_along_one_way() {
        // Yaw 0 looks south, 90 west; pitch 90 down.
        assert_eq!(nearest_looking(0.0, 0.0), [0, 0, 1]);
        assert_eq!(nearest_looking(90.0, 10.0), [-1, 0, 0]);
        assert_eq!(nearest_looking(30.0, 80.0), [0, -1, 0]);
    }
}
