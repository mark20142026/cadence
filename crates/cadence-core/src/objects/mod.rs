//! Object ids: their type and collision rectangle, as reported by the real game.

mod table;
mod visuals;

use std::sync::OnceLock;

/// The game's object type (`GameObjectType`). Stored as the raw value so unknown types survive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectType(pub u8);

impl ObjectType {
    pub const SOLID: Self = Self(0);
    pub const HAZARD: Self = Self(2);
    pub const INVERSE_GRAVITY_PORTAL: Self = Self(3);
    pub const NORMAL_GRAVITY_PORTAL: Self = Self(4);
    pub const SHIP_PORTAL: Self = Self(5);
    pub const CUBE_PORTAL: Self = Self(6);
    pub const DECORATION: Self = Self(7);
    pub const YELLOW_PAD: Self = Self(8);
    pub const PINK_PAD: Self = Self(9);
    pub const GRAVITY_PAD: Self = Self(10);
    pub const YELLOW_RING: Self = Self(11);
    pub const PINK_RING: Self = Self(12);
    pub const GRAVITY_RING: Self = Self(13);
    pub const MIRROR_PORTAL: Self = Self(14);
    pub const MIRROR_OFF_PORTAL: Self = Self(15);
    pub const BALL_PORTAL: Self = Self(16);
    pub const NORMAL_SIZE_PORTAL: Self = Self(17);
    pub const MINI_PORTAL: Self = Self(18);
    pub const DUAL_PORTAL: Self = Self(23);
    pub const SOLO_PORTAL: Self = Self(24);
    pub const GREEN_RING: Self = Self(29);
    pub const RED_PAD: Self = Self(34);
    pub const RED_RING: Self = Self(35);
    /// Dash orbs: green (1704) dashes, pink (1751) flips gravity first.
    /// The black orb (1330): shoots the player toward gravity.
    pub const BLACK_RING: Self = Self(32);
    pub const DASH_RING: Self = Self(37);
    pub const PINK_DASH_RING: Self = Self(38);
    pub const UFO_PORTAL: Self = Self(19);
    pub const MODIFIER: Self = Self(20);
    pub const BREAKABLE: Self = Self(21);
    /// Not a game type: speed portals are modifiers (20) that Cadence collides like portals.
    pub const SPEED_PORTAL: Self = Self(250);
    pub const SLOPE: Self = Self(25);
    pub const WAVE_PORTAL: Self = Self(26);
    pub const ROBOT_PORTAL: Self = Self(27);
    /// The blue teleport portal (747); its orange end is a decoration.
    pub const TELEPORT_PORTAL: Self = Self(28);
    pub const SPIDER_PORTAL: Self = Self(33);
    /// The spider orb (3004): a spider jump for any mode.
    pub const SPIDER_RING: Self = Self(43);
    /// Special blocks (`PlayerObject::touchedObject`): 1755 dart slide, 1813 no auto jump, 1829
    /// stop dash, 1859 hit head, 2866 flip gravity; 2069/3645 force blocks.
    pub const SPECIAL_BLOCK: Self = Self(40);
    pub const SWING_PORTAL: Self = Self(41);
    pub const ANIMATED_HAZARD: Self = Self(47);
    /// Custom (toggle / spawn) orbs (1594): pressed like an orb, they toggle (key 56 on) or,
    /// with key 504, spawn group 51 (`activateCustomRing`).
    pub const CUSTOM_RING: Self = Self(36);

    /// Orbs (`RingObject`): their hitbox ignores the object's rotation (`RingObject::setRotation`
    /// leaves the box alone; real s13-22: a dash orb at −45° is touched by its 36×36 square).
    pub fn is_ring_object(self) -> bool {
        matches!(self.0, 11 | 12 | 13 | 29 | 32 | 35 | 36 | 37 | 38 | 43)
    }
}

/// What the game says about one object id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectInfo {
    pub id: u16,
    pub kind: ObjectType,
    /// Collision rect relative to the object's position (unrotated, unscaled).
    pub rect_x: f32,
    pub rect_y: f32,
    pub rect_w: f32,
    pub rect_h: f32,
    /// Base size (`m_width`/`m_height`) and radius.
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

/// Look up an object id (O(1): a dense index built once).
pub fn info(id: i32) -> Option<&'static ObjectInfo> {
    static INDEX: OnceLock<Vec<u16>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let mut index = vec![u16::MAX; table::MAX_ID + 1];
        for (i, obj) in table::OBJECTS.iter().enumerate() {
            index[obj.id as usize] = i as u16;
        }
        index
    });
    let slot = *index.get(usize::try_from(id).ok()?)?;
    table::OBJECTS.get(slot as usize)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteRole {
    Main,
    Detail,
    Glow,
    Child,
}

/// One sprite of an object's sprite tree. Transforms follow cocos2d: a child's position is in
/// its parent's content space (origin bottom-left); `anchor` is relative to its own size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDef {
    /// Index of the parent in the same list, or -1 for the root (the object itself).
    pub parent: i8,
    pub role: SpriteRole,
    /// Frame name in the game's sprite sheets ("" if unknown).
    pub frame: &'static str,
    pub x: f32,
    pub y: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub w: f32,
    pub h: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub z: i16,
    pub visible: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub additive: bool,
}

/// How an object id looks by default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectVisual {
    pub id: u16,
    /// Default color channels (1004 = OBJ, -1 = none).
    pub base_color: i16,
    pub detail_color: i16,
    pub z_layer: i8,
    pub z_order: i16,
    pub glow: bool,
    pub sprites: &'static [SpriteDef],
}

/// Look up an object id's visual (O(1)).
pub fn visual(id: i32) -> Option<&'static ObjectVisual> {
    static INDEX: OnceLock<Vec<u16>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let max = visuals::VISUALS.iter().map(|v| v.id as usize).max().unwrap_or(0);
        let mut index = vec![u16::MAX; max + 1];
        for (i, v) in visuals::VISUALS.iter().enumerate() {
            index[v.id as usize] = i as u16;
        }
        index
    });
    let slot = *index.get(usize::try_from(id).ok()?)?;
    visuals::VISUALS.get(slot as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_has_the_small_hazard_box() {
        let spike = info(8).unwrap();
        assert_eq!(spike.kind, ObjectType::HAZARD);
        assert_eq!((spike.rect_x, spike.rect_y, spike.rect_w, spike.rect_h), (-3.0, -6.0, 6.0, 12.0));
        assert_eq!(info(1).unwrap().kind, ObjectType::SOLID);
        assert!(info(0).is_none());
        assert!(info(-1).is_none());
    }

    #[test]
    fn portal_has_its_extra_sprites() {
        let portal = visual(10).unwrap();
        assert_eq!(portal.sprites[0].frame, "portal_01_front_001.png");
        assert!(portal.sprites.iter().any(|s| s.frame == "portal_01_extra_001.png" && s.parent == 0));
    }
}
