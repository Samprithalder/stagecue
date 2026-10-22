//! Typed data model for StageCue projects.
//!
//! This module must stay free of Tauri/runtime dependencies so it can be
//! unit-tested in plain `cargo test` and reused by tooling later.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const PROJECT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    Audio,
    Image,
    Video,
    Slide,
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: String,
    pub kind: MediaKind,
    /// Absolute or project-relative path to the source file.
    pub path: PathBuf,
    pub display_name: String,
    /// Duration in seconds, if computed at import time.
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub loudness_db: Option<f32>,
    #[serde(default)]
    pub peak_db: Option<f32>,
}

impl MediaAsset {
    pub fn new(kind: MediaKind, path: PathBuf) -> Self {
        let display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unnamed asset".into());
        Self {
            id: Uuid::new_v4().to_string(),
            kind,
            path,
            display_name,
            duration_secs: None,
            loudness_db: None,
            peak_db: None,
        }
    }
}

/// How a cue is triggered relative to the previous cue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TriggerMode {
    /// Operator presses GO.
    Manual,
    /// Auto-start N ms after the previous cue finishes.
    #[serde(rename_all = "camelCase")]
    AutoFollow { delay_ms: u64 },
}

/// Transition applied on the stage output when this cue becomes current.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VisualTransition {
    Cut,
    /// Fade in from black.
    #[serde(rename_all = "camelCase")]
    Fade { duration_ms: u64 },
    /// Crossfade with previous visual.
    #[serde(rename_all = "camelCase")]
    Crossfade { duration_ms: u64 },
}

impl VisualTransition {
    pub fn duration_ms(&self) -> u64 {
        match self {
            VisualTransition::Cut => 0,
            VisualTransition::Fade { duration_ms } => *duration_ms,
            VisualTransition::Crossfade { duration_ms } => *duration_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FadeCurve {
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    EqualPower,
}

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CueColor {
    None,
    Amber,
    Blue,
    Green,
    Red,
    Purple,
}

/// What a cue's audio does when the next cue starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopBehavior {
    /// Keep playing (with current visual).
    Hold,
    FadeOut,
    StopImmediately,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioBank {
    Music,
    Effects,
    Ambience,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Cue {
    pub id: String,
    pub number: String,
    pub name: String,
    pub notes: String,
    pub color: CueColor,
    pub group_id: Option<String>,
    pub trigger_mode: TriggerMode,
    pub audio_asset_ids: Vec<String>,
    pub visual_asset_id: Option<String>,
    // Audio
    pub volume_db: f32,
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
    pub fade_in_curve: FadeCurve,
    pub fade_out_curve: FadeCurve,
    pub crossfade_ms: u64,
    pub loop_count: u32,
    pub start_offset_secs: f64,
    pub trim_start_secs: f64,
    pub trim_end_secs: Option<f64>,
    pub audio_bus: AudioBank,
    pub mute: bool,
    pub solo: bool,
    pub duck_others: bool,
    // Visual
    pub transition: VisualTransition,
    pub fit_mode: FitMode,
    pub hold_visual_after_audio: bool,
    pub auto_advance_ms: Option<u64>,
    // Flow
    pub auto_arm_next: bool,
    pub stop_behavior: StopBehavior,
}

impl Default for Cue {
    fn default() -> Self {
        Self::new("", "")
    }
}

impl Cue {
    pub fn new(number: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            number: number.into(),
            name: name.into(),
            notes: String::new(),
            color: CueColor::None,
            group_id: None,
            trigger_mode: TriggerMode::Manual,
            audio_asset_ids: Vec::new(),
            visual_asset_id: None,
            volume_db: 0.0,
            fade_in_ms: 0,
            fade_out_ms: 5000,
            fade_in_curve: FadeCurve::Linear,
            fade_out_curve: FadeCurve::EqualPower,
            crossfade_ms: 0,
            loop_count: 1,
            start_offset_secs: 0.0,
            trim_start_secs: 0.0,
            trim_end_secs: None,
            audio_bus: AudioBank::Music,
            mute: false,
            solo: false,
            duck_others: false,
            transition: VisualTransition::Cut,
            fit_mode: FitMode::Contain,
            hold_visual_after_audio: true,
            auto_advance_ms: None,
            auto_arm_next: true,
            stop_behavior: StopBehavior::FadeOut,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.audio_asset_ids.is_empty() && self.visual_asset_id.is_none()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    #[default]
    Contain,
    Cover,
    Stretch,
    Center,
}

/// Top-level show project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub cues: Vec<Cue>,
    pub assets: Vec<MediaAsset>,
    pub created_at_ms: u64,
    pub modified_at_ms: u64,
    pub autosave_interval_secs: u32,
    pub max_master_db: f32,
    pub safety_limiter: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self::new("")
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION,
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            cues: Vec::new(),
            assets: Vec::new(),
            created_at_ms: 0,
            modified_at_ms: 0,
            autosave_interval_secs: 30,
            max_master_db: 0.0,
            safety_limiter: true,
        }
    }

    pub fn asset(&self, id: &str) -> Option<&MediaAsset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn cue(&self, id: &str) -> Option<&Cue> {
        self.cues.iter().find(|c| c.id == id)
    }

    pub fn next_cue_number(&self) -> String {
        let mut n = self.cues.len() + 1;
        loop {
            let candidate = format!("Q{n}");
            if self.cues.iter().all(|c| c.number != candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Voices of missing media for every referenced asset.
    pub fn missing_assets(&self) -> Vec<&MediaAsset> {
        self.assets
            .iter()
            .filter(|a| !a.path.exists())
            .collect()
    }

    /// Migrate from any older supported format version to the current one.
    pub fn migrate(project: serde_json::Value) -> Result<Self, String> {
        let version = project
            .get("formatVersion")
            .and_then(|v| v.as_u64())
            .ok_or("project file has no format version")?;
        if version > u64::from(PROJECT_FORMAT_VERSION) {
            return Err(format!(
                "project was written by a newer StageCue (format v{version}) — please update the application"
            ));
        }
        // v1 -> v1: no-op. Add migration steps here as the format evolves.
        serde_json::from_value(project).map_err(|e| format!("invalid project file: {e}"))
    }
}

/// A file extension helper shared with the media scanner.
pub fn known_media_extension(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "wav" | "mp3" | "flac" | "ogg" | "oga" | "m4a" | "aac" | "opus" | "aiff" | "aif" | "wma" => MediaKind::Audio,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => MediaKind::Image,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" => MediaKind::Video,
        "pdf" => MediaKind::Slide,
        "txt" | "md" => MediaKind::Text,
        _ => return None,
    })
}
