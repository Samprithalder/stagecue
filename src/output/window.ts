/** Manages the separate Stage Output window (audience-facing display). */
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow, getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import type { StageVisualDTO } from "../ipc";

export function stageEvent(go: (v: StageVisualDTO) => void): Promise<() => void> {
  return listen<StageVisualDTO>("stagecue://visual", (e) => go(e.payload));
}

/** Open (or focus) the dedicated stage-output window, borderless fullscreen. */
export async function openOutputWindow(): Promise<void> {
  const windows = await getAllWebviewWindows();
  const existing = windows.find((w) => w.label === "stage");
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return;
  }
  await new WebviewWindow("stage", {
    url: "output.html",
    title: "StageCue — Stage Output",
    width: 1280,
    height: 720,
    decorations: false,
    fullscreen: true,
  });
}
