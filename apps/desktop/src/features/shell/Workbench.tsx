import { lazy, Suspense, useEffect, useState } from "react";
import { CategoryDialog } from "@/features/categories/CategoryDialog";
import { GitDialog } from "@/features/git/GitDialog";
import { NewGuideDialog } from "@/features/guides/NewGuideDialog";
import { GuideList } from "@/features/library/GuideList";
import { GuideReader } from "@/features/reader/GuideReader";
import { IssuesDialog } from "@/features/settings/IssuesDialog";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import { ShortcutsDialog } from "@/features/settings/ShortcutsDialog";
import { cn } from "@/lib/utils";
import { useApp } from "@/store/app";
import { Sidebar } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { TitleBar } from "./TitleBar";

// The editor (CodeMirror) is only needed once editing starts.
const GuideEditor = lazy(() =>
  import("@/features/editor/GuideEditor").then((module) => ({
    default: module.GuideEditor,
  })),
);

function useWidth() {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}

export function Workbench() {
  const sidebarOpen = useApp((state) => state.sidebarOpen);
  const editing = useApp((state) => state.draft !== null);
  const width = useWidth();
  const compact = width < 1000;
  const narrow = width < 720;
  const [showReaderOnNarrow, setShowReaderOnNarrow] = useState(false);

  // The sidebar becomes an overlay on smaller windows; start it closed there.
  useEffect(() => {
    if (compact) useApp.setState({ sidebarOpen: false });
    else useApp.setState({ sidebarOpen: true });
  }, [compact]);

  // On narrow windows, explicitly opening a Guide shows the reader view.
  const readerRequest = useApp((state) => state.readerRequest);
  useEffect(() => {
    if (readerRequest > 0) setShowReaderOnNarrow(true);
  }, [readerRequest]);
  useEffect(() => {
    if (editing) setShowReaderOnNarrow(true);
  }, [editing]);

  const showSidebar = sidebarOpen && !compact;
  const listHidden = editing || (narrow && showReaderOnNarrow);
  const readerHidden = narrow && !showReaderOnNarrow && !editing;

  return (
    <div className="flex h-full flex-col bg-bg">
      <TitleBar compact={compact} />
      <div className="flex min-h-0 flex-1">
        {showSidebar ? <Sidebar /> : null}
        {compact && sidebarOpen ? <Sidebar overlay /> : null}
        <main
          id="main"
          tabIndex={-1}
          className="flex min-w-0 flex-1 outline-none"
          aria-label="Guides"
        >
          {listHidden ? null : (
            <GuideList
              className={cn(narrow ? "w-full" : "w-[340px] shrink-0 border-r")}
            />
          )}
          {readerHidden ? null : (
            <section
              aria-label={editing ? "Guide editor" : "Guide"}
              className="flex min-w-0 flex-1 flex-col bg-panel"
            >
              {editing ? (
                <Suspense
                  fallback={<div className="flex-1" aria-busy="true" />}
                >
                  <GuideEditor />
                </Suspense>
              ) : (
                <GuideReader
                  onBack={
                    narrow ? () => setShowReaderOnNarrow(false) : undefined
                  }
                />
              )}
            </section>
          )}
        </main>
      </div>
      <StatusBar />
      <NewGuideDialog />
      <CategoryDialog />
      <SettingsDialog />
      <IssuesDialog />
      <ShortcutsDialog />
      <GitDialog />
    </div>
  );
}
