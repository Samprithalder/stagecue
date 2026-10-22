/** Global show state, driven exclusively by Rust events and typed IPC. */
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { ipc, type ProjectSummaryDTO } from "../ipc";
import type { Project } from "../types";
import type { ShowSnapshotDTO, StageVisualDTO, ValidationIssueDTO } from "../ipc";

export type VoiceState = { cueId: string; gainDb: number; muted: boolean; soloed: boolean };

export type StatusMessage = { level: "info" | "warn" | "error"; text: string; at: number };

interface ShowStore {
  summary: ProjectSummaryDTO | null;
  project: Project | null;
  snapshot: ShowSnapshotDTO | null;
  selectedCueId: string | null;
  voices: VoiceState[];
  masterDb: number;
  masterMuted: boolean;
  performanceMode: boolean;
  cueListLocked: boolean;
  deviceLost: boolean;
  stageVisual: StageVisualDTO | null;
  issues: ValidationIssueDTO[];
  status: StatusMessage | null;

  init(): Promise<void>;
  setProject(p: Project, summary: ProjectSummaryDTO): void;
  setSnapshot(s: ShowSnapshotDTO): void;
  selectCue(id: string | null): void;
  notify(level: StatusMessage["level"], text: string): void;
  setMaster(db: number): Promise<void>;
  toggleMasterMute(): Promise<void>;
  setPerformanceMode(on: boolean): void;
  setCueListLocked(v: boolean): void;
  setIssues(issues: ValidationIssueDTO[]): Promise<void>;
}

export const useShow = create<ShowStore>((set, get) => ({
  summary: null,
  project: null,
  snapshot: null,
  selectedCueId: null,
  voices: [],
  masterDb: 0,
  masterMuted: false,
  performanceMode: false,
  cueListLocked: false,
  deviceLost: false,
  stageVisual: null,
  issues: [],
  status: null,

  async init() {
    await Promise.all([
      listen<ProjectSummaryDTO>("stagecue://summary", (e) =>
        set({ summary: e.payload, issues: [] }),
      ),
      listen<ShowSnapshotDTO>("stagecue://show", (e) => set({ snapshot: e.payload })),
      listen<StageVisualDTO>("stagecue://visual", (e) => set({ stageVisual: e.payload })),
      listen<VoiceState[]>("stagecue://voices", (e) => set({ voices: e.payload })),
      listen<{ masterDb: number; muted: boolean }>("stagecue://master", (e) =>
        set({ masterDb: e.payload.masterDb, masterMuted: e.payload.muted }),
      ),
      listen<{ deviceLost?: boolean }>("stagecue://device", (e) =>
        set({ deviceLost: !!e.payload?.deviceLost }),
      ),
    ]);
    const snap = await ipc.showSnapshot().catch(() => null);
    if (snap) set({ snapshot: snap });
    const recent = await ipc.recentProjects().catch(() => []);
    set({
      summary: {
        path: recent[0] ?? null,
        name: "Untitled show",
        hasUnsavedChanges: false,
        cueCount: 0,
        assetCount: 0,
        missingAssets: 0,
      },
    });
  },

  setProject(project, summary) {
    set({ project, summary, selectedCueId: project.cues[0]?.id ?? null });
  },

  setSnapshot(s) {
    set({ snapshot: s });
  },

  selectCue(id) {
    set({ selectedCueId: id });
  },

  notify(level, text) {
    set({ status: { level, text, at: Date.now() } });
  },

  async setMaster(db) {
    const applied = await ipc.audioSetMasterDb(db).catch(() => null);
    if (applied !== null) set({ masterDb: applied });
  },

  async toggleMasterMute() {
    const muted = await ipc.audioSetMasterMute(!get().masterMuted).catch(() => null);
    if (muted !== null) set({ masterMuted: muted });
  },

  setPerformanceMode(on) {
    set({ performanceMode: on });
  },

  setCueListLocked(v) {
    set({ cueListLocked: v });
  },

  async setIssues(issues) {
    set({ issues });
  },
}));

/** Derived helpers keep components dumb and readable. */
export const currentTimeDisplay = (snapshot: ShowSnapshotDTO | null): string => {
  if (!snapshot?.currentCue) return "--:--";
  const secs = Math.floor(snapshot.phaseMs / 1000);
  return `${String(Math.floor(secs / 60)).padStart(2, "0")}:${String(secs % 60).padStart(2, "0")}`;
};

/* Ensure the store module initializes listeners once, application-wide. */
let initialized = false;
export async function ensureStoreInit(): Promise<void> {
  if (initialized) return;
  initialized = true;
  await useShow.getState().init();
}
