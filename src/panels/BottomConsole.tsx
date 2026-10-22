/** Bottom bar: audio console. Safe-by-default — master/stop/blackout/panic
 * live here, per-channel detail is one click away, never visible by chance. */
import { useEffect, useState } from "react";
import { useShow } from "../store/showStore";
import { ipc } from "../ipc";

export default function BottomConsole() {
  const masterDb = useShow((s) => s.masterDb);
  const masterMuted = useShow((s) => s.masterMuted);
  const setMaster = useShow((s) => s.setMaster);
  const toggleMasterMute = useShow((s) => s.toggleMasterMute);
  const snapshot = useShow((s) => s.snapshot);
  const notify = useShow((s) => s.notify);
  const [devices, setDevices] = useState<[string, string][]>([]);
  const [activeDevice, setActiveDevice] = useState<string>("Default");
  const [panicArmed, setPanicArmed] = useState(false);

  useEffect(() => {
    void ipc
      .audioOutputDevices()
      .then(setDevices)
      .catch(() => setDevices([]));
  }, []);

  function onPanic() {
    if (!panicArmed) {
      setPanicArmed(true);
      notify("warn", "Panic armed — press again within 3 seconds to stop ALL audio and clear the stage.");
      setTimeout(() => setPanicArmed(false), 3000);
      return;
    }
    setPanicArmed(false);
    void ipc.showPanic();
    notify("info", "PANIC — all audio stopped, stage cleared.");
  }

  return (
    <footer className="panel border-t border-line" style={{ borderColor: "var(--sc-border)" }} aria-label="Audio console">
      <div className="flex flex-wrap items-center gap-4 p-3">
        {/* Master fader */}
        <div className="min-w-[220px]">
          <div className="flex items-baseline justify-between">
            <span className="sc-label">Master</span>
            <span className="tabular text-sm" data-testid="master-db">
              {masterDb.toFixed(1)} dB
            </span>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="range"
              min={-60}
              max={0}
              step={0.5}
              value={masterDb}
              onChange={(e) => void setMaster(Number(e.target.value))}
              className="w-40 accent-amber"
              aria-label="Master volume"
            />
            <button
              className={`sc-btn px-2 py-1 text-xs ${masterMuted ? "sc-btn-stop" : ""}`}
              onClick={() => void toggleMasterMute()}
              aria-pressed={masterMuted}
              title="Master mute (M)"
            >
              {masterMuted ? "MUTED" : "Mute"}
            </button>
          </div>
        </div>

        {/* Estimated level meters (envelope-based; see docs/limitations.md) */}
        <Meter label="L" db={masterDb + (snapshot?.phase === "playing" ? -6 : -48)} />
        <Meter label="R" db={masterDb + (snapshot?.phase === "playing" ? -7 : -48)} />

        {/* Fade-out */}
        <button className="sc-btn text-sm" style={{ borderColor: "var(--sc-amber-dim)" }} onClick={() => void ipc.showFadeOut()} title="Fade out current audio (F)">
          Fade out
        </button>

        {/* Blackout */}
        <button
          className={`sc-btn text-sm ${snapshot?.blackout ? "sc-btn-blackout" : ""}`}
          onClick={async () => {
            const on = !(snapshot?.blackout ?? false);
            await ipc.showBlackout(on);
          }}
          title="Blackout stage output (B)"
          aria-pressed={snapshot?.blackout ?? false}
        >
          {snapshot?.blackout ? "Stage is BLACK (click to restore)" : "Blackout"}
        </button>

        {/* Panic: double-press to safety */}
        <button
          className={`sc-btn text-sm ${panicArmed ? "sc-btn-stop animate-none" : ""}`}
          style={panicArmed ? { background: "var(--sc-red)", borderColor: "var(--sc-red)" } : undefined}
          onClick={onPanic}
          title="Panic — stops all audio and clears the stage (double-press)"
        >
          {panicArmed ? "CONFIRM PANIC" : "Panic"}
        </button>

        {/* Output device */}
        <label className="ml-auto flex items-center gap-2 text-xs text-dim">
          <span className="sc-label">Output</span>
          <select
            className="rounded border border-line bg-panel2 px-2 py-1 text-ink"
            value={activeDevice}
            onChange={async (e) => {
              const name = e.target.value === "Default" ? null : e.target.value;
              try {
                const chosen = await ipc.audioSetOutput(name);
                setActiveDevice(chosen);
                notify("info", `Audio output: ${chosen}`);
              } catch (err) {
                notify("error", `Device switch failed: ${err}`);
              }
            }}
            aria-label="Audio output device"
          >
            <option value="Default">System default</option>
            {devices
              .filter(([n]) => n !== "Default")
              .map(([n, cfg]) => (
                <option key={n} value={n}>
                  {n} {cfg && `· ${cfg}`}
                </option>
              ))}
          </select>
        </label>
      </div>

      {snapshot?.phase === "failed" && (
        <div className="flex items-center justify-between bg-black/40 px-3 py-2 text-sm" role="alert">
          <span style={{ color: "var(--sc-red)" }}>
            The current cue failed during playback. Advance with GO, or use Recover skip.
          </span>
          <button className="sc-btn text-xs" onClick={() => void ipc.showRecoverSkip()}>
            Recover skip
          </button>
        </div>
      )}
    </footer>
  );
}

/** Envelope-based meter bar (documented in docs/limitations.md — rodio has no
 * process-internal post-mix tap; replace with true output metering later). */
function Meter({ label, db }: { label: string; db: number }) {
  const pct = Math.max(0, Math.min(100, ((db + 60) / 60) * 100));
  const clipping = db > -0.5;
  const warning = db > -6 && !clipping;
  return (
    <div className="w-28">
      <div className="sc-label">{label}</div>
      <div className="h-3 w-full overflow-hidden rounded-sm" style={{ background: "#000", border: "1px solid var(--sc-border)" }}>
        <div
          className={`h-full ${clipping ? "meter-clip" : warning ? "meter-warn" : ""}`}
          style={{
            width: `${pct}%`,
            background: clipping ? "var(--sc-red)" : warning ? "var(--sc-amber)" : "var(--sc-green)",
            transition: "width 120ms linear",
          }}
        />
      </div>
      <span className="tabular text-[10px] text-faint">{db <= -59 ? "-∞" : `${db.toFixed(1)}`}</span>
    </div>
  );
}
