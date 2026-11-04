//! The player's regular trail: cocos2d's `CCMotionStreak` (fast mode) as the game sets it up
//! (`PlayerObject::setupStreak` 0x3677b8, trail 1: `streak_01_001.png`, stroke 10, fade 0.3 s,
//! min segment 5; player color 2, additive). A real dump (Stereo Madness, ship) matches: one point
//! a frame once it moved more than 5 units, each fading by 1/0.3 a second, alpha = state·255.

/// A trail point: position and remaining life (1 → 0).
#[derive(Clone, Copy, Debug)]
struct Point {
    pos: (f32, f32),
    state: f32,
}

#[derive(Clone, Debug)]
pub struct MotionStreak {
    points: Vec<Point>,
    /// Polygon edge vertices, two per point (`ccVertexLineToPolygon`).
    vertices: Vec<(f32, f32)>,
    pub stroke: f32,
    fade_delta: f32,
    min_seg_sq: f32,
    max_points: usize,
    /// Adding points (`resumeStroke` / `stopStroke`); old points keep fading either way.
    pub active: bool,
}

impl MotionStreak {
    /// `initWithFade(fade, minSeg, stroke, ...)`.
    pub fn new(fade: f32, min_seg: f32, stroke: f32) -> MotionStreak {
        let min_seg = if min_seg == -1.0 { stroke / 5.0 } else { min_seg };
        MotionStreak {
            points: Vec::new(),
            vertices: Vec::new(),
            stroke,
            fade_delta: 1.0 / fade,
            min_seg_sq: min_seg * min_seg,
            max_points: (fade * 60.0) as usize + 2,
            active: false,
        }
    }

    pub fn reset(&mut self) {
        self.points.clear();
        self.vertices.clear();
    }

    /// `CCMotionStreak::update(dt)` with the node at `pos`.
    pub fn update(&mut self, dt: f32, pos: (f32, f32)) {
        let delta = dt * self.fade_delta;
        let mut i = 0;
        while i < self.points.len() {
            self.points[i].state -= delta;
            if self.points[i].state <= 0.0 {
                self.points.remove(i);
                self.vertices.drain(2 * i..(2 * i + 2).min(self.vertices.len()));
            } else {
                i += 1;
            }
        }
        if !self.active {
            return;
        }
        let n = self.points.len();
        let dist_sq = |a: (f32, f32)| (a.0 - pos.0).powi(2) + (a.1 - pos.1).powi(2);
        let append = n < self.max_points
            && !(n > 0
                && (dist_sq(self.points[n - 1].pos) < self.min_seg_sq
                    || (n > 1 && dist_sq(self.points[n - 2].pos) < self.min_seg_sq * 2.0)));
        if append {
            self.points.push(Point { pos, state: 1.0 });
            self.rebuild();
        }
    }

    /// `ccVertexLineToPolygon` over all points: each point's two edge vertices, offset by half the
    /// stroke along the perpendicular of the neighbouring segments (mitered).
    fn rebuild(&mut self) {
        let n = self.points.len();
        self.vertices.clear();
        if n < 2 {
            return;
        }
        let half = self.stroke * 0.5;
        let p = |i: usize| self.points[i].pos;
        let norm = |v: (f32, f32)| {
            let l = (v.0 * v.0 + v.1 * v.1).sqrt().max(1e-6);
            (v.0 / l, v.1 / l)
        };
        for i in 0..n {
            let perp = if i == 0 {
                let d = norm((p(1).0 - p(0).0, p(1).1 - p(0).1));
                (-d.1, d.0)
            } else if i == n - 1 {
                let d = norm((p(i).0 - p(i - 1).0, p(i).1 - p(i - 1).1));
                (-d.1, d.0)
            } else {
                let a = norm((p(i).0 - p(i - 1).0, p(i).1 - p(i - 1).1));
                let b = norm((p(i + 1).0 - p(i).0, p(i + 1).1 - p(i).1));
                let pa = (-a.1, a.0);
                let pb = (-b.1, b.0);
                let m = norm((pa.0 + pb.0, pa.1 + pb.1));
                // Miter: lengthen by 1/cos of half the turn (cocos2d clamps nothing here).
                let cos = (m.0 * pa.0 + m.1 * pa.1).max(0.1);
                (m.0 / cos, m.1 / cos)
            };
            let q = p(i);
            self.vertices.push((q.0 - perp.0 * half, q.1 - perp.1 * half));
            self.vertices.push((q.0 + perp.0 * half, q.1 + perp.1 * half));
        }
    }

    /// The band as triangles: (corners, texture coordinates across 0..1 and along i/n, alpha).
    pub fn triangles(&self) -> impl Iterator<Item = ([(f32, f32); 3], [(f32, f32); 3], f32)> + '_ {
        let n = self.points.len();
        let along = move |i: usize| if n > 0 { i as f32 / n as f32 } else { 0.0 };
        (0..n.saturating_sub(1)).filter(move |&i| 2 * i + 3 < self.vertices.len()).flat_map(move |i| {
            let v = &self.vertices;
            let alpha = (self.points[i].state + self.points[i + 1].state) * 0.5;
            let (t0, t1) = (along(i), along(i + 1));
            [
                ([v[2 * i], v[2 * i + 1], v[2 * i + 2]], [(0.0, t0), (1.0, t0), (0.0, t1)], alpha),
                ([v[2 * i + 1], v[2 * i + 3], v[2 * i + 2]], [(1.0, t0), (1.0, t1), (0.0, t1)], alpha),
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_fade_and_space_like_the_dump() {
        // Ship at ~41.5 units a frame: one point a frame, alpha state·255 = 255, 141, 28 for
        // ages 0, 2, 4 frames at 0.3 s fade... the dump showed 1.0, 0.5556, 0.1111.
        let mut s = MotionStreak::new(0.3, 5.0, 10.0);
        s.active = true;
        for f in 0..5 {
            s.update(1.0 / 60.0, (f as f32 * 41.5, 0.0));
        }
        let states: Vec<f32> = s.points.iter().map(|p| p.state).collect();
        assert_eq!(states.len(), 5);
        assert!((states[4] - 1.0).abs() < 1e-6);
        assert!((states[2] - (1.0 - 2.0 / 18.0)).abs() < 1e-4);
    }
}

/// The wave's trail (`m_waveTrail`, a `HardStreak` draw node; `updateStroke` 0x5d1dc): a band
/// through the points where the wave turned and its current position, mitered at the turns,
/// cut square (perpendicular, `quadCornerOffset` 0x5dfcc) at both ends. Width 6 × size × pulse
/// (pulse = 0.4 + (audio − 0.1)·2.1, `PlayerObject::update` 0x36875c), drawn twice additively:
/// player color 1, then white at alpha 165 a third as wide. Matches a real dump (level 17).
#[derive(Clone, Debug, Default)]
pub struct WaveTrail {
    points: Vec<(f32, f32)>,
    /// The direction of the last step (−1, 0, 1) and where the player was before it.
    dir: i8,
    last: Option<(f32, f32)>,
}

impl WaveTrail {
    pub fn clear(&mut self) {
        self.points.clear();
        self.last = None;
        self.dir = 0;
    }

    /// One step with the player at `pos`: a turn adds the point it turned at. Points far
    /// behind `cull_x` are dropped.
    pub fn step(&mut self, pos: (f32, f32), cull_x: f32) {
        match self.last {
            None => self.points.push(pos),
            Some(last) => {
                let dy = pos.1 - last.1;
                let dir = if dy > 1e-3 { 1 } else if dy < -1e-3 { -1 } else { 0 };
                if dir != self.dir && self.points.last() != Some(&last) {
                    self.points.push(last);
                }
                self.dir = dir;
            }
        }
        self.last = Some(pos);
        while self.points.len() > 2 && self.points[1].0 < cull_x {
            self.points.remove(0);
        }
    }

    /// Triangles of the band of full `width` through the points and `head`.
    pub fn band(&self, head: (f32, f32), width: f32) -> Vec<[(f32, f32); 3]> {
        let mut p: Vec<(f32, f32)> = self.points.clone();
        if p.last().is_none_or(|l| (l.0 - head.0).powi(2) + (l.1 - head.1).powi(2) > 0.01) {
            p.push(head);
        }
        let n = p.len();
        if n < 2 {
            return Vec::new();
        }
        let half = width * 0.5;
        let unit = |a: (f32, f32), b: (f32, f32)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            (dx / l, dy / l)
        };
        let offsets: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let normal = |d: (f32, f32)| (-d.1, d.0);
                if i == 0 || i == n - 1 {
                    let d = if i == 0 { unit(p[0], p[1]) } else { unit(p[n - 2], p[n - 1]) };
                    let nn = normal(d);
                    (nn.0 * half, nn.1 * half)
                } else {
                    let (a, b) = (normal(unit(p[i - 1], p[i])), normal(unit(p[i], p[i + 1])));
                    let (mx, my) = (a.0 + b.0, a.1 + b.1);
                    let l = (mx * mx + my * my).sqrt().max(1e-6);
                    let m = (mx / l, my / l);
                    let cos = (m.0 * a.0 + m.1 * a.1).max(0.2);
                    (m.0 * half / cos, m.1 * half / cos)
                }
            })
            .collect();
        let mut tris = Vec::with_capacity(2 * (n - 1));
        for i in 0..n - 1 {
            let (a0, a1) = ((p[i].0 + offsets[i].0, p[i].1 + offsets[i].1), (p[i].0 - offsets[i].0, p[i].1 - offsets[i].1));
            let (b0, b1) = (
                (p[i + 1].0 + offsets[i + 1].0, p[i + 1].1 + offsets[i + 1].1),
                (p[i + 1].0 - offsets[i + 1].0, p[i + 1].1 - offsets[i + 1].1),
            );
            tris.push([a0, a1, b0]);
            tris.push([a1, b1, b0]);
        }
        tris
    }
}
