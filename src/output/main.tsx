import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { convertFileSrc } from "@tauri-apps/api/core";
import { stageEvent, openOutputWindow } from "./window";
import type { StageVisualDTO } from "../ipc";
import "../design/theme.css";

/** Map the cue's fit mode to Tailwind object-fit classes ("none" = Center). */
function fitClass(fitMode: string): string {
  switch (fitMode) {
    case "cover":
      return "object-cover";
    case "stretch":
      return "object-fill";
    case "center":
      return "object-none";
    case "contain":
    default:
      return "object-contain";
  }
}

/** Stage Output window: only ever shows what the audience should see.
 * Operator controls NEVER appear here — this component has no buttons. */
function StageOutput() {
  const [visual, setVisual] = useState<StageVisualDTO | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void openOutputWindow().then(() =>
      // In a second display the output window reuses this same JS bundle;
      // only the "stage" window keeps rendering (console renders index.html).
      void 0,
    );
    let unlisten: (() => void) | undefined;
    void stageEvent((v) => {
      setVisual(v);
      setError(null);
      if (v.kind === "video" || v.kind === "slide") {
        // Validate media existence from the Rust side before render errors.
        if (v.assetPath && v.assetPath.length === 0) {
          setError("media missing");
        }
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const content = () => {
    if (error) {
      return <div className="sc-error">Media could not be displayed — check the operator console.</div>;
    }
    if (visual == null) {
      return <div />;
    }
    if (visual.testPattern) {
      return <TestPattern />;
    }
    if (visual.blackout) {
      return null; // absolute black
    }
    if (!visual.assetPath) {
      return null;
    }
    switch (visual.kind) {
      case "image":
        return (
          <img
            src={convertFileSrc(visual.assetPath)}
            className={`h-full w-full ${fitClass(visual.fitMode)}`}
            alt=""
          />
        );
      case "video":
        return (
          <video
            src={convertFileSrc(visual.assetPath)}
            className={`h-full w-full ${fitClass(visual.fitMode)}`}
            autoPlay
            loop
            muted
          />
        );
      case "slide":
      case "audio":
      case "text":
      default:
        return (
          <div className="flex h-full w-full items-center justify-center bg-black">
            <p className="text-center px-8 text-white">{visual.cueNumber || ""}</p>
          </div>
        );
    }
  };

  return (
    <div className="stage-output-body fixed inset-0">
      {content()}
      {visual != null && visual.blackout && !visual.testPattern && <div className="absolute inset-0 bg-black" />}
    </div>
  );
}

/** SMPTE-like bars for output alignment during setup. */
function TestPattern() {
  const bars = ["#c0c0c0", "#c0c000", "#00c0c0", "#00c000", "#c000c0", "#c00000", "#0000c0"];
  return (
    <div className="h-full w-full grid grid-cols-7">
      {bars.map((c, i) => (
        <div key={i} style={{ background: c }} />
      ))}
    </div>
  );
}

createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <StageOutput />
  </React.StrictMode>,
);
