//! Audio engine adapter (rodio 0.23).
//!
//! Rules:
//! - Fade ramps run on Rust threads at ~85 Hz with curve math; the frontend
//!   never contributes to audio timing.
//! - Stop always goes through a short 8 ms ramp so playback never clicks.
//! - Transport commands are idempotent: one voice per cue id. Replaying a cue
//!   fines the previous voice first. Multiple voices coexist so held audio
//!   under the next cue is possible.
//! - Gain model: player volume = curve(t) * voice_base * master, and the
//!   master gain is re-applied to every voice when it changes.

use crate::core::model::FadeCurve;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::mixer::Mixer;
use rodio::{Decoder, Player, Source};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Fade ramp update rate (Hz).
pub const FADE_UPDATE_HZ: u32 = 85;
/// Click-free hard-stop fade duration (ms).
pub const HARD_STOP_FADE_MS: u64 = 8;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio device unavailable: {0}")]
    Device(String),
    #[error("no audio stream is open — pick an output device in Settings")]
    NoStream,
    #[error("could not decode '{name}': unsupported or corrupted file")]
    Decode { name: String },
    #[error("unknown voice '{0}'")]
    UnknownVoice(String),
}

// ---------------------------------------------------------------------------
// Gain math
// ---------------------------------------------------------------------------

pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 { -90.0 } else { 20.0 * linear.log10() }
}

pub fn fade_factor(curve: FadeCurve, t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0) as f32;
    match curve {
        FadeCurve::Linear => t,
        FadeCurve::Exponential => t.powi(4),
        FadeCurve::Logarithmic => 1.0 - (1.0 - t).powi(4),
        FadeCurve::SCurve => t * t * (3.0 - 2.0 * t),
        FadeCurve::EqualPower => (t * std::f32::consts::FRAC_PI_2).sin(),
    }
}

// ---------------------------------------------------------------------------
// Voice data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceState {
    pub cue_id: String,
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
}

struct Voice {
    player: Player,
    state: VoiceState,
    /// Pre-fader base gain (linear) from the cue's volume setting.
    base_linear: f32,
    finished: Arc<AtomicBool>,
}

// ---------------------------------------------------------------------------
// Equal-power stereo pan filter (applied at voice start)
// ---------------------------------------------------------------------------

struct PanSource<S> {
    inner: S,
    /// -1.0 (full L) .. +1.0 (full R)
    pan: f32,
    first_of_frame: bool,
}

impl<S> PanSource<S> {
    fn gains(pan: f32) -> (f32, f32) {
        let theta = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        (theta.cos(), theta.sin())
    }
}

impl<S> Iterator for PanSource<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.inner.channels().get() == 2 {
            let (gl, gr) = Self::gains(self.pan);
            if self.first_of_frame {
                self.first_of_frame = false;
                self.inner.next().map(|v| v * gl)
            } else {
                self.first_of_frame = true;
                self.inner.next().map(|v| v * gr)
            }
        } else {
            let (g, _) = Self::gains(self.pan);
            self.inner.next().map(|v| v * g)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for PanSource<S>
where
    S: Source<Item = f32>,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

// ---------------------------------------------------------------------------
// Finish detection
// ---------------------------------------------------------------------------

/// Polls a voice inside the shared map and reports when playback is empty.
fn watch_finish(voices: Arc<Mutex<HashMap<String, Voice>>>, cue_id: String, done: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        loop {
            let player_empty = {
                let map = voices.lock().unwrap();
                match map.get(&cue_id) {
                    Some(v) => v.player.empty(),
                    // Voice was removed externally; the receiver is likely
                    // dead — signal and exit.
                    None => true,
                }
            };
            if player_empty {
                done.store(true, Ordering::Relaxed);
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

struct MasterState {
    device_sink: Option<rodio::stream::MixerDeviceSink>,
    master_db: f32,
    max_master_db: f32,
    muted: bool,
}

pub struct AudioEngine {
    master: Arc<Mutex<MasterState>>,
    voices: Arc<Mutex<HashMap<String, Voice>>>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            master: Arc::new(Mutex::new(MasterState {
                device_sink: None,
                master_db: 0.0,
                max_master_db: 0.0,
                muted: false,
            })),
            voices: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // ------------------------------------------------------------------
    // Output devices
    // ------------------------------------------------------------------

    pub fn output_devices(&self) -> Vec<(String, String)> {
        let host = rodio::cpal::platform::default_host();
        let mut out = Vec::new();
        if let Ok(devices) = host.output_devices() {
            for (idx, d) in devices.enumerate() {
                let name = d
                    .description()
                    .map(|desc| desc.name().to_string())
                    .unwrap_or_else(|_| format!("Output device {idx}"));
                let config = d
                    .default_output_config()
                    .map(|c| format!("{} Hz · {} ch", c.sample_rate(), c.channels()))
                    .unwrap_or_default();
                out.push((name, config));
            }
        }
        out
    }

    /// Open a device by name; fall back safely to default if unavailable.
    pub fn open_output(&self, device_name: Option<String>) -> Result<String, AudioError> {
        let mut guard = self.master.lock().unwrap();
        guard.device_sink = None;
        self.voices.lock().unwrap().clear();

        let sink = match device_name.as_deref() {
            Some(name) => {
                let host = rodio::cpal::platform::default_host();
                let attempt = host
                    .output_devices()
                    .ok()
                    .and_then(|mut ds| {
                        ds.find(|d| {
                            d.description()
                                .map(|desc| desc.name().to_string())
                                .ok()
                                .as_deref()
                                == Some(name)
                        })
                    })
                    .and_then(|d| rodio::DeviceSinkBuilder::from_device(d).ok())
                    .and_then(|b| b.open_stream().ok());
                match attempt {
                    Some(sink) => Some(sink),
                    None => DeviceSinkFallback::open(),
                }
            }
            None => DeviceSinkFallback::open(),
        }
        .ok_or_else(|| AudioError::Device("could not open any output device".into()))?;

        let chosen = device_name.unwrap_or_else(|| "Default".into());
        guard.device_sink = Some(sink);
        Ok(chosen)
    }

    fn mixer_access<T>(&self, f: impl FnOnce(&Mixer) -> Result<T, AudioError>) -> Result<T, AudioError> {
        let guard = self.master.lock().unwrap();
        let sink = guard.device_sink.as_ref().ok_or(AudioError::NoStream)?;
        f(sink.mixer())
    }

    // ------------------------------------------------------------------
    // Master gain
    // ------------------------------------------------------------------

    pub fn master_db(&self) -> f32 {
        self.master.lock().unwrap().master_db
    }

    pub fn master_muted(&self) -> bool {
        self.master.lock().unwrap().muted
    }

    pub fn set_master_db(&self, db: f32) {
        {
            let mut g = self.master.lock().unwrap();
            g.master_db = if g.max_master_db.is_finite() && g.max_master_db != 0.0 {
                db.min(g.max_master_db)
            } else {
                db
            };
        }
        self.refresh_all_gains();
    }

    pub fn set_master_muted(&self, muted: bool) {
        self.master.lock().unwrap().muted = muted;
        self.refresh_all_gains();
    }

    pub fn set_max_master_db(&self, max: f32) {
        self.master.lock().unwrap().max_master_db = max;
        self.set_master_db(self.master_db());
    }

    /// Whether any voice is soloed (then only soloed voices sound).
    fn solo_mode(&self) -> bool {
        self.voices.lock().unwrap().values().any(|v| v.state.soloed)
    }

    /// Recompute every voice volume against current master/mute/solo state.
    fn refresh_all_gains(&self) {
        let master_db = self.effective_master_db();
        let solo = self.solo_mode();
        let voices = self.voices.lock().unwrap();
        for v in voices.values() {
            v.player.set_volume(Self::effective_linear(v, master_db, solo).clamp(0.0, 4.0));
        }
    }

    fn effective_master_db(&self) -> f32 {
        let g = self.master.lock().unwrap();
        if g.muted { -96.0 } else { g.master_db }
    }

    fn effective_linear(v: &Voice, master_db: f32, solo_mode: bool) -> f32 {
        if v.state.muted || (solo_mode && !v.state.soloed) {
            0.0
        } else {
            v.base_linear * db_to_linear(master_db)
        }
    }

    fn apply_one(&self, cue_id: &str) {
        let master = self.effective_master_db();
        let solo = self.solo_mode();
        let voices = self.voices.lock().unwrap();
        if let Some(v) = voices.get(cue_id) {
            v.player.set_volume(Self::effective_linear(v, master, solo).clamp(0.0, 4.0));
        }
    }

    // ------------------------------------------------------------------
    // Transport
    // ------------------------------------------------------------------

    /// Play `file` into a per-cue voice. Returns the decoded duration if known.
    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &self,
        cue_id: &str,
        file: &Path,
        start_secs: f64,
        fade_in_ms: u64,
        fade_curve: FadeCurve,
        gain_db: f32,
        pan: f32,
    ) -> Result<Option<f64>, AudioError> {
        if !file.exists() {
            // Missing media is surfaced upstream with a UI error; stay silent.
            return Ok(None);
        }

        // Idempotence: replaying a cue stops the previous instance first.
        let _ = self.stop(cue_id);

        let opened =
            File::open(file).map_err(|_| AudioError::Decode { name: file.display().to_string() })?;
        let decoder =
            Decoder::new(BufReader::new(opened)).map_err(|_| AudioError::Decode { name: cue_id.into() })?;
        let total = decoder.total_duration().map(|d| d.as_secs_f64());

        let (total, player) = self.mixer_access(|mixer| {
            let player = Player::connect_new(mixer);
            if pan.abs() > f32::EPSILON {
                player.append(PanSource { inner: decoder, pan, first_of_frame: true });
            } else {
                player.append(decoder);
            }
            if start_secs > 0.0 {
                let _ = player.try_seek(Duration::from_secs_f64(start_secs));
            }
            Ok::<_, AudioError>((total, player))
        })?;

        let done = Arc::new(AtomicBool::new(false));
        watch_finish(self.voices.clone(), cue_id.to_string(), done.clone());

        self.voices.lock().unwrap().insert(
            cue_id.to_string(),
            Voice {
                player: player,
                state: VoiceState { cue_id: cue_id.into(), gain_db, muted: false, soloed: false },
                base_linear: db_to_linear(gain_db),
                finished: done.clone(),
            },
        );

        if fade_in_ms > 0 {
            self.spawn_ramp(cue_id, 0.0, db_to_linear(gain_db), fade_in_ms, fade_curve, false);
        } else {
            self.apply_one(cue_id);
        }
        Ok(total)
    }

    /// Spawn an independent ramp thread for one voice.
    ///
    /// `stop_at_end` removes the voice once the ramp completes (used by
    /// stop/fade-out); a fade-in just settles at the target volume.
    fn spawn_ramp(&self, cue_id: &str, from: f32, to: f32, ms: u64, curve: FadeCurve, stop_at_end: bool) {
        let voices = self.voices.clone();
        let master = self.master.clone();
        let cue = cue_id.to_string();
        let interval = Duration::from_millis(1000 / FADE_UPDATE_HZ as u64).max(Duration::from_millis(2));
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            loop {
                let t = started.elapsed().as_secs_f64() * 1000.0 / ms.max(1) as f64;
                if t >= 1.0 {
                    break;
                }
                let factor = fade_factor(curve, t);
                let mut value = from + (to - from) * factor;
                // Master/mute state scales the audible ramp coordinate.
                let (master_db, muted) = {
                    let g = master.lock().unwrap();
                    (g.master_db, g.muted)
                };
                if muted {
                    value = 0.0;
                } else if master_db < 0.0 {
                    value *= db_to_linear(master_db);
                }
                let map = voices.lock().unwrap();
                if let Some(v) = map.get(&cue) {
                    v.player.set_volume((value * v.base_linear / to.max(1e-6) * 0.0 + value.max(0.0)).clamp(0.0, 4.0));
                } else {
                    return; // voice removed mid-ramp: ramp dies silently
                }
                drop(map);
                std::thread::sleep(interval);
            }
            let map = voices.lock().unwrap();
            if let Some(v) = map.get(&cue) {
                v.player.set_volume(to.max(0.0).clamp(0.0, 4.0));
                if stop_at_end {
                    v.player.stop();
                    drop(map);
                    voices.lock().unwrap().remove(&cue);
                }
            }
        });
    }

    pub fn pause(&self, cue_id: &str) -> Result<(), AudioError> {
        let voices = self.voices.lock().unwrap();
        voices
            .get(cue_id)
            .ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?
            .player
            .pause();
        Ok(())
    }

    pub fn resume(&self, cue_id: &str) -> Result<(), AudioError> {
        let voices = self.voices.lock().unwrap();
        voices
            .get(cue_id)
            .ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?
            .player
            .play();
        Ok(())
    }

    pub fn seek(&self, cue_id: &str, secs: f64) -> Result<(), AudioError> {
        let voices = self.voices.lock().unwrap();
        let v = voices.get(cue_id).ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?;
        let _ = v.player.try_seek(Duration::from_secs_f64(secs.max(0.0)));
        Ok(())
    }

    /// Position within the media being played (seconds).
    pub fn position(&self, cue_id: &str) -> Option<f64> {
        let voices = self.voices.lock().unwrap();
        voices.get(cue_id).map(|v| v.player.get_pos().as_secs_f64())
    }

    /// Stop with the quick click-free ramp, then remove the voice.
    pub fn stop(&self, cue_id: &str) -> Result<(), AudioError> {
        let current = {
            let voices = self.voices.lock().unwrap();
            voices.get(cue_id).map(|v| v.player.volume())
        };
        match current {
            None => return Err(AudioError::UnknownVoice(cue_id.into())),
            Some(0.0) => {
                let voices = self.voices.lock().unwrap();
                if let Some(v) = voices.get(cue_id) {
                    v.player.stop();
                }
                drop(voices);
                self.voices.lock().unwrap().remove(cue_id);
            }
            Some(base) => self.spawn_ramp(cue_id, base, 0.0, HARD_STOP_FADE_MS.max(1), FadeCurve::Linear, true),
        }
        Ok(())
    }

    /// Fade a voice to silence over `ms` then remove the voice.
    pub fn fade_out(&self, cue_id: &str, ms: u64, curve: FadeCurve) -> Result<(), AudioError> {
        let current = {
            let voices = self.voices.lock().unwrap();
            voices.get(cue_id).map(|v| v.player.volume())
        }
        .ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?;
        self.spawn_ramp(cue_id, current, 0.0, ms.max(1), curve, true);
        Ok(())
    }

    pub fn set_voice_gain(&self, cue_id: &str, gain_db: f32) -> Result<(), AudioError> {
        {
            let mut voices = self.voices.lock().unwrap();
            let v = voices.get_mut(cue_id).ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?;
            v.state.gain_db = gain_db;
            v.base_linear = db_to_linear(gain_db);
        }
        self.apply_one(cue_id);
        Ok(())
    }

    pub fn set_voice_mute(&self, cue_id: &str, muted: bool) -> Result<(), AudioError> {
        {
            let mut voices = self.voices.lock().unwrap();
            let v = voices.get_mut(cue_id).ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?;
            v.state.muted = muted;
        }
        self.apply_one(cue_id);
        Ok(())
    }

    pub fn set_voice_solo(&self, cue_id: &str, soloed: bool) -> Result<(), AudioError> {
        {
            let mut voices = self.voices.lock().unwrap();
            let v = voices.get_mut(cue_id).ok_or_else(|| AudioError::UnknownVoice(cue_id.into()))?;
            v.state.soloed = soloed;
        }
        self.refresh_all_gains();
        Ok(())
    }

    pub fn stop_all(&self) {
        let voices = self.voices.lock().unwrap();
        for v in voices.values() {
            v.player.stop();
        }
        drop(voices);
        self.voices.lock().unwrap().clear();
    }

    /// Drains finished-voice notifications (cues that ended naturally).
    pub fn finished_cues(&self) -> Vec<String> {
        let mut finished = Vec::new();
        let voices = self.voices.lock().unwrap();
        for (id, v) in voices.iter() {
            if v.finished.load(Ordering::Relaxed) {
                finished.push(id.clone());
            }
        }
        drop(voices);
        for id in &finished {
            self.voices.lock().unwrap().remove(id);
        }
        finished
    }

    pub fn active_cue_ids(&self) -> Vec<String> {
        self.voices.lock().unwrap().keys().cloned().collect()
    }

    pub fn voice_states(&self) -> Vec<VoiceState> {
        self.voices.lock().unwrap().values().map(|v| v.state.clone()).collect()
    }

    /// Cancel pending ramps for a voice (ramps self-check voice presence each
    /// tick; a per-ramp cancel handle arrives with the Phase-5 envelope work).
    pub fn cancel_fades(&self, _cue_id: &str) {}
}

/// Prefer the requested device; on any error fall back to the default sink.
struct DeviceSinkFallback;

impl DeviceSinkFallback {
    fn open() -> Option<rodio::stream::MixerDeviceSink> {
        rodio::DeviceSinkBuilder::open_default_sink().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_curves_are_monotonic_and_bounded() {
        for curve in [
            FadeCurve::Linear,
            FadeCurve::Exponential,
            FadeCurve::Logarithmic,
            FadeCurve::SCurve,
            FadeCurve::EqualPower,
        ] {
            let mut prev = fade_factor(curve, 0.0);
            for step in 1..=10 {
                let f = fade_factor(curve, step as f64 / 10.0);
                assert!((0.0..=1.0).contains(&f), "{curve:?} out of range at {step}");
                assert!(f >= prev - 1e-6, "{curve:?} not monotonic at {step}");
                prev = f;
            }
            assert!((fade_factor(curve, 1.0) - 1.0).abs() < 0.02, "{curve:?} t=1 != 1");
            assert!(fade_factor(curve, 0.0).abs() < 0.02, "{curve:?} t=0 != 0");
        }
    }

    #[test]
    fn db_conversion_round_trip() {
        for db in [-60.0f32, -20.0, -6.0, 0.0, 6.0] {
            let linear = db_to_linear(db);
            assert!((linear_to_db(linear) - db).abs() < 0.001);
        }
        assert_eq!(linear_to_db(0.0), -90.0);
    }

    #[test]
    fn pan_law_is_equal_power() {
        // Center: both channels at full/2-efficiency.
        let (gl, gr) = PanSource::<Vec<f32>>::gains(0.0);
        assert!((gl - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((gr - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        // Full left: right channel silent.
        let (gl, gr) = PanSource::<Vec<f32>>::gains(-1.0);
        assert!((gl - 1.0).abs() < 1e-4, "gL={gl}");
        assert!(gr.abs() < 1e-4, "gR={gr}");
    }
}
