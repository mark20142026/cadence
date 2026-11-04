//! The spider's jump: `PlayerObject::spiderTestJumpInternal` (m1 0x375de4), classic case (not
//! sideways, not platformer, not going left).
//!
//! A press on the ground (or a spider orb/pad) teleports the player to the nearest surface on
//! the other side: solids, slopes and breakable blocks in a tall rect from the player's center
//! toward the opposite gravity (`staticObjectsInRect`), sorted by distance; a hazard in a narrow
//! rect (`damagingObjectsInRect`) before that surface stops the jump at the hazard. Without a
//! surface the jump goes to the corridor edge. Then gravity flips and the velocity is ±1 toward
//! the new ground.

use crate::objects::ObjectType;
use crate::physics::{round_velocity, Event, Mode, Player};
use crate::world::{Rect, World};

impl Player {
    /// `spiderTestJumpInternal(retry)`. `retry` widens the surface search to the full width
    /// (+4) after a hazard was met first.
    pub(crate) fn spider_jump(&mut self, world: &World, retry: bool) {
        if self.dead {
            return;
        }
        let up = self.upside_down;
        // Sideways the game runs the same search with the axes swapped (its "up" axis is x):
        // work in the transposed frame (an involution, so it also maps back).
        let sw = self.sideways;
        let t = |r: Rect| if sw { Rect { x: r.y, y: r.x, w: r.h, h: r.w } } else { r };
        // bVar24: a non-spider mode using a spider orb or pad searches ±3000 around itself.
        let free = !matches!(self.mode, Mode::Ship | Mode::Ufo | Mode::Wave | Mode::Ball | Mode::Spider | Mode::Swing);
        let size = self.size;
        let p = t(self.rect());
        let (min_y, max_y) = if sw {
            // Sideways: ±3000 along x, floored at 0.
            let lo = if self.x + -3000.0 <= 0.0 { 0.0 } else { self.x + -3000.0 };
            (lo, self.x + 3000.0)
        } else if free {
            let lo = if 90.0 < self.y + -3000.0 { self.y + -3000.0 } else { 90.0 };
            (lo, self.y + 3000.0)
        } else {
            self.portal_y_limits()
        };
        // The search rect toward the opposite gravity.
        let mut c = p;
        if up {
            c.h = (c.y - min_y) + 2.0;
            c.y = min_y;
        } else {
            let t = c.y + c.h + -2.0;
            c.y = t;
            c.h = max_y - t;
        }
        // Hazards: a narrow column (8·size wide) in the middle; the retry rect is 4 wider.
        let mut wide = c;
        wide.w += 4.0;
        let mut narrow = c;
        narrow.w = size * 8.0;
        narrow.x += (p.w - size * 8.0) * 0.5;
        // Surfaces: from the player's center forward (half the width + 1); going left, the
        // strip is [left − half − 1, left] (as the game computes it).
        let half_w = p.w * 0.5;
        c.w = half_w + 1.0;
        c.x = if self.going_left { (p.x - half_w) + -1.0 } else { half_w + p.x };

        let mut statics = self.static_objects_in(world, &t(c));
        let hazards = self.damaging_objects_in(world, &t(narrow));
        let mut widened = false;
        if (statics.is_empty() && !hazards.is_empty()) || retry {
            statics = self.static_objects_in(world, &t(wide));
            widened = true;
        }
        let key = |i: &usize| -> i32 {
            let r = t(world.objects[*i].rect);
            if up { -((r.max_y() * 10.0) as i32) } else { (r.y * 10.0) as i32 }
        };
        statics.sort_by_key(key);
        let mut hazards = hazards;
        hazards.sort_by_key(key);

        if std::env::var_os("CADENCE_DEBUG_SPIDER").is_some() {
            let d = |v: &Vec<usize>| v.iter().map(|&i| (world.objects[i].order, world.objects[i].id, world.objects[i].rect)).collect::<Vec<_>>();
            eprintln!("spider jump at ({}, {}) retry {retry} widened {widened} search {:?} statics {:?} hazards {:?}", self.x, self.y, c, d(&statics), d(&hazards));
        }
        let mut target;
        let (px, py) = if sw { (self.y, self.x) } else { (self.x, self.y) };
        // The hazard's position along the up axis.
        let hazard_y = |h: usize| if sw { world.objects[h].x } else { world.objects[h].y };
        if !statics.is_empty() {
            target = 0.0;
            'candidates: for (n, &i) in statics.iter().enumerate() {
                let o = &world.objects[i];
                let r = t(o.rect);
                let mut cand = if up { r.max_y() + p.h * 0.5 } else { r.y + p.h * -0.5 };
                let mut slope_face = false;
                // (Sideways the game turns slopes about the player first; not ported: they
                // count as their rects.)
                if o.kind == ObjectType::SLOPE && !sw {
                    let floor_top = crate::world::slope_floor_top(o.slope_dir);
                    if up != floor_top {
                        // The slope's surface faces the player: land on it at the player's x.
                        slope_face = true;
                        let s = crate::slopes::slope_y_at(&r, o.slope_uphill, o.slope_hazard, floor_top, px);
                        let d: f32 = if up { 1.0 } else { -1.0 };
                        cand = o.y + (s - o.y as f64) as f32 + d * p.h * 0.5;
                    }
                }
                let behind = if up { py + 10.0 < cand } else { cand < py + -10.0 };
                if behind {
                    continue;
                }
                // Starting behind the player's center (going left: ending ahead of it) lands.
                if if self.going_left { px <= r.max_x() } else { r.x <= px } {
                    target = cand;
                    break;
                }
                // Starting right of the player's center: only if the player at the target
                // doesn't overlap another of the surfaces (shrunk 2 vertically).
                for (m, &j) in statics.iter().enumerate() {
                    if m == n {
                        continue;
                    }
                    let mut at = p;
                    at.y = cand + p.h * -0.5;
                    let mut other = t(world.objects[j].rect);
                    if !slope_face {
                        other.h += -4.0;
                        other.y += 2.0;
                    }
                    if other.intersects(&at) {
                        if slope_face {
                            // (The platformer keeps the nearest; classic falls back below.)
                            continue 'candidates;
                        }
                        continue 'candidates;
                    }
                }
                target = cand;
                break;
            }
            if target == 0.0 {
                target = if up { min_y + p.h * 0.5 } else { max_y - p.h * 0.5 };
            }
        } else if let Some(&h) = hazards.first() {
            target = hazard_y(h);
        } else {
            target = if up { min_y + p.h * 0.5 } else { max_y - p.h * 0.5 };
        }
        if !statics.is_empty() || hazards.is_empty() {
            if let Some(&h) = hazards.first() {
                let hy = hazard_y(h);
                let before = if up { hy <= target } else { target <= hy };
                if !before {
                    target = hy;
                    if !widened {
                        self.spider_jump(world, true);
                        return;
                    }
                }
            }
        }
        let from = (self.x, self.y);
        if sw {
            self.x = target;
        } else {
            self.y = target;
        }
        self.events.push(Event::SpiderDash { from, to: (self.x, self.y) });
        self.flip_gravity_pub(!up);
        self.y_velocity = round_velocity(if self.upside_down { 1.0 } else { -1.0 });
        self.jump_buffered = false;
        self.last_spider_flip_time = self.total_time;
    }

    /// `getMinPortalY` / `getMaxPortalY`: the corridor's edges (1 beyond them), else ±3000.
    fn portal_y_limits(&self) -> (f32, f32) {
        // From the camera when it's being stepped (it holds the corridor and the ground).
        if self.camera.fly_active_pub() || self.camera.static_y_on() {
            return self.camera.portal_limits();
        }
        match self.corridor {
            Some((floor, ceiling)) => (floor, ceiling),
            None => {
                let lo = if 90.0 < self.y + -3000.0 { self.y + -3000.0 } else { 90.0 };
                (lo, self.y + 3000.0)
            }
        }
    }

    /// `staticObjectsInRect`: solids, slopes and breakable blocks touching `r` (enabled ones).
    fn static_objects_in(&self, world: &World, r: &Rect) -> Vec<usize> {
        world
            .near_indexed(r.x, r.max_x())
            .filter(|(i, o)| {
                (o.kind == ObjectType::SOLID
                    || o.kind == ObjectType::SLOPE
                    || (o.kind == ObjectType::BREAKABLE && !self.destroyed.contains(i)))
                    && r.intersects(&o.rect)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// `damagingObjectsInRect`: hazards touching `r` (circle for round ones, both oriented-box
    /// tests for rotated ones).
    fn damaging_objects_in(&self, world: &World, r: &Rect) -> Vec<usize> {
        let reach = world.max_radius();
        world
            .near_indexed(r.x - reach, r.max_x() + reach)
            .filter(|(_, o)| {
                if o.kind != ObjectType::HAZARD {
                    return false;
                }
                if o.radius > 0.0 {
                    crate::physics::rect_hits_circle_pub(r, o.x, o.y, o.radius)
                } else if let Some(obb) = &o.obb {
                    let mine = crate::world::oriented_box(r.x + r.w * 0.5, r.y + r.h * 0.5, r.w, r.h, 0.0);
                    r.intersects(&o.rect) && crate::world::obb_overlaps(&mine, obb)
                } else {
                    r.intersects(&o.rect)
                }
            })
            .map(|(i, _)| i)
            .collect()
    }
}
