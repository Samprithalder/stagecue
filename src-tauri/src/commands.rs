//! Typed IPC surface. The frontend only talks to these commands and the
//! `stagecue://` event channel — never to the engines directly.

use crate::core::machine::ShowCommand;
use crate::core::media::{self, ValidationIssue};
use crate::core::model::{Cue, MediaAsset, MediaKind, Project};
use crate::core::persistence::{self, PersistError};
use crate::show::{ShowLogEntry, ShowRunner};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub path: Option<String>,
    name: String,
    pub has_unsaved_changes: bool,
    pub cue_count: usize,
    pub asset_count: usize,
    pub missing_assets: usize,
}

pub struct AppState {
    pub project: std::sync::Mutex<Option<Arc<Project>>>,
    pub project_path: std::sync::Mutex<Option<PathBuf>>,
    pub dirty: std::sync::atomic::AtomicBool,
    pub runner: std::sync::Mutex<Option<Arc<ShowRunner>>>,
    pub recent: std::sync::Mutex<Vec<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            project: std::sync::Mutex::new(None),
            project_path: std::sync::Mutex::new(None),
            dirty: Default::default(),
            runner: std::sync::Mutex::new(None),
            recent: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn runner(&self) -> Result<Arc<ShowRunner>, String> {
        self.runner
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no show is loaded yet".into())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Project lifecycle
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn project_new(app: tauri::AppHandle, state: State<'_, AppState>, name: String) -> Result<ProjectSummary, String> {
    let project = Arc::new(Project::new(name));
    *state.project.lock().unwrap() = Some(project.clone());
    *state.project_path.lock().unwrap() = None;
    state.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    let runner = Arc::new(ShowRunner::new(project, app));
    *state.runner.lock().unwrap() = Some(runner.clone());
    Ok(summary(&state))
}

pub(crate) fn summary(state: &AppState) -> ProjectSummary {
    let project = state.project.lock().unwrap().clone().unwrap_or_default();
    ProjectSummary {
        path: state
            .project_path
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| p.display().to_string()),
        name: project.name.clone(),
        has_unsaved_changes: state.dirty.load(std::sync::atomic::Ordering::Relaxed),
        cue_count: project.cues.len(),
        asset_count: project.assets.len(),
        missing_assets: project.missing_assets().len(),
    }
}

#[tauri::command]
pub fn project_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Project, String> {
    let path_buf = PathBuf::from(&path);
    let project = persistence::load(&path_buf).map_err(|e: PersistError| e.to_string())?;
    let project = Arc::new(project);
    *state.project.lock().unwrap() = Some(project.clone());
    *state.project_path.lock().unwrap() = Some(path_buf.clone());
    state.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
    *state.recent.lock().unwrap() = push_recent(state.recent.lock().unwrap().clone(), path.clone());
    let runner = Arc::new(ShowRunner::new(project, app));
    *state.runner.lock().unwrap() = Some(runner.clone());
    Ok((**state.project.lock().unwrap().as_ref().unwrap()).clone())
}

fn push_recent(mut recent: Vec<String>, path: String) -> Vec<String> {
    recent.retain(|p| p != &path);
    recent.insert(0, path);
    recent.truncate(10);
    recent
}

#[tauri::command]
pub fn project_save(state: State<'_, AppState>) -> Result<ProjectSummary, String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project loaded")?;
    let path = state
        .project_path
        .lock()
        .unwrap()
        .clone()
        .ok_or("project has no file yet — use Save As")?;
    persistence::save(&path, &project).map_err(|e| e.to_string())?;
    state.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
    Ok(summary(&state))
}

#[tauri::command]
pub fn project_save_as(state: State<'_, AppState>, path: String) -> Result<ProjectSummary, String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project loaded")?;
    persistence::save(Path::new(&path), &project).map_err(|e| e.to_string())?;
    *state.project_path.lock().unwrap() = Some(PathBuf::from(&path));
    state.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
    Ok(summary(&state))
}

/// If a crash left a newer recovery snapshot, surface it to the user.
#[tauri::command]
pub fn recovery_candidate(path: String) -> Result<Option<String>, String> {
    let candidate = persistence::scan_recovery(Path::new(&path));
    Ok(candidate.map(|c| c.snapshot_path.display().to_string()))
}

/// Validate before show mode: exact cue + asset issues.
#[tauri::command]
pub fn validate_show(state: State<'_, AppState>) -> Result<Vec<ValidationIssue>, String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project loaded")?;
    Ok(media::validate_for_show(&project))
}

#[tauri::command]
pub fn project_mark_dirty(state: State<'_, AppState>) -> Result<bool, String> {
    state.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(true)
}

#[tauri::command]
pub fn recent_projects(state: State<'_, AppState>) -> Vec<String> {
    state.recent.lock().unwrap().clone()
}

// ---------------------------------------------------------------------------
// Audio devices
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn audio_output_devices(state: State<'_, AppState>) -> Result<Vec<(String, String)>, String> {
    let runner = state.runner()?;
    Ok(runner.audio.output_devices())
}

#[tauri::command]
pub fn audio_set_output(state: State<'_, AppState>, device_name: Option<String>) -> Result<String, String> {
    let runner = state.runner()?;
    // Refuse to swap devices mid-show unless nothing is playing.
    if !runner.audio.active_cue_ids().is_empty() {
        return Err("stop or fade out all audio before switching output device".into());
    }
    runner.audio.open_output(device_name).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Mix / transport
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn audio_set_master_db(state: State<'_, AppState>, db: f32) -> Result<f32, String> {
    let runner = state.runner()?;
    runner.audio.set_master_db(db);
    Ok(runner.audio.master_db())
}

#[tauri::command]
pub fn audio_set_master_mute(state: State<'_, AppState>, muted: bool) -> Result<bool, String> {
    let runner = state.runner()?;
    runner.audio.set_master_muted(muted);
    Ok(runner.audio.master_muted())
}

#[tauri::command]
pub fn audio_set_max_master_db(state: State<'_, AppState>, max_db: f32) -> Result<f32, String> {
    let runner = state.runner()?;
    runner.audio.set_max_master_db(max_db);
    Ok(runner.audio.master_db())
}

#[tauri::command]
pub fn audio_set_voice_gain(state: State<'_, AppState>, cue_id: String, db: f32) -> Result<(), String> {
    state.runner()?.audio.set_voice_gain(&cue_id, db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn audio_set_voice_mute(state: State<'_, AppState>, cue_id: String, muted: bool) -> Result<(), String> {
    state.runner()?.audio.set_voice_mute(&cue_id, muted).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn audio_set_voice_solo(state: State<'_, AppState>, cue_id: String, soloed: bool) -> Result<(), String> {
    state.runner()?.audio.set_voice_solo(&cue_id, soloed).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Show transport (performance mode)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn show_go(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::Go)?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_stop_current(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::StopCurrent)?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_fade_out(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::FadeOutCurrent)?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_toggle_pause(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::TogglePause)?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_restart_cue(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::RestartCue)?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_blackout(state: State<'_, AppState>, on: bool) -> Result<bool, String> {
    state.runner()?.dispatch(ShowCommand::SetBlackout(on))?;
    Ok(on)
}

#[tauri::command]
pub fn show_panic(state: State<'_, AppState>) -> Result<(), String> {
    state.runner()?.dispatch(ShowCommand::Panic)?;
    Ok(())
}

#[tauri::command]
pub fn show_recover_skip(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::RecoverSkip)?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_arm_cue(state: State<'_, AppState>, index: usize) -> Result<crate::core::machine::ShowSnapshot, String> {
    state.runner()?.dispatch(ShowCommand::ArmCue(index))?;
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_snapshot(state: State<'_, AppState>) -> Result<crate::core::machine::ShowSnapshot, String> {
    Ok(state.runner()?.snapshot())
}

#[tauri::command]
pub fn show_log(state: State<'_, AppState>) -> Result<Vec<ShowLogEntry>, String> {
    Ok(state.runner()?.log_entries())
}

#[tauri::command]
pub fn output_test_pattern(state: State<'_, AppState>, on: bool) -> Result<bool, String> {
    state.runner()?.set_test_pattern(on).map_err(|e| e.to_string())?;
    Ok(on)
}

// ---------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn cue_add(state: State<'_, AppState>, number: Option<String>, name: String) -> Result<Vec<Cue>, String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    let cue = Cue::new(
        number.unwrap_or_else(|| p.next_cue_number()),
        name,
    );
    p.cues.push(cue);
    commit(&state, p)?;
    let updated = state.project.lock().unwrap().clone().unwrap();
    Ok(updated.cues.clone())
}

#[tauri::command]
pub fn cue_remove(state: State<'_, AppState>, cue_id: String) -> Result<(), String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    p.cues.retain(|c| c.id != cue_id);
    commit(&state, p)?;
    Ok(())
}

#[tauri::command]
pub fn cue_duplicate(state: State<'_, AppState>, cue_id: String) -> Result<Vec<Cue>, String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    p.cues = crate::core::ops::duplicate_cue(&p, &cue_id);
    commit(&state, p)?;
    let updated = state.project.lock().unwrap().clone().unwrap();
    Ok(updated.cues.clone())
}

#[tauri::command]
pub fn cue_update(state: State<'_, AppState>, cue: Cue) -> Result<(), String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    if let Some(existing) = p.cues.iter_mut().find(|c| c.id == cue.id) {
        *existing = cue;
    } else {
        return Err("cue not found".into());
    }
    commit(&state, p)?;
    Ok(())
}

#[tauri::command]
pub fn cue_reorder(state: State<'_, AppState>, from: usize, to: usize) -> Result<(), String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    crate::core::ops::move_cue(&mut p, from, to)?;
    commit(&state, p)?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: Vec<MediaAsset>,
    pub rejected: Vec<String>,
}

#[tauri::command]
pub fn asset_import(state: State<'_, AppState>, paths: Vec<String>) -> Result<ImportResult, String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    let pathbufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let outcome = media::import_files_lenient(&mut p, &pathbufs);
    if !outcome.imported.is_empty() {
        commit(&state, p)?;
    }
    Ok(ImportResult {
        imported: outcome.imported,
        rejected: outcome.rejected,
    })
}

#[tauri::command]
pub fn asset_relink(state: State<'_, AppState>, asset_id: String, new_path: String) -> Result<(), String> {
    let project = ensure_project(&state)?;
    let mut p = (*project).clone();
    media::relink_asset(&mut p, &asset_id, PathBuf::from(new_path)).map_err(|e| e.to_string())?;
    commit(&state, p)?;
    Ok(())
}

#[tauri::command]
pub fn asset_kind_for(state: State<'_, AppState>, path: String) -> Result<Option<MediaKind>, String> {
    let _ = state;
    Ok(crate::core::model::known_media_extension(Path::new(&path)))
}

/// Commands that mutate the project require a loaded project. The console
/// always creates one at startup ("Untitled show") — if state was somehow
/// uninitialized, fall back to the runner's project instead of failing the
/// operation with "no project".
fn ensure_project(state: &State<'_, AppState>) -> Result<Arc<Project>, String> {
    if let Some(p) = state.project.lock().unwrap().clone() {
        return Ok(p);
    }
    let runner = state
        .runner
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "no show handler is running".to_string())?;
    let project = runner.project();
    *state.project.lock().unwrap() = Some(project.clone());
    Ok(project)
}

fn commit(state: &State<AppState>, project: Project) -> Result<(), String> {
    // Sync machine to edited cue list without touching playback.
    if let Some(runner) = state.runner.lock().unwrap().as_ref() {
        runner.replace_project(Arc::new(project.clone()));
    }
    *state.project.lock().unwrap() = Some(Arc::new(project));
    state.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
