//! Music and sound effects. The song for an attempt starts at the level's song offset when the
//! attempt starts and stops when the player dies (like the game). Natively it plays through
//! rodio, reading the player's own files; on the web the page decodes the file with WebAudio
//! straight from the dropped folder (a small bridge in index.html), so songs aren't copied into
//! wasm memory.

/// What the game asks of the audio backend.
pub trait Audio {
    /// Start `file` (a path in Resources) `offset` seconds in, replacing any music playing.
    fn play_music(&mut self, file: &str, offset: f32);
    fn stop_music(&mut self);
    /// Play a one-shot sound (e.g. the death sound) over the music.
    fn play_effect(&mut self, file: &str);
    /// A one-shot sound at a relative volume (SFX triggers).
    fn play_effect_volume(&mut self, file: &str, _volume: f32) {
        self.play_effect(file);
    }
    /// The music's peak level (0..1) over its last mixed block, averaged over the channels,
    /// like FMOD's metering (`FMODAudioEngine::updateMetering` reads it once a frame).
    fn peak(&self) -> f32 {
        0.0
    }
    /// Pause or resume the music (the pause menu).
    fn set_paused(&mut self, _paused: bool) {}
    /// Play `file` from its start, looping (the menu music).
    fn play_loop(&mut self, file: &str) {
        self.play_music(file, 0.0);
    }
    /// The music and sound effect volumes (0..1), the options' sliders.
    fn set_volumes(&mut self, _music: f32, _sfx: f32) {}
    /// An SFX trigger's sound (`playEffectAdvanced`: volume, playback rate, pitch shift, start /
    /// end, fades, loop, reverb; see `cadence_core::audiofx`), kept so edit SFX triggers and
    /// proximity can change it. Returns its handle (0: not kept).
    fn play_trigger_effect(&mut self, file: &str, p: &EffectParams) -> u64 {
        self.play_effect_volume(file, p.volume);
        0
    }
    /// A kept trigger sound's volume and playback rate from now on.
    fn set_trigger_effect(&mut self, _handle: u64, _volume: f32, _speed: f32) {}
    fn stop_trigger_effect(&mut self, _handle: u64) {}
    /// Edit SFX 414: the sound stops looping and plays on to its end (`setLoopCount(0)`).
    fn end_trigger_loop(&mut self, _handle: u64) {}
    /// Whether a kept trigger sound still plays.
    fn trigger_effect_playing(&self, _handle: u64) -> bool {
        false
    }
    /// Every trigger sound stops (`stopAllEffects`: a death, a restart, leaving the level).
    fn stop_trigger_effects(&mut self) {}
    /// The reverb group's preset (`updateReverb`, an SFX trigger with 503 and type 502).
    fn set_reverb(&mut self, _kind: i32) {}
    /// The options' SFX volume (`playEffectAdvanced` plays nothing but loops at 0).
    fn sfx_volume(&self) -> f32 {
        1.0
    }
    /// Edit song triggers: the music's own volume (on top of the options') toward `volume` over
    /// `duration` seconds (stepped by `update`), and its speed.
    fn fade_music(&mut self, _volume: f32, _duration: f32) {}
    fn set_music_speed(&mut self, _speed: f32) {}
    /// Once a frame: advance volume fades.
    fn update(&mut self, _dt: f32) {}
}

/// The song an official level plays (`LevelTools::getAudioFileName`, track = level − 1;
/// The Challenge uses track 26; the Tower levels use their bundled library songs).
pub fn level_song(level_id: i32) -> Option<String> {
    const TRACKS: [&str; 27] = [
        "StereoMadness.mp3", "BackOnTrack.mp3", "Polargeist.mp3", "DryOut.mp3", "BaseAfterBase.mp3",
        "CantLetGo.mp3", "Jumper.mp3", "TimeMachine.mp3", "Cycles.mp3", "xStep.mp3",
        "Clutterfunk.mp3", "TheoryOfEverything.mp3", "Electroman.mp3", "Clubstep.mp3",
        "Electrodynamix.mp3", "HexagonForce.mp3", "BlastProcessing.mp3", "TheoryOfEverything2.mp3",
        "GeometricalDominator.mp3", "Deadlocked.mp3", "Fingerdash.mp3", "Dash.mp3", "Explorers.mp3",
        "The7Seas.mp3", "VikingArena.mp3", "AirborneRobots.mp3", "DJRubRub.mp3",
    ];
    let track = match level_id {
        1..=22 => (level_id - 1) as usize,
        3001 => 26,
        5001 => return Some("songs/10003039.ogg".into()),
        5002 => return Some("songs/10003129.ogg".into()),
        5003 => return Some("songs/10002875.ogg".into()),
        5004 => return Some("songs/10006086.ogg".into()),
        _ => return None,
    };
    Some(TRACKS[track].to_string())
}

/// The death sound.
pub const DEATH_EFFECT: &str = "explode_11.ogg";
/// The level-complete sound.
pub const END_EFFECT: &str = "endStart_02.ogg";

/// A value tweened linearly over a duration (`FMODAudioEngine::updateChannel`'s fades).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub value: f32,
    from: f32,
    to: f32,
    elapsed: f32,
    duration: f32,
}

impl Tween {
    pub fn new(value: f32) -> Tween {
        Tween { value, from: value, to: value, elapsed: 0.0, duration: 0.0 }
    }

    /// Toward `to` over `duration` seconds from the current value (at once without one).
    pub fn start(&mut self, to: f32, duration: f32) {
        (self.from, self.to, self.elapsed, self.duration) = (self.value, to, 0.0, duration);
        if duration <= 0.0 {
            self.value = to;
        }
    }

    pub fn step(&mut self, dt: f32) {
        if self.value == self.to {
            return;
        }
        self.elapsed += dt;
        let k = if self.duration <= 0.0 { 1.0 } else { (self.elapsed / self.duration).min(1.0) };
        self.value = if k >= 1.0 { self.to } else { self.from + (self.to - self.from) * k };
    }
}

pub use cadence_core::audiofx::{pitch_for_index, EffectParams};

/// A sound an SFX trigger started: its own volume and speed (tweened by edit SFX triggers) and
/// its proximity effect, each per sound like the game's per-channel state.
#[derive(Clone, Debug)]
pub struct TriggerSound {
    pub handle: u64,
    /// The SFX triggers whose channel it is (`activateSFXTrigger` keeps the channel per trigger;
    /// a trigger whose unique id was playing gets that channel), its SFX group (455) and
    /// unique id.
    pub orders: Vec<u32>,
    pub group: i32,
    pub unique: i32,
    pub volume: Tween,
    pub speed: Tween,
    pub proximity: Option<cadence_core::groups::Proximity>,
    /// What the backend was last sent (volume with the proximity factor, speed).
    sent: (f32, f32),
}

/// The sounds SFX triggers started (`activateSFXTrigger`'s channel ids per trigger, the
/// channels' tweens and the proximity effects by channel), the C `rand()` the random ranges
/// roll, and `playEffectAdvanced`'s minimum-interval times by reference id.
#[derive(Clone, Debug, Default)]
pub struct TriggerSounds {
    pub sounds: Vec<TriggerSound>,
    rand: cadence_core::audiofx::CRand,
    clock: f32,
    last_played: Vec<(i32, f32)>,
}

impl TriggerSounds {
    /// Start an SFX trigger's sound; `factor` is its proximity volume now (1 without one).
    /// With a proximity group and the test on (no 489), a factor of 0 or less doesn't play it.
    pub fn play(&mut self, audio: &mut dyn Audio, file: &str, play: &cadence_core::groups::SfxPlay, factor: f32) {
        if play.proximity.is_some() && play.test_proximity && factor <= 0.0 {
            return;
        }
        // activateSFXTrigger: 503 sets the reverb type first
        if play.reverb_on {
            audio.set_reverb(play.reverb_type);
        }
        // playSFXTrigger rolls the ranges, then playEffectAdvanced:
        let mut p = play.effect(&mut self.rand);
        // nothing with the SFX volume at 0, unless it loops
        if audio.sfx_volume() <= 0.0 && !p.looped {
            return;
        }
        // the minimum interval (434) per reference id
        if play.min_interval > 0.0 {
            match self.last_played.iter_mut().find(|l| l.0 == play.ref_id) {
                Some(l) if self.clock - l.1 < play.min_interval => return,
                Some(l) => l.1 = self.clock,
                None => self.last_played.push((play.ref_id, self.clock)),
            }
        }
        // a unique id already playing: kept (the trigger takes its channel) unless 420 overrides
        if play.unique != 0 {
            if let Some(i) = self.sounds.iter().position(|s| s.unique == play.unique) {
                if !play.override_unique {
                    let s = &mut self.sounds[i];
                    if !s.orders.contains(&play.order) {
                        s.orders.push(play.order);
                    }
                    return;
                }
                audio.stop_trigger_effect(self.sounds[i].handle);
                self.sounds.remove(i);
            }
        }
        let base = p.volume;
        let factor = if play.proximity.is_some() { factor } else { 1.0 };
        p.volume = base * factor;
        let handle = audio.play_trigger_effect(file, &p);
        if handle != 0 {
            self.sounds.push(TriggerSound {
                handle,
                orders: vec![play.order],
                group: play.group,
                unique: play.unique,
                volume: Tween::new(base),
                speed: Tween::new(p.speed),
                proximity: play.proximity,
                sent: (p.volume, p.speed),
            });
        }
    }

    /// An edit SFX trigger (`activateSFXEditTrigger` → `applySFXEditTrigger`): the sounds of the
    /// SFX triggers in its group 457 (`triggers`), the ones in its SFX group 455 and the one with
    /// its unique id 416 stop (417; with 414 they only stop looping), then tween their speed /
    /// volume over its duration; its proximity (51) replaces theirs (not for the unique id's).
    pub fn edit(&mut self, audio: &mut dyn Audio, edit: &cadence_core::groups::AudioEdit, triggers: &[u32]) {
        let mut i = 0;
        while i < self.sounds.len() {
            let s = &mut self.sounds[i];
            let by_group = s.orders.iter().any(|o| triggers.contains(o)) || (edit.sfx_group > 0 && s.group == edit.sfx_group);
            let by_unique = edit.unique > 0 && s.unique == edit.unique;
            if !(by_group || by_unique) {
                i += 1;
                continue;
            }
            if edit.stop_loop {
                audio.end_trigger_loop(s.handle);
            } else if edit.stop {
                audio.stop_trigger_effect(s.handle);
                self.sounds.remove(i);
                continue;
            }
            if let Some(speed) = edit.speed {
                s.speed.start(pitch_for_index(speed), edit.duration);
            }
            if let Some(v) = edit.volume {
                s.volume.start(v, edit.duration);
            }
            if by_group {
                if let Some(p) = edit.proximity {
                    s.proximity = Some(p);
                }
            }
            i += 1;
        }
    }

    /// `stopAllEffects` (death, restart, leaving).
    pub fn stop_all(&mut self, audio: &mut dyn Audio) {
        audio.stop_trigger_effects();
        self.sounds.clear();
    }

    /// One step: the tweens advance, the proximity factors (`factor`) apply, changes go to the
    /// backend and finished sounds are dropped.
    pub fn step(&mut self, audio: &mut dyn Audio, dt: f32, factor: &dyn Fn(&cadence_core::groups::Proximity) -> f32) {
        self.clock += dt;
        self.sounds.retain(|s| audio.trigger_effect_playing(s.handle));
        for s in &mut self.sounds {
            s.volume.step(dt);
            s.speed.step(dt);
            let k = s.proximity.as_ref().map_or(1.0, factor);
            let now = (s.volume.value * k, s.speed.value);
            if now != s.sent {
                s.sent = now;
                audio.set_trigger_effect(s.handle, now.0, now.1);
            }
        }
    }
}

/// No sound (no output device, or tests).
pub struct NoAudio;

impl Audio for NoAudio {
    fn play_music(&mut self, _file: &str, _offset: f32) {}
    fn stop_music(&mut self) {}
    fn play_effect(&mut self, _file: &str) {}
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeAudio;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use rodio::Source;

    use super::bus::{self, Bus, BusSource, Decoded};
    use super::{Audio, EffectParams, Meter};
    use crate::files::Files;

    /// Plays through the default output device. File bytes are cached so restarting an
    /// attempt doesn't touch the disk.
    pub struct NativeAudio {
        sink: rodio::MixerDeviceSink,
        music: Option<rodio::Player>,
        /// The music's last block peak (f32 bits), written by `Meter` on the audio thread.
        peak: Arc<AtomicU32>,
        files: Box<dyn Files>,
        cache: HashMap<String, Arc<[u8]>>,
        volumes: (f32, f32),
        /// Edit song triggers: the music's gain and its fade (from, to, duration, elapsed).
        gain: f32,
        fade: Option<(f32, f32, f32, f32)>,
        /// SFX triggers' sounds: mixed by `BusSource` on the audio thread (FMOD's channels:
        /// rate, fades, loops, the reverb group), decoded once per file and pitch.
        bus: Arc<std::sync::Mutex<Bus>>,
        decoded: HashMap<(String, u32, bool), Arc<Decoded>>,
        next_handle: u64,
    }

    impl NativeAudio {
        /// `None` when there's no output device.
        pub fn new(files: Box<dyn Files>) -> Option<NativeAudio> {
            let mut sink = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
            sink.log_on_drop(false);
            let bus = Arc::new(std::sync::Mutex::new(Bus::new()));
            sink.mixer().add(BusSource::new(bus.clone()));
            Some(NativeAudio {
                sink,
                music: None,
                peak: Arc::new(AtomicU32::new(0)),
                files,
                cache: HashMap::new(),
                volumes: (1.0, 1.0),
                gain: 1.0,
                fade: None,
                bus,
                decoded: HashMap::new(),
                next_handle: 1,
            })
        }

        fn bus(&self) -> std::sync::MutexGuard<'_, Bus> {
            self.bus.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// A sound file's samples, pitch-shifted when `pitch` isn't 1 (cached).
        fn samples(&mut self, file: &str, pitch: f32, big_fft: bool) -> Option<Arc<Decoded>> {
            let shifted = pitch > 0.0 && pitch != 1.0;
            let key = (file.to_string(), if shifted { pitch.to_bits() } else { 0 }, shifted && big_fft);
            if let Some(d) = self.decoded.get(&key) {
                return Some(d.clone());
            }
            let plain = (file.to_string(), 0, false);
            let base = match self.decoded.get(&plain) {
                Some(d) => d.clone(),
                None => {
                    let decoder = self.decoder(file)?;
                    let (channels, rate) = (decoder.channels().get() as usize, decoder.sample_rate().get() as f32);
                    let d = Arc::new(Decoded { samples: decoder.collect::<Vec<f32>>().into(), channels, rate });
                    self.decoded.insert(plain, d.clone());
                    d
                }
            };
            if !shifted {
                return Some(base);
            }
            let samples = cadence_core::audiofx::pitch_shift_interleaved(&base.samples, base.channels, base.rate, pitch, big_fft);
            let d = Arc::new(Decoded { samples: samples.into(), channels: base.channels, rate: base.rate });
            self.decoded.insert(key, d.clone());
            Some(d)
        }

        fn decoder(&mut self, file: &str) -> Option<rodio::Decoder<Cursor<Arc<[u8]>>>> {
            if !self.cache.contains_key(file) {
                let bytes: Arc<[u8]> = self.files.read(file)?.into();
                self.cache.insert(file.to_string(), bytes);
            }
            rodio::Decoder::new(Cursor::new(self.cache[file].clone())).ok()
        }
    }

    impl Audio for NativeAudio {
        fn play_music(&mut self, file: &str, offset: f32) {
            self.stop_music();
            let Some(decoder) = self.decoder(file) else { return };
            let player = rodio::Player::connect_new(self.sink.mixer());
            let source = decoder.skip_duration(Duration::from_secs_f32(offset.max(0.0)));
            player.append(Meter::new(source, self.peak.clone()));
            self.gain = 1.0;
            self.fade = None;
            player.set_volume(self.volumes.0);
            self.music = Some(player);
        }

        fn fade_music(&mut self, volume: f32, duration: f32) {
            if duration <= 0.0 {
                self.fade = None;
                self.gain = volume.max(0.0);
            } else {
                self.fade = Some((self.gain, volume.max(0.0), duration, 0.0));
            }
            if let Some(player) = &self.music {
                player.set_volume(self.volumes.0 * self.gain);
            }
        }

        fn set_music_speed(&mut self, speed: f32) {
            if let Some(player) = &self.music {
                player.set_speed(speed.clamp(0.1, 10.0));
            }
        }

        fn update(&mut self, dt: f32) {
            if let Some((from, to, duration, elapsed)) = &mut self.fade {
                *elapsed += dt;
                let t = (*elapsed / *duration).min(1.0);
                self.gain = *from + (*to - *from) * t;
                if t >= 1.0 {
                    self.fade = None;
                }
                if let Some(player) = &self.music {
                    player.set_volume(self.volumes.0 * self.gain);
                }
            }
        }

        fn play_trigger_effect(&mut self, file: &str, p: &EffectParams) -> u64 {
            if self.volumes.1 <= 0.0 && !p.looped {
                return 0;
            }
            let Some(d) = self.samples(file, p.pitch, p.big_fft) else { return 0 };
            let voice = cadence_core::audiofx::Voice::new(d.samples.clone(), d.channels, d.rate, bus::RATE as f32, p);
            let handle = self.next_handle;
            self.next_handle += 1;
            self.bus().voices.push((handle, voice));
            handle
        }

        fn set_trigger_effect(&mut self, handle: u64, volume: f32, speed: f32) {
            if let Some((_, v)) = self.bus().voices.iter_mut().find(|(h, _)| *h == handle) {
                // FMOD caps a channel's volume at 2; its pitch is the playback rate
                v.volume = volume.clamp(0.0, 2.0);
                v.speed = speed.clamp(0.01, 16.0);
            }
        }

        fn stop_trigger_effect(&mut self, handle: u64) {
            self.bus().voices.retain(|(h, _)| *h != handle);
        }

        fn end_trigger_loop(&mut self, handle: u64) {
            if let Some((_, v)) = self.bus().voices.iter_mut().find(|(h, _)| *h == handle) {
                v.end_loop();
            }
        }

        fn trigger_effect_playing(&self, handle: u64) -> bool {
            self.bus().voices.iter().any(|(h, v)| *h == handle && !v.done())
        }

        fn stop_trigger_effects(&mut self) {
            self.bus().voices.clear();
        }

        fn set_reverb(&mut self, kind: i32) {
            self.bus().set_reverb(kind);
        }

        fn sfx_volume(&self) -> f32 {
            self.volumes.1
        }

        fn set_paused(&mut self, paused: bool) {
            if let Some(player) = &self.music {
                if paused { player.pause() } else { player.play() }
            }
            self.bus().paused = paused;
        }

        fn play_loop(&mut self, file: &str) {
            self.stop_music();
            if !self.cache.contains_key(file) {
                let Some(bytes) = self.files.read(file) else { return };
                self.cache.insert(file.to_string(), bytes.into());
            }
            let Ok(decoder) = rodio::Decoder::new_looped(Cursor::new(self.cache[file].clone())) else { return };
            let player = rodio::Player::connect_new(self.sink.mixer());
            player.append(Meter::new(decoder, self.peak.clone()));
            player.set_volume(self.volumes.0);
            self.music = Some(player);
        }

        fn stop_music(&mut self) {
            if let Some(player) = self.music.take() {
                player.stop();
            }
            self.peak.store(0, Ordering::Relaxed);
        }

        fn peak(&self) -> f32 {
            f32::from_bits(self.peak.load(Ordering::Relaxed))
        }

        fn play_effect(&mut self, file: &str) {
            self.play_effect_volume(file, 1.0);
        }

        fn play_effect_volume(&mut self, file: &str, volume: f32) {
            if self.volumes.1 <= 0.0 {
                return;
            }
            let v = self.volumes.1 * volume.clamp(0.0, 2.0);
            if let Some(decoder) = self.decoder(file) {
                self.sink.mixer().add(decoder.amplify(v));
            }
        }

        fn set_volumes(&mut self, music: f32, sfx: f32) {
            self.volumes = (music.clamp(0.0, 1.0), sfx.clamp(0.0, 1.0));
            if let Some(player) = &self.music {
                player.set_volume(self.volumes.0);
            }
            let sfx = self.volumes.1;
            self.bus().volume = sfx;
        }
    }
}

/// The SFX triggers' mixer: FMOD's effect channels (`cadence_core::audiofx::Voice`) summed, the
/// ones sent to the reverb group (407) also through its reverb, times the options' SFX volume.
#[cfg(not(target_arch = "wasm32"))]
mod bus {
    use std::num::NonZero;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use cadence_core::audiofx::{reverb_preset, Reverb, Voice};
    use rodio::{ChannelCount, Sample, SampleRate, Source};

    /// The mixer's clock (FMOD's DSP clock for fades and end times).
    pub const RATE: u32 = 44100;
    const BLOCK: usize = 256;

    /// A decoded sound: interleaved samples, channels, sample rate.
    pub struct Decoded {
        pub samples: Arc<[f32]>,
        pub channels: usize,
        pub rate: f32,
    }

    pub struct Bus {
        pub voices: Vec<(u64, Voice)>,
        reverb: (i32, Reverb),
        pub paused: bool,
        pub volume: f32,
    }

    impl Bus {
        pub fn new() -> Bus {
            // the engine starts with type 0: FMOD's generic preset
            Bus { voices: Vec::new(), reverb: (0, Reverb::new(&reverb_preset(0), RATE as f32)), paused: false, volume: 1.0 }
        }

        /// `updateReverb`: only a different type changes it.
        pub fn set_reverb(&mut self, kind: i32) {
            if kind != self.reverb.0 {
                self.reverb = (kind, Reverb::new(&reverb_preset(kind), RATE as f32));
            }
        }

        /// Mixes `out.len() / 2` stereo frames.
        pub fn mix(&mut self, out: &mut [f32]) {
            out.iter_mut().for_each(|v| *v = 0.0);
            if self.paused {
                return;
            }
            for frame in out.chunks_exact_mut(2) {
                let (mut l, mut r, mut send) = (0.0f32, 0.0f32, 0.0f32);
                for (_, v) in &mut self.voices {
                    if let Some((a, b)) = v.next_frame() {
                        l += a;
                        r += b;
                        if v.reverb {
                            send += (a + b) * 0.5;
                        }
                    }
                }
                let (wl, wr) = self.reverb.1.process(send);
                frame[0] = (l + wl) * self.volume;
                frame[1] = (r + wr) * self.volume;
            }
            self.voices.retain(|(_, v)| !v.done());
        }
    }

    /// The bus as a never-ending rodio source (silence when nothing plays).
    pub struct BusSource {
        bus: Arc<Mutex<Bus>>,
        block: Vec<f32>,
        at: usize,
    }

    impl BusSource {
        pub fn new(bus: Arc<Mutex<Bus>>) -> BusSource {
            BusSource { bus, block: vec![0.0; BLOCK * 2], at: BLOCK * 2 }
        }
    }

    impl Iterator for BusSource {
        type Item = Sample;

        fn next(&mut self) -> Option<Sample> {
            if self.at >= self.block.len() {
                let mut bus = self.bus.lock().unwrap_or_else(|e| e.into_inner());
                bus.mix(&mut self.block);
                self.at = 0;
            }
            let v = self.block[self.at];
            self.at += 1;
            Some(v)
        }
    }

    impl Source for BusSource {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> ChannelCount {
            NonZero::new(2).unwrap()
        }
        fn sample_rate(&self) -> SampleRate {
            NonZero::new(RATE).unwrap()
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use cadence_core::audiofx::EffectParams;

        #[test]
        fn mixes_voices_and_the_reverb_send() {
            let mut bus = Bus::new();
            let data: Arc<[f32]> = Arc::from(vec![0.5f32; 4410]);
            bus.voices.push((1, Voice::new(data.clone(), 1, 44100.0, RATE as f32, &EffectParams::default())));
            bus.volume = 0.5;
            let mut out = vec![0.0; 512];
            bus.mix(&mut out);
            assert_eq!((out[0], out[1]), (0.25, 0.25));
            // a reverb voice adds a wet tail that outlives it
            let mut bus = Bus::new();
            let p = EffectParams { reverb: true, end_ms: 20, ..Default::default() };
            bus.voices.push((2, Voice::new(data, 1, 44100.0, RATE as f32, &p)));
            let mut out = vec![0.0; 44100 * 2];
            bus.mix(&mut out);
            assert!(bus.voices.is_empty());
            assert!(out[44100..].iter().any(|v| v.abs() > 1e-4), "no tail");
            // paused: silence, nothing advances
            bus.paused = true;
            bus.mix(&mut out);
            assert!(out.iter().all(|v| *v == 0.0));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod meter {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use rodio::{ChannelCount, Sample, SampleRate, Source};

    /// Passes a source through, publishing each 1024-frame block's peak (per channel, averaged
    /// over the channels) like an FMOD DSP's metering.
    pub struct Meter<S> {
        inner: S,
        out: Arc<AtomicU32>,
        channel: usize,
        frames: usize,
        peaks: Vec<f32>,
    }

    impl<S: Source> Meter<S> {
        pub fn new(inner: S, out: Arc<AtomicU32>) -> Meter<S> {
            let channels = inner.channels().get() as usize;
            Meter { inner, out, channel: 0, frames: 0, peaks: vec![0.0; channels] }
        }
    }

    impl<S: Source> Iterator for Meter<S> {
        type Item = Sample;

        fn next(&mut self) -> Option<Sample> {
            let v = self.inner.next()?;
            let n = self.peaks.len().max(1);
            if let Some(p) = self.peaks.get_mut(self.channel) {
                *p = p.max(v.abs());
            }
            self.channel += 1;
            if self.channel >= n {
                self.channel = 0;
                self.frames += 1;
                if self.frames >= 1024 {
                    let level = self.peaks.iter().take(2).sum::<f32>() / self.peaks.len().min(2) as f32;
                    self.out.store(level.to_bits(), Ordering::Relaxed);
                    self.frames = 0;
                    self.peaks.iter_mut().for_each(|p| *p = 0.0);
                }
            }
            Some(v)
        }
    }

    impl<S: Source> Source for Meter<S> {
        fn current_span_len(&self) -> Option<usize> {
            self.inner.current_span_len()
        }
        fn channels(&self) -> ChannelCount {
            self.inner.channels()
        }
        fn sample_rate(&self) -> SampleRate {
            self.inner.sample_rate()
        }
        fn total_duration(&self) -> Option<Duration> {
            self.inner.total_duration()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
use meter::Meter;

/// A song decoded for offline metering (headless screenshots have no audio device): samples,
/// interleaved, with the channel count and sample rate.
#[cfg(not(target_arch = "wasm32"))]
pub struct DecodedSong {
    samples: Vec<f32>,
    channels: usize,
    rate: f32,
}

#[cfg(not(target_arch = "wasm32"))]
impl DecodedSong {
    pub fn decode(files: &dyn crate::files::Files, file: &str) -> Option<DecodedSong> {
        use rodio::Source;
        let bytes = files.read(file)?;
        let decoder = rodio::Decoder::new(std::io::Cursor::new(bytes)).ok()?;
        let (channels, rate) = (decoder.channels().get() as usize, decoder.sample_rate().get() as f32);
        Some(DecodedSong { samples: decoder.collect(), channels, rate })
    }

    /// The peak of the 1024 frames before `secs`, like `Meter` (per channel, averaged over the
    /// first two).
    pub fn peak(&self, secs: f32) -> f32 {
        let end = ((secs * self.rate) as usize).min(self.samples.len() / self.channels.max(1));
        let start = end.saturating_sub(1024);
        let used = self.channels.clamp(1, 2);
        let mut peaks = [0.0f32; 2];
        for frame in start..end {
            for (c, p) in peaks.iter_mut().enumerate().take(used) {
                *p = p.max(self.samples[frame * self.channels + c].abs());
            }
        }
        peaks.iter().take(used).sum::<f32>() / used as f32
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{web_add_file, web_download, web_has_file, WebAudio};

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::Cell;

    use wasm_bindgen::prelude::*;

    use super::{Audio, EffectParams};

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playMusic)]
        fn js_play_music(file: &str, offset: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = stopMusic)]
        fn js_stop_music();
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playEffect)]
        fn js_play_effect(file: &str, volume: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playTriggerEffect)]
        fn js_play_trigger_effect(
            file: &str, volume: f32, speed: f32, pitch: f32, big_fft: bool, start_ms: i32, end_ms: i32, fade_in_ms: i32, fade_out_ms: i32, looped: bool,
            reverb: bool,
        ) -> u32;
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = endTriggerLoop)]
        fn js_end_trigger_loop(handle: u32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = stopTriggerEffects)]
        fn js_stop_trigger_effects();
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = setReverb)]
        fn js_set_reverb(impulse: &[f32], rate: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = setTriggerEffect)]
        fn js_set_trigger_effect(handle: u32, volume: f32, speed: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = stopTriggerEffect)]
        fn js_stop_trigger_effect(handle: u32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = triggerEffectPlaying)]
        fn js_trigger_effect_playing(handle: u32) -> bool;
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = peak)]
        fn js_peak() -> f32;
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = setPaused)]
        fn js_set_paused(paused: bool);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playLoop)]
        fn js_play_loop(file: &str);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = setVolumes)]
        fn js_set_volumes(music: f32, sfx: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = addFile)]
        fn js_add_file(name: &str, bytes: &[u8]);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = hasFile)]
        fn js_has_file(name: &str) -> bool;
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = download)]
        fn js_download(name: &str, bytes: &[u8]);
    }

    /// The page's pitch shift for a trigger sound (the pitch-shift DSP; see
    /// `cadence_core::audiofx::pitch_shift`), one channel at a time.
    #[wasm_bindgen]
    pub fn sfx_pitch_shift(samples: &[f32], rate: f32, pitch: f32, big_fft: bool) -> Vec<f32> {
        cadence_core::audiofx::pitch_shift(samples, rate, pitch, if big_fft { 2048 } else { 1024 })
    }

    thread_local! {
        /// The reverb type the page's convolver has (None: none yet) and the SFX volume.
        static REVERB: Cell<Option<i32>> = const { Cell::new(None) };
        static SFX_VOLUME: Cell<f32> = const { Cell::new(1.0) };
    }

    /// The reverb group's preset as an impulse response for the page's convolver.
    fn send_reverb(kind: i32) {
        if REVERB.with(|r| r.get()) == Some(kind) {
            return;
        }
        REVERB.with(|r| r.set(Some(kind)));
        let preset = cadence_core::audiofx::reverb_preset(kind);
        let rate = 44100.0;
        let ir = cadence_core::audiofx::Reverb::impulse(&preset, rate, cadence_core::audiofx::Reverb::tail_secs(&preset));
        js_set_reverb(&ir, rate);
    }

    /// Give the player a file to save (an exported level).
    pub fn web_download(name: &str, bytes: &[u8]) {
        js_download(name, bytes);
    }

    /// A downloaded song or sound (`net/...`), kept by the page for this visit.
    pub fn web_add_file(name: &str, bytes: &[u8]) {
        js_add_file(name, bytes);
    }

    pub fn web_has_file(name: &str) -> bool {
        js_has_file(name)
    }

    /// Forwards to `window.cadenceAudio` in index.html (WebAudio).
    pub struct WebAudio;

    impl Audio for WebAudio {
        fn play_music(&mut self, file: &str, offset: f32) {
            js_play_music(file, offset);
        }
        fn stop_music(&mut self) {
            js_stop_music();
        }
        fn play_effect(&mut self, file: &str) {
            js_play_effect(file, 1.0);
        }
        fn play_effect_volume(&mut self, file: &str, volume: f32) {
            js_play_effect(file, volume);
        }
        fn play_trigger_effect(&mut self, file: &str, p: &EffectParams) -> u64 {
            // the engine starts with the generic reverb
            if p.reverb && REVERB.with(|r| r.get()).is_none() {
                send_reverb(0);
            }
            js_play_trigger_effect(
                file,
                p.volume.clamp(0.0, 2.0),
                p.speed,
                p.pitch,
                p.big_fft,
                p.start_ms,
                p.end_ms,
                p.fade_in_ms,
                p.fade_out_ms,
                p.looped,
                p.reverb,
            ) as u64
        }
        fn end_trigger_loop(&mut self, handle: u64) {
            js_end_trigger_loop(handle as u32);
        }
        fn stop_trigger_effects(&mut self) {
            js_stop_trigger_effects();
        }
        fn set_reverb(&mut self, kind: i32) {
            send_reverb(kind);
        }
        fn sfx_volume(&self) -> f32 {
            SFX_VOLUME.with(|v| v.get())
        }
        fn set_trigger_effect(&mut self, handle: u64, volume: f32, speed: f32) {
            js_set_trigger_effect(handle as u32, volume.clamp(0.0, 2.0), speed);
        }
        fn stop_trigger_effect(&mut self, handle: u64) {
            js_stop_trigger_effect(handle as u32);
        }
        fn trigger_effect_playing(&self, handle: u64) -> bool {
            js_trigger_effect_playing(handle as u32)
        }
        fn peak(&self) -> f32 {
            js_peak()
        }
        fn set_paused(&mut self, paused: bool) {
            js_set_paused(paused);
        }
        fn play_loop(&mut self, file: &str) {
            js_play_loop(file);
        }
        fn set_volumes(&mut self, music: f32, sfx: f32) {
            SFX_VOLUME.with(|v| v.set(sfx));
            js_set_volumes(music, sfx);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use rodio::Source;

    /// Decodes the player's own song and death sound when the game is installed (skipped
    /// otherwise; nothing is played).
    #[test]
    fn decodes_game_audio() {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        let root = home.join("Library/Application Support/Steam/steamapps/common/Geometry Dash/Geometry Dash.app/Contents/Resources");
        if !root.exists() {
            return;
        }
        for file in [super::level_song(1).unwrap(), super::DEATH_EFFECT.to_string(), super::level_song(5001).unwrap()] {
            let bytes = std::fs::read(root.join(&file)).unwrap();
            let decoder = rodio::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
            assert!(decoder.sample_rate().get() > 0, "{file}");
        }
    }
}

/// The web build has no offline decoder (headless screenshots are native only).
#[cfg(target_arch = "wasm32")]
pub struct DecodedSong;

#[cfg(target_arch = "wasm32")]
impl DecodedSong {
    pub fn decode(_files: &dyn crate::files::Files, _file: &str) -> Option<DecodedSong> {
        None
    }

    pub fn peak(&self, _secs: f32) -> f32 {
        0.0
    }
}

#[cfg(test)]
mod trigger_tests {
    use super::{pitch_for_index, Audio, TriggerSounds};
    use cadence_core::groups::{AudioEdit, Proximity, SfxPlay};
    use cadence_core::level::LevelString;

    /// Records what the sounds ask of a backend.
    #[derive(Default)]
    struct Probe {
        next: u64,
        playing: Vec<(u64, f32, f32)>,
        stopped: Vec<u64>,
        params: Vec<super::EffectParams>,
        loops_ended: Vec<u64>,
        reverb: Vec<i32>,
        muted: bool,
    }

    impl Audio for Probe {
        fn play_music(&mut self, _file: &str, _offset: f32) {}
        fn stop_music(&mut self) {}
        fn play_effect(&mut self, _file: &str) {}
        fn play_trigger_effect(&mut self, _file: &str, p: &super::EffectParams) -> u64 {
            self.next += 1;
            self.playing.push((self.next, p.volume, p.speed));
            self.params.push(*p);
            self.next
        }
        fn end_trigger_loop(&mut self, handle: u64) {
            self.loops_ended.push(handle);
        }
        fn set_reverb(&mut self, kind: i32) {
            self.reverb.push(kind);
        }
        fn sfx_volume(&self) -> f32 {
            if self.muted { 0.0 } else { 1.0 }
        }
        fn set_trigger_effect(&mut self, handle: u64, volume: f32, speed: f32) {
            if let Some(p) = self.playing.iter_mut().find(|p| p.0 == handle) {
                (p.1, p.2) = (volume, speed);
            }
        }
        fn stop_trigger_effect(&mut self, handle: u64) {
            self.playing.retain(|p| p.0 != handle);
            self.stopped.push(handle);
        }
        fn trigger_effect_playing(&self, handle: u64) -> bool {
            self.playing.iter().any(|p| p.0 == handle)
        }
    }

    impl Probe {
        fn of(&self, handle: u64) -> (f32, f32) {
            self.playing.iter().find(|p| p.0 == handle).map(|p| (p.1, p.2)).unwrap()
        }
    }

    fn sfx(keys: &str, order: u32) -> SfxPlay {
        let level = LevelString::parse(&format!("kA2,0;1,3602,2,0,3,0,{keys}"));
        SfxPlay::from_raw(&level.objects[0], order)
    }

    fn edit(keys: &str) -> AudioEdit {
        let level = LevelString::parse(&format!("kA2,0;1,3603,2,0,3,0,{keys}"));
        match cadence_core::groups::Trigger::parse(&level.objects[0], 0).unwrap().action {
            cadence_core::groups::Action::AudioEdit(e) => e,
            a => panic!("{a:?}"),
        }
    }

    #[test]
    fn sfx_trigger_plays_at_its_speed() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        // speed index 12 (an octave up), −12 (down), volume 0.5
        sounds.play(&mut probe, "s.ogg", &sfx("392,1,404,12,406,0.5", 3), 1.0);
        sounds.play(&mut probe, "s.ogg", &sfx("392,1,404,-12", 4), 1.0);
        assert_eq!(probe.of(1), (0.5, 2.0));
        assert_eq!(probe.of(2), (1.0, 0.5));
        assert_eq!(pitch_for_index(1), 2f32.powf(1.0 / 12.0));
    }

    #[test]
    fn edit_sfx_tweens_speed_and_volume_of_its_sounds_only() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        sounds.play(&mut probe, "a.ogg", &sfx("392,1", 7), 1.0);
        sounds.play(&mut probe, "b.ogg", &sfx("392,2", 8), 1.0);
        // the SFX triggers of group 457 are level order 7: speed to +12 and volume to 0.2 over 1 s
        let e = edit("457,9,419,1,404,12,418,1,406,0.2,10,1");
        sounds.edit(&mut probe, &e, &[7]);
        sounds.step(&mut probe, 0.5, &|_| 1.0);
        let (v, s) = probe.of(1);
        assert!((v - 0.6).abs() < 1e-6 && (s - 1.5).abs() < 1e-6, "{v} {s}");
        assert_eq!(probe.of(2), (1.0, 1.0));
        sounds.step(&mut probe, 0.75, &|_| 1.0);
        assert_eq!(probe.of(1), (0.2, 2.0));
    }

    #[test]
    fn edit_sfx_by_sfx_group_and_unique_id() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,455,4", 1), 1.0);
        sounds.play(&mut probe, "b.ogg", &sfx("392,1,415,1,416,33", 2), 1.0);
        sounds.play(&mut probe, "c.ogg", &sfx("392,1", 3), 1.0);
        // SFX group 4: stop; unique 33: speed −12 at once
        sounds.edit(&mut probe, &edit("455,4,417,1"), &[]);
        sounds.edit(&mut probe, &edit("416,33,419,1,404,-12"), &[]);
        sounds.step(&mut probe, 0.01, &|_| 1.0);
        assert_eq!(probe.stopped, vec![1]);
        assert_eq!(probe.of(2), (1.0, 0.5));
        assert_eq!(probe.of(3), (1.0, 1.0));
        assert_eq!(sounds.sounds.len(), 2);
    }

    #[test]
    fn proximity_is_per_sound_and_gates_the_sfx_trigger() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        // proximity to group 5 (the factor function gives group 5 → 0.25, else 1)
        let near = sfx("392,1,51,5,138,1,421,1,424,10", 1);
        assert!(near.proximity.is_some() && near.test_proximity);
        sounds.play(&mut probe, "a.ogg", &near, 0.0);
        assert!(probe.playing.is_empty(), "a proximity of 0 doesn't play");
        // 489: no test, plays at volume 0
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,51,5,138,1,489,1", 1), 0.0);
        assert_eq!(probe.of(1), (0.0, 1.0));
        sounds.play(&mut probe, "b.ogg", &sfx("392,1", 2), 1.0);
        let factor = |p: &Proximity| if p.group == 5 { 0.25 } else { 1.0 };
        sounds.step(&mut probe, 0.01, &factor);
        assert_eq!(probe.of(1), (0.25, 1.0));
        assert_eq!(probe.of(2), (1.0, 1.0));
        // an edit SFX trigger's proximity replaces the first sound's (group 6 → its factor 1)
        sounds.edit(&mut probe, &edit("457,9,51,6,138,1"), &[1]);
        sounds.step(&mut probe, 0.01, &factor);
        assert_eq!(probe.of(1), (1.0, 1.0));
        // a finished sound is dropped
        probe.playing.retain(|p| p.0 != 2);
        sounds.step(&mut probe, 0.01, &factor);
        assert_eq!(sounds.sounds.len(), 1);
    }

    #[test]
    fn sfx_trigger_passes_pitch_times_fades_loop_and_reverb() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        // pitch +5 with the big FFT, start 100, fade in 50, end 900, fade out 200, loop, reverb
        // into type 9 (arena) first
        let s = sfx("392,1,405,5,412,1,408,100,409,50,410,900,411,200,413,1,407,1,503,1,502,9", 1);
        sounds.play(&mut probe, "a.ogg", &s, 1.0);
        let p = probe.params[0];
        assert_eq!((p.pitch, p.big_fft), (pitch_for_index(5), true));
        assert_eq!((p.start_ms, p.fade_in_ms, p.end_ms, p.fade_out_ms), (100, 50, 900, 200));
        assert!(p.looped && p.reverb);
        assert_eq!(probe.reverb, vec![9]);
    }

    #[test]
    fn edit_sfx_414_ends_the_loop_and_417_stops() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,413,1,455,3", 1), 1.0);
        sounds.play(&mut probe, "b.ogg", &sfx("392,1,413,1,455,4", 2), 1.0);
        // 414 (with or without 417): stop looping, keep playing
        sounds.edit(&mut probe, &edit("455,3,414,1,417,1"), &[]);
        assert_eq!(probe.loops_ended, vec![1]);
        assert!(probe.stopped.is_empty());
        assert_eq!(sounds.sounds.len(), 2);
        // 417 alone stops
        sounds.edit(&mut probe, &edit("455,4,417,1"), &[]);
        assert_eq!(probe.stopped, vec![2]);
        sounds.stop_all(&mut probe);
        assert!(sounds.sounds.is_empty());
    }

    #[test]
    fn unique_ids_override_and_minimum_interval() {
        let mut probe = Probe::default();
        let mut sounds = TriggerSounds::default();
        // unique 7 playing: a second trigger with it doesn't play, it takes the channel over
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,415,1,416,7", 1), 1.0);
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,415,1,416,7", 2), 1.0);
        assert_eq!(probe.playing.len(), 1);
        assert_eq!(sounds.sounds[0].orders, vec![1, 2]);
        // an edit through trigger 2 reaches it
        sounds.edit(&mut probe, &edit("457,9,419,1,404,12"), &[2]);
        sounds.step(&mut probe, 0.01, &|_| 1.0);
        assert_eq!(probe.of(1).1, 2.0);
        // 420: the old one stops, the new one plays
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,415,1,416,7,420,1", 3), 1.0);
        assert_eq!(probe.stopped, vec![1]);
        assert_eq!(probe.playing.len(), 1);
        // 434: 0.5 s between plays of the same reference id (416, else the trigger's own)
        let mut sounds = TriggerSounds::default();
        let mut probe = Probe::default();
        let t = sfx("392,1,434,0.5", 4);
        sounds.play(&mut probe, "a.ogg", &t, 1.0);
        sounds.step(&mut probe, 0.25, &|_| 1.0);
        sounds.play(&mut probe, "a.ogg", &t, 1.0);
        assert_eq!(probe.params.len(), 1);
        sounds.step(&mut probe, 0.3, &|_| 1.0);
        sounds.play(&mut probe, "a.ogg", &t, 1.0);
        assert_eq!(probe.params.len(), 2);
    }

    #[test]
    fn muted_effects_only_play_loops() {
        let mut probe = Probe { muted: true, ..Default::default() };
        let mut sounds = TriggerSounds::default();
        sounds.play(&mut probe, "a.ogg", &sfx("392,1", 1), 1.0);
        assert!(probe.params.is_empty());
        sounds.play(&mut probe, "a.ogg", &sfx("392,1,413,1", 2), 1.0);
        assert_eq!(probe.params.len(), 1);
    }
}
