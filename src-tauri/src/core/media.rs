//! Media catalog: import scanning, kind detection, missing-asset validation
//! and relinking. Deliberately engine-agnostic — the audio/video adapters
//! enrich assets with duration/loudness after import.

use crate::core::model::{known_media_extension, Cue, MediaAsset, MediaKind, Project};
use serde::Serialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("'{0}' is not a recognized media file (supported: wav, mp3, flac, ogg, m4a, aac, png, jpg, webp, mp4, mov, mkv, webm, pdf)")]
    Unsupported(String),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("file is unreadable: {0}")]
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub cue_id: String,
    pub cue_number: String,
    pub cue_name: String,
    pub asset_id: String,
    pub asset_path: String,
    pub kind: MediaKind,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// File does not exist on disk.
    Missing,
    /// Exists but unreadable/zero bytes.
    Unreadable,
}

/// Validate every cue-referenced asset before a show. Returns issues per cue,
/// so the UI can name the exact cue that would fail.
pub fn validate_for_show(project: &Project) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for cue in &project.cues {
        let mut asset_ids: Vec<&str> = cue.audio_asset_ids.iter().map(String::as_str).collect();
        if let Some(v) = &cue.visual_asset_id {
            asset_ids.push(v);
        }
        for id in asset_ids {
            let Some(asset) = project.asset(id) else { continue };
            if !asset.path.exists() {
                issues.push(issue(cue, asset, Severity::Missing));
            } else if asset.path.is_file() {
                match std::fs::metadata(&asset.path) {
                    Ok(meta) if meta.len() == 0 => issues.push(issue(cue, asset, Severity::Unreadable)),
                    Err(_) => issues.push(issue(cue, asset, Severity::Unreadable)),
                    Ok(_) => {}
                }
            } else {
                issues.push(issue(cue, asset, Severity::Missing));
            }
        }
    }
    issues
}

fn issue(cue: &Cue, asset: &MediaAsset, severity: Severity) -> ValidationIssue {
    ValidationIssue {
        cue_id: cue.id.clone(),
        cue_number: cue.number.clone(),
        cue_name: cue.name.clone(),
        asset_id: asset.id.clone(),
        asset_path: asset.path.display().to_string(),
        kind: asset.kind,
        severity,
    }
}

/// Import one or more files into the catalog.
pub fn import_files(project: &mut Project, paths: &[PathBuf]) -> Result<Vec<MediaAsset>, MediaError> {
    let mut imported = Vec::new();
    for p in paths {
        if !p.exists() {
            return Err(MediaError::NotFound(p.display().to_string()));
        }
        let Some(kind) = known_media_extension(p) else {
            return Err(MediaError::Unsupported(
                p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            ));
        };
        if let Ok(meta) = std::fs::metadata(p) {
            if meta.len() == 0 {
                return Err(MediaError::Unreadable(p.display().to_string()));
            }
        }
        let mut asset = MediaAsset::new(kind, normalize_path(p));
        if let Some(dur) = probe_duration(p, kind) {
            asset.duration_secs = Some(dur);
        }
        imported.push(asset);
    }
    project.assets.extend(imported.iter().cloned());
    Ok(imported)
}

/// Audio formats probeable natively today; duration is advisory metadata.
fn probe_duration(path: &Path, kind: MediaKind) -> Option<f64> {
    // Lightweight probe: file size + rough bitrate heuristics are unreliable,
    // so only claim duration we can actually compute cheaply. The frontend
    // timeline receives real duration once the audio engine measures the
    // decoder on first load.
    match kind {
        MediaKind::Audio => audio_duration_fallback(path),
        _ => None,
    }
}

/// Parse a WAV header's byte rate for a real duration; other formats surface
/// duration after engine load (documented in docs/limitations.md).
fn audio_duration_fallback(path: &Path) -> Option<f64> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 12];
    f.read_exact(&mut header).ok()?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return None;
    }
    let mut byte_rate = 0u32;
    let mut data_size = 0u32;
    while let Ok(chunk) = read_chunk(&mut f) {
        match chunk.id.as_str() {
            "fmt " => {
                let mut fmt_data = vec![0u8; chunk.size as usize];
                f.read_exact(&mut fmt_data).ok()?;
                if chunk.size >= 16 {
                    byte_rate = u32::from_le_bytes([fmt_data[8], fmt_data[9], fmt_data[10], fmt_data[11]]);
                }
            }
            "data" => {
                data_size = chunk.size;
                break;
            }
            _ => {
                std::io::copy(
                    &mut f.by_ref().take(chunk.size as u64),
                    &mut std::io::sink(),
                )
                .ok()?;
            }
        }
    }
    if byte_rate > 0 && data_size > 0 {
        Some(data_size as f64 / byte_rate as f64)
    } else {
        None
    }
}

struct Chunk {
    id: String,
    size: u32,
}

fn read_chunk(f: &mut std::fs::File) -> Result<Chunk, std::io::Error> {
    use std::io::Read;
    let mut idb = [0u8; 4];
    let mut sz = [0u8; 4];
    f.read_exact(&mut idb)?;
    f.read_exact(&mut sz)?;
    Ok(Chunk {
        id: String::from_utf8_lossy(&idb).into_owned(),
        size: u32::from_le_bytes(sz),
    })
}

/// Normalize to absolute, canonicalized path so a moved app (portable
/// installs) can still find assets. Relinking rewrites these paths.
pub fn normalize_path(p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Relink an asset to a new file. Optionally updates every asset whose name
/// matches the old display name within the same relink batch (folder moves).
pub fn relink_asset(project: &mut Project, asset_id: &str, new_path: PathBuf) -> Result<(), MediaError> {
    let Some(kind) = known_media_extension(&new_path) else {
        return Err(MediaError::Unsupported(new_path.display().to_string()));
    };
    let old_name = project
        .asset(asset_id)
        .map(|a| a.display_name.clone())
        .ok_or_else(|| MediaError::NotFound(asset_id.to_string()))?;
    let idx = project
        .assets
        .iter()
        .position(|a| a.id == asset_id)
        .ok_or_else(|| MediaError::NotFound(asset_id.to_string()))?;
    {
        let asset = &mut project.assets[idx];
        asset.path = normalize_path(&new_path);
        asset.kind = kind;
        asset.duration_secs = probe_duration(&asset.path, asset.kind);
        if new_path.exists() {
            asset.display_name = new_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| asset.display_name.clone());
        }
    }
    // Same-name relinks from a folder move: update matching siblings.
    let new_path_abs = project.assets[idx].path.clone();
    for other in project.assets.iter_mut() {
        if other.id != asset_id && old_name == other.display_name && !other.path.exists() {
            other.path = new_path_abs.clone();
        }
    }
    Ok(())
}

/// Relink all missing assets currently rooted under `old_root` to `new_root`.
pub fn relink_root(project: &mut Project, old_root: &Path, new_root: &Path) -> usize {
    let old_str = old_root.to_string_lossy().to_string();
    let mut count = 0;
    for asset in project.assets.iter_mut() {
        if let Some(rest) = asset.path.to_string_lossy().strip_prefix(old_str.as_str()) {
            let candidate = new_root.join(rest.trim_start_matches(['/', '\\']));
            if candidate.exists() {
                asset.path = candidate;
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Project;

    fn project_with_missing_audio() -> Project {
        let mut p = Project::new("t");
        let mut cue = Cue::new("Q1", "Uses missing file");
        p.assets.push(MediaAsset::new(MediaKind::Audio, PathBuf::from("/does/not/exist/never.wav")));
        cue.audio_asset_ids = vec![p.assets[0].id.clone()];
        p.cues.push(cue);
        p
    }

    #[test]
    fn missing_asset_is_flagged_with_cue_identity() {
        let p = project_with_missing_audio();
        let issues = validate_for_show(&p);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].cue_number, "Q1");
        assert_eq!(issues[0].severity, Severity::Missing);
        assert!(issues[0].asset_path.contains("never.wav"));
    }

    #[test]
    fn zero_byte_file_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("empty.wav");
        std::fs::write(&f, b"").unwrap();
        let mut p = Project::new("t");
        p.assets.push(MediaAsset::new(MediaKind::Audio, f.clone()));
        let mut cue = Cue::new("Q1", "x");
        cue.audio_asset_ids = vec![p.assets[0].id.clone()];
        p.cues.push(cue);
        let issues = validate_for_show(&p);
        assert_eq!(issues[0].severity, Severity::Unreadable);
    }

    #[test]
    fn unsupported_extension_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("docs.zip");
        std::fs::write(&f, b"zip").unwrap();
        let mut p = Project::new("t");
        let err = import_files(&mut p, &[f]).unwrap_err();
        assert!(matches!(err, MediaError::Unsupported(_)));
    }

    #[test]
    fn wav_duration_parsed_from_header() {
        // 1 second at 48000 Hz / stereo / 16-bit ≈ 192000 data bytes.
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("clip.wav");
        let mut file: Vec<u8> = Vec::new();
        let data_len: u32 = 192000;
        let fmt_len: u32 = 16;
        let total = 4 + 8 + fmt_len as u32 + 8 + data_len;
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(total - 8).to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(b"fmt ");
        file.extend_from_slice(&fmt_len.to_le_bytes());
        file.extend_from_slice(&[1, 0, 2, 0, 0x80, 0xBB, 0, 0]); // pcm stereo 48000
        file.extend_from_slice(&192000_u32.to_le_bytes()); // byteRate
        file.extend_from_slice(&[4, 0, 16, 0]); // block align, bits
        file.extend_from_slice(b"data");
        file.extend_from_slice(&data_len.to_le_bytes());
        file.extend_from_slice(&vec![0u8; data_len as usize]);
        std::fs::write(&f, file).unwrap();
        let mut p = Project::new("t");
        let assets = import_files(&mut p, &[f]).unwrap();
        assert_eq!(assets[0].kind, MediaKind::Audio);
        let dur = assets[0].duration_secs.expect("wav duration from header");
        assert!((dur - 1.0).abs() < 0.05, "got {dur}");
    }

    #[test]
    fn relink_replaces_asset_and_updates_display_name() {
        let dir = tempfile::tempdir().unwrap();
        let alt = dir.path().join("other");
        std::fs::create_dir_all(&alt).unwrap();
        let real = alt.join("scored.wav");
        std::fs::write(&real, b"RIFF....WAVE").unwrap();

        let mut p = project_with_missing_audio();
        let asset_id = p.assets[0].id.clone();
        relink_asset(&mut p, &asset_id, real).unwrap();
        assert!(p.asset(&asset_id).unwrap().path.exists());
        assert_eq!(p.asset(&asset_id).unwrap().display_name, "scored.wav");
    }

    #[test]
    fn missing_gone_assets_for_show_cover_visuals_too() {
        let mut p = Project::new("t");
        p.assets.push(MediaAsset::new(MediaKind::Image, PathBuf::from("/gone/backdrop.png")));
        let mut cue = Cue::new("Q2", "Backdrop cue");
        cue.visual_asset_id = Some(p.assets[0].id.clone());
        p.cues.push(cue);
        let issues = validate_for_show(&p);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].asset_path.contains("backdrop.png"));
    }
}
