// App-wide side effects: startup, search, file watching, keyboard shortcuts,
// and unsaved-work protection. Kept in one hook so App stays declarative.
import { useEffect } from "react";
import { api } from "@/api/client";
import { startGitAutomation } from "@/features/git/gitStore";
import {
  boot,
  draftIsDirty,
  openDialog,
  refreshWorkspace,
  saveDraft,
  selectGuide,
  startEditing,
  storeDraft,
  toggleSidebar,
  useApp,
  visibleGuides,
} from "@/store/app";

const SEARCH_DEBOUNCE_MS = 90;

export function focusSearch() {
  const input = document.getElementById(
    "guide-search",
  ) as HTMLInputElement | null;
  input?.focus();
  input?.select();
}

function isTyping(target: EventTarget | null) {
  return (
    target instanceof HTMLElement &&
    (target.matches("input, textarea, select, [contenteditable='true']") ||
      target.closest(".cm-editor") !== null)
  );
}

export function moveSelection(direction: 1 | -1) {
  const state = useApp.getState();
  const list = visibleGuides(state);
  if (list.length === 0) return;
  const index = list.findIndex((guide) => guide.id === state.selectedId);
  const next =
    list[Math.min(list.length - 1, Math.max(0, index + direction))] ?? list[0];
  if (next && next.id !== state.selectedId) {
    void selectGuide(next.id);
    requestAnimationFrame(() =>
      document
        .querySelector(`[data-guide-row="${next.id}"]`)
        ?.scrollIntoView({ block: "nearest" }),
    );
  }
}

export function useAppEffects() {
  useEffect(() => {
    void boot();
  }, []);

  // Ranked search runs in the backend; previous results stay on screen until
  // the new ones arrive, so the list never flickers while typing.
  useEffect(() => {
    let timer: number | undefined;
    let generation = 0;
    const unsubscribe = useApp.subscribe((state, previous) => {
      if (
        state.query === previous.query &&
        state.scope === previous.scope &&
        state.workspace?.watchSession === previous.workspace?.watchSession &&
        state.workspace === previous.workspace
      )
        return;
      const query = state.query.trim();
      window.clearTimeout(timer);
      if (query === "" || state.workspace === null) {
        if (state.hits !== null) useApp.setState({ hits: null, hitsQuery: "" });
        return;
      }
      const current = ++generation;
      timer = window.setTimeout(async () => {
        try {
          const hits = await api.search(query, state.scope);
          if (current === generation)
            useApp.setState({ hits, hitsQuery: query });
        } catch {
          if (current === generation) useApp.setState({ hits: null });
        }
      }, SEARCH_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      window.clearTimeout(timer);
    };
  }, []);

  // External file changes: refresh automatically unless there is unsaved work.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api
      .onWorkspaceInvalidated((event) => {
        const { workspace } = useApp.getState();
        if (
          workspace === null ||
          event.workspaceId !== workspace.id ||
          event.watchSession !== workspace.watchSession
        )
          return;
        void refreshWorkspace({ automatic: true });
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => undefined);
    let blurred = false;
    const onBlur = () => {
      blurred = true;
    };
    const onFocus = () => {
      if (!blurred) return;
      blurred = false;
      void refreshWorkspace({ automatic: true });
    };
    window.addEventListener("blur", onBlur);
    window.addEventListener("focus", onFocus);
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  // Mirror unsaved drafts locally and warn before the window unloads.
  useEffect(() => {
    let timer: number | undefined;
    const unsubscribe = useApp.subscribe((state, previous) => {
      if (
        state.draft === previous.draft ||
        state.draft === null ||
        state.guide === null
      )
        return;
      const { draft, guide } = state;
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (draftIsDirty()) storeDraft(draft, guide.revision);
      }, 400);
    });
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if (!draftIsDirty()) return;
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => {
      unsubscribe();
      window.clearTimeout(timer);
      window.removeEventListener("beforeunload", onBeforeUnload);
    };
  }, []);

  useEffect(() => startGitAutomation(), []);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const mod = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      const state = useApp.getState();
      const hasWorkspace = state.workspace !== null;

      if (mod && key === "k") {
        event.preventDefault();
        openDialog(state.dialog === "palette" ? null : "palette");
        return;
      }
      // Prevent the webview from reloading and dropping state.
      if (mod && key === "r") {
        event.preventDefault();
        if (hasWorkspace) void refreshWorkspace();
        return;
      }
      if (state.dialog !== null || !hasWorkspace) return;

      if (mod && key === "s") {
        event.preventDefault();
        if (state.draft !== null)
          void saveDraft({ keepEditing: !event.shiftKey });
        return;
      }
      if (mod && key === ",") {
        event.preventDefault();
        openDialog("settings");
        return;
      }
      if (mod && key === "n") {
        event.preventDefault();
        openDialog("newGuide");
        return;
      }
      if (mod && key === "b") {
        event.preventDefault();
        toggleSidebar();
        return;
      }
      // In the editor, Ctrl+F belongs to find-in-Guide.
      if (mod && (key === "p" || (key === "f" && state.draft === null))) {
        event.preventDefault();
        focusSearch();
        return;
      }
      if (mod && key === "e") {
        event.preventDefault();
        if (state.draft === null) startEditing();
        return;
      }
      if (isTyping(event.target) || mod || event.altKey) return;
      if (key === "/") {
        event.preventDefault();
        focusSearch();
      } else if (key === "j" || key === "arrowdown") {
        event.preventDefault();
        moveSelection(1);
      } else if (key === "k" || key === "arrowup") {
        event.preventDefault();
        moveSelection(-1);
      } else if (key === "e" && state.draft === null && state.guide !== null) {
        event.preventDefault();
        startEditing();
      } else if (key === "?") {
        event.preventDefault();
        openDialog("shortcuts");
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
