//! ShowRunner: the runtime adapter that connects the deterministic cue state
//! machine (`core::machine`) to the audio engine and the UI (via Tauri events).
//!
//! All timing-sensitive logic stays here:
//! - A 30 ms tick loop advances fades, auto-follow schedules and reports cue
//!   completion from the audio engine — never the frontend render loop.
//! - CueEvents from the machine are translated into concrete engine calls
//!   in one place, so behavior is traceable and testable at the core level.

use crate::audio::engine::AudioEngine;
use crate::core::machine::Clock;
use crate::core::machine::{CueEvent, ShowCommand, ShowMachine};
use crate::core::model::{Cue, FitMode, MediaKind, Project, TriggerMode};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageVisual {
    pub cue_id: String,
    pub cue_number: String,
    pub kind: MediaKind,
    pub asset_path: Option<String>,
    pub fit_mode: FitMode,
    pub transition_ms: u64,
    pub blackout: bool,
    pub test_pattern: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowLogEntry {
    pub at_ms: u64,
    pub level: String,
    pub message: String,
}

pub struct ShowRunner {
    pub audio: Arc<AudioEngine>,
    machine: Mutex<ShowMachine>,
    project: Mutex<Arc<Project>>,
    blacked_out: Mutex<bool>,
    test_pattern: Mutex<bool>,
    show_log: Mutex<Vec<ShowLogEntry>>,
    app: AppHandle,
}

impl ShowRunner {
    pub fn new(project: Arc<Project>, app: AppHandle) -> Self {
        let audio = Arc::new(AudioEngine::new());
        Self {
            audio,
            machine: Mutex::new(ShowMachine::from_cues(&project.cues)),
            project: Mutex::new(project),
            blacked_out: Mutex::new(false),
            test_pattern: Mutex::new(false),
            show_log: Mutex::new(Vec::new()),
            app,
        }
    }

    pub fn replace_project(&self, project: Arc<Project>) {
        *self.project.lock().unwrap() = project.clone();
        self.machine.lock().unwrap().sync_cues(&project.cues);
    }

    pub fn project(&self) -> Arc<Project> {
        self.project.lock().unwrap().clone()
    }

    // ------------------------------------------------------------------
    // Event translation
    // ------------------------------------------------------------------

    fn translate(&self, events: Vec<CueEvent>, commands: &mut Vec<ShowCommand>) {
        let project = self.project();
        for event in events {
            match event {
                CueEvent::StartAudio { cue_index, fade_ms, fade_curve, .. } => {
                    if let Some((cue, path)) = self.audio_target(&project, cue_index) {
                        if let Err(err) = self.audio.play(
                            &cue.id,
                            &path,
                            cue.start_offset_secs,
                            fade_ms,
                            fade_curve,
                            cue.volume_db,
                            0.0,
                        ) {
                            self.log("error", &format!("audio start failed: {err}"));
                            commands.push(ShowCommand::MarkCueFailed(cue_index));
                        }
                    }
                }
                CueEvent::FadeOutAudio { cue_index, fade_ms, fade_curve, .. } => {
                    if let Some(cue) = project.cues.get(cue_index) {
                        if let Err(err) = self.audio.fade_out(&cue.id, fade_ms.max(1), fade_curve) {
                            self.log("warn", &format!("fade-out failed: {err}"));
                        }
                    }
                }
                CueEvent::StopAudio { cue_index: Some(idx) } => {
                    if let Some(cue) = project.cues.get(idx) {
                        if let Err(err) = self.audio.stop(&cue.id) {
                            self.log("warn", &format!("stop failed: {err}"));
                        }
                    }
                }
                CueEvent::StopAudio { cue_index: None } => {
                    self.audio.stop_all();
                }
                CueEvent::PauseAudio { cue_index } => {
                    if let Some(cue) = project.cues.get(cue_index) {
                        let _ = self.audio.pause(&cue.id);
                    }
                }
                CueEvent::ResumeAudio { cue_index } => {
                    if let Some(cue) = project.cues.get(cue_index) {
                        let _ = self.audio.resume(&cue.id);
                    }
                }
                CueEvent::SetVisual { cue_index, transition_ms, .. }
                | CueEvent::KeepAudioChangeVisual { cue_index, transition_ms, .. } => {
                    if let Some(cue) = project.cues.get(cue_index) {
                        if cue.visual_asset_id.is_some() {
                            self.emit_visual(&project, cue, transition_ms);
                        }
                    }
                }
                CueEvent::ClearVisual => {
                    let _ = self
                        .app
                        .emit("stagecue://visual", StageVisual {
                            cue_id: String::new(),
                            cue_number: String::new(),
                            kind: MediaKind::Text,
                            asset_path: None,
                            fit_mode: FitMode::Contain,
                            transition_ms: 0,
                            blackout: false,
                            test_pattern: false,
                        });
                }
                CueEvent::Blackout { active } => {
                    *self.blacked_out.lock().unwrap() = active;
                    let _ = self.emit_visual_state(None, "");
                }
                CueEvent::Armed { .. } => {}
                CueEvent::DuckBus { .. } => {} // Phase 5: bus engine
                CueEvent::CueFailed { cue_index } => {
                    if let Some(cue) = project.cues.get(cue_index) {
                        self.log("error", &format!("cue {} failed during playback", cue.number));
                    }
                }
                CueEvent::ShowStopped { .. } => {}
            }
        }
    }

    fn audio_target(&self, project: &Project, cue_index: usize) -> Option<(Cue, PathBuf)> {
        let cue = project.cues.get(cue_index)?;
        let asset_id = cue.audio_asset_ids.first()?;
        let asset = project.asset(asset_id)?;
        Some((cue.clone(), asset.path.clone()))
    }

    fn emit_visual(&self, _project: &Project, cue: &Cue, transition_ms: u64) {
        let project = self.project();
        let asset = cue.visual_asset_id.as_deref().and_then(|id| project.asset(id));
        let kind = asset.map(|a| a.kind).unwrap_or(MediaKind::Text);
        let _ = self
            .app
            .emit("stagecue://visual", StageVisual {
                cue_id: cue.id.clone(),
                cue_number: cue.number.clone(),
                kind,
                asset_path: asset.map(|a| a.path.display().to_string()),
                fit_mode: cue.fit_mode,
                transition_ms,
                blackout: *self.blacked_out.lock().unwrap(),
                test_pattern: *self.test_pattern.lock().unwrap(),
            });
    }

    fn emit_visual_state(&self, _project: Option<Project>, _unused: &str) -> Result<(), tauri::Error> {
        self.app.emit("stagecue://visual", StageVisual {
            cue_id: String::new(),
            cue_number: String::new(),
            kind: MediaKind::Text,
            asset_path: None,
            fit_mode: FitMode::Contain,
            transition_ms: 0,
            blackout: *self.blacked_out.lock().unwrap(),
            test_pattern: *self.test_pattern.lock().unwrap(),
        })
    }

    fn log(&self, level: &str, message: &str) {
        log::log!(
            match level {
                "error" => log::Level::Error,
                "warn" => log::Level::Warn,
                _ => log::Level::Info,
            },
            "{message}"
        );
        self.show_log.lock().unwrap().push(ShowLogEntry {
            at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            level: level.into(),
            message: message.into(),
        });
    }

    pub fn log_entries(&self) -> Vec<ShowLogEntry> {
        self.show_log.lock().unwrap().clone()
    }

    pub fn set_test_pattern(&self, on: bool) -> Result<(), tauri::Error> {
        *self.test_pattern.lock().unwrap() = on;
        self.emit_visual_state(None, "")
    }

    // ------------------------------------------------------------------
    // Command & tick entrypoints
    // ------------------------------------------------------------------

    pub fn dispatch(&self, cmd: ShowCommand) -> Result<(), String> {
        let clock = crate::core::machine::SystemClock;
        let mut follow: Vec<ShowCommand> = Vec::new();
        let events = self
            .machine
            .lock()
            .unwrap()
            .dispatch(cmd, &clock)
            .map_err(|e| e.to_string())?;
        self.translate(events, &mut follow);
        // One follow-up level (e.g. cue failure → stop audio); failures never
        // cascade further than that by design.
        for cmd in follow {
            let clock = crate::core::machine::SystemClock;
            if let Ok(events) = self.machine.lock().unwrap().dispatch(cmd, &clock) {
                self.translate(events, &mut Vec::new());
            }
        }
        self.emit_snapshot();
        Ok(())
    }

    pub fn dispatch_collect(&self, cmd: ShowCommand) -> Result<Vec<CueEvent>, String> {
        let clock = crate::core::machine::SystemClock;
        let mut follow: Vec<ShowCommand> = Vec::new();
        let events = self
            .machine
            .lock()
            .unwrap()
            .dispatch(cmd, &clock)
            .map_err(|e| e.to_string())?;
        self.translate(events.clone(), &mut follow);
        for cmd in follow {
            let clock = crate::core::machine::SystemClock;
            if let Ok(events) = self.machine.lock().unwrap().dispatch(cmd, &clock) {
                self.translate(events, &mut Vec::new());
            }
        }
        self.emit_snapshot();
        Ok(events)
    }

    pub fn snapshot(&self) -> crate::core::machine::ShowSnapshot {
        let clock = crate::core::machine::SystemClock;
        self.machine.lock().unwrap().snapshot(clock.now_ms())
    }

    pub fn emit_snapshot(&self) {
        let clock = crate::core::machine::SystemClock;
        let snap = self.machine.lock().unwrap().snapshot(clock.now_ms());
        let details = self.snapshot_details(&snap);
        let _ = self.app.emit("stagecue://show", details);
    }

    fn snapshot_details(&self, snap: &crate::core::machine::ShowSnapshot) -> serde_json::Value {
        let project = self.project();
        let machine = self.machine.lock().unwrap();
        let audio_engine = &self.audio;
        let _ = audio_engine;
        let positions: Vec<(String, Option<f64>)> = audio_engine
            .active_cue_ids()
            .into_iter()
            .map(|id| {
                let p = audio_engine.position(&id);
                (id, p)
            })
            .collect();
        serde_json::json!({
            "status": snap.status,
            "currentIndex": snap.current_index,
            "phase": snap.phase,
            "armedIndex": snap.armed_index,
            "phaseMs": snap.phase_ms,
            "blackout": snap.blackout,
            "heldAudioIndex": snap.held_audio_index,
            "pendingAuto": snap.pending_auto,
            "poses": positions,
            "cueCount": project.cues.len(),
            "currentCue": snap.current_index.map(|i| cue_json(&project, i, &machine)),
            "armedCue": snap.armed_index.map(|i| cue_json(&project, i, &machine)),
        })
    }

    /// Background service loop: advance fades, poll audio completions,
    /// publish state. Never touches blocking UI threads.
    pub fn spawn_tick_loop(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let runner = Arc::clone(self);
        std::thread::Builder::new()
            .name("stagecue-show-tick".into())
            .spawn(move || loop {
                let started = Instant::now();
                // 1) Does the audio engine say any cue finished naturally?
                let finished = runner.audio.finished_cues();
                if !finished.is_empty() {
                    let project = runner.project();
                    let mut follow = Vec::new();
                    for cue_id in finished {
                        if let Some(index) = project.cues.iter().position(|c| c.id == cue_id) {
                            let clock = crate::core::machine::SystemClock;
                            let events = runner
                                .machine
                                .lock()
                                .unwrap()
                                .dispatch(ShowCommand::MarkAudioFinished(index), &clock)
                                .unwrap_or_default();
                            runner.translate(events, &mut follow);
                        }
                    }
                    for cmd in follow {
                        if cmd != ShowCommand::Tick {
                            let _ = runner.dispatch_collect(cmd);
                        }
                    }
                    runner.emit_snapshot();
                }
                // 2) Advance timed transitions.
                if let Err(err) = runner.dispatch(ShowCommand::Tick) {
                    log::error!("tick failed: {err}");
                }
                // 3) Steady state at 30 Hz; adaptively longer under load.
                let elapsed = started.elapsed();
                let frame = Duration::from_millis(33);
                if elapsed < frame {
                    std::thread::sleep(frame - elapsed);
                }
            })
            .expect("spawn show tick loop")
    }
}

fn cue_json(project: &Project, index: usize, _machine: &ShowMachine) -> serde_json::Value {
    let Some(cue) = project.cues.get(index) else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "id": cue.id,
        "number": cue.number,
        "name": cue.name,
        "notes": cue.notes,
        "color": cue.color,
        "hasAudio": !cue.audio_asset_ids.is_empty(),
        "hasVisual": cue.visual_asset_id.is_some(),
        "stopBehavior": cue.stop_behavior,
        "triggerMode": cue.trigger_mode,
        "fadeOutMs": cue.fade_out_ms,
        "audioAssetId": cue.audio_asset_ids.first(),
        "visualAssetId": cue.visual_asset_id,
    })
}

/// Trigger-mode helper retained for cue editor validation.
pub fn is_auto_follow(cue: &Cue) -> bool {
    matches!(cue.trigger_mode, TriggerMode::AutoFollow { .. })
}
