/** Shared data shapes mirroring the Rust core model (camelCase serde). */

export type MediaKind = "audio" | "image" | "video" | "slide" | "text";

export type MediaAsset = {
  id: string;
  kind: MediaKind;
  path: string;
  displayName: string;
  durationSecs: number | null;
  loudnessDb: number | null;
  peakDb: number | null;
};

export type TriggerMode = { type: "manual" } | { type: "autoFollow"; delayMs: number };

export type VisualTransition =
  | { type: "cut" }
  | { type: "fade"; durationMs: number }
  | { type: "crossfade"; durationMs: number };

export type FadeCurve = "linear" | "exponential" | "logarithmic" | "sCurve" | "equalPower";
export type StopBehavior = "hold" | "fadeOut" | "stopImmediately";
export type AudioBank = "music" | "effects" | "ambience" | "direct";
export type CueColor = "none" | "amber" | "blue" | "green" | "red" | "purple";
export type FitMode = "contain" | "cover" | "stretch" | "center";

export type Cue = {
  id: string;
  number: string;
  name: string;
  notes: string;
  color: CueColor;
  groupId: string | null;
  triggerMode: TriggerMode;
  audioAssetIds: string[];
  visualAssetId: string | null;
  volumeDb: number;
  fadeInMs: number;
  fadeOutMs: number;
  fadeInCurve: FadeCurve;
  fadeOutCurve: FadeCurve;
  crossfadeMs: number;
  loopCount: number;
  startOffsetSecs: number;
  trimStartSecs: number;
  trimEndSecs: number | null;
  audioBus: AudioBank;
  mute: boolean;
  solo: boolean;
  duckOthers: boolean;
  transition: VisualTransition;
  fitMode: FitMode;
  holdVisualAfterAudio: boolean;
  autoAdvanceMs: number | null;
  autoArmNext: boolean;
  stopBehavior: StopBehavior;
};

export type Project = {
  formatVersion: number;
  id: string;
  name: string;
  cues: Cue[];
  assets: MediaAsset[];
  createdAtMs: number;
  modifiedAtMs: number;
  autosaveIntervalSecs: number;
  maxMasterDb: number;
  safetyLimiter: boolean;
};

export const newCueDefaults = (id: string, number: string, name: string): Cue => ({
  id,
  number,
  name,
  notes: "",
  color: "none",
  groupId: null,
  triggerMode: { type: "manual" },
  audioAssetIds: [],
  visualAssetId: null,
  volumeDb: 0,
  fadeInMs: 0,
  fadeOutMs: 5000,
  fadeInCurve: "linear",
  fadeOutCurve: "equalPower",
  crossfadeMs: 0,
  loopCount: 1,
  startOffsetSecs: 0,
  trimStartSecs: 0,
  trimEndSecs: null,
  audioBus: "music",
  mute: false,
  solo: false,
  duckOthers: false,
  transition: { type: "cut" },
  fitMode: "contain",
  holdVisualAfterAudio: true,
  autoAdvanceMs: null,
  autoArmNext: true,
  stopBehavior: "fadeOut",
});
