/** Typed IPC client — the frontend only exchanges strongly shaped data with the Rust core. */
import { invoke } from "@tauri-apps/api/core";
import type { Project, Cue, MediaAsset, MediaKind } from "./types";

export type ShowSnapshotDTO = {
  status: "idle" | "running" | "stopped";
  currentIndex: number | null;
  phase: "starting" | "playing" | "paused" | "fadingOut" | "holdingVisual" | "failed" | null;
  armedIndex: number | null;
  phaseMs: number;
  blackout: boolean;
  heldAudioIndex: number | null;
  pendingAuto: [number, number] | null;
  poses: [string, number | null][];
  cueCount: number;
  currentCue: CueDTO | null;
  armedCue: CueDTO | null;
};

export type CueDTO = {
  id: string;
  number: string;
  name: string;
  notes: string;
  color: string;
  hasAudio: boolean;
  hasVisual: boolean;
  stopBehavior: string;
  triggerMode: { type: string; delayMs?: number };
  fadeOutMs: number;
  audioAssetId: string | null;
  visualAssetId: string | null;
};

export type StageVisualDTO = {
  cueId: string;
  cueNumber: string;
  kind: MediaKind;
  assetPath: string | null;
  fitMode: string;
  transitionMs: number;
  blackout: boolean;
  testPattern: boolean;
};

export type ValidationIssueDTO = {
  cueId: string;
  cueNumber: string;
  cueName: string;
  assetId: string;
  assetPath: string;
  kind: MediaKind;
  severity: "missing" | "unreadable";
};

export type ProjectSummaryDTO = {
  path: string | null;
  name: string;
  hasUnsavedChanges: boolean;
  cueCount: number;
  assetCount: number;
  missingAssets: number;
};

function unwrap<T>(p: Promise<T>): Promise<T> {
  return p;
}

export const ipc = {
  projectNew: (name: string): Promise<ProjectSummaryDTO> =>
    invoke<ProjectSummaryDTO>("project_new", { name }),
  projectOpen: (path: string): Promise<Project> => invoke<Project>("project_open", { path }),
  projectSave: (): Promise<ProjectSummaryDTO> => unwrap(invoke<ProjectSummaryDTO>("project_save")),
  projectSaveAs: (path: string): Promise<ProjectSummaryDTO> =>
    invoke<ProjectSummaryDTO>("project_save_as", { path }),
  recentProjects: (): Promise<string[]> => invoke<string[]>("recent_projects"),
  recoveryCandidate: (path: string): Promise<string | null> =>
    invoke<string | null>("recovery_candidate", { path }),
  validateShow: (): Promise<ValidationIssueDTO[]> => invoke<ValidationIssueDTO[]>("validate_show"),
  audioOutputDevices: (): Promise<[string, string][]> =>
    invoke<[string, string][]>("audio_output_devices"),
  audioSetOutput: (deviceName: string | null): Promise<string> =>
    invoke<string>("audio_set_output", { deviceName }),
  audioSetMasterDb: (db: number): Promise<number> => invoke<number>("audio_set_master_db", { db }),
  audioSetMasterMute: (muted: boolean): Promise<boolean> =>
    invoke<boolean>("audio_set_master_mute", { muted }),
  audioSetMaxMasterDb: (maxDb: number): Promise<number> =>
    invoke<number>("audio_set_max_master_db", { maxDb }),
  showGo: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_go"),
  showStopCurrent: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_stop_current"),
  showFadeOut: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_fade_out"),
  showTogglePause: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_toggle_pause"),
  showRestartCue: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_restart_cue"),
  showBlackout: (on: boolean): Promise<boolean> => invoke<boolean>("show_blackout", { on }),
  showPanic: (): Promise<void> => invoke<void>("show_panic"),
  showRecoverSkip: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_recover_skip"),
  showArmCue: (index: number): Promise<ShowSnapshotDTO> =>
    invoke<ShowSnapshotDTO>("show_arm_cue", { index }),
  showSnapshot: (): Promise<ShowSnapshotDTO> => invoke<ShowSnapshotDTO>("show_snapshot"),
  showLog: (): Promise<{ atMs: number; level: string; message: string }[]> =>
    invoke<{ atMs: number; level: string; message: string }[]>("show_log"),
  outputTestPattern: (on: boolean): Promise<boolean> =>
    invoke<boolean>("output_test_pattern", { on }),
  cueAdd: (number: string | null, name: string): Promise<Cue[]> =>
    invoke<Cue[]>("cue_add", { number, name }),
  cueRemove: (cueId: string): Promise<void> => invoke<void>("cue_remove", { cueId }),
  cueDuplicate: (cueId: string): Promise<Cue[]> => invoke<Cue[]>("cue_duplicate", { cueId }),
  cueUpdate: (cue: Cue): Promise<void> => invoke<void>("cue_update", { cue }),
  cueReorder: (from: number, to: number): Promise<void> =>
    invoke<void>("cue_reorder", { from, to }),
  assetImport: (paths: string[]): Promise<MediaAsset[]> =>
    invoke<MediaAsset[]>("asset_import", { paths }),
  assetRelink: (assetId: string, newPath: string): Promise<void> =>
    invoke<void>("asset_relink", { assetId, newPath }),
};
