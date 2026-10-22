/** Top bar: project identity, save state, stage output control, mode switch. */
import { useState } from "react";
import { useShow } from "../store/showStore";
import { ipc } from "../ipc";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { openOutputWindow } from "../output/window";

export default function TopBar() {
  const summary = useShow((s) => s.summary);
  const snapshot = useShow((s) => s.snapshot);
  const project = useShow((s) => s.project);
  const setProject = useShow((s) => s.setProject);
  const notify = useShow((s) => s.notify);
  const setPerformanceMode = useShow((s) => s.setPerformanceMode);
  const [busy, setBusy] = useState(false);

  const stageStatus = snapshot?.blackout ? "BLACKOUT" : "LIVE PREVIEW";

  async function handleNew() {
    const name = window.prompt("Show project name", "Untitled show");
    if (!name) return;
    const s = await ipc.projectNew(name);
    const project = projectTemplate(name);
    setProject(project, s);
    notify("info", `Started new show “${name}” — save it (Ctrl+S) when ready.`);
  }

  async function handleOpen() {
    const file = await saveFileDialogOpenHelper();
    if (!file) return;
    const project = await ipc.projectOpen(file);
    setProject(project, {
      path: file,
      name: project.name,
      hasUnsavedChanges: false,
      cueCount: project.cues.length,
      assetCount: project.assets.length,
      missingAssets: 0,
    });
  }

  async function handleSave() {
    setBusy(true);
    try {
      if (!useShow.getState().summary?.path) {
        await handleSaveAs();
      } else {
        await ipc.projectSave();
        notify("info", "Project saved.");
      }
    } catch (e) {
      notify("error", `Save failed: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveAs() {
    const path = await saveFileDialog({
      title: "Save show project as",
      filters: [{ name: "StageCue project", extensions: ["stagecue"] }],
    });
    if (!path) return;
    const finalPath = path.endsWith(".stagecue") ? path : `${path}.stagecue`;
    const s = await ipc.projectSaveAs(finalPath);
    notify("info", `Saved to ${finalPath}`);
    useShow.setState({ summary: s });
    const project = await ipc.projectOpen(finalPath);
    setProject(project, s);
  }

  async function validateBeforeShow() {
    const issues = await ipc.validateShow();
    useShow.getState().setIssues(issues);
    if (issues.length > 0) {
      notify("error", `${issues.length} cue issue(s) found — show mode blocked until fixed.`);
      return false;
    }
    notify("info", "Show validation passed.");
    return true;
  }

  return (
    <header
      className="flex items-center gap-3 border-b border-line bg-panel px-4 py-2"
      aria-label="Top bar"
      style={{ borderColor: "var(--sc-border)" }}
    >
      <img src="/icons/icon.png" width={22} height={22} alt="" aria-hidden className="hidden sm:block" />
      <button className="sc-btn text-sm" onClick={handleNew} disabled={busy}>
        New
      </button>
      <button className="sc-btn text-sm" onClick={handleOpen} disabled={busy}>
        Open
      </button>
      <button className="sc-btn text-sm" onClick={handleSave} disabled={busy} title="Ctrl+S">
        Save
      </button>
      <button className="sc-btn text-sm" onClick={handleSaveAs} disabled={busy} title="Ctrl+Shift+S">
        Save As
      </button>

      <div className="mx-4 min-w-0 flex-1 truncate text-dim text-sm" title={summary?.name}>
        <span className="text-ink font-semibold">{summary?.name ?? "No project"}</span>
        {summary?.hasUnsavedChanges && <span className="text-amber"> · unsaved changes</span>}
      </div>

      <button className="sc-btn text-sm" onClick={() => void openOutputWindow()}>
        Stage Output
      </button>
      <span
        className="tabular text-xs"
        style={{
          color:
            snapshot?.blackout
              ? "var(--sc-red)"
              : snapshot?.status === "running"
                ? "var(--sc-green)"
                : "var(--sc-text-faint)",
        }}
        data-testid="stage-status"
      >
        {stageStatus}
      </span>
      <button
        className="sc-btn sc-btn-go text-sm"
        disabled={!project}
        onClick={async () => {
          if (!(await validateBeforeShow())) return;
          setPerformanceMode(true);
        }}
        title="Enter performance mode (validators run first)"
      >
        Performance Mode
      </button>
    </header>
  );
}

/** Small helper drafted to keep dialog imports close to their use-cases. */
async function saveFileDialogOpenHelper(): Promise<string | null> {
  const path = await openFileDialog({
    title: "Open show project",
    filters: [{ name: "StageCue project", extensions: ["stagecue"] }],
    multiple: false,
  });
  return typeof path === "string" ? path : null;
}

function projectTemplate(name: string) {
  return {
    formatVersion: 1,
    id: "",
    name,
    cues: [],
    assets: [],
    createdAtMs: 0,
    modifiedAtMs: 0,
    autosaveIntervalSecs: 30,
    maxMasterDb: 0,
    safetyLimiter: true,
  };
}
