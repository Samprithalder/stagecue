/** Inspector: full configuration for the selected cue. */
import { useEffect, useState } from "react";
import { useShow } from "../store/showStore";
import { ipc } from "../ipc";
import { newCueDefaults, type Cue, type CueColor } from "../types";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";

type Draft = Cue;

export default function Inspector() {
  const project = useShow((s) => s.project);
  const selectedCueId = useShow((s) => s.selectedCueId);
  const cue = project?.cues.find((c) => c.id === selectedCueId) ?? null;
  const [draft, setDraft] = useState<Draft | null>(null);

  async function importFiles() {
    const files = await openFileDialog({
      title: "Add media",
      multiple: true,
      filters: [
        {
          name: "Media",
          extensions: ["wav", "mp3", "flac", "ogg", "oga", "m4a", "aac", "opus", "aiff", "aif", "wma", "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "mp4", "mov", "mkv", "webm", "avi", "m4v", "pdf", "ppt", "pptx", "odp"],
        },
      ],
    });
    if (!files) return;
    const paths = Array.isArray(files) ? files : [files];
    try {
      const assets = await ipc.assetImport(paths);
      useShow.getState().notify("info", `Imported ${assets.length} file(s).`);
    } catch (e) {
      useShow.getState().notify("error", `Import failed: ${e}`);
    }
  }

  async function importFolder() {
    const folder = await openFileDialog({ title: "Import folder of media", directory: true, multiple: false });
    if (!folder) return;
    try {
      const assets = await ipc.assetImport([String(folder)]);
      if (assets.length) {
        useShow.getState().notify("info", `Imported ${assets.length} media file(s) from folder.`);
      } else {
        useShow.getState().notify("warn", "That folder contains no media files StageCue recognizes.");
      }
    } catch (e) {
      useShow.getState().notify("error", `Import failed: ${e}`);
    }
  }


  useEffect(() => {
    setDraft(cue ? structuredClone(cue) : null);
  }, [cue?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!project) return <aside className="panel w-[300px] shrink-0 p-3 text-dim">No project.</aside>;
  if (!cue || !draft) {
    return (
      <aside className="panel w-[300px] shrink-0 p-4 text-dim" aria-label="Inspector">
        <p>Select a cue to inspect it. Drag media from the library into a cue to assign it.</p>
        <ImportZone onImport={() => void importFiles()} onImportFolder={() => void importFolder()} />
      </aside>
    );
  }

  const update = (patch: Partial<Cue>) => setDraft({ ...draft, ...patch });

  async function commit() {
    if (!draft) return;
    await ipc.cueUpdate(draft).catch((e) => useShow.getState().notify("error", `Update failed: ${e}`));
  }

  const num = (v: string): number => (v === "" || Number.isNaN(Number(v)) ? 0 : Number(v));

  return (
    <aside className="panel flex w-[300px] shrink-0 flex-col overflow-y-auto p-3" aria-label="Inspector">
      <h2 className="sc-label mb-2">Cue {draft.number}</h2>

      <label className="block text-xs text-dim">
        Name
        <input
          className="mt-1 w-full rounded border border-line bg-panel2 px-2 py-1 text-ink"
          value={draft.name}
          onChange={(e) => update({ name: e.target.value })}
          onBlur={commit}
        />
      </label>

      <label className="mt-2 block text-xs text-dim">
        Notes
        <textarea
          className="mt-1 min-h-[60px] w-full rounded border border-line bg-panel2 px-2 py-1 text-ink"
          value={draft.notes}
          onChange={(e) => update({ notes: e.target.value })}
          onBlur={commit}
          placeholder="Operator notes for this cue"
        />
      </label>

      <div className="mt-3">
        <span className="sc-label">Color</span>
        <div className="mt-1 flex gap-1">
          {(["none", "amber", "blue", "green", "red", "purple"] as CueColor[]).map((c) => (
            <button
              key={c}
              className="h-5 w-5 rounded-sm border"
              style={{
                background: c === "none" ? "var(--sc-panel-2)" : `var(--sc-${c === "amber" ? "amber-dim" : c === "red" ? "red-deep" : c})`,
                borderColor: draft.color === c ? "var(--sc-text)" : "var(--sc-border)",
              }}
              onClick={() => {
                update({ color: c });
                setTimeout(commit, 0);
              }}
              aria-label={`Color ${c}`}
            />
          ))}
        </div>
      </div>

      <div className="mt-3 grid grid-cols-2 gap-2">
        <NumField label="Fade in (ms)" value={draft.fadeInMs} onChange={(v) => update({ fadeInMs: v })} onCommit={commit} />
        <NumField label="Fade out (ms)" value={draft.fadeOutMs} onChange={(v) => update({ fadeOutMs: v })} onCommit={commit} />
        <NumField label="Volume (dB)" value={draft.volumeDb} onChange={(v) => update({ volumeDb: v })} onCommit={commit} step={0.5} />
        <NumField label="Start offset (s)" value={draft.startOffsetSecs} onChange={(v) => update({ startOffsetSecs: v })} onCommit={commit} step={0.1} />
      </div>

      <label className="mt-3 block text-xs text-dim">
        Trigger mode
        <select
          className="mt-1 w-full rounded border border-line bg-panel2 px-2 py-1 text-ink"
          value={draft.triggerMode.type}
          onChange={async (e) => {
            const t = e.target.value;
            update({ triggerMode: t === "autoFollow" ? { type: "autoFollow", delayMs: 1000 } : { type: "manual" } });
            setTimeout(commit, 0);
          }}
        >
          <option value="manual">Manual GO</option>
          <option value="autoFollow">Auto-follow after previous cue</option>
        </select>
      </label>
      {draft.triggerMode.type === "autoFollow" && (
        <label className="mt-2 block text-xs text-dim">
          Follow delay (ms)
          <input
            className="mt-1 w-full rounded border border-line bg-panel2 px-2 py-1 tabular"
            value={draft.triggerMode.delayMs ?? 1000}
            onChange={(e) => update({ triggerMode: { type: "autoFollow", delayMs: num(e.target.value) } })}
            onBlur={commit}
          />
        </label>
      )}

      <label className="mt-3 block text-xs text-dim">
        Stop behavior (on next GO)
        <select
          className="mt-1 w-full rounded border border-line bg-panel2 px-2 py-1 text-ink"
          value={draft.stopBehavior}
          onChange={(e) => {
            update({ stopBehavior: e.target.value as Cue["stopBehavior"] });
            setTimeout(commit, 0);
          }}
        >
          <option value="hold">Hold audio (keep playing)</option>
          <option value="fadeOut">Fade out</option>
          <option value="stopImmediately">Stop immediately</option>
        </select>
      </label>

      <label className="mt-3 block text-xs text-dim">
        Fade-out curve
        <select
          className="mt-1 w-full rounded border border-line bg-panel2 px-2 py-1 text-ink"
          value={draft.fadeOutCurve}
          onChange={(e) => {
            update({ fadeOutCurve: e.target.value as Cue["fadeOutCurve"] });
            setTimeout(commit, 0);
          }}
        >
          <option value="linear">Linear</option>
          <option value="equalPower">Equal power</option>
          <option value="exponential">Exponential</option>
          <option value="logarithmic">Logarithmic</option>
          <option value="sCurve">S-curve</option>
        </select>
      </label>

      <label className="mt-3 flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          checked={draft.holdVisualAfterAudio}
          onChange={(e) => {
            update({ holdVisualAfterAudio: e.target.checked });
            setTimeout(commit, 0);
          }}
        />
        Hold visual after audio ends
      </label>

      <label className="mt-1 flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          checked={draft.autoArmNext}
          onChange={(e) => {
            update({ autoArmNext: e.target.checked });
            setTimeout(commit, 0);
          }}
        />
        Automatically arm next cue
      </label>

      <div className="mt-4">
        <span className="sc-label">Media</span>
        <p className="mt-1 text-xs text-dim">
          Audio: {draft.audioAssetIds.length} assigned · Visual: {draft.visualAssetId ? nameOf(project, draft.visualAssetId) : "none"}
        </p>
        <button
          className="sc-btn mt-2 w-full text-xs"
          onClick={async () => {
            const files = await openFileDialog({ title: "Assign audio", multiple: true });
            if (!files) return;
            const paths = Array.isArray(files) ? files : [files];
            const imported = await ipc.assetImport(paths);
            update({ audioAssetIds: [...draft.audioAssetIds, ...imported.map((a) => a.id)] });
            setTimeout(commit, 0);
          }}
        >
          Assign audio file…
        </button>
        <button
          className="sc-btn mt-1 w-full text-xs"
          onClick={async () => {
            const file = await openFileDialog({ title: "Assign visual" });
            if (!file) return;
            const [imported] = await ipc.assetImport([file as string]);
            update({ visualAssetId: imported.id });
            setTimeout(commit, 0);
          }}
        >
          Assign visual…
        </button>
      </div>

      <ImportZone onImport={() => void importFiles()} onImportFolder={() => void importFolder()} />
      <button className="sc-btn mt-3 text-xs" onClick={commit}>
        Apply changes
      </button>
    </aside>
  );
}

function nameOf(project: NonNullable<ReturnType<typeof useShow.getState>["project"]>, assetId: string): string {
  return project.assets.find((a) => a.id === assetId)?.displayName ?? assetId.slice(0, 8);
}

function NumField(props: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  onCommit: () => void | Promise<void>;
  step?: number;
}) {
  return (
    <label className="block text-xs text-dim">
      {props.label}
      <input
        type="number"
        step={props.step ?? 1}
        className="mt-1 w-full rounded border border-line bg-panel2 px-2 py-1 tabular text-ink"
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
        onBlur={() => void props.onCommit()}
      />
    </label>
  );
}

function ImportZone({
  onImport,
  onImportFolder,
}: {
  onImport?: () => void;
  onImportFolder?: () => void;
}) {
  return (
    <div className="mt-4 rounded border border-dashed border-line2 p-3 text-center text-xs text-faint" style={{ borderColor: "var(--sc-border-strong)" }}>
      Drag audio/image/video files onto this panel to import into the project library.
      <div className="mt-2 flex justify-center gap-2">
        <button className="sc-btn text-xs" onClick={() => onImport?.()}>
          Import media files…
        </button>
        <button className="sc-btn text-xs" onClick={() => onImportFolder?.()}>
          Import folder…
        </button>
      </div>
    </div>
  );
}

export function makeEmptyCue(id: string, number: string, name: string): Cue {
  return newCueDefaults(id, number, name);
}
