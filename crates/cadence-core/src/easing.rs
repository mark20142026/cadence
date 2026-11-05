//! `GameToolbox::getEasedValue` (0x45155c): the easings move, rotate and other tween triggers
//! use. Ported instruction by instruction: f32 arithmetic with the fused multiply-adds where
//! the game has them, the bounce segments compared in f64 (`t < 1/2.75` as doubles), the libm
//! calls as the game makes them (`crate::mathf`).

use crate::mathf::{cosf, exp2f, powf, sinf};

/// The f32 π the game multiplies by (0x40490fdb).
const PI: f32 = std::f32::consts::PI;
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

/// cocos2d's `bounceTime` as the game compiles it (0x451474).
fn bounce_time(t: f32) -> f32 {
    let d = t as f64;
    let (c, o) = if d < 1.0 / 2.75 {
        return (t * 7.5625) * t;
    } else if d < 2.0 / 2.75 {
        (-0.545_454_56f32, 0.75f32)
    } else if d < 2.5 / 2.75 {
        (-0.818_181_8, 0.9375)
    } else {
        (-0.954_545_44, 0.984_375)
    };
    let a = t + c;
    (a * 7.5625).mul_add(a, o)
}

/// The eased value of `t` (0..1). `rate` ≤ 0 means the default 2.
///
/// Easings: 0 none, 1 ease in-out, 2 ease in, 3 ease out, 4–6 elastic (in-out, in, out),
/// 7–9 bounce, 10–12 exponential, 13–15 sine, 16–18 back.
pub fn eased(t: f32, rate: f32, easing: i32) -> f32 {
    let rate = if rate > 0.0 { rate } else { 2.0 };
    match easing {
        1 => {
            let t2 = t + t;
            if t2 < 1.0 { powf(t2, rate) * 0.5 } else { powf(2.0 - t2, rate).mul_add(-0.5, 1.0) }
        }
        2 => powf(t, rate),
        3 => powf(t, 1.0 / rate),
        4 => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let quarter = rate * 0.25;
            let s = (t + t) + -1.0;
            if s < 0.0 {
                let e = exp2f(s * 10.0) * -0.5;
                let a = (s - quarter) * PI;
                e * sinf((a + a) / rate)
            } else {
                let e = exp2f(s * -10.0);
                let a = (s - quarter) * PI;
                (e * sinf((a + a) / rate)).mul_add(0.5, 1.0)
            }
        }
        5 => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let quarter = rate * -0.25;
            let s = t + -1.0;
            let e = exp2f(s * 10.0);
            let a = (s + quarter) * -PI;
            sinf((a + a) / rate) * e
        }
        6 => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let quarter = rate * -0.25;
            let e = exp2f(t * -10.0);
            let a = (t + quarter) * PI;
            e.mul_add(sinf((a + a) / rate), 1.0)
        }
        7 => {
            if t < 0.5 {
                (1.0 - bounce_time(1.0 - (t + t))) * 0.5
            } else {
                bounce_time(t.mul_add(2.0, -1.0)).mul_add(0.5, 0.5)
            }
        }
        8 => 1.0 - bounce_time(1.0 - t),
        9 => bounce_time(t),
        10 => {
            let t2 = t + t;
            let s = t2 + -1.0;
            if t2 < 1.0 { exp2f(s * 10.0) * 0.5 } else { (2.0 - exp2f(s * -10.0)) * 0.5 }
        }
        11 => {
            if t == 0.0 {
                0.0
            } else {
                exp2f((t + -1.0) * 10.0) + -0.001
            }
        }
        12 => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - exp2f(t * -10.0)
            }
        }
        13 => (cosf(t * PI) + -1.0) * -0.5,
        14 => 1.0 - cosf(t * HALF_PI),
        15 => sinf(t * HALF_PI),
        16 => {
            let t2 = t + t;
            if t2 < 1.0 {
                (t2 * t2) * t2.mul_add(3.594_909_4, -2.594_909_4) * 0.5
            } else {
                let s = t2 + -2.0;
                (s * s) * s.mul_add(3.594_909_4, 2.594_909_4) * 0.5 + 1.0
            }
        }
        17 => (t * t) * t.mul_add(2.701_58, -1.701_58),
        18 => {
            let s = t + -1.0;
            (s * s).mul_add(s.mul_add(2.701_58, 1.701_58), 1.0)
        }
        _ => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_binary() {
        assert_eq!(PI.to_bits(), 0x4049_0fdb);
        assert_eq!(HALF_PI.to_bits(), 0x3fc9_0fdb);
        assert_eq!((-0.545_454_56f32).to_bits(), 0xbf0b_a2e9);
        assert_eq!((-0.818_181_8f32).to_bits(), 0xbf51_745d);
        assert_eq!((-0.954_545_44f32).to_bits(), 0xbf74_5d17);
        assert_eq!(3.594_909_4f32.to_bits(), 0x4066_12ff);
        assert_eq!(2.594_909_4f32.to_bits(), 0x4026_12ff);
        assert_eq!(2.701_58f32.to_bits(), 0x402c_e6b0);
        assert_eq!(1.701_58f32.to_bits(), 0x3fd9_cd60);
        assert_eq!((-0.001f32).to_bits(), 0xba83_126f);
    }

    #[test]
    fn ends() {
        for e in 0..=18 {
            assert!((eased(1.0, 2.0, e) - 1.0).abs() < 2e-3, "easing {e}");
            assert!(eased(0.0, 2.0, e).abs() < 2e-3, "easing {e}");
        }
    }
}
