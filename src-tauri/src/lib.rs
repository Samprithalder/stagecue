pub mod audio {
    pub mod engine;
}
pub mod commands;
pub mod core {
    pub mod machine;
    pub mod media;
    pub mod model;
    pub mod ops;
    pub mod persistence;
}
pub mod show;

use commands::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Payload for the `stagecue://project` event (file opened from the OS /
/// second launch), so the console can refresh its whole project view.
#[derive(Clone, serde::Serialize)]
struct OpenedProject {
    project: crate::core::model::Project,
    summary: commands::ProjectSummary,
}

/// Picks the first forwarded file path (strip argv[0]-style executable path).
fn forwarded_file(argv: &[String]) -> Option<String> {
    argv.iter()
        .skip(1) // first entry is the binary path (both env::args and plugin argv)
        .find(|a| !a.trim().is_empty() && !a.starts_with('-'))
        .cloned()
}

/// Opening a file from the OS: a `.stagecue` project is loaded; a media file
/// (e.g. a PowerPoint deck) seeds a brand-new project with that asset.
fn open_stagecue_file(app: &tauri::AppHandle, path: String) {
    let result: Result<(), String> = (|| {
        let is_project = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("stagecue"))
            .unwrap_or(false);
        if is_project {
            commands::project_open(app.clone(), app.state::<AppState>(), path.clone())?;
        } else {
            if crate::core::model::known_media_extension(std::path::Path::new(&path)).is_none() {
                return Err(format!("StageCue does not recognize this file type: {path}"));
            }
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "New show".into());
            let _summary = commands::project_new(app.clone(), app.state::<AppState>(), name.clone())?;
            if let Err(err) = commands::asset_import(app.state::<AppState>(), vec![path.clone()]) {
                log::error!("importing {path} into a new project failed: {err}");
            } else {
                log::info!("created project \"{name}\" with imported asset {path}");
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            let state = app.state::<AppState>();
            let summary = commands::summary(&state);
            if let Some(project) = state.project.lock().unwrap().clone() {
                let _ = app.emit(
                    "stagecue://project",
                    OpenedProject {
                        project: (*project).clone(),
                        summary,
                    },
                );
            }
            log::info!("opened file from OS: {path}");
        }
        Err(err) => log::error!("opening file {path} failed: {err}"),
    }
}

pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: forwards files from double-click /
        // "Open with" launches into the already-running console instance.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = forwarded_file(&argv) {
                open_stagecue_file(app, path);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir { file_name: Some("stagecue".into()) },
                ))
                .build(),
        )
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::project_new,
            commands::project_open,
            commands::project_save,
            commands::project_save_as,
            commands::project_mark_dirty,
            commands::recent_projects,
            commands::recovery_candidate,
            commands::validate_show,
            commands::audio_output_devices,
            commands::audio_set_output,
            commands::audio_set_master_db,
            commands::audio_set_master_mute,
            commands::audio_set_max_master_db,
            commands::audio_set_voice_gain,
            commands::audio_set_voice_mute,
            commands::audio_set_voice_solo,
            commands::show_go,
            commands::show_stop_current,
            commands::show_fade_out,
            commands::show_toggle_pause,
            commands::show_restart_cue,
            commands::show_blackout,
            commands::show_panic,
            commands::show_recover_skip,
            commands::show_arm_cue,
            commands::show_snapshot,
            commands::show_log,
            commands::output_test_pattern,
            commands::cue_add,
            commands::cue_remove,
            commands::cue_duplicate,
            commands::cue_update,
            commands::cue_reorder,
            commands::asset_import,
            commands::asset_relink,
            commands::asset_kind_for,
        ])
        .setup(|app| {
            // Open the default audio output eagerly so failures surface at
            // startup instead of mid-show.
            let handle = app.handle().clone();
            let state: tauri::State<AppState> = app.state();
            let runner = Arc::new(show::ShowRunner::new(
                Arc::new(core::model::Project::new("Untitled show")),
                handle,
            ));
            *state.runner.lock().unwrap() = Some(runner.clone());
            match runner.audio.open_output(None) {
                Ok(name) => log::info!("audio output: {name}"),
                Err(e) => log::warn!("audio output unavailable: {e}"),
            }
            // Start the deterministic show tick loop (30 Hz).
            runner.spawn_tick_loop();
            // A file may have been handed to us on first launch (e.g.
            // double-click: `stagecue show.stagecue`). Handle it after the
            // console window exists so it can react to the event.
            if let Some(path) = forwarded_file(&std::env::args().collect::<Vec<String>>()) {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    open_stagecue_file(&handle, path);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running StageCue");
}
