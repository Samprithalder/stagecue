/** Operator console. The layout follows the console blueprint:
 * top bar → left cue list · center stage preview/output · right inspector →
 * bottom audio console. Performance mode replaces the body wholesale. */
import { useEffect } from "react";
import { ensureStoreInit, useShow } from "../store/showStore";
import TopBar from "../panels/TopBar";
import CueList from "../panels/CueList";
import StagePreviewPanel from "../panels/StagePreviewPanel";
import Inspector from "../panels/Inspector";
import BottomConsole from "../panels/BottomConsole";
import PerformanceMode from "../panels/PerformanceMode";
import { openOutputWindow } from "../output/window";
import { installShortcuts } from "./shortcuts";

export default function App() {
  const performanceMode = useShow((s) => s.performanceMode);

  useEffect(() => {
    void ensureStoreInit();
    void openOutputWindow();
    const removeShortcuts = installShortcuts();
    return removeShortcuts;
  }, []);

  return (
    <div className="flex h-screen flex-col bg-bg text-ink">
      {performanceMode ? (
        <PerformanceMode />
      ) : (
        <>
          <TopBar />
          <main className="flex min-h-0 flex-1">
            <CueList />
            <StagePreviewPanel />
            <Inspector />
          </main>
          <BottomConsole />
        </>
      )}
    </div>
  );
}
