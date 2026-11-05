//! How the game plays an SFX trigger's sound (`GameManager::playSFXTrigger` 0x2ef690 →
//! `FMODAudioEngine::playEffectAdvanced` 0x351e14), as data and DSP the audio backends share:
//!
//! - [`SfxPlay::effect`]: the random speed / pitch / volume ranges (596 / 597 / 598, 599 whole
//!   semitone steps) rolled with the C library's `rand()`, as `playSFXTrigger` rolls them;
//! - [`Voice`]: one FMOD channel's playback — the start offset (408), the end (410) as a stop
//!   at a DSP clock computed with the starting speed, fade in (409) / fade out (411) as FMOD
//!   fade points in real time, loops (413) between the start and end, `setLoopCount(0)` (edit
//!   SFX 414), the speed (`setPitch`: the playback rate) and volume changing while it plays;
//! - [`pitch_shift`]: the pitch (405) is a pitch-shift DSP on the channel
//!   (FMOD_DSP_TYPE_PITCHSHIFT, FFT size 1024, or 2048 with 412), which keeps the duration: a
//!   phase vocoder with the same FFT sizes and 4× overlap;
//! - [`Reverb`]: the reverb channel group's FMOD_DSP_TYPE_SFXREVERB with the game's 22 presets
//!   (`updateReverb` 0x34e628, the tables at 0x1008e5544). FMOD's reverb itself isn't public;
//!   this is a feedback-delay-network reverb driven by the same parameters (decay, early and
//!   late delays, HF decay ratio at 5 kHz, diffusion, density, high cut, early/late mix, wet
//!   level), so it sounds alike but not sample for sample.

use std::sync::Arc;

/// `FMODAudioEngine::pitchForIdx` (0x353438): semitones −12..12 as a rate (its table; any other
/// index is 1).
pub fn pitch_for_index(index: i32) -> f32 {
    const UP: [u32; 13] = [
        0x3f800000, 0x3f879c7d, 0x3f8fb01d, 0x3f9837f0, 0x3fa14518, 0x3faadc08, 0x3fb504f3, 0x3fbfc887, 0x3fcb2ff5, 0x3fd744fd, 0x3fe411f0,
        0x3ff1a1bf, 0x40000000,
    ];
    const DOWN: [u32; 13] = [
        0x3f800000, 0x3f71a1bf, 0x3f6411f0, 0x3f5744fd, 0x3f4b2ff5, 0x3f3fc887, 0x3f3504f3, 0x3f2adc08, 0x3f214518, 0x3f1837f0, 0x3f0facd6,
        0x3f079c7d, 0x3f000000,
    ];
    match index {
        0..=12 => f32::from_bits(UP[index as usize]),
        -12..=-1 => f32::from_bits(DOWN[(-index) as usize]),
        _ => 1.0,
    }
}

/// The C library's `rand()` as the macOS libc has it (Park–Miller "minimal standard", 16807 mod
/// 2³¹ − 1, seed 1 unless seeded), divided by 2.1474836e9 like the game: 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CRand {
    state: u32,
}

impl Default for CRand {
    fn default() -> CRand {
        CRand { state: 1 }
    }
}

impl CRand {
    pub fn new(seed: u32) -> CRand {
        CRand { state: seed }
    }

    pub fn next(&mut self) -> i32 {
        let mut x = self.state as i64;
        if x == 0 {
            x = 123459876;
        }
        let (hi, lo) = (x / 127773, x % 127773);
        x = 16807 * lo - 2836 * hi;
        if x < 0 {
            x += 0x7fffffff;
        }
        self.state = x as u32;
        (x % (0x7fffffff_i64 + 1)) as i32
    }

    /// `(float)rand() / 2.1474836e9`.
    pub fn unit(&mut self) -> f32 {
        self.next() as f32 / 2.147_483_6e9
    }
}

/// What a backend needs to start an SFX trigger's sound (`playEffectAdvanced`'s arguments).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectParams {
    /// Channel volume (≤ 2) and rate (`setPitch`).
    pub volume: f32,
    pub speed: f32,
    /// Pitch-shift factor (1: none) and its FFT size (412: 2048, else 1024).
    pub pitch: f32,
    pub big_fft: bool,
    /// Milliseconds: start offset, end (≤ 0: the sound's end), fade in, fade out.
    pub start_ms: i32,
    pub end_ms: i32,
    pub fade_in_ms: i32,
    pub fade_out_ms: i32,
    pub looped: bool,
    /// Into the reverb channel group (407).
    pub reverb: bool,
}

impl Default for EffectParams {
    fn default() -> EffectParams {
        EffectParams { volume: 1.0, speed: 1.0, pitch: 1.0, big_fft: false, start_ms: 0, end_ms: 0, fade_in_ms: 0, fade_out_ms: 0, looped: false, reverb: false }
    }
}

impl crate::groups::SfxPlay {
    /// `playSFXTrigger`: speed and pitch from their indices, each with its random range (599:
    /// a whole number of semitones within ±range; else a rate between the range's ends), the
    /// volume ± its range clamped to 0..2 (each bound test rolls again, as the game's clamp
    /// does), then `playEffectAdvanced`'s volume cap at 2.
    pub fn effect(&self, rand: &mut CRand) -> EffectParams {
        let roll = |rand: &mut CRand, index: i32, range: i32| -> f32 {
            if range < 1 {
                return pitch_for_index(index);
            }
            if self.rand_steps {
                let r = rand.unit();
                return pitch_for_index((((r * 2.0 - 1.0) * range as f32) as i32 as f32 + index as f32) as i32);
            }
            let low = pitch_for_index(index - range);
            let high = pitch_for_index(index + range);
            low + (high - low) * rand.unit()
        };
        let speed = roll(rand, self.speed, self.speed_rand);
        let pitch = roll(rand, self.pitch, self.pitch_rand);
        let mut volume = self.volume;
        if self.volume_rand > 0.0 {
            let mut v = || self.volume + (rand.unit() * 2.0 - 1.0) * self.volume_rand;
            volume = if v() < 2.0 {
                if v() > 0.0 {
                    if v() < 2.0 { v() } else { 2.0 }
                } else {
                    0.0
                }
            } else if v() < 2.0 {
                v()
            } else {
                2.0
            };
        }
        EffectParams {
            volume: volume.min(2.0),
            speed: if speed <= 0.0 { 1.0 } else { speed },
            pitch,
            big_fft: self.big_fft,
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            fade_in_ms: self.fade_in_ms,
            fade_out_ms: self.fade_out_ms,
            looped: self.looped,
            reverb: self.reverb,
        }
    }
}

/// One playing sound, as an FMOD channel plays it, mixed at `out_rate` (the DSP clock).
#[derive(Clone, Debug)]
pub struct Voice {
    data: Arc<[f32]>,
    channels: usize,
    rate: f32,
    out_rate: f32,
    frames: f64,
    /// Read position (source frames).
    pos: f64,
    pub speed: f32,
    pub volume: f32,
    /// Output frames since it started.
    clock: u64,
    /// FMOD fade points (clock, gain), sorted.
    fades: Vec<(u64, f32)>,
    /// `setDelay(0, end, stopchannels)`: the clock it stops at.
    stop_at: Option<u64>,
    /// Loop region (source frames) and whether it still loops (`setLoopCount(-1)` / 0).
    region: (f64, f64),
    looping: bool,
    pub reverb: bool,
    done: bool,
}

impl Voice {
    /// `playEffectAdvanced` from `setPosition` on: `data` interleaved at `rate` with `channels`.
    pub fn new(data: Arc<[f32]>, channels: usize, rate: f32, out_rate: f32, p: &EffectParams) -> Voice {
        let channels = channels.max(1);
        let frames = (data.len() / channels) as f64;
        let ms = |v: i32| v as f64 * rate as f64 / 1000.0;
        let length_ms = frames * 1000.0 / rate as f64;
        let speed = if p.speed <= 0.0 { 1.0 } else { p.speed };
        let start = p.start_ms;
        let pos = if start > 0 { ms(start).min(frames) } else { 0.0 };
        // the length to play: end − start (≥ 0) when there's an end, else ≤ 0
        let len = if p.end_ms > 0 { (p.end_ms - start).max(0) } else { p.end_ms };
        let clock = |secs: f32| -> u64 { (out_rate * secs).max(0.0) as u64 };
        let mut fades = Vec::new();
        if p.fade_in_ms > 0 {
            fades.push((0, 0.0));
            fades.push((clock(p.fade_in_ms as f32 / 1000.0), 1.0));
        }
        let mut stop_at = None;
        if (len > 0 || p.fade_out_ms > 0) && !p.looped {
            // the end in real time at the starting speed (the sound's length − start without one)
            let play_ms = if len != 0 { len as f32 } else { (length_ms as f32 - start as f32).max(0.0) };
            let secs = play_ms / speed / 1000.0;
            if len > 0 {
                stop_at = Some(clock(secs));
            }
            if p.fade_out_ms > 0 {
                fades.push((clock(secs - p.fade_out_ms as f32 / 1000.0), 1.0));
                fades.push((clock(secs), 0.0));
            }
        }
        fades.sort_by_key(|f| f.0);
        let region = if p.looped && (p.end_ms >= 1 || start >= 1) {
            let end = if p.end_ms >= 1 { ms(p.end_ms).min(frames) } else { frames };
            (ms(start.max(0)).min(end), end)
        } else {
            (0.0, frames)
        };
        Voice {
            data,
            channels,
            rate,
            out_rate,
            frames,
            pos,
            speed,
            volume: p.volume.clamp(0.0, 2.0),
            clock: 0,
            fades,
            stop_at,
            region,
            looping: p.looped,
            reverb: p.reverb,
            done: frames <= 0.0,
        }
    }

    /// Edit SFX's 414 (`setLoopCount(0)`): no more loops; it plays on to the sound's end.
    pub fn end_loop(&mut self) {
        self.looping = false;
    }

    pub fn stop(&mut self) {
        self.done = true;
    }

    pub fn done(&self) -> bool {
        self.done
    }

    /// The fade points' gain now (held before the first and after the last).
    pub fn envelope(&self) -> f32 {
        let Some(first) = self.fades.first() else { return 1.0 };
        if self.clock <= first.0 {
            return first.1;
        }
        for w in self.fades.windows(2) {
            let (a, b) = (w[0], w[1]);
            if self.clock <= b.0 {
                if b.0 == a.0 {
                    return b.1;
                }
                let t = (self.clock - a.0) as f32 / (b.0 - a.0) as f32;
                return a.1 + (b.1 - a.1) * t;
            }
        }
        self.fades.last().unwrap().1
    }

    fn sample(&self, frame: usize, c: usize) -> f32 {
        let f = frame.min(self.frames as usize - 1);
        self.data[f * self.channels + c.min(self.channels - 1)]
    }

    /// The next output frame (left, right) with the volume and fades, or `None` when it ended.
    pub fn next_frame(&mut self) -> Option<(f32, f32)> {
        if self.done {
            return None;
        }
        if self.stop_at.is_some_and(|s| self.clock >= s) || self.pos >= self.frames {
            self.done = true;
            return None;
        }
        let i = self.pos.floor();
        let t = (self.pos - i) as f32;
        let i = i as usize;
        let mut lr = [0.0f32; 2];
        for (c, out) in lr.iter_mut().enumerate() {
            let a = self.sample(i, c);
            let b = if (i + 1) as f64 >= self.frames { a } else { self.sample(i + 1, c) };
            *out = a + (b - a) * t;
        }
        let g = self.volume * self.envelope();
        self.clock += 1;
        self.pos += self.speed as f64 * self.rate as f64 / self.out_rate as f64;
        if self.looping && self.pos >= self.region.1 && self.region.1 > self.region.0 {
            let span = self.region.1 - self.region.0;
            self.pos = self.region.0 + (self.pos - self.region.1) % span;
        }
        Some((lr[0] * g, lr[1] * g))
    }
}

/// In-place radix-2 FFT (`inverse`: e^{+i}, unscaled). `re.len()` is a power of two.
fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] as f64 * cr - im[b] as f64 * ci;
                let ti = re[b] as f64 * ci + im[b] as f64 * cr;
                re[b] = (re[a] as f64 - tr) as f32;
                im[b] = (im[a] as f64 - ti) as f32;
                re[a] = (re[a] as f64 + tr) as f32;
                im[a] = (im[a] as f64 + ti) as f32;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
}

/// The pitch-shift DSP on one channel of samples: a phase vocoder (FFT `size`, 4× overlap)
/// moving every bin's frequency by `pitch`, so the sound keeps its length. Like the DSP on a
/// playing channel, its output is late by `size − size/4` samples and ends with the sound.
pub fn pitch_shift(input: &[f32], rate: f32, pitch: f32, size: usize) -> Vec<f32> {
    if pitch <= 0.0 || pitch == 1.0 || input.is_empty() {
        return input.to_vec();
    }
    let n = size.next_power_of_two().max(64);
    let osamp = 4;
    let step = n / osamp;
    let latency = n - step;
    let half = n / 2;
    let two_pi = std::f32::consts::TAU;
    let per_bin = rate / n as f32;
    let expect = two_pi * step as f32 / n as f32;
    let window: Vec<f32> = (0..n).map(|k| 0.5 - 0.5 * (two_pi * k as f32 / n as f32).cos()).collect();
    let (mut in_fifo, mut out_fifo) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut accum = vec![0.0f32; 2 * n];
    let (mut last_phase, mut sum_phase) = (vec![0.0f32; half + 1], vec![0.0f32; half + 1]);
    let (mut ana_mag, mut ana_freq) = (vec![0.0f32; half + 1], vec![0.0f32; half + 1]);
    let (mut syn_mag, mut syn_freq) = (vec![0.0f32; half + 1], vec![0.0f32; half + 1]);
    let (mut re, mut im) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut out = vec![0.0f32; input.len()];
    let mut rover = latency;
    for (i, &x) in input.iter().enumerate() {
        in_fifo[rover] = x;
        out[i] = out_fifo[rover - latency];
        rover += 1;
        if rover < n {
            continue;
        }
        rover = latency;
        for k in 0..n {
            re[k] = in_fifo[k] * window[k];
            im[k] = 0.0;
        }
        fft(&mut re, &mut im, false);
        for k in 0..=half {
            let mag = 2.0 * (re[k] * re[k] + im[k] * im[k]).sqrt();
            let phase = im[k].atan2(re[k]);
            let mut d = phase - last_phase[k];
            last_phase[k] = phase;
            d -= k as f32 * expect;
            // wrap to ±π
            let mut q = (d / std::f32::consts::PI) as i64;
            if q >= 0 {
                q += q & 1;
            } else {
                q -= q & 1;
            }
            d -= std::f32::consts::PI * q as f32;
            let dev = osamp as f32 * d / two_pi;
            ana_mag[k] = mag;
            ana_freq[k] = k as f32 * per_bin + dev * per_bin;
        }
        syn_mag.iter_mut().for_each(|v| *v = 0.0);
        syn_freq.iter_mut().for_each(|v| *v = 0.0);
        for k in 0..=half {
            let index = (k as f32 * pitch) as usize;
            if index <= half {
                syn_mag[index] += ana_mag[k];
                syn_freq[index] = ana_freq[k] * pitch;
            }
        }
        for k in 0..=half {
            let mut d = syn_freq[k] - k as f32 * per_bin;
            d /= per_bin;
            d = two_pi * d / osamp as f32 + k as f32 * expect;
            sum_phase[k] += d;
            re[k] = syn_mag[k] * sum_phase[k].cos();
            im[k] = syn_mag[k] * sum_phase[k].sin();
        }
        for k in half + 1..n {
            re[k] = 0.0;
            im[k] = 0.0;
        }
        fft(&mut re, &mut im, true);
        // the Hann window twice over 4× overlap sums to 1.5
        let scale = 2.0 / (half as f32 * osamp as f32 * 1.5);
        for k in 0..n {
            accum[k] += window[k] * re[k] * scale;
        }
        out_fifo[..step].copy_from_slice(&accum[..step]);
        accum.copy_within(step..step + n, 0);
        for v in &mut accum[n..] {
            *v = 0.0;
        }
        in_fifo.copy_within(step..n, 0);
    }
    out
}

/// Interleaved samples pitch-shifted channel by channel.
pub fn pitch_shift_interleaved(data: &[f32], channels: usize, rate: f32, pitch: f32, big_fft: bool) -> Vec<f32> {
    let channels = channels.max(1);
    let size = if big_fft { 2048 } else { 1024 };
    let mut out = vec![0.0f32; data.len()];
    for c in 0..channels {
        let one: Vec<f32> = data.iter().skip(c).step_by(channels).copied().collect();
        for (i, v) in pitch_shift(&one, rate, pitch, size).into_iter().enumerate() {
            out[i * channels + c] = v;
        }
    }
    out
}

/// FMOD_DSP_TYPE_SFXREVERB parameters (`updateReverb`): decay (ms), early delay (ms), late
/// delay (ms), HF reference (Hz), HF decay ratio (%), diffusion (%), density (%), high cut (Hz),
/// early/late mix (%), wet level (dB). The dry level stays at its 0 dB default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbPreset {
    pub decay: f32,
    pub early: f32,
    pub late: f32,
    pub hf_reference: f32,
    pub hf_ratio: f32,
    pub diffusion: f32,
    pub density: f32,
    pub high_cut: f32,
    pub early_late: f32,
    pub wet_db: f32,
}

/// The game's reverb types 1–22 (FMOD's presets padded cell … underwater, from its tables);
/// any other type is the generic room (`updateReverb`'s defaults).
pub fn reverb_preset(kind: i32) -> ReverbPreset {
    const T: [[f32; 9]; 22] = [
        [170.0, 1.0, 2.0, 10.0, 100.0, 100.0, 160.0, 84.0, -7.8],
        [400.0, 2.0, 3.0, 83.0, 100.0, 100.0, 6050.0, 88.0, -9.4],
        [1500.0, 7.0, 11.0, 54.0, 100.0, 60.0, 2900.0, 83.0, 0.5],
        [500.0, 3.0, 4.0, 10.0, 100.0, 100.0, 160.0, 58.0, -19.0],
        [2300.0, 12.0, 17.0, 64.0, 100.0, 100.0, 7800.0, 71.0, -8.5],
        [4300.0, 20.0, 30.0, 59.0, 100.0, 100.0, 5850.0, 64.0, -11.7],
        [3900.0, 20.0, 29.0, 70.0, 100.0, 100.0, 5650.0, 80.0, -9.8],
        [2900.0, 15.0, 22.0, 100.0, 100.0, 100.0, 20000.0, 59.0, -11.3],
        [7200.0, 20.0, 30.0, 33.0, 100.0, 100.0, 4500.0, 80.0, -9.6],
        [10000.0, 20.0, 30.0, 23.0, 100.0, 100.0, 3400.0, 72.0, -7.4],
        [300.0, 2.0, 30.0, 10.0, 100.0, 100.0, 500.0, 56.0, -24.0],
        [1500.0, 7.0, 11.0, 59.0, 100.0, 100.0, 7800.0, 87.0, -5.5],
        [270.0, 13.0, 20.0, 79.0, 100.0, 100.0, 9000.0, 86.0, -6.0],
        [1500.0, 7.0, 11.0, 86.0, 100.0, 100.0, 8300.0, 80.0, -9.8],
        [1500.0, 162.0, 88.0, 54.0, 79.0, 100.0, 760.0, 94.0, -12.3],
        [1500.0, 7.0, 11.0, 67.0, 50.0, 100.0, 4050.0, 66.0, -26.0],
        [1500.0, 300.0, 100.0, 21.0, 27.0, 100.0, 1220.0, 82.0, -24.0],
        [1500.0, 61.0, 25.0, 83.0, 100.0, 100.0, 3400.0, 100.0, -5.0],
        [1500.0, 179.0, 100.0, 50.0, 21.0, 100.0, 1670.0, 65.0, -28.0],
        [1700.0, 8.0, 12.0, 100.0, 100.0, 100.0, 20000.0, 56.0, -19.5],
        [2800.0, 14.0, 21.0, 14.0, 80.0, 60.0, 3400.0, 66.0, 1.2],
        [1500.0, 7.0, 11.0, 10.0, 100.0, 100.0, 500.0, 92.0, 7.0],
    ];
    let r = match kind {
        1..=22 => T[(kind - 1) as usize],
        _ => [1500.0, 7.0, 11.0, 83.0, 100.0, 100.0, 14500.0, 96.0, -8.0],
    };
    ReverbPreset {
        decay: r[0],
        early: r[1],
        late: r[2],
        hf_reference: 5000.0,
        hf_ratio: r[3],
        diffusion: r[4],
        density: r[5],
        high_cut: r[6],
        early_late: r[7],
        wet_db: r[8],
    }
}

/// A delay line with a read `len` behind the write.
#[derive(Clone, Debug)]
struct Delay {
    buf: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(len: usize) -> Delay {
        Delay { buf: vec![0.0; len.max(1)], at: 0 }
    }

    /// The sample written `len` samples ago, then write `x`.
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.at];
        self.buf[self.at] = x;
        self.at = (self.at + 1) % self.buf.len();
        y
    }

    /// The sample written `back` (1..=len) samples ago, without writing.
    fn tap(&self, back: usize) -> f32 {
        let n = self.buf.len();
        self.buf[(self.at + n - back.clamp(1, n)) % n]
    }
}

/// One-pole lowpass coefficient `a` (y = (1 − a)·x + a·y) whose gain at `freq` is `ratio`.
fn one_pole_for(ratio: f32, freq: f32, rate: f32) -> f32 {
    if ratio >= 1.0 || freq <= 0.0 {
        return 0.0;
    }
    let w = (std::f32::consts::TAU * freq / rate).min(std::f32::consts::PI);
    let r2 = ratio * ratio;
    let b = 1.0 - r2 * w.cos();
    let c = 1.0 - r2;
    ((b - (b * b - c * c).max(0.0).sqrt()) / c).clamp(0.0, 0.9999)
}

const LINES: usize = 8;

/// The reverb channel group's SFX reverb (see the module notes): early reflections after the
/// early delay, an 8-line feedback delay network after the late delay whose lines lose 60 dB
/// over the decay time (the HF decay ratio's share of it at 5 kHz), diffusing all-passes,
/// the early/late mix, a high cut and the wet level. Mono in, stereo wet out.
#[derive(Clone, Debug)]
pub struct Reverb {
    pre: Delay,
    early_taps: Vec<(usize, f32, bool)>,
    late_at: usize,
    diffusers: Vec<(Delay, f32)>,
    lines: Vec<Delay>,
    gains: [f32; LINES],
    damp: [f32; LINES],
    damp_state: [f32; LINES],
    late_gain: f32,
    early_gain: f32,
    cut: f32,
    cut_state: (f32, f32),
    wet: f32,
}

impl Reverb {
    pub fn new(p: &ReverbPreset, rate: f32) -> Reverb {
        let ms = |v: f32| ((v * rate / 1000.0) as usize).max(1);
        let early = ms(p.early);
        let late = ms(p.early + p.late);
        // early reflections spread over the late delay, alternating sides, falling off
        let spread = [0.0f32, 0.17, 0.31, 0.46, 0.62, 0.79];
        let mut early_taps: Vec<(usize, f32, bool)> = spread
            .iter()
            .enumerate()
            .map(|(i, f)| (early + (f * p.late.max(1.0) * rate / 1000.0) as usize, 0.85f32.powi(i as i32), i % 2 == 1))
            .collect();
        let norm = early_taps.iter().map(|t| t.1 * t.1).sum::<f32>().sqrt();
        early_taps.iter_mut().for_each(|t| t.1 /= norm);
        let pre_len = early_taps.iter().map(|t| t.0).max().unwrap_or(1).max(late) + 1;
        // line lengths: denser rooms use shorter, closer lines
        const BASE_MS: [f32; LINES] = [29.7, 37.1, 41.1, 43.7, 47.3, 53.9, 59.5, 67.3];
        let size = 0.45 + 0.55 * (p.density / 100.0).clamp(0.0, 1.0);
        let decay = (p.decay / 1000.0).max(0.05);
        let hf_decay = decay * (p.hf_ratio / 100.0).clamp(0.05, 1.0);
        let mut lines = Vec::with_capacity(LINES);
        let (mut gains, mut damp) = ([0.0; LINES], [0.0; LINES]);
        for (k, base) in BASE_MS.iter().enumerate() {
            let len = ((base * size) * rate / 1000.0) as usize;
            let secs = len as f32 / rate;
            gains[k] = 10f32.powf(-3.0 * secs / decay);
            let hf = 10f32.powf(-3.0 * secs / hf_decay);
            damp[k] = one_pole_for(hf / gains[k], p.hf_reference, rate);
            lines.push(Delay::new(len));
        }
        let g = gains.iter().sum::<f32>() / LINES as f32;
        let diff = 0.7 * (p.diffusion / 100.0).clamp(0.0, 1.0);
        let diffusers = [4.77f32, 3.59, 12.73, 9.31].iter().map(|m| (Delay::new(ms(*m)), diff)).collect();
        let mix = (p.early_late / 100.0).clamp(0.0, 1.0);
        Reverb {
            pre: Delay::new(pre_len),
            early_taps,
            late_at: late,
            diffusers,
            lines,
            gains,
            damp,
            damp_state: [0.0; LINES],
            late_gain: mix.sqrt() * (1.0 - g * g).max(1e-4).sqrt() * (2.0 / LINES as f32).sqrt(),
            early_gain: (1.0 - mix).sqrt(),
            cut: (-std::f32::consts::TAU * p.high_cut.min(rate * 0.45) / rate).exp(),
            cut_state: (0.0, 0.0),
            wet: 10f32.powf(p.wet_db / 20.0),
        }
    }

    /// One sample in, the wet (left, right) out.
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        self.pre.tick(x);
        let (mut el, mut er) = (0.0f32, 0.0f32);
        for &(at, g, right) in &self.early_taps {
            let v = self.pre.tap(at) * g;
            if right {
                er += v;
            } else {
                el += v;
            }
        }
        let mut d = self.pre.tap(self.late_at);
        for (line, g) in &mut self.diffusers {
            let delayed = line.tap(line.buf.len());
            let y = -*g * d + delayed;
            line.tick(d + *g * y);
            d = y;
        }
        // Householder feedback: each line's output, damped, minus 2/N of their sum
        let mut outs = [0.0f32; LINES];
        for k in 0..LINES {
            let y = self.lines[k].tap(self.lines[k].buf.len()) * self.gains[k];
            let a = self.damp[k];
            self.damp_state[k] = (1.0 - a) * y + a * self.damp_state[k];
            outs[k] = self.damp_state[k];
        }
        let sum: f32 = outs.iter().sum::<f32>() * (2.0 / LINES as f32);
        let input = d * (1.0 / (LINES as f32).sqrt());
        let (mut ll, mut lr) = (0.0f32, 0.0f32);
        for k in 0..LINES {
            self.lines[k].tick(outs[k] - sum + input);
            if k % 2 == 0 {
                ll += outs[k];
            } else {
                lr += outs[k];
            }
        }
        let l = el * self.early_gain + ll * self.late_gain;
        let r = er * self.early_gain + lr * self.late_gain;
        let c = self.cut;
        self.cut_state.0 = (1.0 - c) * l + c * self.cut_state.0;
        self.cut_state.1 = (1.0 - c) * r + c * self.cut_state.1;
        (self.cut_state.0 * self.wet, self.cut_state.1 * self.wet)
    }

    /// The wet impulse response, interleaved stereo, `secs` long (the web build convolves with it).
    pub fn impulse(p: &ReverbPreset, rate: f32, secs: f32) -> Vec<f32> {
        let mut r = Reverb::new(p, rate);
        let n = (rate * secs) as usize;
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (a, b) = r.process(if i == 0 { 1.0 } else { 0.0 });
            out.push(a);
            out.push(b);
        }
        out
    }

    /// How long its tail lasts (the decay plus the delays), for the impulse response.
    pub fn tail_secs(p: &ReverbPreset) -> f32 {
        ((p.decay + p.early + p.late) / 1000.0 * 1.1 + 0.1).min(12.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (std::f32::consts::TAU * freq * i as f32 / rate).sin() * 0.5).collect()
    }

    /// The strongest frequency of a signal (a plain DFT over a window).
    fn peak_freq(x: &[f32], rate: f32) -> f32 {
        let n = 4096.min(x.len());
        let x = &x[x.len() - n..];
        let mut re: Vec<f32> = x.iter().enumerate().map(|(i, v)| v * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im, false);
        let k = (1..n / 2).max_by(|&a, &b| (re[a].hypot(im[a])).total_cmp(&re[b].hypot(im[b]))).unwrap();
        k as f32 * rate / n as f32
    }

    #[test]
    fn pitch_table_is_the_games() {
        assert_eq!(pitch_for_index(0), 1.0);
        assert_eq!(pitch_for_index(12), 2.0);
        assert_eq!(pitch_for_index(-12), 0.5);
        assert!((pitch_for_index(7) - 1.4983071).abs() < 1e-6);
        assert!((pitch_for_index(-1) - 0.94387431).abs() < 1e-6);
        // out of the table: 1 (not clamped)
        assert_eq!(pitch_for_index(13), 1.0);
        assert_eq!(pitch_for_index(-20), 1.0);
    }

    #[test]
    fn c_rand_is_park_miller() {
        let mut r = CRand::default();
        assert_eq!([r.next(), r.next(), r.next()], [16807, 282475249, 1622650073]);
    }

    #[test]
    fn pitch_shift_keeps_the_length_and_moves_the_frequency() {
        let rate = 44100.0;
        let x = sine(440.0, rate, 44100);
        let y = pitch_shift(&x, rate, pitch_for_index(7), 1024);
        assert_eq!(y.len(), x.len());
        let f = peak_freq(&y, rate);
        assert!((f - 440.0 * pitch_for_index(7)).abs() < 15.0, "{f}");
        // unity gain (a shift of almost nothing gives the input back, late)
        let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt();
        let same = pitch_shift(&x, rate, 1.0001, 1024);
        assert!((rms(&same[8192..]) / rms(&x[8192..]) - 1.0).abs() < 0.01);
        assert!(rms(&y[8192..]) > 0.1);
        // the DSP's latency: nothing comes out before its first frame (size / 4 samples in)
        assert!(y[..256].iter().all(|v| v.abs() < 1e-6));
        let y2 = pitch_shift(&x, rate, 0.5, 2048);
        assert!((peak_freq(&y2, rate) - 220.0).abs() < 15.0);
    }

    fn voice(frames: usize, p: EffectParams) -> Voice {
        // a 1 kHz "sound" of ones, mono, mixed at 1 kHz: one frame per ms
        Voice::new(Arc::from(vec![1.0f32; frames]), 1, 1000.0, 1000.0, &p)
    }

    fn run(v: &mut Voice, max: usize) -> Vec<f32> {
        let mut out = Vec::new();
        while let Some((l, _)) = v.next_frame() {
            out.push(l);
            if out.len() >= max {
                break;
            }
        }
        out
    }

    #[test]
    fn start_end_and_speed() {
        // start 200 ms, end 700 ms at speed 2: 250 ms of real time
        let mut v = voice(1000, EffectParams { start_ms: 200, end_ms: 700, speed: 2.0, ..Default::default() });
        assert_eq!(run(&mut v, 5000).len(), 250);
        // no end: plays the rest of the sound (800 ms at speed 2 → 400)
        let mut v = voice(1000, EffectParams { start_ms: 200, speed: 2.0, ..Default::default() });
        assert_eq!(run(&mut v, 5000).len(), 400);
        // the stop time was set at the start: a later speed change doesn't move it
        let mut v = voice(1000, EffectParams { end_ms: 500, ..Default::default() });
        run(&mut v, 100);
        v.speed = 0.5;
        assert_eq!(run(&mut v, 5000).len(), 400);
    }

    #[test]
    fn fades_are_real_time() {
        let mut v = voice(1000, EffectParams { fade_in_ms: 100, fade_out_ms: 200, speed: 2.0, ..Default::default() });
        let out = run(&mut v, 5000);
        assert_eq!(out.len(), 500);
        assert_eq!(out[0], 0.0);
        assert!((out[50] - 0.5).abs() < 1e-6);
        assert_eq!(out[200], 1.0);
        // fade out over the last 200 ms of the 500 ms it plays
        assert!((out[400] - 0.5).abs() < 1e-6, "{}", out[400]);
        assert!(out[499] < 0.01);
    }

    #[test]
    fn loops_between_start_and_end_until_told_to_stop() {
        let data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let p = EffectParams { start_ms: 100, end_ms: 300, looped: true, fade_out_ms: 50, ..Default::default() };
        let mut v = Voice::new(Arc::from(data), 1, 1000.0, 1000.0, &p);
        let out = run(&mut v, 1000);
        // loops 100..300 forever (no stop, no fade out while looping)
        assert_eq!(out.len(), 1000);
        assert_eq!((out[0], out[199], out[200], out[999]), (100.0, 299.0, 100.0, 299.0));
        // 414: the loop ends and it plays on to the sound's end
        v.end_loop();
        let rest = run(&mut v, 5000);
        assert_eq!(rest.len(), 900);
        assert_eq!(*rest.last().unwrap(), 999.0);
    }

    #[test]
    fn random_ranges() {
        use crate::level::LevelString;
        let sfx = |keys: &str| crate::groups::SfxPlay::from_raw(&LevelString::parse(&format!("kA2,0;1,3602,2,0,3,0,{keys}")).objects[0], 0);
        let mut r = CRand::default();
        // no ranges: the indices and volume as they are
        let p = sfx("392,1,404,5,405,-3,406,0.7").effect(&mut r);
        assert_eq!((p.speed, p.pitch, p.volume), (pitch_for_index(5), pitch_for_index(-3), 0.7));
        assert_eq!(r, CRand::default(), "no rolls");
        // a rate between the range's ends, whole semitones with 599, volume within ± 0.5
        for _ in 0..200 {
            let p = sfx("392,1,404,2,596,3,405,1,597,2,599,1,406,1,598,0.5").effect(&mut r);
            assert!(p.speed >= pitch_for_index(-1) && p.speed <= pitch_for_index(5), "{}", p.speed);
            assert!((-1..=3).any(|i| pitch_for_index(i) == p.pitch), "{}", p.pitch);
            assert!((0.5..=1.5).contains(&p.volume), "{}", p.volume);
        }
        // out of the table: the edge rolls 1.0 (pitchForIdx's default), the volume clamps at 2
        for _ in 0..50 {
            let p = sfx("392,1,404,12,596,6,406,1.9,598,1").effect(&mut r);
            assert!(p.speed <= 2.0 && p.speed >= 1.0);
            assert!((0.0..=2.0).contains(&p.volume));
        }
    }

    #[test]
    fn reverb_decays_over_its_time() {
        let rate = 22050.0;
        for kind in [0, 1, 9, 22] {
            let p = reverb_preset(kind);
            let ir = Reverb::impulse(&p, rate, Reverb::tail_secs(&p));
            assert!(ir.iter().all(|v| v.is_finite()), "{kind}");
            let energy = |from: f32, to: f32| -> f32 {
                let (a, b) = ((from * rate) as usize * 2, ((to * rate) as usize * 2).min(ir.len()));
                ir[a..b].iter().map(|v| v * v).sum::<f32>() / ((b - a).max(1)) as f32
            };
            let d = p.decay / 1000.0;
            // −60 dB over the decay time, roughly: the energy a decay later is far down
            let early = energy(0.0, d * 0.25);
            let later = energy(d, d * 1.1);
            assert!(early > 0.0 && later < early * 1e-3, "{kind}: {early} {later}");
        }
        assert_eq!(reverb_preset(99), reverb_preset(0));
    }
}
