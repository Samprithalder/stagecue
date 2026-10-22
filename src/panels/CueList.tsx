/** Cue list with current/armed emphasis, drag reorder and editing ops. */
import { useState } from "react";
import { useShow } from "../store/showStore";
import { ipc } from "../ipc";
import type { Cue } from "../types";

const colorStyles: Record<string, { bg: string; label: string }> = {
  none: { bg: "transparent", label: "" },
  amber: { bg: "var(--sc-amber-dim)", label: "A" },
  blue: { bg: "#3b6c95", label: "B" },
  green: { bg: "#3f8f66", label: "G" },
  red: { bg: "var(--sc-red-deep)", label: "R" },
  purple: { bg: "#6d4e8e", label: "P" },
};

export default function CueList() {
  const project = useShow((s) => s.project);
  const snapshot = useShow((s) => s.snapshot);
  const selectedCueId = useShow((s) => s.selectedCueId);
  const selectCue = useShow((s) => s.selectCue);
  const locked = useShow((s) => s.cueListLocked);
  const [pendingDelete, setPendingDelete] = useState<Cue | null>(null);

  if (!project) {
    return (
      <aside className="panel w-[320px] shrink-0 overflow-y-auto p-4 text-dim">
        <p>No project loaded. Create or open a show to start cueing.</p>
      </aside>
    );
  }

  const currentIndex = snapshot?.currentIndex ?? null;
  const armedIndex = snapshot?.armedIndex ?? null;

  async function onAddCue() {
    const cues = await ipc.cueAdd(null, "New cue");
    if (cues.length > 0) selectCue(cues[cues.length - 1].id);
  }

  async function onDuplicate(id: string) {
    const cues = await ipc.cueDuplicate(id);
    const copy = cues.find((c) => c.name.endsWith("(copy)"));
    if (copy) selectCue(copy.id);
  }

  async function onRemove(id: string) {
    await ipc.cueRemove(id);
    if (selectedCueId === id) selectCue(null);
    setPendingDelete(null);
  }

  return (
    <aside className="panel flex w-[340px] shrink-0 flex-col overflow-hidden" aria-label="Cue list">
      <div className="flex items-center justify-between border-b border-line px-3 py-2" style={{ borderColor: "var(--sc-border)" }}>
        <h2 className="sc-label">Cues ({project.cues.length})</h2>
        <div className="flex gap-1">
          <button className="sc-btn px-2 py-1 text-xs" onClick={onAddCue} disabled={locked} title="Add cue">
            + Add
          </button>
          <button
            className="sc-btn px-2 py-1 text-xs"
            onClick={() => useShow.getState().setCueListLocked(!locked)}
            title={locked ? "Unlock cue list" : "Lock cue list against accidental edits"}
          >
            {locked ? "🔒 Locked" : "Unlocked"}
          </button>
        </div>
      </div>

      <ul className="min-h-0 flex-1 overflow-y-auto" role="listbox" aria-label="Cues">
        {project.cues.map((cue, index) => {
          const isCurrent = index === currentIndex;
          const isArmed = index === armedIndex && !isCurrent;
          const swatch = colorStyles[cue.color] ?? colorStyles.none;
          return (
            <li key={cue.id}>
              <div
                role="option"
                aria-selected={cue.id === selectedCueId}
                tabIndex={0}
                onClick={() => selectCue(cue.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") selectCue(cue.id);
                }}
                onDragStart={(e) => e.dataTransfer.setData("text/cue-index", String(index))}
                onDragOver={(e) => e.preventDefault()}
                onDrop={(e) => {
                  const from = Number(e.dataTransfer.getData("text/cue-index"));
                  if (!Number.isNaN(from) && from !== index && !locked) {
                    void ipc.cueReorder(from, index);
                  }
                }}
                draggable={!locked}
                className={`flex cursor-pointer items-center gap-2 border-b px-3 py-2 text-sm ${
                  isCurrent ? "cue-row-current" : isArmed ? "cue-row-armed" : "border-line"
                }`}
                style={isCurrent && isArmed ? {} : { borderBottomColor: "var(--sc-border)" }}
              >
                <span className="tabular w-10 shrink-0 text-amber font-semibold" style={{ color: "var(--sc-amber)" }}>
                  {cue.number}
                </span>
                <span className="flex-1 truncate" title={cue.name}>
                  {cue.name}
                </span>
                {cue.audioAssetIds.length > 0 && (
                  <span title="has audio" className="text-blu" style={{ color: "var(--sc-blue)" }}>
                    ♫
                  </span>
                )}
                {cue.visualAssetId && (
                  <span title="has visual" style={{ color: "var(--sc-blue)" }}>
                    ▣
                  </span>
                )}
                {swatch.label && (
                  <span
                    className="grid h-4 w-4 place-items-center rounded-sm text-[10px] font-bold"
                    style={{ background: swatch.bg, color: "#111" }}
                    aria-label={`color ${cue.color}`}
                  >
                    {swatch.label}
                  </span>
                )}
                {isCurrent && <StateBadge snapshot={snapshot} />}
                {isArmed && (
                  <span className="text-xs font-bold" style={{ color: "var(--sc-green)" }}>
                    ARMED
                  </span>
                )}
              </div>
            </li>
          );
        })}
      </ul>

      <div className="flex gap-1 border-t border-line p-2" style={{ borderColor: "var(--sc-border)" }}>
        <button
          className="sc-btn px-2 py-1 text-xs"
          disabled={!selectedCueId || locked}
          onClick={() => selectedCueId && void onDuplicate(selectedCueId)}
        >
          Duplicate
        </button>
        <button
          className="sc-btn px-2 py-1 text-xs"
          style={{ borderColor: "var(--sc-red-deep)" }}
          disabled={!selectedCueId || locked}
          onClick={() => {
            const cue = project.cues.find((c) => c.id === selectedCueId);
            if (cue) setPendingDelete(cue);
          }}
        >
          Delete
        </button>
      </div>

      {pendingDelete && (
        <div role="dialog" aria-modal className="absolute inset-0 grid place-items-center bg-black/60">
          <div className="panel max-w-sm p-4">
            <p>
              Delete cue <b>{pendingDelete.number} {pendingDelete.name}</b>? This cannot be undone.
            </p>
            <div className="mt-3 flex justify-end gap-2">
              <button className="sc-btn" onClick={() => setPendingDelete(null)}>
                Cancel
              </button>
              <button className="sc-btn sc-btn-stop" onClick={() => void onRemove(pendingDelete.id)}>
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}

function StateBadge({ snapshot }: { snapshot: ReturnType<typeof useShow.getState>["snapshot"] }) {
  const map: Record<string, { label: string; color: string }> = {
    starting: { label: "FADING IN", color: "var(--sc-blue)" },
    playing: { label: "PLAYING", color: "var(--sc-green)" },
    paused: { label: "PAUSED", color: "var(--sc-amber)" },
    fadingOut: { label: "FADING", color: "var(--sc-amber)" },
    holdingVisual: { label: "HOLDING VISUAL", color: "var(--sc-blue)" },
    failed: { label: "ERROR", color: "var(--sc-red)" },
  };
  const state = snapshot?.phase ? map[snapshot.phase] : null;
  if (!state) return null;
  return (
    <span className="ml-1 text-[10px] font-bold tracking-wide" style={{ color: state.color }}>
      {state.label}
    </span>
  );
}
