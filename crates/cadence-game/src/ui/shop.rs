//! `GJShopLayer`: the five shops' items from the game's own table (`createStoreItems`,
//! 0x1b2a8), eight a page as the game lays them out (the slots, prices and dots measured from
//! `scenes/shop*.txt`), bought with Mana Orbs (stat 14) or, in the diamond shop, Diamond
//! Shards (stat 29) through `PurchaseItemPopup` (layout captured from the game). What's bought
//! is kept like the game's `GS_6`; a bought item is darkened in its shop's color with no price.
//! Cadence's icon kit has every icon unlocked anyway, so buying is for the collection.

use super::canvas::{self, Canvas, Look};
use super::garage;
use super::menu::color_for_idx;
use super::replay::{self, Scene};
use super::save::Progress;
use super::{Btn, Ctx};

/// (index, icon / item id, unlock type, price, shop) in the order the game adds them (its
/// dictionary keeps that order, and the shop shows them so). Unlock types: 1 cube, 2 / 3 main
/// / secondary color, 4 ship, 5 ball, 6 UFO, 7 wave, 8 robot, 9 spider, 10 trail, 11 death
/// effect, 12 item, 13 swing, 14 jetpack, 15 ship fire. Shop 5 (the paths) has no layer.
pub const ITEMS: [(u16, u16, u8, u32, u8); 283] = [
    (34, 79, 1, 500, 0), (1, 77, 1, 1000, 0), (2, 86, 1, 1000, 0), (3, 73, 1, 1000, 0),
    (4, 102, 1, 1000, 0), (5, 107, 1, 1000, 0), (6, 27, 4, 2000, 0), (7, 25, 6, 2000, 0),
    (8, 23, 6, 2500, 0), (9, 20, 5, 1000, 0), (10, 19, 5, 1500, 0), (11, 21, 7, 500, 0),
    (12, 2, 9, 2000, 0), (13, 12, 8, 3000, 0), (14, 8, 11, 7000, 0), (15, 11, 11, 7000, 0),
    (16, 18, 7, 2000, 1), (17, 94, 1, 3000, 1), (18, 85, 1, 2500, 1), (19, 96, 1, 2000, 1),
    (20, 4, 12, 1000, 1), (21, 14, 8, 3000, 1), (22, 21, 6, 4000, 1), (23, 3, 9, 3000, 1),
    (24, 25, 5, 2000, 1), (25, 13, 11, 10000, 1), (26, 35, 2, 1000, 0), (27, 36, 2, 1000, 0),
    (28, 39, 2, 1000, 1), (29, 40, 2, 1000, 1), (30, 39, 3, 1000, 0), (32, 32, 3, 1000, 1),
    (33, 41, 3, 1000, 1), (35, 5, 10, 4000, 0), (36, 6, 10, 4000, 0), (37, 7, 10, 4000, 1),
    (38, 117, 1, 4000, 2), (39, 118, 1, 2000, 2), (40, 119, 1, 2000, 2), (41, 120, 1, 4000, 2),
    (42, 121, 1, 2000, 2), (43, 122, 1, 2000, 2), (44, 123, 1, 6000, 2), (45, 124, 1, 8000, 2),
    (46, 125, 1, 4000, 2), (47, 126, 1, 2000, 2), (48, 127, 1, 6000, 2), (49, 128, 1, 8000, 2),
    (50, 129, 1, 6000, 2), (51, 130, 1, 4000, 2), (52, 131, 1, 4000, 2), (53, 132, 1, 4000, 2),
    (54, 133, 1, 8000, 2), (55, 134, 1, 6000, 2), (56, 135, 1, 6000, 2), (57, 30, 5, 4000, 2),
    (58, 31, 5, 6000, 2), (59, 32, 5, 6000, 2), (60, 33, 5, 2000, 2), (61, 34, 5, 4000, 2),
    (62, 35, 5, 8000, 2), (63, 36, 5, 6000, 2), (64, 37, 5, 2000, 2), (65, 38, 5, 2000, 2),
    (66, 36, 4, 2000, 2), (67, 37, 4, 2000, 2), (68, 38, 4, 6000, 2), (69, 39, 4, 2000, 2),
    (70, 40, 4, 4000, 2), (71, 41, 4, 4000, 2), (72, 42, 4, 6000, 2), (73, 43, 4, 2000, 2),
    (74, 44, 4, 6000, 2), (75, 45, 4, 8000, 2), (76, 46, 4, 4000, 2), (77, 47, 4, 6000, 2),
    (78, 48, 4, 4000, 2), (79, 29, 6, 6000, 2), (80, 30, 6, 6000, 2), (81, 31, 6, 4000, 2),
    (82, 32, 6, 6000, 2), (83, 33, 6, 4000, 2), (84, 34, 6, 2000, 2), (85, 35, 6, 2000, 2),
    (86, 24, 7, 2000, 2), (87, 25, 7, 4000, 2), (88, 26, 7, 2000, 2), (89, 27, 7, 2000, 2),
    (90, 28, 7, 6000, 2), (91, 29, 7, 6000, 2), (92, 30, 7, 4000, 2), (93, 31, 7, 6000, 2),
    (94, 32, 7, 2000, 2), (95, 33, 7, 6000, 2), (96, 18, 8, 2000, 2), (97, 19, 8, 6000, 2),
    (98, 20, 8, 2000, 2), (99, 21, 8, 4000, 2), (100, 22, 8, 6000, 2), (101, 23, 8, 2000, 2),
    (102, 24, 8, 6000, 2), (103, 25, 8, 4000, 2), (104, 11, 9, 4000, 2), (105, 12, 9, 6000, 2),
    (106, 13, 9, 4000, 2), (107, 14, 9, 8000, 2), (108, 15, 9, 2000, 2), (109, 16, 9, 6000, 2),
    (111, 140, 1, 3000, 0), (112, 109, 1, 4000, 0), (113, 113, 1, 4000, 0), (114, 40, 5, 4000, 0),
    (115, 35, 7, 4000, 0), (116, 33, 2, 1000, 0), (117, 28, 3, 1000, 0), (118, 136, 1, 5000, 1),
    (119, 110, 1, 5000, 1), (120, 16, 11, 15000, 1), (121, 6, 12, 25000, 5), (122, 7, 12, 25000, 5),
    (123, 8, 12, 25000, 5), (124, 9, 12, 25000, 5), (125, 10, 12, 25000, 5),
    (126, 11, 12, 25000, 5), (127, 12, 12, 25000, 5), (128, 13, 12, 25000, 5),
    (129, 14, 12, 25000, 5), (130, 15, 12, 25000, 5), (131, 162, 1, 3000, 0), (132, 40, 6, 3500, 0),
    (133, 84, 2, 1500, 0), (134, 104, 3, 2000, 0), (135, 7, 13, 5000, 0), (136, 97, 2, 2500, 0),
    (137, 33, 8, 3500, 0), (138, 18, 9, 4000, 0), (139, 57, 5, 3000, 0), (140, 49, 3, 2500, 0),
    (141, 38, 7, 5000, 0), (142, 191, 1, 3000, 0), (143, 99, 2, 1500, 0), (144, 88, 3, 2000, 0),
    (145, 229, 1, 5000, 1), (146, 86, 3, 1500, 1), (147, 58, 4, 4000, 1), (148, 83, 6, 3000, 1),
    (149, 45, 8, 3500, 1), (150, 12, 13, 6000, 1), (151, 206, 1, 2000, 1), (152, 94, 2, 1000, 1),
    (153, 60, 5, 2500, 1), (154, 107, 5, 3000, 1), (155, 69, 9, 5000, 1), (156, 77, 4, 4000, 1),
    (157, 49, 2, 1000, 1), (158, 70, 3, 1000, 1), (159, 137, 4, 4000, 1), (160, 27, 13, 6000, 1),
    (161, 225, 1, 3000, 1), (162, 85, 3, 1500, 1), (163, 78, 7, 2500, 1), (164, 58, 9, 3500, 1),
    (165, 104, 6, 2500, 1), (166, 168, 1, 5000, 3), (167, 230, 1, 6000, 3), (168, 253, 1, 7000, 3),
    (169, 358, 1, 6000, 3), (170, 126, 4, 7000, 3), (171, 111, 4, 6000, 3), (172, 144, 4, 5000, 3),
    (173, 164, 4, 7000, 3), (174, 64, 5, 5000, 3), (175, 49, 5, 6000, 3), (176, 107, 6, 6000, 3),
    (177, 136, 6, 5000, 3), (178, 86, 6, 7000, 3), (179, 77, 7, 7000, 3), (180, 85, 7, 6000, 3),
    (181, 68, 7, 5000, 3), (182, 52, 8, 6000, 3), (183, 68, 8, 7000, 3), (184, 50, 9, 6000, 3),
    (185, 56, 9, 6000, 3), (186, 6, 13, 6000, 3), (187, 16, 12, 40000, 3), (188, 17, 12, 2000, 4),
    (189, 181, 1, 300, 4), (190, 186, 1, 500, 4), (191, 196, 1, 400, 4), (192, 203, 1, 600, 4),
    (193, 250, 1, 300, 4), (194, 263, 1, 600, 4), (195, 314, 1, 400, 4), (196, 421, 1, 500, 4),
    (197, 443, 1, 600, 4), (198, 450, 1, 500, 4), (199, 471, 1, 400, 4), (200, 66, 4, 500, 4),
    (201, 81, 4, 600, 4), (202, 125, 4, 500, 4), (203, 129, 4, 400, 4), (204, 131, 4, 500, 4),
    (205, 168, 4, 600, 4), (206, 53, 5, 600, 4), (207, 98, 5, 300, 4), (208, 112, 5, 500, 4),
    (209, 113, 5, 400, 4), (210, 116, 5, 500, 4), (211, 45, 6, 500, 4), (212, 52, 6, 500, 4),
    (213, 100, 6, 600, 4), (214, 140, 6, 600, 4), (215, 149, 6, 600, 4), (216, 56, 7, 400, 4),
    (217, 65, 7, 300, 4), (218, 75, 7, 500, 4), (219, 87, 7, 400, 4), (220, 34, 8, 500, 4),
    (221, 51, 8, 500, 4), (222, 63, 8, 600, 4), (223, 55, 5, 300, 4), (224, 29, 9, 500, 4),
    (225, 54, 9, 300, 4), (226, 60, 9, 400, 4), (227, 65, 9, 300, 4), (228, 10, 13, 500, 4),
    (229, 30, 13, 500, 4), (230, 40, 13, 600, 4), (231, 5, 14, 600, 4), (232, 77, 2, 100, 4),
    (233, 71, 2, 100, 4), (234, 64, 2, 100, 4), (235, 100, 2, 100, 4), (236, 89, 3, 100, 4),
    (237, 106, 3, 100, 4), (238, 43, 3, 100, 4), (239, 72, 3, 100, 4), (240, 98, 2, 100, 4),
    (241, 77, 3, 100, 4), (242, 74, 2, 100, 4), (243, 59, 3, 100, 4), (244, 18, 12, 15000, 3),
    (245, 19, 12, 15000, 3), (246, 20, 12, 15000, 3), (247, 2, 15, 20000, 3),
    (248, 3, 15, 20000, 3), (249, 4, 15, 20000, 3), (250, 5, 15, 20000, 3), (251, 6, 15, 20000, 3),
    (252, 18, 11, 1500, 4), (253, 19, 11, 15000, 3), (254, 465, 1, 400, 4), (255, 62, 4, 500, 4),
    (256, 63, 4, 300, 4), (257, 81, 5, 500, 4), (258, 78, 6, 300, 4), (259, 80, 6, 400, 4),
    (260, 80, 7, 300, 4), (261, 82, 7, 500, 4), (262, 49, 8, 500, 4), (263, 62, 8, 300, 4),
    (264, 64, 9, 500, 4), (265, 66, 9, 500, 4), (266, 41, 13, 400, 4), (267, 42, 13, 500, 4),
    (268, 463, 1, 400, 4), (269, 464, 1, 500, 4), (270, 185, 1, 6000, 3), (271, 190, 1, 5000, 3),
    (272, 65, 5, 5000, 3), (273, 54, 5, 5000, 3), (274, 95, 6, 7000, 3), (275, 141, 6, 500, 4),
    (276, 22, 9, 400, 4), (277, 290, 1, 400, 4), (278, 360, 1, 6000, 3), (279, 80, 4, 6000, 3),
    (280, 87, 4, 400, 4), (281, 456, 1, 400, 4), (282, 480, 1, 5000, 3), (283, 132, 6, 6000, 3),
    (284, 478, 1, 400, 4), (285, 474, 1, 300, 4),
];

pub const PER_PAGE: usize = 8;

/// The shop's state: its page, the item whose Buy popup is up, and how many times the
/// shopkeeper has said you can't afford something (his three lines take turns).
#[derive(Clone, Debug, Default)]
pub struct Shop {
    pub page: usize,
    pub buying: Option<u16>,
    pub react: u8,
}

pub enum Effect {
    None,
    /// The shopkeeper's line (text, his portrait)
    Say(String, u8),
    Bought,
}

pub fn items(shop: u8) -> impl Iterator<Item = &'static (u16, u16, u8, u32, u8)> {
    ITEMS.iter().filter(move |it| it.4 == shop)
}

pub fn item(index: u16) -> Option<&'static (u16, u16, u8, u32, u8)> {
    ITEMS.iter().find(|it| it.0 == index)
}

pub fn pages(shop: u8) -> usize {
    items(shop).count().div_ceil(PER_PAGE).max(1)
}

/// `GJStoreItem::getCurrencyKey`: the diamond shop takes Diamond Shards, the others orbs.
pub fn currency(shop: u8) -> u32 {
    if shop == 4 { 29 } else { 14 }
}

pub fn bought(p: &Progress, index: u16) -> bool {
    p.flag(&format!("bought {index}"))
}

/// `ItemInfoPopup::nameForUnlockType`.
pub fn type_name(kind: u8, id: u16) -> &'static str {
    match kind {
        1 => "Cube",
        2 => "Main Color",
        3 => "Secondary Color",
        4 => "Ship",
        5 => "Ball",
        6 => "UFO",
        7 => "Wave",
        8 => "Robot",
        9 => "Spider",
        10 => "Trail",
        11 => "Death Effect",
        13 => "Swing",
        14 => "Jetpack",
        15 => "Ship Fire",
        _ => match id {
            4 => "Master Emblem",
            16 => "Music Customizer",
            17 => "Music Unlocker",
            _ => "Item",
        },
    }
}

/// `GameToolbox::pointsToString`: thousands with commas.
pub fn points(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The slot's center (569 × 320 frame): four across, two rows.
fn slot(i: usize) -> (f32, f32) {
    (157.0 + 85.0 * (i % 4) as f32, if i < 4 { 130.5 } else { 55.5 })
}

/// `GJItemIcon::darkenStoreItem`: each shop's dark color.
fn dark(shop: u8) -> [u8; 3] {
    match shop {
        1 => [10, 30, 20],
        2 => [33, 22, 66],
        3 => [20, 20, 40],
        4 => [20, 30, 50],
        5 => [50, 50, 50],
        _ => [60, 30, 20],
    }
}

/// Which icon kind (`garage::KINDS`) an unlock type is, for the icon types.
fn icon_kind(kind: u8) -> Option<usize> {
    Some(match kind {
        1 => 0,
        4 => 1,
        5 => 2,
        6 => 3,
        7 => 4,
        8 => 5,
        9 => 6,
        13 => 7,
        14 => 8,
        _ => return None,
    })
}

/// An item as `GJItemIcon::createStoreItem` shows it (white and cyan icons, or its color's
/// swatch with "1" / "2"), at (x, y) scaled `s` (1 in the shop); `dim` darkens it as bought.
pub fn draw_item(c: &mut Canvas, kind: u8, id: u16, x: f32, y: f32, s: f32, dim: Option<[u8; 3]>) {
    let alpha = if dim.is_some() { 120 } else { 255 };
    let (c1, c2) = (dim.unwrap_or([255, 255, 255]), [0, 255, 255]);
    if let Some(k) = icon_kind(kind) {
        let (dy, sc) = match k {
            2 => (0.0, 0.99),
            3 => (-7.7, 1.1),
            6 => (-1.65, 1.1),
            _ => (0.0, 1.1),
        };
        let start = c.mark();
        garage::draw_icon(c, &garage::icon_sheet(k, id as u32), x, y + dy * s, sc * s, c1, if dim.is_some() { c1 } else { c2 });
        if dim.is_some() {
            c.fade_since(start, alpha as f32 / 255.0);
        }
        return;
    }
    let look = |l: Look| match dim {
        Some(d) => l.rgb(d).alpha(alpha),
        None => l,
    };
    match kind {
        2 | 3 => {
            let col = color_for_idx(id as usize);
            c.sprite("player_special_01_001.png", x, y, look(Look::scaled(1.1 * s).rgb(col)));
            c.text("bigFont", if kind == 2 { "1" } else { "2" }, x, y, look(Look::scaled(0.55 * s).rgb(col)));
        }
        10 => c.sprite(&format!("player_special_{id:02}_001.png"), x, y, look(Look::scaled(1.1 * s))),
        11 => c.sprite(&format!("explosionIcon_{id:02}_001.png"), x, y, look(Look::scaled(1.1 * s))),
        15 => c.sprite(&format!("shipfireIcon_{id:02}_001.png"), x, y, look(Look::scaled(1.1 * s))),
        _ => c.sprite(&format!("gjItem_{id:02}_001.png"), x, y, look(Look::scaled(1.1 * s))),
    }
}

/// The icon sheets a page needs.
pub fn wanted_sheets(shop: u8, s: &Shop) -> Vec<String> {
    let page = items(shop).skip(s.page * PER_PAGE).take(PER_PAGE);
    let buying = s.buying.and_then(item);
    page.chain(buying).filter_map(|it| icon_kind(it.2).map(|k| garage::icon_sheet(k, it.1 as u32))).collect()
}

/// The shop: the captured scene without its captured shelves, then this page's items, the
/// page dots and the currency counter.
pub fn draw(c: &mut Canvas, ctx: &mut Ctx, shop: u8, s: &Shop, p: &Progress) {
    let dx = (c.width - 569.0) * 0.5;
    const ICONS: [&str; 15] = ["player_", "ship_", "bird_", "dart_", "robot_", "spider_", "swing_", "jetpack_", "explosionIcon_", "gjItem_", "shipfireIcon_", "currencyOrbIcon_001", "currencyDiamondIcon_001", "smallDot", "player_special_"];
    // the community shop's room is the shop's in another hue (`CCSpriteWithHue`; −110° in the
    // code, but the real screen's colors are the channels turned by exactly −120°)
    let hued = shop == 2;
    if hued {
        let bg = Look::default().scale_xy(1.1958, 1.0156).anchor(0.0, 0.0).rgb([240, 0, 240]).hue(-120.0);
        c.sprite("shopBG_001.png", -2.5 + dx, -2.5, bg);
        c.sprite("storeDesk_001.png", 284.5 + dx, 95.0, Look::default().scale_xy(0.96, 1.0).hue(-120.0));
    }
    replay::draw_where(c, ctx, Scene::Shop(shop), &|k, name, x, y| match k {
        'S' if hued && (name == "shopBG_001.png" || name == "storeDesk_001.png") => false,
        // the shelves' items and prices, on every captured page
        'S' => !(ICONS.iter().any(|p| name.starts_with(p)) && (y < 150.0 || !(0.0..=569.0).contains(&x))),
        'T' => y >= 150.0,
        _ => true,
    });
    let cur = currency(shop);
    let icon = if cur == 29 { "currencyDiamondIcon_001.png" } else { "currencyOrbIcon_001.png" };
    // the price's currency icon sits right of the label (offsets from the captured "2,000")
    let (rw, _) = c.text_size("bigFont", "2,000");
    let gap = if cur == 29 { 26.18 } else { 25.08 } - rw * 0.385 * 0.5;
    for (i, it) in items(shop).skip(s.page * PER_PAGE).take(PER_PAGE).enumerate() {
        let (x, y) = slot(i);
        let (x, y) = (x + dx, y);
        let owned = bought(p, it.0);
        draw_item(c, it.2, it.1, x, y, 1.0, owned.then(|| dark(shop)));
        if !owned {
            let text = points(it.3);
            let (tw, _) = c.text_size("bigFont", &text);
            let lx = x - 4.4;
            c.text("bigFont", &text, lx, y - 23.65, Look::scaled(0.385));
            c.sprite(icon, lx + tw * 0.385 * 0.5 + gap, y - 24.2, Look::scaled(0.55));
        }
    }
    let n = pages(shop);
    for i in 0..n {
        let x = 284.5 - (n as f32 - 1.0) * 8.0 + 16.0 * i as f32 + dx;
        let col = if i == s.page { [255, 255, 255] } else { [125, 125, 125] };
        c.sprite("smallDot.png", x, 6.0, Look::default().rgb(col));
    }
    // the counter (`CCCounterLabel`, bigFont), right of it the currency's icon
    let have = p.currency(cur).max(0);
    c.text("bigFont", &have.to_string(), 532.0 + dx, 304.0, Look::scaled(0.6).anchor(1.0, 0.5));
}

/// `PurchaseItemPopup` (GJ_square02, 260 × 190): the item, "Do you want to buy this …",
/// Cancel and Buy.
pub fn draw_buy(c: &mut Canvas, ctx: &mut Ctx, index: u16) {
    let Some(&(_, id, kind, price, shop)) = item(index) else { return };
    let (w, h) = (c.width, c.height);
    let (cx, cy) = (w * 0.5, h * 0.5);
    c.nine_slice("GJ_square02.png", cx, cy, 260.0, 190.0, Look::default());
    c.text("bigFont", "Buy Item", cx, cy + 79.0, Look::scaled(0.6));
    draw_item(c, kind, id, cx, cy + 22.0, 1.2 / 1.1, None);
    let mut name = type_name(kind, id).to_string();
    if name.len() > 7 {
        name.insert(0, '\n');
    }
    let what = if currency(shop) == 29 { "<cc>Diamond Shards</c>" } else { "<cj>Mana Orbs</c>" };
    let text = format!("Do you want to <cg>buy</c> this <co>{name}</c>\nfor <cy>{}</c> {what}?", points(price));
    let lines = canvas::rich_lines(c, "bigFont", &text, 999.0);
    canvas::draw_rich(c, "bigFont", &lines, cx, cy - 13.0, 0.4, 35.0, 255);
    for (id, frame, label, x, ls) in [(Btn::ShopCancel, "GJ_button_06.png", "Cancel", cx - 40.5, 0.4149), (Btn::ShopBuy, "GJ_button_01.png", "Buy", cx + 40.5, 0.6)] {
        let f = ctx.factor(id);
        c.nine_slice(frame, x, cy - 73.0, 66.0 * f, 30.0 * f, Look::default());
        c.text("bigFont", label, x - f, cy - 71.0, Look::scaled(ls * f));
        ctx.area(id, x, cy - 73.0, 66.0, 30.0);
    }
}

impl Shop {
    /// A click on the scene's button `i` (`replay` hit areas): back is the caller's.
    pub fn click(&mut self, shop: u8, (bx, by): (f32, f32), p: &Progress) -> Effect {
        let n = pages(shop);
        if (by - 93.0).abs() < 1.0 {
            self.page = if bx < 284.5 { (self.page + n - 1) % n } else { (self.page + 1) % n };
            return Effect::None;
        }
        let Some(i) = (0..PER_PAGE).find(|&i| {
            let (x, y) = slot(i);
            (x - bx).abs() < 1.0 && (y - by).abs() < 1.0
        }) else {
            return Effect::None;
        };
        let Some(&(index, _, _, price, _)) = items(shop).nth(self.page * PER_PAGE + i) else { return Effect::None };
        if bought(p, index) {
            return Effect::None;
        }
        let cur = currency(shop);
        if p.currency(cur) >= price as i64 {
            self.buying = Some(index);
            return Effect::None;
        }
        // GJShopLayer 0x2a33c4: three lines in turn
        let what = if cur == 29 { "<cc>Diamond Shards</c>" } else { "<cl>Mana Orbs</c>" };
        let (text, face) = match self.react {
            1 => (format!("<cg>Collect</c> more {what}."), 5),
            2 => (format!("You <co>do not</c> have enough {what}."), 6),
            _ => ("You cannot <cg>afford</c> that <cl>item</c>.".to_string(), 5),
        };
        self.react = (self.react + 1) % 3;
        Effect::Say(text, face)
    }

    /// The Buy popup's buttons (`PurchaseItemPopup::onPurchase` → `purchaseItem`).
    pub fn popup(&mut self, buy: bool, p: &mut Progress) -> Effect {
        let Some(index) = self.buying.take() else { return Effect::None };
        if !buy || bought(p, index) {
            return Effect::None;
        }
        let Some(&(_, _, _, price, shop)) = item(index) else { return Effect::None };
        let cur = currency(shop);
        if p.currency(cur) < price as i64 {
            return Effect::None;
        }
        p.add_currency(cur, -(price as i64));
        p.set_flag(&format!("bought {index}"));
        Effect::Bought
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shops_like_the_game() {
        // the page dots the captured shops show
        assert_eq!((0..5).map(pages).collect::<Vec<_>>(), [6, 5, 9, 5, 10]);
        // the first page of the shop, as captured
        let first: Vec<u16> = items(0).take(8).map(|it| it.1).collect();
        assert_eq!(first, [79, 77, 86, 73, 102, 107, 27, 25]);
        assert_eq!(points(500), "500");
        assert_eq!(points(40000), "40,000");
        let mut p = Progress::default();
        let mut s = Shop::default();
        p.add_currency(14, 600);
        // slot 0 (500 orbs): the popup, then Buy spends the orbs
        assert!(matches!(s.click(0, (157.0, 130.5), &p), Effect::None));
        assert_eq!(s.buying, Some(34));
        assert!(matches!(s.popup(true, &mut p), Effect::Bought));
        assert_eq!(p.currency(14), 100);
        assert!(bought(&p, 34));
        // 1,000 orbs: the shopkeeper's lines in turn
        assert!(matches!(s.click(0, (242.0, 130.5), &p), Effect::Say(ref t, 5) if t.contains("afford")));
        assert!(matches!(s.click(0, (242.0, 130.5), &p), Effect::Say(ref t, 5) if t.starts_with("<cg>Collect")));
        assert!(matches!(s.click(0, (242.0, 130.5), &p), Effect::Say(_, 6)));
    }
}
