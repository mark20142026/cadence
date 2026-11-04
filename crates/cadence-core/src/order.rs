//! Order channels: how the game activates position triggers (2.2).
//!
//! `PlayLayer::addObject` puts every trigger that isn't touch- or spawn-activated in a list;
//! `orderSpawnObjects` → `LevelTools::sortChannelOrderObjects` splits it by order channel (key
//! 170, `+0x6f0`) and sorts each channel by its ord value (key 115, `+0x6ec`), then by the
//! truncated position along the channel's direction, then by unique id. A channel's direction
//! comes from the first rotate-gameplay trigger (2900) that switches to it (key 171, target 173):
//! its object direction (`getObjectDirection`, from the slope direction of its rotation/flips)
//! picks x ascending, y ascending (1), y descending (2) or x descending (3).
//!
//! Each physics step, after the collision pass and before `updateRotation` (`FUN_1000e7d4c`),
//! the current channel's list is walked from where it stopped: the next trigger fires once the
//! player has passed it — x (y while sideways), the other way when the channel is reversed (a
//! 2900 switching to it with its second direction 2/3) — and the walk stops at the first one
//! not passed. A 2900 can switch the channel and turn the gameplay; the walk goes on in the new
//! channel along the new axis.

use std::collections::HashMap;

use crate::level::LevelString;

/// What a list entry does when it fires.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderKind {
    /// A world (group) trigger, by level order (`GroupTriggers::fire_orders`).
    World(u32),
    /// A camera or rotate-gameplay trigger, by index in `CameraTriggers`.
    Camera(usize),
    /// Options trigger (2899): key 165 player 1 controls (1 disable, −1 enable, 0 nothing).
    Options(i8),
    /// End trigger (3600): the level is complete.
    End,
    /// A trigger Cadence doesn't run: it still holds its place in the walk.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderItem {
    pub x: f32,
    /// In level units (raw y + the ground offset), like the player.
    pub y: f32,
    /// Level order (the toggled-off test).
    pub order: u32,
    pub kind: OrderKind,
}

/// The level's channels, each sorted (`m_orderSpawnObjects` by channel).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderLists {
    pub channels: HashMap<i32, Vec<OrderItem>>,
}

/// The player's walk through the channels (`m_currentChannel` +0x33c, the per-channel next
/// index and reversed flag).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderState {
    pub channel: i32,
    pub next: HashMap<i32, usize>,
    pub reversed: HashMap<i32, bool>,
}

/// Speed portals are collided by their rect (see physics), not walked.
fn is_speed_portal(id: i32) -> bool {
    matches!(id, 200 | 201 | 202 | 203 | 1334)
}

impl OrderLists {
    /// `camera`: level order → index in `CameraTriggers`.
    pub fn from_level(level: &LevelString, camera: &HashMap<u32, usize>) -> OrderLists {
        struct Entry {
            item: OrderItem,
            channel: i32,
            ord: i32,
        }
        let mut entries = Vec::new();
        // Channel directions from the first 2900 switching to each channel.
        let mut directions: HashMap<i32, i32> = HashMap::new();
        for (order, raw) in level.objects.iter().enumerate() {
            let id = raw.canonical_id();
            let Some(info) = crate::objects::info(id) else { continue };
            // triggers (type 20) and the area triggers (3006–3015, type 45) walk with the player
            let trigger = info.kind == crate::objects::ObjectType::MODIFIER || info.kind == crate::objects::ObjectType(45);
            if !trigger || id == 31 || is_speed_portal(id) {
                continue;
            }
            if id == 2900 && raw.get(171) == Some("1") {
                let target = raw.get_i32(173).unwrap_or(0);
                directions.entry(target).or_insert_with(|| {
                    let flip_x = raw.get(4) == Some("1");
                    let flip_y = raw.get(5) == Some("1");
                    let (dir, _) = crate::world::slope_direction(flip_x, flip_y, raw.get_f32(6).unwrap_or(0.0));
                    match dir {
                        2 | 3 => 3,
                        4 | 7 => 2,
                        5 | 6 => 1,
                        _ => 4,
                    }
                });
            }
            if raw.get(11) == Some("1") || raw.get(62) == Some("1") {
                continue;
            }
            let order = order as u32;
            let kind = if let Some(&i) = camera.get(&order) {
                OrderKind::Camera(i)
            } else if crate::groups::Trigger::parse(raw, order).is_some() {
                OrderKind::World(order)
            } else if id == 3600 {
                OrderKind::End
            } else if id == 2899 {
                OrderKind::Options(raw.get_i32(165).unwrap_or(0).clamp(-1, 1) as i8)
            } else {
                OrderKind::Other
            };
            entries.push(Entry {
                item: OrderItem { x: raw.x(), y: raw.y() + crate::world::GROUND_OFFSET, order, kind },
                channel: raw.get_i32(170).unwrap_or(0),
                ord: raw.get_i32(115).unwrap_or(0),
            });
        }
        let mut channels: HashMap<i32, Vec<Entry>> = HashMap::new();
        for e in entries {
            channels.entry(e.channel).or_default().push(e);
        }
        let mut out = OrderLists::default();
        for (channel, mut list) in channels {
            let dir = directions.get(&channel).copied().unwrap_or(4);
            // The comparators (FUN_100456e68 / f98 / 030 / f00): ord, then the truncated
            // position (+ ord, or ord − position for the descending ones), then unique id.
            let key = |e: &Entry| -> i32 {
                let ord = e.ord as f32;
                match dir {
                    1 => (e.item.y + ord) as i32,
                    2 => (ord - e.item.y) as i32,
                    3 => (ord - e.item.x) as i32,
                    _ => (e.item.x + ord) as i32,
                }
            };
            list.sort_by(|a, b| a.ord.cmp(&b.ord).then(key(a).cmp(&key(b))).then(a.item.order.cmp(&b.item.order)));
            out.channels.insert(channel, list.into_iter().map(|e| e.item).collect());
        }
        out
    }
}
