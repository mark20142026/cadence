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
    /// An SFX trigger's sound at a relative volume and playback rate (the FMOD channel's pitch:
    /// speed and pitch together), kept so edit SFX triggers and proximity can change it. Returns
    /// its handle (0: not kept).
    fn play_trigger_effect(&mut self, file: &str, volume: f32, _speed: f32) -> u64 {
        self.play_effect_volume(file, volume);
        0
    }
    /// A kept trigger sound's volume and playback rate from now on.
    fn set_trigger_effect(&mut self, _handle: u64, _volume: f32, _speed: f32) {}
    fn stop_trigger_effect(&mut self, _handle: u64) {}
    /// Whether a kept trigger sound still plays.
    fn trigger_effect_playing(&self, _handle: u64) -> bool {
        false
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

/// `FMODAudioEngine::pitchForIdx`: a speed index in semitones (−12..12) as a playback rate.
pub fn pitch_for_index(index: i32) -> f32 {
    2f32.powf(index.clamp(-12, 12) as f32 / 12.0)
}

/// A sound an SFX trigger started: its own volume and speed (tweened by edit SFX triggers) and
/// its proximity effect, each per sound like the game's per-channel state.
#[derive(Clone, Debug)]
pub struct TriggerSound {
    pub handle: u64,
    /// The SFX trigger's level order, its SFX group (455) and unique id.
    pub order: u32,
    pub group: i32,
    pub unique: i32,
    pub volume: Tween,
    pub speed: Tween,
    pub proximity: Option<cadence_core::groups::Proximity>,
    /// What the backend was last sent (volume with the proximity factor, speed).
    sent: (f32, f32),
}

/// The sounds SFX triggers started (`activateSFXTrigger`'s channel ids per trigger, the
/// channels' tweens and the proximity effects by channel).
#[derive(Clone, Debug, Default)]
pub struct TriggerSounds {
    pub sounds: Vec<TriggerSound>,
}

impl TriggerSounds {
    /// Start an SFX trigger's sound; `factor` is its proximity volume now (1 without one).
    /// With a proximity group and the test on (no 489), a factor of 0 or less doesn't play it.
    pub fn play(&mut self, audio: &mut dyn Audio, file: &str, play: &cadence_core::groups::SfxPlay, factor: f32) {
        if play.proximity.is_some() && play.test_proximity && factor <= 0.0 {
            return;
        }
        let speed = pitch_for_index(play.speed);
        let factor = if play.proximity.is_some() { factor } else { 1.0 };
        let volume = play.volume * factor;
        let handle = audio.play_trigger_effect(file, volume, speed);
        if handle != 0 {
            self.sounds.push(TriggerSound {
                handle,
                order: play.order,
                group: play.group,
                unique: play.unique,
                volume: Tween::new(play.volume),
                speed: Tween::new(speed),
                proximity: play.proximity,
                sent: (volume, speed),
            });
        }
    }

    /// An edit SFX trigger (`activateSFXEditTrigger`): the sounds of the SFX triggers in its
    /// group 457 (`triggers`), the ones in its SFX group 455 and the one with its unique id 416
    /// stop, or tween their speed / volume over its duration; its proximity (51) replaces theirs
    /// (not for the unique id's sound).
    pub fn edit(&mut self, audio: &mut dyn Audio, edit: &cadence_core::groups::AudioEdit, triggers: &[u32]) {
        let mut i = 0;
        while i < self.sounds.len() {
            let s = &mut self.sounds[i];
            let by_group = triggers.contains(&s.order) || (edit.sfx_group > 0 && s.group == edit.sfx_group);
            let by_unique = edit.unique > 0 && s.unique == edit.unique;
            if !(by_group || by_unique) {
                i += 1;
                continue;
            }
            if edit.stop {
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

    /// One step: the tweens advance, the proximity factors (`factor`) apply, changes go to the
    /// backend and finished sounds are dropped.
    pub fn step(&mut self, audio: &mut dyn Audio, dt: f32, factor: &dyn Fn(&cadence_core::groups::Proximity) -> f32) {
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

    use super::{Audio, Meter};
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
        /// SFX triggers' sounds by handle (each its own player: volume and playback rate).
        trigger_effects: Vec<(u64, rodio::Player)>,
        next_handle: u64,
    }

    impl NativeAudio {
        /// `None` when there's no output device.
        pub fn new(files: Box<dyn Files>) -> Option<NativeAudio> {
            let mut sink = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
            sink.log_on_drop(false);
            Some(NativeAudio {
                sink,
                music: None,
                peak: Arc::new(AtomicU32::new(0)),
                files,
                cache: HashMap::new(),
                volumes: (1.0, 1.0),
                gain: 1.0,
                fade: None,
                trigger_effects: Vec::new(),
                next_handle: 1,
            })
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
            self.trigger_effects.retain(|(_, p)| !p.empty());
        }

        fn play_trigger_effect(&mut self, file: &str, volume: f32, speed: f32) -> u64 {
            if self.volumes.1 <= 0.0 {
                return 0;
            }
            let Some(decoder) = self.decoder(file) else { return 0 };
            let player = rodio::Player::connect_new(self.sink.mixer());
            player.append(decoder);
            // FMOD caps a channel's volume at 2; its pitch is the playback rate (resampled)
            player.set_volume(self.volumes.1 * volume.clamp(0.0, 2.0));
            player.set_speed(speed.clamp(0.01, 16.0));
            let handle = self.next_handle;
            self.next_handle += 1;
            self.trigger_effects.push((handle, player));
            handle
        }

        fn set_trigger_effect(&mut self, handle: u64, volume: f32, speed: f32) {
            if let Some((_, p)) = self.trigger_effects.iter().find(|(h, _)| *h == handle) {
                p.set_volume(self.volumes.1 * volume.clamp(0.0, 2.0));
                p.set_speed(speed.clamp(0.01, 16.0));
            }
        }

        fn stop_trigger_effect(&mut self, handle: u64) {
            if let Some(i) = self.trigger_effects.iter().position(|(h, _)| *h == handle) {
                self.trigger_effects.remove(i).1.stop();
            }
        }

        fn trigger_effect_playing(&self, handle: u64) -> bool {
            self.trigger_effects.iter().any(|(h, p)| *h == handle && !p.empty())
        }

        fn set_paused(&mut self, paused: bool) {
            if let Some(player) = &self.music {
                if paused { player.pause() } else { player.play() }
            }
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
    use wasm_bindgen::prelude::*;

    use super::Audio;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playMusic)]
        fn js_play_music(file: &str, offset: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = stopMusic)]
        fn js_stop_music();
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playEffect)]
        fn js_play_effect(file: &str, volume: f32);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playTriggerEffect)]
        fn js_play_trigger_effect(file: &str, volume: f32, speed: f32) -> u32;
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
        fn play_trigger_effect(&mut self, file: &str, volume: f32, speed: f32) -> u64 {
            js_play_trigger_effect(file, volume.clamp(0.0, 2.0), speed) as u64
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
    }

    impl Audio for Probe {
        fn play_music(&mut self, _file: &str, _offset: f32) {}
        fn stop_music(&mut self) {}
        fn play_effect(&mut self, _file: &str) {}
        fn play_trigger_effect(&mut self, _file: &str, volume: f32, speed: f32) -> u64 {
            self.next += 1;
            self.playing.push((self.next, volume, speed));
            self.next
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
}
