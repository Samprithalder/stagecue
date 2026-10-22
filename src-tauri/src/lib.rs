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
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running StageCue");
}
