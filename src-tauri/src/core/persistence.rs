//! Project persistence: versioned JSON format with atomic writes, backup
//! rotation, autosave and crash recovery.
//!
//! `.stagecue` files are UTF-8 JSON. Every save is atomic (write to a unique
//! temp file, fsync, then rename), so a crash mid-write can never corrupt the
//! previous good copy. Rotating backups live in `backups/` next to the file.

use crate::core::model::{Project, PROJECT_FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const PROJECT_EXTENSION: &str = "stagecue";
const RECOVERY_DIR: &str = "recovery";
const BACKUP_DIR: &str = "backups";
const BACKUP_KEEP: usize = 10;

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

/// File envelope: separates user data from metadata we never want the user to
/// edit, and gives migrations a stable place to live across versions.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    kind: String, // "stagecue.project"
    format_version: u32,
    #[serde(flatten)]
    project: Project,
}

impl ProjectFile {
    fn from(project: &Project) -> Self {
        let mut p = project.clone();
        p.format_version = PROJECT_FORMAT_VERSION;
        Self {
            kind: "stagecue.project".into(),
            format_version: PROJECT_FORMAT_VERSION,
            project: p,
        }
    }
}

/// Update modification timestamp + save the exact in-memory copy last.
pub fn touch(project: &mut Project) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    project.modified_at_ms = now;
}

/// Atomic save. Never leaves a truncated file on disk.
pub fn save(path: &Path, project: &Project) -> Result<(), PersistError> {
    let mut p = project.clone();
    touch(&mut p);
    let envelope = ProjectFile::from(&p);
    let json = serde_json::to_string_pretty(&envelope)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Unique temp file in the same directory → same filesystem → atomic rename.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = path.with_extension(format!("{PROJECT_EXTENSION}.tmp-{stamp}"));
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
    }
    // The previous good copy becomes a rotating backup.
    if path.exists() {
        spawn_backup(path)?;
    }
    fs::rename(&tmp_path, path)?;

    // Mirror for crash recovery in case the rename target is lost.
    write_recovery_snapshot(path, &json)?;
    Ok(())
}

fn write_recovery_snapshot(project_path: &Path, json: &str) -> Result<(), PersistError> {
    if let Some(recovery_dir) = recovery_dir_for(project_path) {
        fs::create_dir_all(&recovery_dir)?;
        let snap = recovery_dir.join("last-known.json");
        fs::write(snap, json)?;
    }
    Ok(())
}

fn recovery_dir_for(project_path: &Path) -> Option<PathBuf> {
    project_path.parent().map(|d| d.join(RECOVERY_DIR))
}

pub fn recovery_dir(project_path: &Path) -> Option<PathBuf> {
    recovery_dir_for(project_path)
}

fn backup_dir_for(path: &Path) -> Option<PathBuf> {
    path.parent().map(|d| d.join(BACKUP_DIR))
}

pub fn backup_dir(path: &Path) -> Option<PathBuf> {
    backup_dir_for(path)
}

fn spawn_backup(path: &Path) -> Result<(), PersistError> {
    let Some(dir) = backup_dir_for(path) else {
        return Ok(());
    };
    fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());
    fs::copy(path, dir.join(format!("{stem}.{stamp}.{PROJECT_EXTENSION}")))?;
    prune_backups(&dir, &stem)?;
    Ok(())
}

fn prune_backups(dir: &Path, stem: &str) -> Result<(), PersistError> {
    let mut entries: Vec<(PathBuf, u128)> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with(stem) && n.to_string_lossy().ends_with(PROJECT_EXTENSION))
                .unwrap_or(false)
        })
        .filter_map(|p| {
            // "stem.stamp.stagecue" → sort by stamp, newest first.
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .and_then(|n| n.split('.').nth(1).map(str::to_owned))
                .and_then(|s| s.parse::<u128>().ok())
                .map(|stamp| (p, stamp))
        })
        .collect();
    if entries.len() > BACKUP_KEEP {
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        for (path, _) in entries.iter().skip(BACKUP_KEEP) {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

pub fn list_backups(path: &Path) -> Vec<PathBuf> {
    backup_dir_for(path)
        .and_then(|d| fs::read_dir(d).ok())
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map(|e| e == PROJECT_EXTENSION).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default()
}

/// Restore from a specific backup file.
pub fn restore_backup(project_path: &Path, backup: &Path) -> Result<Project, PersistError> {
    load_from_file(backup).and_then(|mut p| {
        // Force a fresh save next time.
        p.modified_at_ms = 0;
        save(project_path, &p)?;
        Ok(p)
    })
}

/// Manual backup, returns path.
pub fn create_backup(path: &Path, project: &Project) -> Result<PathBuf, PersistError> {
    let dir = backup_dir_for(path).ok_or_else(|| PersistError::Other("project has no path".into()))?;
    fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());
    let dest = dir.join(format!("{stem}.{stamp}.manual.{PROJECT_EXTENSION}"));
    save(&dest, project)?;
    Ok(dest)
}

/// Read a file straight into a structured Project (used by tests & backups).
fn load_from_file(path: &Path) -> Result<Project, PersistError> {
    let raw = fs::read_to_string(path)?;
    load_from_string(&raw)
}

pub fn load_from_string(raw: &str) -> Result<Project, PersistError> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let version = value.get("formatVersion").and_then(|v| v.as_u64()).unwrap_or(0);
    // Migration chain (add future steps above the current step).
    let migrated = migrate(value, version)?;
    Project::migrate(migrated).map_err(PersistError::Other)
}

fn migrate(value: serde_json::Value, from: u64) -> Result<serde_json::Value, PersistError> {
    let current = u64::from(PROJECT_FORMAT_VERSION);
    if from > current {
        return Err(PersistError::Other(format!(
            "project format v{from} is newer than this app (v{current}) — please update StageCue"
        )));
    }
    if from == 0 {
        // Legacy bare-project file (raw Project JSON from tooling): accept as v1.
        return Ok(value);
    }
    Ok(value)
}

pub fn load(path: &Path) -> Result<Project, PersistError> {
    load_from_file(path)
}

/// Find recoverable sessions: recovery snapshots of recent projects that are
/// newer in mtime than their saved file.
pub struct RecoveryCandidate {
    pub project_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub snapshot_modified_ms: u64,
}

pub fn scan_recovery(project_path: &Path) -> Option<RecoveryCandidate> {
    let dir = recovery_dir(project_path)?;
    let snap = dir.join("last-known.json");
    let meta = fs::metadata(&snap).ok()?;
    let saved_meta = fs::metadata(project_path).ok();
    let snap_ms = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)?;
    // Recover only if the snapshot is at least as new as the saved file.
    if let Some(sm) = saved_meta {
        let saved_ms = sm
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        if let Some(saved) = saved_ms {
            if snap_ms <= saved {
                return None;
            }
        }
    }
    Some(RecoveryCandidate {
        project_path: project_path.to_path_buf(),
        snapshot_path: snap,
        snapshot_modified_ms: snap_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Cue, Project};

    fn temp_project_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("show.stagecue");
        (dir, path)
    }

    fn sample(name: &str) -> Project {
        let mut p = Project::new(name);
        let mut c1 = Cue::new("Q1", "Opening");
        let mut c2 = Cue::new("Q2", "Scene 2");
        c1.audio_asset_ids = vec!["a1".into()];
        c2.visual_asset_id = Some("v1".into());
        p.cues = vec![c1, c2];
        p
    }

    #[test]
    fn save_and_round_trip() {
        let (_dir, path) = temp_project_path();
        let p = sample("test");
        save(&path, &p).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.cues.len(), 2);
        assert_eq!(loaded.cues[0].number, "Q1");
        assert_eq!(loaded.cues[1].name, "Scene 2");
        assert!(loaded.assets.is_empty());
    }

    #[test]
    fn save_is_atomic_and_backups_rotate() {
        let (_dir, path) = temp_project_path();
        for round in 0..14 {
            let mut p = sample("rotate");
            p.name = format!("rotate-{round}");
            save(&path, &p).unwrap();
        }
        let backups = list_backups(&path);
        assert!(!backups.is_empty(), "backups kept");
        assert!(backups.len() <= 10, "rotation respects keep-count, got {}", backups.len());
        // No temp litter left behind.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn backup_restore_round_trip() {
        let (_dir, path) = temp_project_path();
        let p = sample("versioned");
        save(&path, &p).unwrap();
        let mut p2 = sample("versioned");
        p2.name = "renamed-later".to_string();
        p2.cues.clear();
        save(&path, &p2).unwrap();
        let backup = list_backups(&path).into_iter().next().unwrap();
        let restored = restore_backup(&path, &backup).unwrap();
        assert_eq!(restored.name, "versioned");
        assert_eq!(restored.cues.len(), 2);
        assert_eq!(load(&path).unwrap().cues.len(), 2);
    }

    #[test]
    fn invalid_project_reports_clear_error() {
        let (_dir, path) = temp_project_path();
        std::fs::write(&path, "{ not json }").unwrap();
        let err = load(&path).unwrap_err().to_string();
        assert!(!err.is_empty(), "error should be actionable");
    }

    #[test]
    fn legacy_bare_json_is_accepted() {
        let p = sample("legacy");
        let raw = serde_json::to_string_pretty(&p).unwrap();
        let loaded = load_from_string(&raw).unwrap();
        assert_eq!(loaded.cues.len(), 2);
    }

    #[test]
    fn future_version_is_rejected_with_guidance() {
        let raw = r#"{ "formatVersion": 99, "name": "x" }"#;
        let err = load_from_string(raw).unwrap_err().to_string();
        assert!(err.contains("newer"), "got: {err}");
    }
}

