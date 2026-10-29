//! Boats, minecarts, item frames, armor stands and paintings for a server's
//! client: single player on its server thread, a LAN guest, a player on a
//! dedicated server.
//!
//! The client aims at them like at mobs (`Minecraft.hitResult`, then
//! `ServerboundInteractPacket`), and rides the vehicle the server seats it in
//! (`ClientPacketListener.handleSetEntityPassengersPacket`). A boat is
//! controlled by its rider's client: it ticks here and its moves go to the
//! server (`LocalPlayer.tick`: `ServerboundMoveVehiclePacket`,
//! `AbstractBoat.tick`: `ServerboundPaddleBoatPacket`). A minecart moves on
//! the server with the rider's input, and the rider sits in it.
use super::{Game, Keys, ray_box};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_protocol::packets::play::{self, Serverbound as S};

/// An object under the crosshair: its entity id, how far along the view it
/// was hit, and where (`EntityHitResult.getLocation` minus the entity's
/// position, the interact packet's location).
pub(super) struct ObjectHit {
    pub id: i32,
    pub distance: f64,
    pub location: [f64; 3],
}

impl Game<'_> {
    /// `Entity.positionRider` for a display or interaction entity riding `vehicle`, `partial` of the
    /// way through the tick: the vehicle's passenger attachment, on top of it (EntityAttachments'
    /// default); a display rides another display at that one's place.
    pub(super) fn mount_point(&self, vehicle: i32, partial: f64) -> Option<Vec3> {
        let r = self.remote.as_ref()?;
        let lerp = |a: Vec3, b: Vec3| {
            Vec3::new(
                a.x + (b.x - a.x) * partial,
                a.y + (b.y - a.y) * partial,
                a.z + (b.z - a.z) * partial,
            )
        };
        let up = |p: Vec3, h: f64| Vec3::new(p.x, p.y + h, p.z);
        let mut vehicle = vehicle;
        // A model's parts may ride one another; a loop ends.
        for _ in 0..8 {
            if vehicle == r.entity_id {
                let p = self.player.as_ref()?;
                let height = f64::from(p.pose.dimensions(1.0).height);
                return Some(up(lerp(self.previous_position, p.position), height));
            }
            if let Some(m) = self.mobs.iter().find(|m| m.id as i32 == vehicle) {
                return Some(up(lerp(m.previous, m.position), f64::from(m.height())));
            }
            if let Some(a) = self.armor_stands.iter().find(|a| a.id as i32 == vehicle) {
                let height = if a.marker {
                    0.0
                } else if a.small {
                    0.9875
                } else {
                    1.975
                };
                return Some(up(a.position, height));
            }
            match r.mounts.get(&vehicle) {
                Some(&next) => vehicle = next,
                None => {
                    let t = r.entities.get(&vehicle)?;
                    return Some(Vec3::new(t.position[0], t.position[1], t.position[2]));
                }
            }
        }
        None
    }

    /// Where display or interaction entity `id` is: on its vehicle when it rides one.
    pub(super) fn ridden_position(&self, id: i32, partial: f64) -> Option<Vec3> {
        let vehicle = *self.remote.as_ref()?.mounts.get(&id)?;
        self.mount_point(vehicle, partial)
    }

    /// The nearest interaction entity the view hits within the player's entity interaction range:
    /// its id and how far along the view.
    pub(super) fn interaction_hit(&self) -> Option<(u32, f64)> {
        let eye = self.eye_position()?;
        let dir = view_vector(self.yaw, self.pitch);
        let reach = if self.creative { 5.0 } else { 3.0 };
        self.interactions
            .iter()
            .filter_map(|(id, i)| {
                let mut i = super::remote_objects::Interaction { ..*i };
                if let Some(at) = self.ridden_position(*id as i32, 1.0) {
                    i.position = at;
                }
                Some((*id, ray_box(eye, dir, reach, i.bounds())?))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
    /// The nearest boat, minecart, item frame, armor stand or painting the
    /// view hits within reach; also End crystals when attacking.
    pub(super) fn remote_object_hit(&self, attacking: bool) -> Option<ObjectHit> {
        let (eye, dir) = self.aim()?;
        let at = |d: f64| {
            [
                eye[0] + dir[0] * d,
                eye[1] + dir[1] * d,
                eye[2] + dir[2] * d,
            ]
        };
        let block_centre = |p: [i32; 3]| {
            [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + 0.5,
                f64::from(p[2]) + 0.5,
            ]
        };
        let mut hits: Vec<(i32, f64, [f64; 3])> = Vec::new();
        if let Some((i, d)) = self.painting_hit() {
            let p = &self.paintings[i];
            hits.push((p.id as i32, d, block_centre(p.pos)));
        }
        if let Some((i, d)) = self.frame_hit() {
            let f = &self.item_frames[i];
            hits.push((f.id as i32, d, block_centre(f.pos)));
        }
        if let Some((i, d, _)) = self.armor_stand_hit() {
            let a = &self.armor_stands[i];
            let p = a.position;
            hits.push((a.id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((i, d, _)) = self.cushion_hit() {
            let c = &self.cushions[i];
            let p = c.position;
            hits.push((c.id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((i, d)) = self.boat_hit() {
            let p = self.boats[i].position;
            hits.push((self.boats[i].id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((i, d)) = self.cart_hit() {
            let p = self.carts[i].position;
            hits.push((self.carts[i].id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((id, d)) = self.interaction_hit() {
            let p = self.interactions[&id].position;
            hits.push((id as i32, d, [p.x, p.y, p.z]));
        }
        if attacking {
            // EndCrystal.isPickable: its 2 x 2 box, with the server's entity
            // id, not special_targets' local projectile target id.
            // Entity.interact passes on a crystal, so right clicks use the item.
            let reach = if self.creative { 5.0 } else { 3.0 };
            for c in self.crystals.iter().filter(|c| c.id != 0) {
                let p = c.position;
                let bounds = Aabb::new(
                    Vec3::new(p.x - 1.0, p.y, p.z - 1.0),
                    Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0),
                );
                if let Some(d) = ray_box(eye, dir, reach, bounds) {
                    hits.push((c.id as i32, d, [p.x, p.y, p.z]));
                }
            }
            // EnderDragonPart.isPickable: the server's dragon hurts by part.
            hits.extend(self.remote_dragon_part_hit(eye, dir, reach));
        }
        let (id, distance, origin) = hits.into_iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
        let point = at(distance);
        Some(ObjectHit {
            id,
            distance,
            location: [
                point[0] - origin[0],
                point[1] - origin[1],
                point[2] - origin[2],
            ],
        })
    }

    /// `ClientPacketListener.handleSetEntityPassengersPacket`: who rides
    /// `vehicle` now. The local player gets in or out; mobs and other
    /// players sit where the server keeps them.
    pub(super) fn remote_set_passengers(&mut self, vehicle: i32, passengers: &[i32]) {
        let Some(me) = self.remote.as_ref().map(|r| r.entity_id) else {
            return;
        };
        let vehicle_id = vehicle as u32;
        for m in &mut self.mobs {
            if m.extra.mount == Some(vehicle_id) && !passengers.contains(&(m.id as i32)) {
                m.extra.mount = None;
                m.extra.passenger = false;
            } else if passengers.contains(&(m.id as i32)) {
                m.extra.mount = Some(vehicle_id);
                m.extra.passenger = true;
            }
        }
        // Displays and interaction boxes ride along (`mount_point`).
        let mobs = &self.mobs;
        if let Some(r) = self.remote.as_mut() {
            r.mounts.retain(|_, v| *v != vehicle);
            for &p in passengers {
                if p != me && !mobs.iter().any(|m| m.id as i32 == p) {
                    r.mounts.insert(p, vehicle);
                }
            }
        }
        if passengers.contains(&me) {
            if self.riding != Some(vehicle_id) {
                self.riding = Some(vehicle_id);
                if let Some(p) = self.player.as_mut() {
                    p.velocity = Vec3::ZERO;
                    p.fall_distance = 0.0;
                }
            }
        } else if self.riding == Some(vehicle_id) {
            // The server takes the player out, then teleports it.
            self.riding = None;
            // HappyGhast.removePassenger: a still timeout of 10 (the server's
            // STAYS_STILL confirms it), and the rider lands on top of the ghast
            // (`place_ghast_under`).
            if let Some(m) = self.mobs.iter_mut().find(|m| {
                m.id == vehicle_id && m.species() == horizon_client::mobs::Species::HappyGhast
            }) {
                m.extra.counter = 10;
                if let Some(r) = self.remote.as_mut() {
                    r.ghast_dismount = Some(vehicle_id);
                }
            }
        }
    }

    /// The local player's tick in its vehicle, in place of its own movement:
    /// false when it rides nothing.
    pub(super) fn remote_ride(&mut self, keys: Keys) -> bool {
        let Some(id) = self.riding else {
            return false;
        };
        if let Some(i) = self.mobs.iter().position(|m| m.id == id) {
            // An animal moves on the server with the rider's input
            // (PlayerInput); here the jump bar charges for the HUD and the
            // rider sits where `positionRider` puts it.
            self.charge_mount_jump(i, keys);
            self.position_rider();
            return true;
        }
        let seat = if let Some(c) = self.cushions.iter().find(|c| c.id == id) {
            // The seat does not move; the rider sits on the cushion.
            Some(c.seat())
        } else if let Some(i) = self.boats.iter().position(|b| b.id == id) {
            // AbstractBoat.tick on the controlling client: steer and move.
            let mut b = self.boats[i].clone();
            b.input = [keys.left, keys.right, keys.forward, keys.backward];
            let b = self.tick_boat(b, true);
            // positionRider: the seat turns with the boat; clampRotation.
            self.yaw += b.delta_rotation;
            let delta = (self.yaw - b.yaw + 180.0).rem_euclid(360.0) - 180.0;
            self.yaw += delta.clamp(-105.0, 105.0) - delta;
            self.boats[i] = b;
            // In front of a mob that shares the boat.
            Some(self.player_boat_seat(i))
        } else {
            // A minecart: the server moves it, the rider sits in it.
            self.carts.iter().find(|c| c.id == id).map(|c| {
                c.position
                    .add_vector(Vec3::new(0.0, super::minecarts::SEAT, 0.0))
            })
        };
        if let Some(p) = self.player.as_mut() {
            self.previous_position = p.position;
            if let Some(v) = seat {
                p.position = v;
            }
            p.velocity = Vec3::ZERO;
            p.fall_distance = 0.0;
        }
        true
    }

    /// `LocalPlayer.tick` for a passenger: the boat it controls goes to the
    /// server every tick with its paddles (the rider's own position does not).
    pub(super) fn remote_vehicle_packets(&self) -> Vec<S> {
        let Some(b) = self
            .riding
            .and_then(|id| self.boats.iter().find(|b| b.id == id))
        else {
            return Vec::new();
        };
        vec![
            S::MoveVehicle(play::MoveVehicleRequest {
                position: [b.position.x, b.position.y, b.position.z],
                yaw: b.yaw,
                pitch: 0.0,
                on_ground: b.on_ground,
            }),
            S::PaddleBoat(play::PaddleBoat {
                left: b.paddling[0],
                right: b.paddling[1],
            }),
        ]
    }

    /// `ClientPacketListener.handleMoveVehicle`: the server put the boat the
    /// player drives back.
    pub(super) fn remote_move_vehicle(&mut self, position: [f64; 3], yaw: f32) {
        let Some(b) = self
            .riding
            .and_then(|id| self.boats.iter_mut().find(|b| b.id == id))
        else {
            return;
        };
        let at = Vec3::new(position[0], position[1], position[2]);
        b.previous = at;
        b.position = at;
        b.yaw = yaw;
        b.previous_yaw = yaw;
        b.velocity = Vec3::ZERO;
    }
}
