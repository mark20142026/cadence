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
    /// An SFX trigger's sound, kept so edit SFX triggers can stop it or change its volume.
    fn play_trigger_effect(&mut self, file: &str, volume: f32) {
        self.play_effect_volume(file, volume);
    }
    /// Edit song triggers: the music's own volume (on top of the options') toward `volume` over
    /// `duration` seconds (stepped by `update`), and its speed.
    fn fade_music(&mut self, _volume: f32, _duration: f32) {}
    fn set_music_speed(&mut self, _speed: f32) {}
    /// Edit SFX triggers: stop the trigger sounds, or set their volume.
    fn stop_effects(&mut self) {}
    fn set_effects_volume(&mut self, _volume: f32) {}
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
        /// SFX triggers' sounds, with their own volumes.
        trigger_effects: Vec<(rodio::Player, f32)>,
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
            self.trigger_effects.retain(|(p, _)| !p.empty());
        }

        fn play_trigger_effect(&mut self, file: &str, volume: f32) {
            if self.volumes.1 <= 0.0 {
                return;
            }
            let Some(decoder) = self.decoder(file) else { return };
            let player = rodio::Player::connect_new(self.sink.mixer());
            player.append(decoder);
            let v = volume.clamp(0.0, 2.0);
            player.set_volume(self.volumes.1 * v);
            self.trigger_effects.push((player, v));
        }

        fn stop_effects(&mut self) {
            for (p, _) in self.trigger_effects.drain(..) {
                p.stop();
            }
        }

        fn set_effects_volume(&mut self, volume: f32) {
            for (p, v) in &mut self.trigger_effects {
                *v = volume.clamp(0.0, 2.0);
                p.set_volume(self.volumes.1 * *v);
            }
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
pub use web::WebAudio;

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
        fn js_play_effect(file: &str);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = peak)]
        fn js_peak() -> f32;
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = setPaused)]
        fn js_set_paused(paused: bool);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = playLoop)]
        fn js_play_loop(file: &str);
        #[wasm_bindgen(js_namespace = cadenceAudio, js_name = setVolumes)]
        fn js_set_volumes(music: f32, sfx: f32);
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
            js_play_effect(file);
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
