/** Performance mode: large, high-contrast console for live operation.
 * Editing controls are hidden; the cue list is locked; shortcuts drive flow. */
import { useState } from "react";
import type { CueDTO } from "../ipc";
import { useShow, currentTimeDisplay } from "../store/showStore";
import { ipc } from "../ipc";

export default function PerformanceMode() {
  const snapshot = useShow((s) => s.snapshot);
  const setPerformanceMode = useShow((s) => s.setPerformanceMode);
  const issues = useShow((s) => s.issues);
  const [exitArmed, setExitArmed] = useState(false);

  const current = snapshot?.currentCue ?? null;
  const next = snapshot?.armedCue ?? null;

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-bg text-ink" aria-label="Performance mode">
      {/* Top: show identity + status */}
      <header className="flex items-center gap-4 border-b border-line px-4 py-3" style={{ borderColor: "var(--sc-border)" }}>
        <h1 className="text-lg font-bold">{useShow.getState().summary?.name ?? "Show"}</h1>
        <span className="tabular text-sm" style={{ color: snapshot?.status === "running" ? "var(--sc-green)" : "var(--sc-amber)" }}>
          {snapshot?.status.toUpperCase() ?? "IDLE"}
        </span>
        {snapshot?.blackout && (
          <span className="font-bold" style={{ color: "var(--sc-red)" }}>
            · STAGE BLACK
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <StageOutButton />
          <button
            className="sc-btn text-sm"
            onClick={() => {
              if (!exitArmed) {
                setExitArmed(true);
                setTimeout(() => setExitArmed(false), 3000);
                return;
              }
              setExitArmed(false);
              useShow.getState().setCueListLocked(true);
              setPerformanceMode(false);
            }}
            title="Exit performance mode (double-press)"
          >
            {exitArmed ? "Confirm exit" : "Exit mode"}
          </button>
        </div>
      </header>

      {issues.length > 0 && (
        <div role="alert" className="bg-black/50 px-4 py-2 text-sm" style={{ color: "var(--sc-red)" }}>
          {issues.length} missing/unreadable media item(s) — check cue list before going live.
        </div>
      )}

      {/* Body: previous+current | big GO | next */}
      <main className="grid min-h-0 flex-1 grid-cols-[1fr_auto_1fr] gap-4 p-4">
        {/* LEFT: previous & current */}
        <section className="flex min-h-0 flex-col gap-3">
          <BigCue card="prev" cue={null} />
          <BigCue card="current" cue={current} snapshot={snapshot} />
        </section>

        {/* CENTER: the GO button — can't miss it */}
        <section className="flex flex-col items-center justify-center gap-3">
          <button
            className="sc-btn-go sc-btn"
            style={{ height: "min(38vh, 260px)", width: "min(30vw, 320px)", fontSize: "3.2rem", borderRadius: 8 }}
            onClick={() => void ipc.showGo()}
            title="GO — trigger armed cue (Enter / Right)"
            data-testid="go-button"
          >
            GO
          </button>
          <div className="tabular text-4xl font-bold" aria-label="Current cue time">
            {currentTimeDisplay(snapshot)}
          </div>
        </section>

        {/* RIGHT: next cue */}
        <section className="flex min-h-0 flex-col gap-3">
          <BigCue card="next" cue={next} snapshot={snapshot} />
          <div className="panel flex-1 p-3 text-sm text-dim">
            <div className="sc-label">Next cue details</div>
            <ul className="mt-2 space-y-1 text-xs">
              {next && (
                <>
                  <li>Trigger: {next.triggerMode?.type === "autoFollow" ? "auto-follow" : "manual GO"}</li>
                  <li>Stop behavior: {next.stopBehavior}</li>
                  <li>Audio: {next.hasAudio ? "yes" : "no"} · Visual: {next.hasVisual ? "yes" : "no"}</li>
                  <li>Fade out: {next.fadeOutMs} ms</li>
                </>
              )}
            </ul>
          </div>
        </section>
      </main>

      {/* Bottom: transport row */}
      <footer className="flex flex-wrap items-center gap-3 border-t border-line px-4 py-3" style={{ borderColor: "var(--sc-border)" }}>
        <button className="sc-btn text-base" onClick={() => void ipc.showTogglePause()} title="Pause / resume">
          Pause / Resume
        </button>
        <button className="sc-btn text-base" onClick={() => void ipc.showRestartCue()}>
          Restart
        </button>
        <button className="sc-btn text-base" style={{ borderColor: "var(--sc-amber-dim)" }} onClick={() => void ipc.showFadeOut()} title="Fade out (F)">
          Fade out
        </button>
        <button className="sc-btn text-base sc-btn-stop" onClick={() => void ipc.showStopCurrent()} title="Stop cue (S)">
          Stop cue
        </button>
        <button
          className="sc-btn text-base sc-btn-blackout"
          onClick={async () => {
            const nextBlackout = !(snapshot?.blackout ?? false);
            await ipc.showBlackout(nextBlackout);
          }}
          title="Blackout (B)"
        >
          {snapshot?.blackout ? "Restore stage" : "Blackout"}
        </button>
        <div className="ml-auto flex items-center gap-3">
          <span className="sc-label">Master</span>
          <input
            type="range"
            min={-60}
            max={0}
            step={0.5}
            value={useShow.getState().masterDb}
            onChange={(e) => void useShow.getState().setMaster(Number(e.target.value))}
            className="w-48"
            aria-label="Master volume"
          />
          <MasterMuteBig />
        </div>
      </footer>
    </div>
  );
}

function MasterMuteBig() {
  const muted = useShow((s) => s.masterMuted);
  return (
    <button
      className={`sc-btn ${muted ? "sc-btn-stop" : ""} text-base`}
      onClick={() => void useShow.getState().toggleMasterMute()}
      title="Master mute (M)"
    >
      {muted ? "MUTED" : "Mute"}
    </button>
  );
}

function BigCue({
  card,
  cue,
  snapshot,
}: {
  card: "prev" | "current" | "next";
  cue: CueDTO | null;
  snapshot?: ReturnType<typeof useShow.getState>["snapshot"];
}) {
  const label = card === "prev" ? "PREVIOUS" : card === "current" ? "CURRENT" : "NEXT";
  const color = card === "current" ? "var(--sc-amber)" : card === "next" ? "var(--sc-green)" : "var(--sc-text-faint)";
  return (
    <div
      className="panel p-4"
      style={{ borderColor: card === "current" ? "var(--sc-amber-dim)" : "var(--sc-border)", flex: card === "current" ? 3 : 1 }}
    >
      <span className="sc-label" style={{ color }}>
        {label}
      </span>
      {cue ? (
        <>
          <div className="flex items-baseline gap-3">
            <span className="tabular text-2xl font-bold" style={{ color }}>
              {cue.number}
            </span>
            <span className="min-w-0 flex-1 truncate text-lg font-semibold">{cue.name}</span>
          </div>
          <div className="mt-1 flex gap-3 text-xs text-dim">
            <span>{cue.hasAudio ? "♫ audio" : "— audio"}</span>
            <span>{cue.hasVisual ? "▣ visual" : "— visual"}</span>
            {card === "current" && snapshot?.phase && <PhaseTag phase={snapshot.phase} />}
          </div>
          {cue.notes && card === "current" && <p className="mt-2 max-h-24 overflow-y-auto text-xs text-dim">{cue.notes}</p>}
        </>
      ) : (
        <div className="py-4 text-lg text-faint">{card === "prev" ? "—" : "no cue armed"}</div>
      )}
    </div>
  );
}

function PhaseTag({ phase }: { phase: string }) {
  const map: Record<string, [string, string]> = {
    starting: ["FADING IN", "var(--sc-blue)"],
    playing: ["PLAYING", "var(--sc-green)"],
    paused: ["PAUSED", "var(--sc-amber)"],
    fadingOut: ["FADING OUT", "var(--sc-amber)"],
    holdingVisual: ["HOLDING VISUAL", "var(--sc-blue)"],
    failed: ["ERROR", "var(--sc-red)"],
  };
  const [label, color] = map[phase] ?? [phase.toUpperCase(), "var(--sc-text-dim)"];
  return (
    <span style={{ color }} className="font-bold">
      {label}
    </span>
  );
}

function StageOutButton() {
  return (
    <button
      className="sc-btn text-sm"
      onClick={async () => {
        const { openOutputWindow } = await import("../output/window");
        await openOutputWindow();
      }}
      title="Open stage output window"
    >
      Stage output
    </button>
  );
}
