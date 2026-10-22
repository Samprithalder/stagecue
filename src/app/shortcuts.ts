/** Configurable keyboard shortcuts. Destructive actions (Stop All / Panic)
 * always require the double-press confirm built into their buttons — the
 * keyboard path reuses those same guarded handlers. */
import { ipc } from "../ipc";
import { useShow } from "../store/showStore";

type Keymap = Record<string, string>;

export const DEFAULT_KEYMAP: Keymap = {
  " ": "playPause",
  Enter: "go",
  ArrowRight: "go",
  ArrowLeft: "previousCue",
  s: "stopCurrent",
  f: "fadeOut",
  b: "blackout",
  m: "masterMute",
  Escape: "cancelPending",
  F10: "togglePerformance",
};

const isTypingTarget = (t: EventTarget | null): boolean => {
  const el = t as HTMLElement | null;
  if (!el) return false;
  return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable;
};

export function installShortcuts(): () => void {
  const handler = (e: KeyboardEvent) => {
    // Never hijack typing or platform combos (Ctrl/Cmd+s etc. are handled by
    // the shell menu in Phase 6; here we cover the transport defaults).
    if (isTypingTarget(e.target)) return;
    if (e.ctrlKey || e.metaKey || e.altKey) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void ipc.projectSave().catch((err) => useShow.getState().notify("error", `Save failed: ${err}`));
      }
      return;
    }
    const action = DEFAULT_KEYMAP[e.key];
    const s = useShow.getState();
    switch (action) {
      case undefined:
        return;
      case "go":
        e.preventDefault();
        void ipc.showGo().catch((err) => s.notify("error", String(err)));
        break;
      case "previousCue":
        e.preventDefault();
        // Arm the cue before the current one, without touching audio.
        if ((s.snapshot?.currentIndex ?? 0) > 0) {
          void ipc
            .showArmCue((s.snapshot?.currentIndex ?? 1) - 1)
            .catch((err) => s.notify("error", String(err)));
        }
        break;
      case "playPause":
        e.preventDefault();
        void ipc.showTogglePause().catch(() => void 0);
        break;
      case "stopCurrent":
        e.preventDefault();
        void ipc.showStopCurrent().catch((err) => s.notify("error", String(err)));
        break;
      case "fadeOut":
        e.preventDefault();
        void ipc.showFadeOut().catch((err) => s.notify("error", String(err)));
        break;
      case "blackout":
        e.preventDefault();
        void ipc.showBlackout(!(s.snapshot?.blackout ?? false));
        break;
      case "masterMute":
        e.preventDefault();
        void s.toggleMasterMute();
        break;
      case "cancelPending":
        // Escape closes dialogs via their own key handling; nothing global.
        break;
      case "togglePerformance":
        e.preventDefault();
        s.setPerformanceMode(!s.performanceMode);
        break;
      default:
        break;
    }
  };
    if (typeof window === "undefined") return () => void 0;
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}
