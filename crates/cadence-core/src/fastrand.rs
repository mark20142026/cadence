//! `GameToolbox::fast_rand` (0x44d2b8): the game's own random numbers for the random triggers
//! and advanced follow. A 64-bit linear congruential generator, seed · 214013 + 2531011, giving
//! bits 16..31 masked to 15 bits; `fast_rand_0_1` divides that by 32767 (so 1.0 can come out),
//! `fast_rand_minus1_1` maps it to −1..1.
//!
//! The game seeds it (`fast_srand`) at every `PlayLayer::resetLevel` from the clock
//! (`gettimeofday`: seconds · microseconds), so its rolls can't be replayed; Cadence keeps a
//! fixed seed per run instead, so a run replays exactly. The numbers it can produce, and their
//! steps of 1/32767, are the game's.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FastRand {
    pub seed: u64,
}

impl FastRand {
    pub fn new(seed: u64) -> FastRand {
        FastRand { seed }
    }

    /// `fast_rand`: 0..=32767.
    pub fn next(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(214013).wrapping_add(2531011);
        ((self.seed >> 16) & 0x7fff) as u32
    }

    /// `fast_rand_0_1`: 0..=1 in steps of 1/32767.
    pub fn unit(&mut self) -> f32 {
        self.next() as f32 / 32767.0
    }

    /// `fast_rand_minus1_1`.
    pub fn minus1_1(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_lcg() {
        // the classic MSVC rand() sequence for seed 1
        let mut r = FastRand::new(1);
        assert_eq!([r.next(), r.next(), r.next(), r.next()], [41, 18467, 6334, 26500]);
        let mut r = FastRand::new(1);
        assert_eq!(r.unit(), 41.0 / 32767.0);
    }
}
