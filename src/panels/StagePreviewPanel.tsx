/** Center panel: live stage preview, next-cue preview, transport, media info. */
import { useShow, currentTimeDisplay } from "../store/showStore";
import { ipc } from "../ipc";
import { convertFileSrc } from "@tauri-apps/api/core";

function Clock({ label, value, tone = "ink" }: { label: string; value: string; tone?: string }) {
  return (
    <div className="min-w-[92px]">
      <div className="sc-label">{label}</div>
      <div
        className={`tabular text-2xl font-semibold ${tone}`}
        style={{ color: tone === "ink" ? "var(--sc-text)" : `var(--sc-${tone})` }}
      >
        {value}
      </div>
    </div>
  );
}

export default function StagePreviewPanel() {
  const snapshot = useShow((s) => s.snapshot);
  const stageVisual = useShow((s) => s.stageVisual);
  const project = useShow((s) => s.project);
  const currentCue = snapshot?.currentCue ?? null;
  const armedCue = snapshot?.armedCue ?? null;
  const currentAsset = currentCue?.visualAssetId
    ? project?.assets.find((a) => a.id === currentCue.visualAssetId)
    : null;

  const phaseLabels: Record<string, string> = {
    starting: "FADING IN",
    playing: "PLAYING",
    paused: "PAUSED",
    fadingOut: "FADING OUT",
    holdingVisual: "HOLDING VISUAL",
    failed: "ERROR",
  };
  const phaseLabel = snapshot == null ? "" : (phaseLabels[snapshot.phase ?? ""] ?? snapshot.status.toUpperCase());

  const phaseColor =
    snapshot?.phase === "failed"
      ? "var(--sc-red)"
      : snapshot?.phase === "paused" || snapshot?.phase === "fadingOut"
        ? "var(--sc-amber)"
        : snapshot?.status === "running"
          ? "var(--sc-green)"
          : "var(--sc-text-faint)";

  return (
    <section className="flex min-w-0 flex-1 flex-col gap-3 p-3" aria-label="Stage preview">
      <div className="flex items-center gap-4">
        <Clock label="Cue time" value={currentTimeDisplay(snapshot)} />
        <Clock label="Next in" value={nextIn(snapshot)} tone="blu" />
        <div
          className="tabular text-sm font-bold tracking-widest"
          style={{ color: phaseColor }}
          data-testid="phase"
        >
          {phaseLabel}
        </div>
        {snapshot?.blackout && (
          <span className="text-xs font-bold" style={{ color: "var(--sc-red)" }}>
            STAGE BLACK
          </span>
        )}
      </div>

      <div className="grid min-h-0 flex-1 grid-rows-2 gap-3">
        {/* LIVE preview */}
        <div className="relative flex min-h-0 flex-col">
          <span className="sc-label absolute left-2 top-1 z-10 text-[10px]" style={{ color: "var(--sc-amber)" }}>
            LIVE · {currentCue ? `${currentCue.number} ${currentCue.name}` : "—"}
          </span>
          <div className="panel relative min-h-0 flex-1 overflow-hidden">
            {currentAsset ? (
              currentAsset.kind === "video" || currentAsset.kind === "image" ? (
                // Preview mirrors the stage fit (see fitMode handling in Phase 3 polish)
                <img
                  src={
                    currentAsset.kind === "video"
                      ? "" /* inline video preview is Phase 5 */
                      : convertFilePathToAssetSrc(currentAsset.path)
                  }
                  alt=""
                  className="h-full w-full object-contain"
                />
              ) : (
                <div className="grid h-full place-items-center text-faint">Audio-only cue</div>
              )
            ) : (
              <div className="grid h-full flex-1 place-items-center text-faint">
                {snapshot?.blackout ? "Stage is blacked out" : "No visual on stage"}
              </div>
            )}
            {snapshot?.blackout && <div className="absolute inset-0 bg-black" />}
          </div>
        </div>

        {/* NEXT preview */}
        <div className="relative flex min-h-0 flex-col">
          <span className="sc-label absolute left-2 top-1 z-10 text-[10px]" style={{ color: "var(--sc-green)" }}>
            NEXT · {armedCue ? `${armedCue.number} ${armedCue.name}` : "no cue armed"}
          </span>
          <div className="panel relative min-h-0 flex-1 overflow-hidden">
            {armedCue?.visualAssetId && project && (
              <img
                src={convertFilePathToAssetSrc(
                  project.assets.find((a) => a.id === armedCue.visualAssetId)?.path ?? "",
                )}
                alt=""
                className="h-full w-full object-contain"
              />
            )}
            {!armedCue && <div className="grid h-full place-items-center text-faint">—</div>}
            {armedCue && !armedCue.visualAssetId && (
              <div className="grid h-full place-items-center text-faint">Next cue has no visual</div>
            )}
          </div>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <button className="sc-btn text-sm" onClick={() => void ipc.showTogglePause()}>
          Pause / Resume
        </button>
        <button className="sc-btn text-sm" onClick={() => void ipc.showRestartCue()}>
          Restart cue
        </button>
        <button className="sc-btn text-sm" onClick={() => void ipc.showStopCurrent()}>
          Stop current
        </button>
        <button className="sc-btn text-sm" onClick={() => void ipc.outputTestPattern(!stageVisual?.testPattern)}>
          {stageVisual?.testPattern ? "Hide test pattern" : "Test pattern"}
        </button>
        <span className="ml-auto flex items-center gap-1 text-xs text-dim">
          <span className="tabular">held audio: {snapshot?.heldAudioIndex != null ? "yes" : "no"}</span>
        </span>
      </div>
    </section>
  );
}

/** Time until a pending auto-follow fires, for the operator countdown. */
function nextIn(snapshot: ReturnType<typeof useShow.getState>["snapshot"]): string {
  const pending = snapshot?.pendingAuto;
  if (!pending) return "GO";
  const remaining = Math.max(0, pending[1] - Date.now());
  const s = Math.floor(remaining / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")} auto`;
}

/** Convert a filesystem path to a webview-usable asset URL. */
export function convertFilePathToAssetSrc(path: string): string {
  if (!path) return "";
  return convertFileSrc(path);
}
