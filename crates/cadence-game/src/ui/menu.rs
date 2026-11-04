//! The main menu: `MenuLayer::init` (0x3066e0) over `MenuGameLayer` (0x4557c4): the scrolling
//! background and ground whose color cycles every 5 s, a cube running and jumping at random,
//! the logo, and the menu buttons.

use cadence_core::level::LevelString;
use cadence_core::physics::Player;
use cadence_core::world::World;

use super::canvas::{Canvas, Look};
use super::{Btn, Ctx, Ui};
use crate::render::Blend;

/// `GameManager::colorForIdx` (0x2f6528): the icon color palette, by index (107 and up: white).
pub const PALETTE: [[u8; 3]; 107] = [
    [125, 255, 0], [0, 255, 0], [0, 255, 125], [0, 255, 255], [0, 125, 255], [0, 0, 255], [125, 0, 255], [255, 0, 255], [255, 0, 125], [255, 0, 0],
    [255, 125, 0], [255, 255, 0], [255, 255, 255], [185, 0, 255], [255, 185, 0], [0, 0, 0], [0, 200, 255], [175, 175, 175], [90, 90, 90], [255, 125, 125],
    [0, 175, 75], [0, 125, 125], [0, 75, 175], [75, 0, 175], [125, 0, 125], [175, 0, 75], [175, 75, 0], [125, 125, 0], [75, 175, 0], [255, 75, 0],
    [150, 50, 0], [150, 100, 0], [100, 150, 0], [0, 150, 100], [0, 100, 150], [100, 0, 150], [150, 0, 100], [150, 0, 0], [0, 150, 0], [0, 0, 150],
    [125, 255, 175], [125, 125, 255], [255, 250, 127], [250, 127, 255], [0, 255, 192], [80, 50, 14], [205, 165, 118], [182, 128, 255], [255, 58, 58], [77, 77, 143],
    [0, 10, 76], [253, 212, 206], [190, 181, 255], [112, 0, 0], [82, 2, 0], [56, 1, 6], [128, 79, 79], [122, 53, 53], [81, 36, 36], [163, 98, 70],
    [117, 73, 54], [86, 53, 40], [255, 185, 114], [255, 160, 64], [102, 49, 30], [91, 39, 0], [71, 32, 0], [167, 123, 77], [109, 83, 57], [81, 62, 42],
    [255, 255, 192], [253, 224, 160], [192, 255, 160], [177, 255, 109], [192, 255, 224], [148, 255, 228], [67, 161, 138], [49, 109, 95], [38, 84, 73], [0, 96, 0],
    [0, 64, 0], [0, 96, 96], [0, 64, 64], [160, 255, 255], [1, 7, 112], [0, 73, 109], [0, 50, 76], [0, 38, 56], [80, 128, 173], [51, 83, 117],
    [35, 60, 86], [224, 224, 224], [61, 6, 140], [55, 8, 96], [64, 64, 64], [111, 73, 164], [84, 54, 127], [66, 42, 99], [252, 181, 255], [175, 87, 175],
    [130, 67, 130], [94, 49, 94], [128, 128, 128], [102, 3, 62], [71, 1, 52], [210, 255, 50], [118, 189, 255],
];

pub fn color_for_idx(i: usize) -> [u8; 3] {
    PALETTE.get(i).copied().unwrap_or([255, 255, 255])
}

/// `MenuGameLayer::getBGColor` / `updateColor`: the background's color cycle.
fn bg_color(i: u32) -> [u8; 3] {
    match i % 8 {
        0 => [0, 102, 255],
        1 => color_for_idx(7),
        2 => color_for_idx(8),
        3 => color_for_idx(9),
        4 => color_for_idx(10),
        5 => color_for_idx(11),
        6 => color_for_idx(1),
        _ => color_for_idx(3),
    }
}

/// A `CCTintTo`: from the color at its start to `to` over `duration` seconds, linearly.
#[derive(Clone, Copy, Debug)]
struct Tint {
    from: [f32; 3],
    to: [f32; 3],
    t: f32,
    duration: f32,
}

impl Tint {
    fn new(c: [f32; 3]) -> Tint {
        Tint { from: c, to: c, t: 0.0, duration: 0.0 }
    }
    fn now(&self) -> [f32; 3] {
        let k = if self.duration > 0.0 { (self.t / self.duration).min(1.0) } else { 1.0 };
        [0, 1, 2].map(|i| self.from[i] + (self.to[i] - self.from[i]) * k)
    }
    fn to(&mut self, c: [f32; 3], duration: f32) {
        *self = Tint { from: self.now(), to: c, t: 0.0, duration };
    }
}

fn bytes(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| v as f32)
}

pub struct Menu {
    /// The ground's scroll (`+0x1a4`): 5.770002 · 0.9 units per 1/60 s.
    cam_x: f32,
    color_index: u32,
    color_timer: f32,
    bg: Tint,
    ground: Tint,
    /// The menu's player on an empty level, and its screen x offset from its world x.
    world: World,
    player: Player,
    offset: f32,
    colors: ([u8; 3], [u8; 3]),
    jump_timer: f32,
    step_accumulator: f32,
}

impl Default for Menu {
    fn default() -> Menu {
        let mut world = World::from_level(&LevelString::parse(""));
        world.level_length = 0.0;
        let player = Player::spawn(&world);
        // MenuGameLayer::init: background (0, 102, 255), then updateColor at once.
        let start = bytes([0, 102, 255]);
        let mut m = Menu {
            cam_x: 0.0,
            color_index: 0,
            color_timer: 0.0,
            bg: Tint::new(start),
            ground: Tint::new(start),
            world,
            player,
            offset: 0.0,
            colors: ([125, 255, 0], [0, 255, 255]),
            jump_timer: 0.0,
            step_accumulator: 0.0,
        };
        m.next_color();
        m
    }
}

impl Menu {
    /// `MenuGameLayer::updateColor` (every 5 s): tint the background to the next color over 4 s,
    /// the ground to 0.8 of it.
    fn next_color(&mut self) {
        let c = bg_color(self.color_index);
        self.color_index += 1;
        self.bg.to(bytes(c), 4.0);
        self.ground.to(c.map(|v| (0.8 * v as f32) as i32 as f32), 4.0);
    }

    /// `MenuGameLayer::resetPlayer`: a new run from off-screen left, random colors and speed.
    /// (The game also picks a random vehicle and icon; Cadence has only icon 1 of each loaded
    /// and runs the cube.)
    pub fn reset_player(&mut self, rng: &mut u64) {
        let mut r = || Ui::random(rng);
        let start_x = r() * 5.0 * -100.0 - 100.0;
        let c1 = color_for_idx((r() * 108.0) as usize);
        let c2 = color_for_idx((r() * 108.0) as usize);
        self.colors = (c1, c2);
        let v = r();
        let speed = if v < 0.2 {
            1.3
        } else if v < 0.4 {
            1.1
        } else if v < 0.6 {
            0.7
        } else if v < 0.65 {
            1.6
        } else {
            0.9
        };
        self.player = Player::spawn(&self.world);
        self.player.set_speed(speed);
        self.offset = start_x - self.player.x;
    }

    pub fn update(&mut self, dt: f32, view: (f32, f32), rng: &mut u64) {
        self.bg.t += dt;
        self.ground.t += dt;
        self.color_timer += dt;
        if self.color_timer >= 5.0 {
            self.color_timer -= 5.0;
            self.next_color();
        }
        // MenuGameLayer::update: the scroll, at 1x speed.
        self.cam_x += dt * 60.0 * 5.770002 * 0.9;
        // tryJump every 0.25 s: the cube presses 20% of the time when on the ground.
        self.jump_timer += dt;
        while self.jump_timer >= 0.25 {
            self.jump_timer -= 0.25;
            let press = Ui::random(rng) <= 0.2 && self.player.on_ground;
            self.player.set_button(press, &self.world);
        }
        // The player runs the real physics on an empty level (240 steps a second).
        self.step_accumulator += dt;
        loop {
            // each step stands for its share of a 1/60 s frame, like the level's loop
            let step = self.world.real_step_secs() as f32;
            if self.step_accumulator < step {
                break;
            }
            self.step_accumulator -= step;
            self.player.begin_step(&self.world);
            self.world.pre_step((self.player.x, self.player.y));
            self.player.step(&self.world);
            self.world.post_step(&self.player);
        }
        if self.player.x + self.offset > view.0 + 100.0 {
            self.reset_player(rng);
        }
    }

    /// The background and ground (`MenuGameLayer`), shared with nothing else.
    fn draw_game(&self, c: &mut Canvas) {
        let (w, _) = (c.width, c.height);
        let bg = self.bg.now().map(|v| v.trunc() / 255.0);
        // The background: scale like the level's (view width / 480), at -0.1 × the scroll.
        let (bw, _) = c.size("game_bg_01_001.png");
        let s = c.width / 480.0;
        if bw > 0.0 {
            let tile = bw * s;
            let mut x = -((self.cam_x * 0.1).rem_euclid(tile));
            while x < w {
                c.sprite("game_bg_01_001.png", x, 0.0, Look { color: bg, blend: Blend::Opaque, ..Look::scaled(s).anchor(0.0, 0.0) });
                x += tile;
            }
        }
        // The player (z 60, under the ground at z 100).
        self.draw_player(c);
        // GJGroundLayer at y 90: tiles, the floor line, the edge shadows.
        let gy = 90.0;
        let g = self.ground.now().map(|v| v.trunc() / 255.0);
        let (tw, _) = c.size("groundSquare_01_001.png");
        if tw > 0.0 {
            let mut x = -(self.cam_x.rem_euclid(tw));
            while x < w {
                c.sprite("groundSquare_01_001.png", x, gy, Look { color: g, ..Look::default().anchor(0.0, 1.0) });
                x += tw;
            }
        }
        c.sprite("floorLine_01_001.png", w * 0.5, gy + 0.5, Look::default().anchor(0.5, 1.0).blend(Blend::Additive));
        let shade = Look::default().anchor(0.0, 1.0).scale_xy(0.7, 1.0).alpha(100).blend(Blend::Shade);
        c.sprite("groundSquareShadow_001.png", -1.0, gy, shade);
        c.sprite("groundSquareShadow_001.png", w + 1.0, gy, Look { anchor: (1.0, 1.0), flip_x: true, ..shade });
    }

    fn draw_player(&self, c: &mut Canvas) {
        let p = &self.player;
        let x = p.x + self.offset;
        let (c1, c2) = self.colors;
        let look = |col: [u8; 3]| Look { rotation: p.rotation, ..Look::default().rgb(col) };
        c.sprite("player_01_2_001.png", x, p.y, look(c2));
        c.sprite("player_01_001.png", x, p.y, look(c1));
    }

    pub fn draw(&self, c: &mut Canvas, ctx: &mut Ctx) {
        self.draw_game(c);
        let (w, h) = (c.width, c.height);
        let l = Look::default();
        // The logo (z 2), 50 below the top.
        c.sprite("GJ_logo_001.png", w * 0.5, h - 50.0, l);
        // Play, icon kit (−110) and creator (+110), at the center + 10.
        let (cx, cy) = (w * 0.5, h * 0.5 + 10.0);
        ctx.button(c, Btn::MenuPlay, "GJ_playBtn_001.png", cx, cy, l, 1.0);
        ctx.button(c, Btn::MenuGarage, "GJ_garageBtn_001.png", cx - 110.0, cy, l, 1.0);
        ctx.button(c, Btn::MenuCreator, "GJ_creatorBtn_001.png", cx + 110.0, cy, l, 1.0);
        // The hints under them until first used (GameManager +0x39f / +0x3a0).
        c.sprite("GJ_chrSel_001.png", cx - 110.0 - 50.0, cy - 50.0, l);
        c.sprite("GJ_lvlEdit_001.png", cx + 110.0 + 50.0, cy - 50.0, l);
        // The bottom row (achievements, options, stats, Newgrounds), padding 5, at bottom + 45.
        // (Real captures show a fifth, pentagon button: that's Geode's, from the harness.)
        let row = [
            (Btn::MenuAchievements, "GJ_achBtn_001.png"),
            (Btn::MenuOptions, "GJ_optionsBtn_001.png"),
            (Btn::MenuStats, "GJ_statsBtn_001.png"),
            (Btn::MenuNewgrounds, "GJ_ngBtn_001.png"),
        ];
        let widths: Vec<f32> = row.iter().map(|(_, n)| c.size(n).0).collect();
        let total = widths.iter().sum::<f32>() + 5.0 * (row.len() - 1) as f32;
        let mut x = w * 0.5 - total * 0.5;
        for ((id, name), bw) in row.iter().zip(&widths) {
            ctx.button(c, *id, name, x + bw * 0.5, 45.0, l, 1.0);
            x += bw + 5.0;
        }
        // The daily rewards chest, right of center (right − 40, center + 20); size mult 1.5.
        ctx.button(c, Btn::MenuDaily, "GJ_dailyRewardBtn_001.png", w - 40.0, h * 0.5 + 20.0, l, 1.5);
        // RobTop's logo (scale 0.8) at (left + 50, bottom + 25) and the social icons (0.8),
        // from (left + 22, bottom + 55), 29 apart; Discord under Twitch.
        ctx.button(c, Btn::MenuRobTop, "robtoplogo_small.png", 50.0, 25.0, Look::scaled(0.8), 1.1);
        let icons = ["gj_fbIcon_001.png", "gj_twIcon_001.png", "gj_ytIcon_001.png", "gj_twitchIcon_001.png"];
        for (i, name) in icons.iter().enumerate() {
            ctx.button(c, Btn::MenuSocial(i as u8), name, 22.0 + 29.0 * i as f32, 55.0, Look::scaled(0.8), 1.5);
        }
        ctx.button(c, Btn::MenuSocial(4), "gj_discordIcon_001.png", 22.0 + 29.0 * 3.0, 55.0 - 29.0, Look::scaled(0.8), 1.5);
        // News (0.85) at (right − 43, bottom + 45); close (0.7) at (18, top − 18).
        ctx.button(c, Btn::MenuNews, "GJ_newsBtn_001.png", w - 43.0, 45.0, Look::scaled(0.85), 1.0);
        // the news button's "!" (unread news)
        c.sprite("exMark_001.png", w - 28.5, 72.0, Look::scaled(0.8));
        ctx.button(c, Btn::MenuClose, "GJ_closeBtn_001.png", 18.0, h - 18.0, Look::scaled(0.7), 1.0);
        // (The profile button at (45, 105) only shows with an account, which Cadence doesn't have.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_cube_runs_across_and_jumps() {
        let mut rng = 12345u64;
        let mut m = Menu::default();
        m.reset_player(&mut rng);
        let x0 = m.player.x + m.offset;
        let mut max_y: f32 = 0.0;
        for _ in 0..60 {
            m.update(1.0 / 60.0, (568.0, 320.0), &mut rng);
            max_y = max_y.max(m.player.y);
        }
        let x1 = m.player.x + m.offset;
        assert!(!m.player.dead);
        assert!(max_y >= 105.0);
        // base speed · speed factor · 60 units a second (311.6 at 1x)
        let expected = (m.player.base_speed * m.player.speed as f64 * 60.0) as f32;
        assert!((x1 - x0 - expected).abs() < 5.0, "the cube runs at its speed: {} vs {expected}", x1 - x0);
    }
}
