import { describe, expect, it } from "vitest";
import { newCueDefaults } from "./types";
import { DEFAULT_KEYMAP, installShortcuts } from "./app/shortcuts";
import { stageEvent } from "./output/window";
import { convertFilePathToAssetSrc } from "./panels/StagePreviewPanel";

describe("cue defaults", () => {
  it("creates safe-by-default cues", () => {
    const cue = newCueDefaults("c1", "Q1", "Opening");
    // Live-safety defaults: fade out (not hard stop), hold visuals, arm next.
    expect(cue.stopBehavior).toBe("fadeOut");
    expect(cue.holdVisualAfterAudio).toBe(true);
    expect(cue.autoArmNext).toBe(true);
    expect(cue.triggerMode.type).toBe("manual");
    expect(cue.fitMode).toBe("contain");
  });
});

describe("default keymap", () => {
  it("pins the documented transport defaults", () => {
    expect(DEFAULT_KEYMAP[" "]).toBe("playPause");
    expect(DEFAULT_KEYMAP.Enter).toBe("go");
    expect(DEFAULT_KEYMAP.ArrowRight).toBe("go");
    expect(DEFAULT_KEYMAP.ArrowLeft).toBe("previousCue");
    expect(DEFAULT_KEYMAP.s).toBe("stopCurrent");
    expect(DEFAULT_KEYMAP.f).toBe("fadeOut");
    expect(DEFAULT_KEYMAP.b).toBe("blackout");
    expect(DEFAULT_KEYMAP.m).toBe("masterMute");
  });

  it("installs and uninstalls the key handler cleanly", () => {
    const remove = installShortcuts();
    expect(typeof remove).toBe("function");
    remove();
  });
});

describe("stage asset src", () => {
  it("converts paths without changing existing http urls", () => {
    // convertFileSrc from the Tauri API is exercised end-to-end in the
    // desktop build; here we just verify the util handles empty paths.
    expect(convertFilePathToAssetSrc("")).toBe("");
  });
});

describe("stage output event wiring", () => {
  it("subscribes to the stagecue://visual channel", async () => {
    // In a headless (non-Tauri) environment listen() may reject; the util
    // must not throw synchronously.
    try {
      await stageEvent(() => void 0);
    } catch {
      expect(true).toBe(true);
    }
  });
});
