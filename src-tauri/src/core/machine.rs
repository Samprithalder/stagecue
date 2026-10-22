//! Deterministic cue state machine.
//!
//! Design rules (live-safety):
//! - All transport commands are **idempotent**: repeated GO / STOP / FADE
//!   presses never create overlapping playback or race conditions.
//! - Timing comes from an abstract millisecond clock, so every transition is
//!   unit-testable deterministically without sleeping.
//! - The machine *describes* actions with `CueEvent`s; the audio/visual
//!   engines execute them. The frontend only renders state, never authors it.
//! - An 80 ms double-press guard makes stray repeated GO presses harmless.
//! - GO always targets a single new cue; the previous cue is finished exactly
//!   once according to its `stop_behavior`. Stopping emits explicit stop
//!   events, so audio engines cannot double-play a cue.

use crate::core::model::{Cue, FadeCurve, StopBehavior, TriggerMode};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Flow cues (the slice of Cue the machine cares about)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FlowCue {
    pub id: String,
    pub number: String,
    pub name: String,
    pub stop_behavior: StopBehavior,
    pub fade_in_ms: u64,
    pub fade_in_curve: FadeCurve,
    pub fade_out_ms: u64,
    pub fade_out_curve: FadeCurve,
    pub hold_visual_after_audio: bool,
    pub auto_arm_next: bool,
    pub auto_follow_delay_ms: Option<u64>,
    pub visual_transition_ms: u64,
    pub has_audio: bool,
    pub has_visual: bool,
    pub duck_others: bool,
    pub bus: String,
}

impl From<&Cue> for FlowCue {
    fn from(c: &Cue) -> Self {
        Self {
            id: c.id.clone(),
            number: c.number.clone(),
            name: c.name.clone(),
            stop_behavior: c.stop_behavior,
            fade_in_ms: c.fade_in_ms,
            fade_in_curve: c.fade_in_curve,
            fade_out_ms: c.fade_out_ms.max(1),
            fade_out_curve: c.fade_out_curve,
            hold_visual_after_audio: c.hold_visual_after_audio,
            auto_arm_next: c.auto_arm_next,
            auto_follow_delay_ms: match &c.trigger_mode {
                TriggerMode::Manual => None,
                TriggerMode::AutoFollow { delay_ms } => Some(*delay_ms),
            },
            visual_transition_ms: c.transition.duration_ms(),
            has_audio: !c.audio_asset_ids.is_empty(),
            has_visual: c.visual_asset_id.is_some(),
            duck_others: c.duck_others,
            bus: format!("{:?}", c.audio_bus),
        }
    }
}

// ---------------------------------------------------------------------------
// Phases & snapshots
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Audio start was just issued; fade-in running.
    Starting,
    Playing,
    Paused,
    FadingOut,
    /// Audio finished; the visual is holding until the next cue.
    HoldingVisual,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowStatus {
    Idle,
    Running,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowSnapshot {
    pub status: ShowStatus,
    pub current_index: Option<usize>,
    pub phase: Option<Phase>,
    pub armed_index: Option<usize>,
    pub phase_ms: u64,
    pub blackout: bool,
    pub held_audio_index: Option<usize>,
    /// Schedule of a pending auto-follow, for the UI countdown: (index, fire_at_ms relative now).
    pub pending_auto: Option<(usize, u64)>,
}

// ---------------------------------------------------------------------------
// Events (instructions for the engines)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CueEvent {
    #[serde(rename_all = "camelCase")]
    StartAudio { cue_index: usize, cue_id: String, cue_number: String, fade_ms: u64, fade_curve: FadeCurve },
    #[serde(rename_all = "camelCase")]
    FadeOutAudio { cue_index: usize, fade_ms: u64, fade_curve: FadeCurve },
    /// Stop one cue's audio; `None` means ALL audio (Panic).
    #[serde(rename_all = "camelCase")]
    StopAudio { cue_index: Option<usize> },
    #[serde(rename_all = "camelCase")]
    PauseAudio { cue_index: usize },
    #[serde(rename_all = "camelCase")]
    ResumeAudio { cue_index: usize },
    #[serde(rename_all = "camelCase")]
    SetVisual { cue_index: usize, cue_id: String, transition_ms: u64 },
    /// Change visuals while audio keeps playing (previous cue's Hold).
    #[serde(rename_all = "camelCase")]
    KeepAudioChangeVisual { cue_index: usize, cue_id: String, transition_ms: u64 },
    ClearVisual,
    Blackout { active: bool },
    #[serde(rename_all = "camelCase")]
    Armed { index: Option<usize>, cue_id: Option<String> },
    #[serde(rename_all = "camelCase")]
    DuckBus { bus: String, duck: bool },
    #[serde(rename_all = "camelCase")]
    CueFailed { cue_index: usize },
    #[serde(rename_all = "camelCase")]
    ShowStopped { duration_ms: u64 },
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShowCommand {
    Tick,
    Go,
    StopCurrent,
    FadeOutCurrent,
    TogglePause,
    /// Hard-stop and restart the current cue.
    RestartCue,
    Panic,
    ToggleBlackout,
    SetBlackout(bool),
    /// Emergency recovery: skip a failed cue to the next.
    RecoverSkip,
    /// Operator selects a cue as next-to-fire.
    ArmCue(usize),
    /// Audio engine reports a cue's audio finished naturally.
    MarkAudioFinished(usize),
    /// Audio engine reports an unrecoverable playback error.
    MarkCueFailed(usize),
}

/// Debounce window for transport idempotence (double-press protection).
pub const DEBOUNCE_MS: u64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ShowError {
    /// Command not valid in the current state — safe no-op, not a failure.
    #[error("command ignored in current state")]
    Ignored,
    #[error("no cue armed")]
    NoCueArmed,
    #[error("cue index out of range")]
    InvalidCueIndex,
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Machine state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum State {
    /// Nothing playing. `armed` is the cue to fire on GO.
    Idle { armed: Option<usize> },
    Running(Run),
    /// Stopped by STOP ALL / PANIC. Restart requires an explicit ArmCue first.
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
struct Run {
    current: usize,
    phase: Phase,
    phase_start_ms: u64,
    fade_ms: u64,
    fade_curve: FadeCurve,
    /// Index of a previous cue whose audio keeps sounding under this cue.
    held_audio: Option<usize>,
    /// (index, fire_at_ms) — scheduled auto-follow of the next cue.
    pending_auto: Option<(usize, u64)>,
    /// Operator-selected next cue; overrides auto-arm ordering.
    armed_override: Option<usize>,
    started_at_ms: u64,
}

pub struct ShowMachine {
    cues: Vec<FlowCue>,
    state: State,
    blackout: bool,
    show_start_ms: u64,
}

impl ShowMachine {
    /// Build from cue list; the first cue is armed initially.
    pub fn new(cues: Vec<FlowCue>, now_ms: u64) -> Self {
        let first = if cues.is_empty() { None } else { Some(0usize) };
        Self {
            cues,
            state: State::Idle { armed: first },
            blackout: false,
            show_start_ms: now_ms,
        }
    }

    pub fn from_cues(cues: &[Cue]) -> Self {
        Self::new(cues.iter().map(FlowCue::from).collect(), 0)
    }

    /// Re-sync to an edited cue list; safe to call after every editor change.
    pub fn sync_cues(&mut self, cues: &[Cue]) {
        self.cues = cues.iter().map(FlowCue::from).collect();
    }

    pub fn cue_count(&self) -> usize {
        self.cues.len()
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self.state, State::Stopped)
    }

    pub fn snapshot(&self, now_ms: u64) -> ShowSnapshot {
        let (status, current, phase, armed, phase_ms, held, pending) = match &self.state {
            State::Idle { armed } => (ShowStatus::Idle, None, None, *armed, 0, None, None),
            State::Stopped => (ShowStatus::Stopped, None, None, None, 0, None, None),
            State::Running(r) => (
                ShowStatus::Running,
                Some(r.current),
                Some(r.phase),
                self.armed_index(r),
                now_ms.saturating_sub(r.phase_start_ms),
                r.held_audio,
                r.pending_auto,
            ),
        };
        ShowSnapshot {
            status,
            current_index: current,
            phase,
            armed_index: armed,
            phase_ms,
            blackout: self.blackout,
            held_audio_index: held,
            pending_auto: pending,
        }
    }

    /// Which cue is "next": the operator's manual arm wins, then a scheduled
    /// auto-follow, else current+1 when the current cue has `auto_arm_next`.
    fn armed_index(&self, r: &Run) -> Option<usize> {
        if let Some(idx) = r.armed_override {
            return Some(idx);
        }
        match r.pending_auto {
            Some((idx, _)) => Some(idx),
            None => {
                let cue = &self.cues[r.current];
                if cue.auto_arm_next && r.current + 1 < self.cues.len() {
                    Some(r.current + 1)
                } else {
                    None
                }
            }
        }
    }

    fn flow(&self, index: usize) -> Option<&FlowCue> {
        self.cues.get(index)
    }

    // ------------------------------------------------------------------
    // Dispatch
    // ------------------------------------------------------------------

    pub fn dispatch(&mut self, cmd: ShowCommand, clock: &dyn Clock) -> Result<Vec<CueEvent>, ShowError> {
        let now = clock.now_ms();
        match cmd {
            ShowCommand::Tick => {
                self.tick(now);
                Ok(Vec::new())
            }
            ShowCommand::Go => self.go(now),
            ShowCommand::StopCurrent => self.stop_current(now),
            ShowCommand::FadeOutCurrent => self.fade_out_current(now),
            ShowCommand::TogglePause => self.toggle_pause(now),
            ShowCommand::RestartCue => self.restart_cue(now),
            ShowCommand::Panic => self.panic(now),
            ShowCommand::ToggleBlackout => {
                self.blackout = !self.blackout;
                Ok(vec![CueEvent::Blackout { active: self.blackout }])
            }
            ShowCommand::SetBlackout(active) => {
                let changed = self.blackout != active;
                self.blackout = active;
                Ok(if changed { vec![CueEvent::Blackout { active }] } else { Vec::new() })
            }
            ShowCommand::RecoverSkip => self.recover_skip(now),
            ShowCommand::ArmCue(index) => self.arm(index),
            ShowCommand::MarkAudioFinished(index) => self.mark_audio_finished(index, now),
            ShowCommand::MarkCueFailed(index) => self.mark_cue_failed(index, now),
        }
    }

    // ------------------------------------------------------------------
    // GO
    // ------------------------------------------------------------------

    /// GO: fire the armed cue, or advance from the current cue. Idempotent.
    fn go(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        // Snapshot running-run decisions before mutating: reading state and
        // dispatching need a clean borrow split.
        let running = if let State::Running(r) = &self.state {
            Some((r.started_at_ms, r.current, r.armed_override))
        } else {
            None
        };
        match (&self.state, running) {
            (State::Idle { armed }, _) => {
                let index = armed.ok_or(ShowError::NoCueArmed)?;
                self.start_cue(index, now, None)
            }
            (State::Stopped, _) => Err(ShowError::NoCueArmed),
            (_, Some((started_at, current, override_next))) => {
                // Double-press guard: a GO landing within the debounce window
                // of the last start is ignored outright.
                if now.saturating_sub(started_at) < DEBOUNCE_MS {
                    return Ok(Vec::new());
                }
                let next = match override_next {
                    Some(i) if i < self.cues.len() => i,
                    _ => current + 1,
                };
                if next >= self.cues.len() {
                    return Ok(Vec::new());
                }
                // Whether the outgoing cue's audio keeps sounding under the
                // next visual (stop behavior Hold).
                let keep_held = match self.flow(current).map(|c| c.stop_behavior) {
                    Some(StopBehavior::Hold) => self.flow(current).map(|c| c.has_audio).unwrap_or(false),
                    _ => false,
                };
                let mut events = self.finish_current(now);
                events.extend(self.start_cue(next, now, if keep_held { Some(current) } else { None })?);
                Ok(events)
            }
            (_, None) => unreachable!(),
        }
    }

    /// Complete the current run's audio/visual obligations for a transition.
    fn finish_current(&mut self, now: u64) -> Vec<CueEvent> {
        let mut events = Vec::new();
        if let State::Running(r) = &self.state {
            let (idx, _) = (r.current, r.pending_auto);
            if let Some(cue) = self.flow(idx).cloned() {
                if cue.has_audio {
                    match cue.stop_behavior {
                        StopBehavior::FadeOut => events.push(CueEvent::FadeOutAudio {
                            cue_index: idx,
                            fade_ms: cue.fade_out_ms,
                            fade_curve: cue.fade_out_curve,
                        }),
                        StopBehavior::StopImmediately => {
                            events.push(CueEvent::StopAudio { cue_index: Some(idx) })
                        }
                        StopBehavior::Hold => {
                            // Audio continues under the next cue; nothing to do.
                        }
                    }
                }
                if cue.duck_others {
                    events.push(CueEvent::DuckBus { bus: cue.bus.clone(), duck: false });
                }
                let _ = now;
            }
        }
        events
    }

    // ------------------------------------------------------------------
    // Starts
    // ------------------------------------------------------------------

    fn start_cue(&mut self, index: usize, now: u64, keep_held: Option<usize>) -> Result<Vec<CueEvent>, ShowError> {
        let cue = self.flow(index).ok_or(ShowError::InvalidCueIndex)?.clone();
        let mut events = Vec::new();

        // Hold-over audio only makes sense over an audio cue.
        let keep_audio = match (keep_held, cue.has_audio) {
            (Some(h), true) => Some(h),
            _ => None,
        };

        if cue.has_audio && !keep_audio.is_some() {
            events.push(CueEvent::SetVisual {
                cue_index: index,
                cue_id: cue.id.clone(),
                transition_ms: cue.visual_transition_ms,
            });
        } else if cue.has_audio {
            // Held audio + new audio: switch the visual, keep the old sound.
            events.push(CueEvent::KeepAudioChangeVisual {
                cue_index: index,
                cue_id: cue.id.clone(),
                transition_ms: cue.visual_transition_ms,
            });
        } else if cue.has_visual {
            events.push(CueEvent::SetVisual {
                cue_index: index,
                cue_id: cue.id.clone(),
                transition_ms: cue.visual_transition_ms,
            });
        }
        if cue.has_audio {
            events.push(CueEvent::StartAudio {
                cue_index: index,
                cue_id: cue.id.clone(),
                cue_number: cue.number.clone(),
                fade_ms: cue.fade_in_ms,
                fade_curve: cue.fade_in_curve,
            });
        }
        if cue.duck_others {
            events.push(CueEvent::DuckBus { bus: cue.bus.clone(), duck: true });
        }

        self.state = State::Running(Run {
            current: index,
            phase: if cue.has_audio && cue.fade_in_ms > 0 { Phase::Starting } else { Phase::Playing },
            phase_start_ms: now,
            fade_ms: cue.fade_in_ms,
            fade_curve: cue.fade_in_curve,
            held_audio: keep_audio,
            pending_auto: None,
            armed_override: None,
            started_at_ms: now,
        });

        // Arm the next cue immediately so GO stays uniform (fire armed cue).
        let next = if cue.auto_arm_next && index + 1 < self.cues.len() {
            Some(index + 1)
        } else {
            None
        };
        events.push(CueEvent::Armed {
            index: next,
            cue_id: next.and_then(|i| self.flow(i)).map(|c| c.id.clone()),
        });
        Ok(events)
    }

    // ------------------------------------------------------------------
    // Ticks & engine callbacks
    // ------------------------------------------------------------------

    /// Advance timed transitions.
    fn tick(&mut self, now: u64) {
        if !matches!(self.state, State::Running(Run { .. })) {
            return;
        }
        // 1) Pending auto-follow fire.
        let fire = match &self.state {
            State::Running(Run { pending_auto: Some((idx, at)), .. }) if now >= *at => Some(*idx),
            _ => None,
        };
        if let Some(idx) = fire {
            if let Ok(events) = self.start_cue_after_finish(idx, now) {
                let _ = events;
            }
            return;
        }
        // 2) Phase transitions (fade-in completion, fade-out completion).
        self.tick_fade(now);
    }

    fn start_cue_after_finish(&mut self, index: usize, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let keep_held = match self.flow(index.saturating_sub(1)).map(|c| c.stop_behavior) {
            Some(StopBehavior::Hold) if index >= 1 && self.flow(index - 1).map(|c| c.has_audio).unwrap_or(false) => {
                Some(index - 1)
            }
            _ => None,
        };
        if index >= self.cues.len() {
            return Err(ShowError::InvalidCueIndex);
        }
        let mut events = self.finish_current(now);
        events.extend(self.start_cue(index, now, keep_held)?);
        Ok(events)
    }

    fn tick_fade(&mut self, now: u64) -> Vec<CueEvent> {
        let Some(r) = (match &self.state {
            State::Running(r) => Some(r.clone()),
            _ => None,
        }) else {
            return Vec::new();
        };
        match r.phase {
            Phase::Starting if now.saturating_sub(r.phase_start_ms) >= r.fade_ms => {
                if let State::Running(run) = &mut self.state {
                    run.phase = Phase::Playing;
                    run.phase_start_ms = now;
                    run.fade_ms = 0;
                }
                Vec::new()
            }
            Phase::FadingOut if now.saturating_sub(r.phase_start_ms) >= r.fade_ms => {
                self.complete_fade_out(now)
            }
            Phase::HoldingVisual => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn complete_fade_out(&mut self, now: u64) -> Vec<CueEvent> {
        let (current, hold_visual, auto_arm) = match &self.state {
            State::Running(r) => match self.flow(r.current).cloned() {
                Some(c) => (r.current, c.hold_visual_after_audio, c.auto_arm_next),
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let mut events = Vec::new();
        if hold_visual {
            if let State::Running(run) = &mut self.state {
                run.phase = Phase::HoldingVisual;
                run.phase_start_ms = now;
                run.fade_ms = 0;
                run.pending_auto = None;
            }
        } else {
            events.push(CueEvent::ClearVisual);
            let next = auto_arm.then_some(current + 1).filter(|i| *i < self.cues.len());
            self.state = State::Idle { armed: next };
            events.push(CueEvent::Armed {
                index: next,
                cue_id: next.and_then(|i| self.flow(i)).map(|c| c.id.clone()),
            });
        }
        events
    }

    fn mark_audio_finished(&mut self, index: usize, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        // Held audio ended silently under the current cue.
        if let State::Running(r) = &self.state {
            if r.held_audio == Some(index) {
                if let State::Running(run) = &mut self.state {
                    run.held_audio = None;
                }
                return Ok(Vec::new());
            }
        }
        if index != self.current_index().unwrap_or(usize::MAX) {
            return Ok(Vec::new());
        }
        let cue = self.flow(index).cloned().ok_or(ShowError::InvalidCueIndex)?;
        if cue.has_visual && cue.hold_visual_after_audio {
            if let State::Running(run) = &mut self.state {
                run.phase = Phase::HoldingVisual;
                run.phase_start_ms = now;
                run.fade_ms = 0;
            }
            // Schedule the auto-follow from *now + delay*.
            if let Some(delay) = cue.auto_follow_delay_ms {
                if index + 1 < self.cues.len() {
                    if let State::Running(run) = &mut self.state {
                        run.pending_auto = Some((index + 1, now + delay));
                    }
                }
            }
            return Ok(Vec::new());
        }
        // Full finish: clear visual (unless holding) and idle.
        let next = cue.auto_arm_next.then_some(index + 1).filter(|i| *i < self.cues.len());
        let mut events = Vec::new();
        if cue.has_visual && !cue.hold_visual_after_audio {
            events.push(CueEvent::ClearVisual);
        }
        events.push(CueEvent::Armed {
            index: next,
            cue_id: next.and_then(|i| self.flow(i)).map(|c| c.id.clone()),
        });
        self.state = State::Idle { armed: next };
        Ok(events)
    }

    fn mark_cue_failed(&mut self, index: usize, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let Some(current) = self.current_index() else {
            return Ok(Vec::new());
        };
        if index != current {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        events.push(CueEvent::CueFailed { cue_index: index });
        events.push(CueEvent::StopAudio { cue_index: Some(index) });
        // Keep the visual if the failure was audio-only.
        if let State::Running(run) = &mut self.state {
            run.phase = Phase::Failed;
            run.phase_start_ms = now;
            run.pending_auto = None;
        }
        // Arm the next cue so RecoverSkip is instant.
        if current + 1 < self.cues.len() {
            events.push(CueEvent::Armed {
                index: Some(current + 1),
                cue_id: self.flow(current + 1).map(|c| c.id.clone()),
            });
        }
        Ok(events)
    }

    pub fn current_index(&self) -> Option<usize> {
        match &self.state {
            State::Running(r) => Some(r.current),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Transport
    // ------------------------------------------------------------------

    fn stop_current(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let Some(current) = self.current_index() else {
            return Ok(Vec::new());
        };
        let held = match &self.state {
            State::Running(r) => r.held_audio,
            _ => None,
        };
        let mut events = Vec::new();
        events.push(CueEvent::StopAudio { cue_index: Some(current) });
        if let Some(h) = held {
            events.push(CueEvent::StopAudio { cue_index: Some(h) });
        }
        // Re-arm the stopped cue so GO restarts it (live-friendly).
        self.state = State::Idle { armed: Some(current) };
        events.push(CueEvent::Armed {
            index: Some(current),
            cue_id: self.flow(current).map(|c| c.id.clone()),
        });
        let _ = now;
        Ok(events)
    }

    fn fade_out_current(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let Some(current) = self.current_index() else {
            return Ok(Vec::new());
        };
        let (phase, has_audio) = match &self.state {
            State::Running(r) => (r.phase, self.flow(r.current).map(|c| c.has_audio).unwrap_or(false)),
            _ => return Ok(Vec::new()),
        };
        if matches!(phase, Phase::FadingOut | Phase::Failed) {
            return Err(ShowError::Ignored);
        }
        let cue = self.flow(current).cloned().ok_or(ShowError::InvalidCueIndex)?;
        if has_audio {
            if let State::Running(run) = &mut self.state {
                run.phase = Phase::FadingOut;
                run.phase_start_ms = now;
                run.fade_ms = cue.fade_out_ms;
                run.fade_curve = cue.fade_out_curve;
                run.pending_auto = None;
            }
            return Ok(vec![CueEvent::FadeOutAudio {
                cue_index: current,
                fade_ms: cue.fade_out_ms,
                fade_curve: cue.fade_out_curve,
            }]);
        }
        // Visual-only cue: fade = hold visual then idle.
        let mut events = Vec::new();
        if cue.has_visual {
            if let State::Running(run) = &mut self.state {
                run.phase = Phase::HoldingVisual;
                run.phase_start_ms = now;
            }
        } else {
            self.state = State::Idle { armed: Some(current + 1).filter(|i| *i < self.cues.len()) };
            events.push(CueEvent::Armed {
                index: Some(current + 1).filter(|i| *i < self.cues.len()),
                cue_id: self.flow(current + 1).map(|c| c.id.clone()),
            });
        }
        Ok(events)
    }

    fn toggle_pause(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let Some(current) = self.current_index() else {
            return Ok(Vec::new());
        };
        let phase = match &self.state {
            State::Running(r) => r.phase,
            _ => return Ok(Vec::new()),
        };
        let mut events = Vec::new();
        match phase {
            Phase::Playing | Phase::Starting | Phase::FadingOut => {
                if let State::Running(run) = &mut self.state {
                    run.phase = Phase::Paused;
                    run.phase_start_ms = now;
                }
                events.push(CueEvent::PauseAudio { cue_index: current });
            }
            Phase::Paused => {
                if let State::Running(run) = &mut self.state {
                    run.phase = Phase::Playing;
                    run.phase_start_ms = now;
                }
                events.push(CueEvent::ResumeAudio { cue_index: current });
            }
            _ => return Err(ShowError::Ignored),
        }
        Ok(events)
    }

    fn restart_cue(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let Some(current) = self.current_index() else {
            return Err(ShowError::Ignored);
        };
        let mut events = Vec::new();
        events.push(CueEvent::StopAudio { cue_index: Some(current) });
        events.extend(self.start_cue(current, now, None)?);
        Ok(events)
    }

    fn panic(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        self.state = State::Stopped;
        self.blackout = false;
        Ok(vec![
            CueEvent::StopAudio { cue_index: None },
            CueEvent::ClearVisual,
            CueEvent::ShowStopped { duration_ms: now.saturating_sub(self.show_start_ms) },
        ])
    }

    fn recover_skip(&mut self, now: u64) -> Result<Vec<CueEvent>, ShowError> {
        let Some(current) = self.current_index() else {
            return Err(ShowError::Ignored);
        };
        let next = current + 1;
        if next >= self.cues.len() {
            return Err(ShowError::Ignored);
        }
        let mut events = self.finish_current(now);
        events.extend(self.start_cue(next, now, None)?);
        Ok(events)
    }

    fn arm(&mut self, index: usize) -> Result<Vec<CueEvent>, ShowError> {
        if index >= self.cues.len() {
            return Err(ShowError::InvalidCueIndex);
        }
        match &mut self.state {
            State::Running(run) => run.armed_override = Some(index),
            state @ (State::Idle { .. } | State::Stopped) => *state = State::Idle { armed: Some(index) },
        }
        Ok(vec![CueEvent::Armed {
            index: Some(index),
            cue_id: self.flow(index).map(|c| c.id.clone()),
        }])
    }
}

// ---------------------------------------------------------------------------
// Tests — deterministic, no sleeps, no audio hardware required.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: builds a flow cue with tuned behavior flags.
    fn fc(
        name: &str,
        has_audio: bool,
        has_visual: bool,
        stop: StopBehavior,
        auto_follow: Option<u64>,
        hold_visual: bool,
        fade_in: u64,
    ) -> FlowCue {
        FlowCue {
            id: format!("cue-{name}"),
            number: name.to_string(),
            name: name.to_string(),
            stop_behavior: stop,
            fade_in_ms: fade_in,
            fade_in_curve: FadeCurve::Linear,
            fade_out_ms: 1000,
            fade_out_curve: FadeCurve::EqualPower,
            hold_visual_after_audio: hold_visual,
            auto_arm_next: true,
            auto_follow_delay_ms: auto_follow,
            visual_transition_ms: 0,
            has_audio,
            has_visual,
            duck_others: false,
            bus: "Music".into(),
        }
    }

    /// Simulated millisecond clock that the tests advance manually.
    struct MockClock(u64);
    impl MockClock {
        fn new() -> Self {
            Self(1000)
        }
        fn advance(&mut self, ms: u64) {
            self.0 += ms;
        }
    }
    impl Clock for MockClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }

    fn audio_only(name: &str, stop: StopBehavior, hold_visual: bool) -> FlowCue {
        fc(name, true, false, stop, None, hold_visual, 0)
    }

    fn visual_only(name: &str) -> FlowCue {
        fc(name, false, true, StopBehavior::FadeOut, None, false, 0)
    }

    #[test]
    fn go_starts_first_cue_and_arms_second() {
        let mut m = ShowMachine::new(
            vec![audio_only("1", StopBehavior::Hold, true), audio_only("2", StopBehavior::FadeOut, true)],
            0,
        );
        let clock = MockClock::new();
        let ev = m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::StartAudio { cue_index: 0, .. })));
        assert!(ev.iter().any(|e| matches!(e, CueEvent::Armed { index: Some(1), .. })));
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.status, ShowStatus::Running);
    }

    #[test]
    fn double_go_press_is_idempotent() {
        let mut m = ShowMachine::new(
            vec![audio_only("1", StopBehavior::FadeOut, true), audio_only("2", StopBehavior::FadeOut, true)],
            0,
        );
        let mut clock = MockClock::new();
        let e1 = m.dispatch(ShowCommand::Go, &clock).unwrap();
        // Second press within the debounce window: ignored entirely.
        let e2 = m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert!(e2.is_empty());
        // Advance past debounce; GO advances to cue 2 exactly once.
        clock.advance(DEBOUNCE_MS + 1);
        let e3 = m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert!(e3.iter().any(|e| matches!(e, CueEvent::StartAudio { cue_index: 1, .. })));
        assert!(matches!(e3.first(), Some(CueEvent::FadeOutAudio { cue_index: 0, .. })));
        let _ = e1;
    }

    #[test]
    fn go_advances_to_next_cue_when_called_twice() {
        let mut m = ShowMachine::new(
            vec![audio_only("1", StopBehavior::FadeOut, true), audio_only("2", StopBehavior::FadeOut, true)],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(200);
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.current_index, Some(1));
    }

    #[test]
    fn stop_behavior_hold_keeps_audio_under_next_cue() {
        let mut m = ShowMachine::new(
            vec![
                audio_only("1", StopBehavior::Hold, true),
                audio_only("2", StopBehavior::FadeOut, true),
            ],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(150);
        let ev = m.dispatch(ShowCommand::Go, &clock).unwrap();
        // No Stop/FadeOut for cue 1, no extra Start for cue 1 audio.
        assert!(!ev.iter().any(|e| matches!(e, CueEvent::StopAudio { .. })));
        assert!(!ev.iter().any(|e| matches!(e, CueEvent::FadeOutAudio { .. })));
        // Visual switch must be an audio-keeping one (or a plain new start).
        assert!(
            ev.iter().any(|e| matches!(e, CueEvent::KeepAudioChangeVisual { .. })),
            "expected KeepAudioChangeVisual, got {ev:?}"
        );
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.held_audio_index, Some(0));
        assert_eq!(snap.current_index, Some(1));
    }

    #[test]
    fn fade_out_completes_into_holding_visual() {
        let mut m = ShowMachine::new(vec![audio_only("1", StopBehavior::FadeOut, true)], 0);
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        m.dispatch(ShowCommand::FadeOutCurrent, &clock).unwrap();
        clock.advance(999);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.phase, Some(Phase::FadingOut), "fade still running at 999 ms");
        clock.advance(1);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.phase, Some(Phase::HoldingVisual));
        clock.advance(2);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.phase, Some(Phase::HoldingVisual), "holding is stable");
    }

    #[test]
    fn fade_out_clears_visual_when_not_holding() {
        let mut m = ShowMachine::new(vec![audio_only("1", StopBehavior::FadeOut, false)], 0);
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        m.dispatch(ShowCommand::FadeOutCurrent, &clock).unwrap();
        clock.advance(1001);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.status, ShowStatus::Idle);
    }

    #[test]
    fn panic_stops_everything_and_locks() {
        let mut m = ShowMachine::new(
            vec![audio_only("1", StopBehavior::Hold, true), audio_only("2", StopBehavior::FadeOut, true)],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        let ev = m.dispatch(ShowCommand::Panic, &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::StopAudio { cue_index: None })));
        assert!(ev.iter().any(|e| matches!(e, CueEvent::ClearVisual)));
        assert_eq!(m.snapshot(clock.now_ms()).status, ShowStatus::Stopped);
        // After PANIC, GO is refused until the operator re-arms.
        let err = m.dispatch(ShowCommand::Go, &clock).unwrap_err();
        assert_eq!(err, ShowError::NoCueArmed);
        m.dispatch(ShowCommand::ArmCue(0), &clock).unwrap();
        let ev = m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::StartAudio { cue_index: 0, .. })));
    }

    #[test]
    fn stop_current_re_arms_same_cue() {
        let mut m = ShowMachine::new(vec![audio_only("1", StopBehavior::FadeOut, true)], 0);
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        let ev = m.dispatch(ShowCommand::StopCurrent, &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::StopAudio { cue_index: Some(0) })));
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.current_index, Some(0), "stopped cue is restageable");
    }

    #[test]
    fn audio_finished_holds_visual_then_arms_next() {
        let mut m = ShowMachine::new(
            vec![
                fc("1", true, true, StopBehavior::FadeOut, None, true, 0),
                fc("2", true, true, StopBehavior::FadeOut, Some(500), true, 0),
            ],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        // Cue 1 audio finishes naturally.
        m.dispatch(ShowCommand::MarkAudioFinished(0), &clock).unwrap();
        clock.advance(100);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.phase, Some(Phase::HoldingVisual));
        assert_eq!(snap.armed_index, Some(1), "next cue armed during hold");
        // GO manually during the hold advances (visual switch + new audio).
        clock.advance(100); // > debounce
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.current_index, Some(1));
    }

    #[test]
    fn manual_go_advances_and_finishes_into_idle() {
        let mut m = ShowMachine::new(
            vec![
                fc("1", true, true, StopBehavior::Hold, Some(500), true, 0),
                fc("2", true, true, StopBehavior::FadeOut, None, true, 0),
            ],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).current_index, Some(1));
        // Cue 2 finishes naturally → visual holds (hold_visual on cue 2).
        let _ = m.dispatch(ShowCommand::MarkAudioFinished(1), &clock).unwrap();
        clock.advance(50);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        let snap = m.snapshot(clock.now_ms());
        assert_eq!(snap.phase, Some(Phase::HoldingVisual));
        assert_eq!(snap.armed_index, None, "no further cue to arm");
    }

    #[test]
    fn auto_follow_fires_automatically_after_hold_finish() {
        let mut m = ShowMachine::new(
            vec![
                fc("1", true, true, StopBehavior::Hold, Some(500), true, 0),
                fc("2", true, true, StopBehavior::FadeOut, None, true, 0),
            ],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        m.dispatch(ShowCommand::MarkAudioFinished(0), &clock).unwrap();
        // Wait 499 ms: must NOT have fired.
        clock.advance(499);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).current_index, Some(0));
        // One more millisecond: must have fired.
        clock.advance(1);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).current_index, Some(1));
    }

    #[test]
    fn fade_in_phase_transitions_to_playing() {
        let mut m = ShowMachine::new(vec![fc("1", true, true, StopBehavior::FadeOut, None, true, 2000)], 0);
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).phase, Some(Phase::Starting));
        clock.advance(1000);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).phase, Some(Phase::Starting));
        clock.advance(1000);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).phase, Some(Phase::Playing));

        // While paused the clock still ticks but the phase must not advance.
        m.dispatch(ShowCommand::TogglePause, &clock).unwrap();
        clock.advance(10000);
        m.dispatch(ShowCommand::Tick, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).phase, Some(Phase::Paused));
    }

    #[test]
    fn blackout_toggles_set_and_unset() {
        let mut m = ShowMachine::new(vec![audio_only("1", StopBehavior::FadeOut, true)], 0);
        let clock = MockClock::new();
        let ev = m.dispatch(ShowCommand::SetBlackout(true), &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::Blackout { active: true })));
        let ev = m.dispatch(ShowCommand::SetBlackout(true), &clock).unwrap();
        assert!(ev.is_empty(), "idempotent blackout");
        m.dispatch(ShowCommand::SetBlackout(false), &clock).unwrap();
        assert!(!m.snapshot(clock.now_ms()).blackout);
    }

    #[test]
    fn pause_resumes() {
        let mut m = ShowMachine::new(vec![fc("1", true, true, StopBehavior::FadeOut, None, true, 2000)], 0);
        let clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        let ev = m.dispatch(ShowCommand::TogglePause, &clock).unwrap();
        assert!(matches!(ev.first(), Some(CueEvent::PauseAudio { .. })));
        let ev = m.dispatch(ShowCommand::TogglePause, &clock).unwrap();
        assert!(matches!(ev.first(), Some(CueEvent::ResumeAudio { .. })));
    }

    #[test]
    fn cue_failure_arms_next_and_recover_skip_advances() {
        let mut m = ShowMachine::new(
            vec![audio_only("1", StopBehavior::FadeOut, true), audio_only("2", StopBehavior::FadeOut, true)],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        let ev = m.dispatch(ShowCommand::MarkCueFailed(0), &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::CueFailed { .. })));
        assert_eq!(m.snapshot(clock.now_ms()).phase, Some(Phase::Failed));
        // Operator hits RecoverSkip → jumps to cue 2 without touching cue 1 again.
        let ev = m.dispatch(ShowCommand::RecoverSkip, &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::StartAudio { cue_index: 1, .. })));
        assert_eq!(m.snapshot(clock.now_ms()).current_index, Some(1));
    }

    #[test]
    fn manual_arm_overrides_sequential_next() {
        let mut m = ShowMachine::new(
            vec![
                audio_only("1", StopBehavior::FadeOut, true),
                audio_only("2", StopBehavior::FadeOut, true),
                audio_only("3", StopBehavior::FadeOut, true),
            ],
            0,
        );
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        m.dispatch(ShowCommand::ArmCue(2), &clock).unwrap();
        clock.advance(100);
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert_eq!(m.snapshot(clock.now_ms()).current_index, Some(2));
    }

    #[test]
    fn visual_visual_sequence_transitions_cleanly() {
        let mut m = ShowMachine::new(vec![visual_only("1"), visual_only("2")], 0);
        let mut clock = MockClock::new();
        m.dispatch(ShowCommand::Go, &clock).unwrap();
        clock.advance(100);
        let ev = m.dispatch(ShowCommand::Go, &clock).unwrap();
        assert!(ev.iter().any(|e| matches!(e, CueEvent::SetVisual { cue_index: 1, .. })));
    }

    #[test]
    fn empty_project_go_reports_no_cue_armed() {
        let mut m = ShowMachine::new(Vec::new(), 0);
        let clock = MockClock::new();
        let err = m.dispatch(ShowCommand::Go, &clock).unwrap_err();
        assert_eq!(err, ShowError::NoCueArmed);
    }
}
