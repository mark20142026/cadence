//! Color channels and the triggers that change them, after `GJEffectManager` (0x267e3c..):
//! - Each channel has a `ColorAction` (from → to color and opacity over a duration, blending,
//!   copy source + HSV, copy opacity). Color triggers restart it from the channel's current
//!   color (`GJBaseGameLayer::updateColor` 0xf4990).
//! - Once per frame, after the physics steps, the game steps the actions and pulses by the
//!   frame's game seconds (`GJEffectManager::updateEffects` 0x269138), then works out each
//!   channel's active color (`processColors` 0x268250): HSV for copies, channel pulses, then the
//!   copy channels parents first (`processInheritedColors` 0x268500, `FUN_100268c28`).
//! - Fades are `GameToolbox::multipliedColorValue` (0x44a824): per component
//!   trunc(fma(to − from, t, from)) in f32. Checked against real frames: Stereo Madness's first
//!   background trigger (x 135, to 158,3,255 over 10 s) gives (49,57,255) at step 300.

use std::collections::HashMap;

use crate::level::LevelString;

/// Color channel ids used by the game.
pub mod channel {
    pub const BACKGROUND: i32 = 1000;
    pub const GROUND: i32 = 1001;
    pub const LINE: i32 = 1002;
    pub const LINE_3D: i32 = 1003;
    pub const OBJECT: i32 = 1004;
    pub const PLAYER_1: i32 = 1005;
    pub const PLAYER_2: i32 = 1006;
    pub const LIGHT_BG: i32 = 1007;
    pub const GROUND_2: i32 = 1009;
    pub const BLACK: i32 = 1010;
    pub const WHITE: i32 = 1011;
    pub const LIGHTER: i32 = 1012;
}

/// `ccHSVValue`: a hue shift in degrees, and saturation/brightness factors (added instead of
/// multiplied when `abs_*`). Strings are "h a s a v a absS a absV".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub abs_s: bool,
    pub abs_v: bool,
}

impl Hsv {
    pub const IDENTITY: Hsv = Hsv { h: 0.0, s: 1.0, v: 1.0, abs_s: false, abs_v: false };

    pub fn parse(s: &str) -> Hsv {
        let p: Vec<&str> = s.split('a').collect();
        let f = |i: usize, d: f32| p.get(i).and_then(|v| v.parse::<f64>().ok()).map_or(d, |v| v as f32);
        let b = |i: usize| p.get(i).and_then(|v| v.parse::<i32>().ok()).is_some_and(|v| v != 0);
        Hsv { h: f(0, 0.0), s: f(1, 1.0), v: f(2, 1.0), abs_s: b(3), abs_v: b(4) }
    }
}

/// `CCControlUtils::HSVfromRGB` as built into the game (0x31cc98): a gray has hue 0, black is
/// (−1, 0, 0).
fn hsv_from_rgb(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let min = if r < g { r } else { g };
    let max = if r > g { r } else { g };
    let min = if min < b { min } else { b };
    let max = if max > b { max } else { b };
    if max <= 0.0 {
        return (-1.0, 0.0, max);
    }
    let delta = max - min;
    let h = if r >= max {
        if delta == 0.0 { 0.0 } else { (g - b) / delta }
    } else if g >= max {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    let h = h * 60.0;
    (if h < 0.0 { h + 360.0 } else { h }, delta / max, max)
}

/// `CCControlUtils::RGBfromHSV` (0x31cd44): hue sectors of h·(float)(1/60).
fn rgb_from_hsv(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if !(s > 0.0) {
        let c = if h.is_nan() { 0.0 } else { v };
        return (c, c, c);
    }
    let hh = if h < 360.0 { h * (1.0f32 / 60.0) as f64 } else { 0.0 };
    let i = hh as i64;
    let ff = hh - i as f64;
    let p = v * (1.0 - s);
    let q = v * (-s).mul_add(ff, 1.0);
    let t = v * (-s).mul_add(1.0 - ff, 1.0);
    match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// `GameToolbox::transformColor` (0x44a668): a color shifted in HSV, truncated back to bytes.
pub fn transform_color(c: [u8; 3], hsv: Hsv) -> [u8; 3] {
    if hsv == Hsv::IDENTITY {
        return c;
    }
    let unit = |v: u8| (v as f32 * 0.003_921_569) as f64;
    let (mut h, s, v) = hsv_from_rgb(unit(c[0]), unit(c[1]), unit(c[2]));
    if hsv.h != 0.0 {
        h += hsv.h as f64;
        if !(h < 0.0) {
            if h > 360.0 {
                h = ((h as i32) % 360) as f64;
            }
        } else {
            h += 360.0;
        }
    }
    let shift = |x: f64, by: f32, abs: bool| {
        let x = if abs { x + by as f64 } else { x * by as f64 };
        x.min(1.0).max(1.0e-4f32 as f64)
    };
    let (r, g, b) = rgb_from_hsv(h, shift(s, hsv.s, hsv.abs_s), shift(v, hsv.v, hsv.abs_v));
    [(r * 255.0) as i32 as u8, (g * 255.0) as i32 as u8, (b * 255.0) as i32 as u8]
}

/// `GameToolbox::multipliedColorValue`: per channel trunc(fma(to − from, t, from)), t clamped.
pub fn multiplied_color(t: f32, from: [u8; 3], to: [u8; 3]) -> [u8; 3] {
    let t = t.min(1.0).max(0.0);
    if t == 0.0 {
        return from;
    }
    if t == 1.0 {
        return to;
    }
    std::array::from_fn(|i| ((to[i] as i32 - from[i] as i32) as f32).mul_add(t, from[i] as f32) as i32 as u8)
}

/// A channel's definition: the level's starting colors (kS38, or kS29..kS37 in old levels) and
/// what a color trigger sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelDef {
    pub color: [u8; 3],
    /// 0..1.
    pub opacity: f32,
    pub blending: bool,
    /// Follow player color 1 or 2 (0: no).
    pub player: u8,
    /// Copy another channel's active color (0: no), shifted by `hsv`.
    pub copy: i32,
    pub hsv: Hsv,
    pub copy_opacity: bool,
}

impl Default for ChannelDef {
    fn default() -> ChannelDef {
        ChannelDef { color: [255; 3], opacity: 1.0, blending: false, player: 0, copy: 0, hsv: Hsv::IDENTITY, copy_opacity: false }
    }
}

/// "1_r_2_g_3_b_4_player_5_blend_6_id_7_opacity_8_opacityOn_9_copy_10_hsv_17_copyOpacity".
fn parse_channel(s: &str) -> (Option<i32>, ChannelDef) {
    let parts: Vec<&str> = s.split('_').collect();
    let get = |k: &str| parts.chunks(2).find(|c| c[0] == k).and_then(|c| c.get(1).copied());
    let num = |k: &str| get(k).and_then(|v| v.parse::<f64>().ok());
    let byte = |k: &str| num(k).map_or(255, |v| v as i32 as u8);
    let def = ChannelDef {
        color: [byte("1"), byte("2"), byte("3")],
        opacity: if num("8").unwrap_or(0.0) != 0.0 { num("7").unwrap_or(1.0) as f32 } else { 1.0 },
        blending: num("5").unwrap_or(0.0) != 0.0,
        player: match num("4").map(|v| v as i32) {
            Some(1) => 1,
            Some(2) => 2,
            _ => 0,
        },
        copy: num("9").map_or(0, |v| v as i32),
        hsv: get("10").map_or(Hsv::IDENTITY, Hsv::parse),
        copy_opacity: num("17").unwrap_or(0.0) != 0.0,
    };
    (num("6").map(|v| v as i32), def)
}

/// The level's channel definitions.
pub fn level_channels(level: &LevelString) -> HashMap<i32, ChannelDef> {
    let mut channels = HashMap::new();
    for (key, id) in [
        ("kS29", channel::BACKGROUND),
        ("kS30", channel::GROUND),
        ("kS31", channel::LINE),
        ("kS32", channel::OBJECT),
        ("kS33", 1),
        ("kS34", 2),
        ("kS35", 3),
        ("kS36", 4),
        ("kS37", channel::LINE_3D),
    ] {
        if let Some(v) = level.setting(key) {
            channels.insert(id, parse_channel(v).1);
        }
    }
    if let Some(list) = level.setting("kS38") {
        for entry in list.split('|').filter(|e| !e.is_empty()) {
            if let (Some(id), def) = parse_channel(entry) {
                channels.insert(id, def);
            }
        }
    }
    channels
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorTrigger {
    pub x: f32,
    pub channel: i32,
    pub color: [u8; 3],
    /// Seconds.
    pub duration: f32,
    /// Opacity (key 35), blending (17), player color (15/16), copy (50 + HSV 49, opacity 60).
    pub def: ChannelDef,
}

/// The channel an object id's color trigger targets (`None` for other objects).
/// 899 is the 2.0+ generic trigger (target in key 23); the rest are the legacy fixed ones.
fn trigger_channel(id: i32, target: Option<i32>) -> Option<i32> {
    Some(match id {
        29 => channel::BACKGROUND,
        30 => channel::GROUND,
        104 | 915 => channel::LINE,
        105 => channel::OBJECT,
        221 => 1,
        717 => 2,
        718 => 3,
        743 => 4,
        744 => channel::LINE_3D,
        900 => channel::GROUND_2,
        899 => target.unwrap_or(1),
        _ => return None,
    })
}

/// A pulse trigger (1006): blends a color channel (key 52 = 0) or a group's objects (52 = 1)
/// toward a color (48 = 0) or an HSV shift (48 = 1, HSV 49, of channel 50's color or the
/// target's own): fade in (45), hold (46), fade out (47) seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PulseTrigger {
    pub x: f32,
    pub target: i32,
    pub group: bool,
    pub color: [u8; 3],
    pub hsv: Option<(Hsv, i32)>,
    pub fade_in: f32,
    pub hold: f32,
    pub fade_out: f32,
}

#[derive(Clone, Copy, Debug)]
struct Pulse {
    trigger: PulseTrigger,
    elapsed: f32,
}

impl Pulse {
    /// `PulseEffectAction::step`'s intensity.
    fn value(&self) -> f32 {
        let t = &self.trigger;
        let e = self.elapsed;
        if e < t.fade_in {
            e / t.fade_in
        } else if t.fade_in + t.hold < e {
            if 0.0 < t.fade_out { 1.0 - ((e - t.fade_in) - t.hold) / t.fade_out } else { 0.0 }
        } else {
            1.0
        }
    }

    fn finished(&self) -> bool {
        let t = &self.trigger;
        t.fade_in + t.hold + t.fade_out <= self.elapsed
    }
}

/// A channel's `ColorAction` (+0x38..) and its sprite's colors (`ColorActionSprite` +0x13c..).
#[derive(Clone, Copy, Debug)]
struct ChannelState {
    def: ChannelDef,
    from: [u8; 3],
    to: [u8; 3],
    from_opacity: f32,
    to_opacity: f32,
    duration: f32,
    elapsed: f32,
    finished: bool,
    /// The action's current color and opacity (0..1).
    current: [u8; 3],
    current_opacity: f32,
    /// The sprite: base color, opacity 0..255, active color (after HSV and pulses).
    base: [u8; 3],
    opacity: f32,
    active: [u8; 3],
    /// Part of a copy loop (+0x7e): no HSV, not resolved.
    cyclic: bool,
}

impl ChannelState {
    fn new(def: ChannelDef, players: [[u8; 3]; 2]) -> ChannelState {
        let color = match def.player {
            1 => players[0],
            2 => players[1],
            _ => def.color,
        };
        ChannelState {
            def,
            from: color,
            to: color,
            from_opacity: def.opacity,
            to_opacity: def.opacity,
            duration: 0.0,
            elapsed: 0.0,
            finished: true,
            current: color,
            current_opacity: def.opacity,
            base: color,
            opacity: def.opacity * 255.0,
            active: color,
            cyclic: false,
        }
    }

    /// `ColorAction::step` (0x263dd0) / the loop in `updateColorEffects` (0x269234).
    fn step(&mut self, dt: f32) {
        if self.finished {
            return;
        }
        self.elapsed += dt;
        let e = self.elapsed;
        if self.duration <= e {
            self.current = self.to;
            self.current_opacity = self.to_opacity;
        } else {
            let t = e / self.duration;
            if 0.0 < e && t < 1.0 && 0.0 < t {
                self.current = multiplied_color(t, self.from, self.to);
                self.current_opacity = (self.to_opacity - self.from_opacity).mul_add(t, self.from_opacity);
            } else if 1.0 <= t {
                self.current = self.to;
                self.current_opacity = self.to_opacity;
            } else {
                self.current = self.from;
                self.current_opacity = self.from_opacity;
            }
        }
        self.finished = self.duration <= e;
    }
}

/// Channel colors over time.
#[derive(Clone, Debug)]
pub struct Colors {
    /// Triggers sorted by x.
    triggers: Vec<ColorTrigger>,
    next: usize,
    channels: crate::fxhash::FxMap<i32, ChannelState>,
    /// Channel ids that copy another, and an order with every source before its copies.
    copy_order: Vec<i32>,
    players: [[u8; 3]; 2],
    /// Pulse triggers sorted by x, and the running pulses in start order.
    pulse_triggers: Vec<PulseTrigger>,
    next_pulse: usize,
    pulses: Vec<Pulse>,
    /// Alpha triggers (1007) sorted by x, and each group's running opacity fade.
    alpha_triggers: Vec<AlphaTrigger>,
    next_alpha: usize,
    group_opacity: crate::fxhash::FxMap<i32, OpacityFade>,
    /// Spawned/touched triggers by level order (see `fire`).
    effects: crate::fxhash::FxMap<u32, Effect>,
}

/// A color, pulse or alpha trigger that a spawn or touch fires (by level order).
#[derive(Clone, Debug)]
enum Effect {
    Color(ColorTrigger),
    Pulse(PulseTrigger),
    Alpha(AlphaTrigger),
}

/// An alpha trigger (1007): fades group 51's objects to opacity 35 over 10 seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaTrigger {
    pub x: f32,
    pub group: i32,
    pub opacity: f32,
    pub duration: f32,
}

/// A group's opacity fade (`GJEffectManager::runOpacityActionOnGroup` 0x26fe0c, stepped in
/// `updateOpacityEffects` 0x269674): from the group's current factor to the trigger's.
#[derive(Clone, Copy, Debug)]
struct OpacityFade {
    from: f32,
    to: f32,
    duration: f32,
    elapsed: f32,
    current: f32,
    finished: bool,
}

impl OpacityFade {
    fn step(&mut self, dt: f32) {
        if self.finished {
            return;
        }
        self.elapsed += dt;
        let e = self.elapsed;
        let t = if e < self.duration { if 0.0 < e { e / self.duration } else { 0.0 } } else { 1.0 };
        self.current = (self.to - self.from).mul_add(t, self.from);
        self.finished = self.duration <= e;
    }
}

impl Colors {
    fn parse_color(o: &crate::level::RawObject) -> Option<ColorTrigger> {
        let channel = trigger_channel(o.id(), o.get_i32(23))?;
        let byte = |k| o.get_f32(k).map_or(255, |v| v as i32 as u8);
        let flag = |k| o.get(k).is_some_and(|v| v != "0");
        let def = ChannelDef {
            color: [byte(7), byte(8), byte(9)],
            opacity: o.get_f32(35).unwrap_or(1.0),
            blending: flag(17),
            player: if flag(15) { 1 } else if flag(16) { 2 } else { 0 },
            copy: o.get_i32(50).unwrap_or(0),
            hsv: o.get(49).map_or(Hsv::IDENTITY, Hsv::parse),
            copy_opacity: flag(60),
        };
        Some(ColorTrigger { x: o.x(), channel, color: def.color, duration: o.get_f32(10).unwrap_or(0.0), def })
    }

    /// A spawned or touched color/pulse/alpha trigger fired (`World::fired_effects`).
    pub fn fire(&mut self, order: u32) {
        match self.effects.get(&order).cloned() {
            Some(Effect::Color(t)) => self.run(&t),
            Some(Effect::Pulse(t)) => self.pulses.push(Pulse { trigger: t, elapsed: 0.0 }),
            Some(Effect::Alpha(t)) => self.start_alpha(&t),
            None => {}
        }
    }

    fn start_alpha(&mut self, t: &AlphaTrigger) {
        let from = self.group_opacity(t.group);
        let mut fade = OpacityFade { from, to: t.opacity, duration: t.duration, elapsed: 0.0, current: from, finished: false };
        fade.step(0.0);
        self.group_opacity.insert(t.group, fade);
    }

    /// `players` are the player's two colors (channels 1005/1006 and player-colored channels).
    pub fn new(level: &LevelString, players: [[u8; 3]; 2]) -> Colors {
        // Fired by level order (`fire`) rather than by x: spawned, touched, or on an order channel
        // other than 0 (the order walk fires those only while their channel is current).
        let spawned = |o: &crate::level::RawObject| {
            o.get(62) == Some("1") || o.get(11) == Some("1") || o.get_i32(170).unwrap_or(0) != 0
        };
        let mut effects: crate::fxhash::FxMap<u32, Effect> = Default::default();
        let mut triggers: Vec<ColorTrigger> = Vec::new();
        for (order, o) in level.objects.iter().enumerate() {
            let Some(t) = Self::parse_color(o) else { continue };
            if spawned(o) {
                effects.insert(order as u32, Effect::Color(t));
            } else {
                triggers.push(t);
            }
        }
        triggers.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut pulse_triggers: Vec<PulseTrigger> = Vec::new();
        let mut alpha_triggers: Vec<AlphaTrigger> = Vec::new();
        for (order, o) in level.objects.iter().enumerate() {
            let effect = match o.id() {
                1006 => {
                    let byte = |k| o.get_f32(k).map_or(255, |v| v as i32 as u8);
                    Effect::Pulse(PulseTrigger {
                        x: o.x(),
                        target: o.get_i32(51).unwrap_or(0),
                        group: o.get(52) == Some("1"),
                        color: [byte(7), byte(8), byte(9)],
                        hsv: (o.get(48) == Some("1"))
                            .then(|| (o.get(49).map_or(Hsv::IDENTITY, Hsv::parse), o.get_i32(50).unwrap_or(0))),
                        fade_in: o.get_f32(45).unwrap_or(0.0),
                        hold: o.get_f32(46).unwrap_or(0.0),
                        fade_out: o.get_f32(47).unwrap_or(0.0),
                    })
                }
                1007 => Effect::Alpha(AlphaTrigger {
                    x: o.x(),
                    group: o.get_i32(51).unwrap_or(0),
                    opacity: o.get_f32(35).unwrap_or(1.0),
                    duration: o.get_f32(10).unwrap_or(0.0),
                }),
                _ => continue,
            };
            match effect {
                e if spawned(o) => {
                    effects.insert(order as u32, e);
                }
                Effect::Pulse(t) => pulse_triggers.push(t),
                Effect::Alpha(t) => alpha_triggers.push(t),
                Effect::Color(_) => {}
            }
        }
        pulse_triggers.sort_by(|a, b| a.x.total_cmp(&b.x));
        alpha_triggers.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut defs = level_channels(level);
        let player = |p: u8| ChannelDef { player: p, blending: true, ..ChannelDef::default() };
        defs.entry(channel::PLAYER_1).or_insert(player(1));
        defs.entry(channel::PLAYER_2).or_insert(player(2));
        let channels = defs.into_iter().map(|(id, def)| (id, ChannelState::new(def, players))).collect();
        let mut colors = Colors {
            triggers,
            next: 0,
            channels,
            copy_order: Vec::new(),
            players,
            pulse_triggers,
            next_pulse: 0,
            pulses: Vec::new(),
            alpha_triggers,
            next_alpha: 0,
            group_opacity: Default::default(),
            effects,
        };
        colors.update_copy_order();
        colors.process();
        colors
    }

    fn state(&mut self, id: i32) -> &mut ChannelState {
        let players = self.players;
        self.channels.entry(id).or_insert_with(|| ChannelState::new(ChannelDef::default(), players))
    }

    /// Copy channels with every source before its copies; channels in a loop are marked
    /// cyclic and left out.
    fn update_copy_order(&mut self) {
        let copies: HashMap<i32, i32> =
            self.channels.iter().filter(|(_, c)| c.def.copy > 0).map(|(&id, c)| (id, c.def.copy)).collect();
        let mut ids: Vec<i32> = copies.keys().copied().collect();
        ids.sort();
        let mut order = Vec::new();
        let mut cyclic = Vec::new();
        for &id in &ids {
            // Walk up to the root; a loop is anything that revisits a channel.
            let mut chain = vec![id];
            let mut at = id;
            let mut looped = false;
            while let Some(&parent) = copies.get(&at) {
                if chain.contains(&parent) {
                    looped = true;
                    break;
                }
                chain.push(parent);
                at = parent;
            }
            if looped {
                cyclic.push(id);
                continue;
            }
            for &c in chain.iter().rev().filter(|c| copies.contains_key(c)) {
                if !order.contains(&c) {
                    order.push(c);
                }
            }
        }
        for (id, c) in self.channels.iter_mut() {
            c.cyclic = cyclic.contains(id);
        }
        self.copy_order = order;
    }

    /// Advance one physics step with the player at `player_x`: triggers the player passed fire.
    pub fn step(&mut self, player_x: f32) {
        while let Some(t) = self.triggers.get(self.next).filter(|t| t.x <= player_x).cloned() {
            self.run(&t);
            self.next += 1;
        }
        while let Some(&t) = self.pulse_triggers.get(self.next_pulse).filter(|t| t.x <= player_x) {
            self.pulses.push(Pulse { trigger: t, elapsed: 0.0 });
            self.next_pulse += 1;
        }
        while let Some(&t) = self.alpha_triggers.get(self.next_alpha).filter(|t| t.x <= player_x) {
            self.start_alpha(&t);
            self.next_alpha += 1;
        }
    }

    /// A group's opacity factor (`GJEffectManager::opacityModForGroup` 0x26ffd4), 1 if none.
    pub fn group_opacity(&self, group: i32) -> f32 {
        self.group_opacity.get(&group).map_or(1.0, |f| f.current)
    }

    /// `GJBaseGameLayer::updateColor` (0xf4990): restart the channel's action from its current
    /// color and opacity toward the trigger's.
    fn run(&mut self, t: &ColorTrigger) {
        let players = self.players;
        let copy_changed = {
            let c = self.state(t.channel);
            let to = match t.def.player {
                1 => players[0],
                2 => players[1],
                _ => t.color,
            };
            let copy_changed = c.def.copy != t.def.copy;
            c.from = c.base;
            c.to = to;
            c.duration = t.duration;
            c.from_opacity = c.opacity / 255.0;
            c.to_opacity = t.def.opacity;
            c.def = ChannelDef { color: to, ..t.def };
            c.elapsed = 0.0;
            c.finished = false;
            c.step(0.0);
            if t.duration <= 0.0 {
                if t.def.copy == 0 {
                    c.base = to;
                    c.opacity = t.def.opacity * 255.0;
                } else if !t.def.copy_opacity {
                    c.opacity = t.def.opacity * 255.0;
                }
            }
            copy_changed
        };
        if copy_changed {
            self.update_copy_order();
        }
        self.process();
    }

    /// The end of a frame of `dt` game seconds: `GJEffectManager::updateEffects` (actions and
    /// pulses) and the channels' active colors.
    pub fn frame(&mut self, dt: f32) {
        for c in self.channels.values_mut() {
            c.step(dt);
            if c.def.copy == 0 {
                c.base = c.current;
                c.opacity = c.current_opacity * 255.0;
            }
        }
        for p in &mut self.pulses {
            p.elapsed += dt;
        }
        self.pulses.retain(|p| !p.finished());
        for f in self.group_opacity.values_mut() {
            f.step(dt);
        }
        self.group_opacity.retain(|_, f| !(f.finished && 1.0 <= f.current));
        self.process();
    }

    /// `processColors`: active colors (HSV on copies, channel pulses), then the copies.
    fn process(&mut self) {
        let channel_pulses: Vec<Pulse> = self.pulses.iter().filter(|p| !p.trigger.group).copied().collect();
        let ids: Vec<i32> = self.channels.keys().copied().collect();
        for id in &ids {
            let c = &self.channels[id];
            let mut active = c.base;
            if c.def.copy > 0 && !c.cyclic {
                active = transform_color(active, c.def.hsv);
            }
            if c.def.copy == 0 {
                for p in channel_pulses.iter().filter(|p| p.trigger.target == *id && p.trigger.hsv.is_none_or(|h| h.1 == 0)) {
                    active = self.pulse_color(active, p);
                }
            }
            self.channels.get_mut(id).unwrap().active = active;
        }
        for id in self.copy_order.clone() {
            let c = self.channels[&id];
            let (mut color, mut opacity) = (self.active(c.def.copy), self.channel_opacity(c.def.copy));
            if 0.0 < c.duration && c.elapsed < c.duration {
                let t = (c.elapsed / c.duration).min(1.0).max(0.0);
                color = multiplied_color(t, c.from, color);
                opacity = ((opacity / 255.0) * t + (1.0 - t) * c.from_opacity) * 255.0;
            }
            color = transform_color(color, c.def.hsv);
            for p in channel_pulses.iter().filter(|p| p.trigger.target == id && p.trigger.hsv.is_none_or(|h| h.1 == 0)) {
                color = self.pulse_color(color, p);
            }
            let s = self.channels.get_mut(&id).unwrap();
            s.base = color;
            s.active = color;
            s.opacity = if c.def.copy_opacity { opacity } else { 255.0 } * c.current_opacity;
        }
        // The light background (1007, `calculateLightBGColor` 0x2746b4, from
        // `PlayLayer::updateVisibility`): the background shifted (s −0.2, v +0.2, absolute),
        // blended toward player color 1 on dark backgrounds (r + g + b < 150).
        let bg = self.active(channel::BACKGROUND);
        let shifted = transform_color(bg, Hsv { h: 0.0, s: -0.2, v: 0.2, abs_s: true, abs_v: true });
        let sum = bg.iter().map(|&v| v as u32).sum::<u32>();
        let lbg = if sum < 150 {
            let t = sum as f32 / 150.0;
            let p1 = self.active(channel::PLAYER_1);
            std::array::from_fn(|i| ((1.0 - t) * p1[i] as f32 + t * shifted[i] as f32).clamp(0.0, 255.0) as i32 as u8)
        } else {
            shifted
        };
        let s = self.state(channel::LIGHT_BG);
        s.base = lbg;
        s.active = lbg;
        // HSV pulses of another channel's color (`processCopyColorPulseActions`).
        for p in channel_pulses.iter().filter(|p| p.trigger.hsv.is_some_and(|h| h.1 != 0)) {
            let active = self.active(p.trigger.target);
            let color = self.pulse_color(active, p);
            if let Some(s) = self.channels.get_mut(&p.trigger.target) {
                s.active = color;
            }
        }
    }

    /// `GJEffectManager::colorForPulseEffect` (0x268e84).
    fn pulse_color(&self, color: [u8; 3], p: &Pulse) -> [u8; 3] {
        let v = p.value();
        if !(0.0 < v) {
            return color;
        }
        match p.trigger.hsv {
            None => multiplied_color(v, color, p.trigger.color),
            Some((hsv, source)) => {
                let base = if source != 0 { self.active(source) } else { color };
                if v == 1.0 {
                    transform_color(base, hsv)
                } else {
                    multiplied_color(v, color, transform_color(base, hsv))
                }
            }
        }
    }

    /// Groups that pulse and alpha triggers target (their objects need their groups known).
    pub fn effect_groups(&self) -> impl Iterator<Item = i32> + '_ {
        let spawned = self.effects.values().filter_map(|e| match e {
            Effect::Pulse(t) if t.group => Some(t.target),
            Effect::Alpha(t) => Some(t.group),
            _ => None,
        });
        self.pulse_triggers.iter().filter(|t| t.group).map(|t| t.target).chain(self.alpha_triggers.iter().map(|t| t.group)).chain(spawned)
    }

    /// A channel's active color (`GJEffectManager::activeColorForIndex` 0x26808c).
    pub fn active(&self, id: i32) -> [u8; 3] {
        match id {
            0 | channel::WHITE => [255; 3],
            channel::BLACK => [0; 3],
            _ => self.channels.get(&id).map_or([255; 3], |c| c.active),
        }
    }

    /// Every stored channel: (id, active color, opacity, blending), for noticing changes.
    pub fn channel_states(&self) -> impl Iterator<Item = (i32, [u8; 3], f32, bool)> + '_ {
        self.channels.iter().map(|(&id, c)| (id, c.active, c.opacity, c.def.blending))
    }

    /// A channel's sprite opacity, 0..255.
    fn channel_opacity(&self, id: i32) -> f32 {
        self.channels.get(&id).map_or(255.0, |c| c.opacity)
    }

    /// Whether objects colored by `id` draw additively (`GameObject::shouldBlendColor`
    /// 0x5af5c0): player colors and the light background always do, 0 and 1012 never.
    pub fn blending(&self, id: i32) -> bool {
        match id {
            channel::PLAYER_1 | channel::PLAYER_2 | channel::LIGHT_BG => true,
            0 | channel::LIGHTER => false,
            _ => self.channels.get(&id).is_some_and(|c| c.def.blending),
        }
    }

    /// An object's opacity from its channel (`GameObject::updateMainColor` 0x5b9d90):
    /// trunc(opacity)·0.004, 1 above 249.
    /// The groups' alpha-trigger factors multiply it, stopping at 0.
    pub fn object_opacity(&self, id: i32, groups: &[i32]) -> f32 {
        let channel = if id < 1 {
            1.0
        } else {
            let o = self.channel_opacity(id) as i32 as u32;
            if o > 249 { 1.0 } else { o as f32 * 0.004 }
        };
        let mut factor = 1.0f32;
        for &g in groups {
            factor *= self.group_opacity(g);
            if factor <= 0.0 {
                factor = 0.0;
                break;
            }
        }
        channel * factor
    }

    /// An object part's displayed color, 0..1 (`GameObject::colorForMode` 0x5b9e88): the
    /// channel's active color (1012: `main` made lighter), the pulses on the object's groups,
    /// then the object's own HSV.
    pub fn object_color(&self, id: i32, main: [u8; 3], groups: &[i32], hsv: Option<Hsv>) -> [f32; 3] {
        let mut c = if id == channel::LIGHTER {
            transform_color(main, Hsv { h: 0.0, s: 0.65, v: 0.15, abs_s: false, abs_v: true })
        } else {
            self.active(id)
        };
        for &g in groups {
            for p in self.pulses.iter().filter(|p| p.trigger.group && p.trigger.target == g) {
                c = self.pulse_color(c, p);
            }
        }
        if let Some(hsv) = hsv {
            c = transform_color(c, hsv);
        }
        c.map(|v| v as f32 / 255.0)
    }

    /// Displayed color of a channel (0..1), e.g. the background.
    pub fn display(&self, id: i32) -> [f32; 3] {
        self.active(id).map(|v| v as f32 / 255.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_madness_first_background_fade() {
        let level = LevelString::parse("kA13,0,kS29,1_40_2_62_3_255;1,29,2,135,3,135,7,158,8,3,9,255,10,10;");
        let mut colors = Colors::new(&level, [[125, 255, 0], [0, 255, 255]]);
        let x_at = |step: u32| (step + 1) as f32 * 1.298_250_4;
        // Four steps a frame; effects step by the frame's 1/60 s after its last step.
        for step in 0..300 {
            colors.step(x_at(step));
            if step % 4 == 3 {
                colors.frame(1.0f32 / 60.0);
            }
        }
        assert_eq!(colors.active(channel::BACKGROUND), [49, 57, 255]);
    }

    #[test]
    fn hsv_round_trip_and_shift() {
        // No shift returns the color as is; a 0 hue / ×1 / ×1 round trip can lose a unit.
        assert_eq!(transform_color([12, 200, 99], Hsv::IDENTITY), [12, 200, 99]);
        // Hue +120: red → green.
        assert_eq!(transform_color([255, 0, 0], Hsv { h: 120.0, ..Hsv::IDENTITY }), [0, 255, 0]);
        // Brightness ×0.5 of white.
        assert_eq!(transform_color([255, 255, 255], Hsv { v: 0.5, ..Hsv::IDENTITY }), [127, 127, 127]);
        assert_eq!(Hsv::parse("0a1a0.62a0a0"), Hsv { h: 0.0, s: 1.0, v: 0.62, abs_s: false, abs_v: false });
    }

    #[test]
    fn copies_follow_their_source() {
        let level = LevelString::parse("kA13,0,kS38,1_200_2_0_3_0_6_1|1_0_2_0_3_0_6_2_9_1_10_0a1a0.5a0a0|1_0_2_0_3_0_6_3_9_2;");
        let colors = Colors::new(&level, [[0; 3], [0; 3]]);
        assert_eq!(colors.active(2), [100, 0, 0]);
        assert_eq!(colors.active(3), [100, 0, 0]);
    }
}
