// Git workflows. Every write still goes through the backend's plan → native
// confirmation → apply sequence; this store only keeps the UI honest about it.
import { toast } from "sonner";
import { create } from "zustand";
import { api, toNativeError } from "@/api/client";
import {
  type GitCommitPlanPayload,
  type GitSyncInspectionPayload,
  gitSyncPresentation,
  normalizedGitCommitMessage,
  normalizedRemoteName,
  normalizedRemoteUrl,
} from "@/api/payloads";
import {
  adoptWorkspace,
  draftIsDirty,
  refreshGitStatus,
  refreshWorkspace,
  reportError,
  useApp,
} from "@/store/app";

type GitState = {
  busy:
    | null
    | "commit"
    | "sync"
    | "push"
    | "remote"
    | "identity"
    | "init"
    | "recover";
  commitPlan: GitCommitPlanPayload | null;
  inspection: GitSyncInspectionPayload | null;
  error: string | null;
};

export const useGit = create<GitState>(() => ({
  busy: null,
  commitPlan: null,
  inspection: null,
  error: null,
}));
const set = useGit.setState;
const get = useGit.getState;

function fail(error: unknown) {
  const failure = toNativeError(error);
  set({ error: failure.message });
  if (failure.code === "recoveryRequired") void refreshGitStatus();
  return failure;
}

export function clearGitError() {
  set({ error: null });
}

export async function initializeGit() {
  set({ busy: "init", error: null });
  try {
    const plan = await api.git.initPlan();
    const result = await api.git.initApply(plan.planToken);
    if (result.initialized) toast.success("Git is set up for this Workspace");
    await refreshGitStatus();
  } catch (error) {
    fail(error);
  } finally {
    set({ busy: null });
  }
}

export async function addRemote(name: string, url: string) {
  const remoteName = normalizedRemoteName(name);
  const remoteUrl = normalizedRemoteUrl(url);
  if (remoteName === null) {
    set({ error: "Use a short lowercase name, like “origin”." });
    return false;
  }
  if (remoteUrl === null) {
    set({ error: "Enter the repository address (HTTPS or SSH)." });
    return false;
  }
  set({ busy: "remote", error: null });
  try {
    const plan = await api.git.remotePlan(remoteName, remoteUrl);
    const result = await api.git.remoteApply(plan.planToken);
    if (!result.connected) return false;
    toast.success(`Connected to ${plan.host}`);
    await refreshGitStatus();
    return true;
  } catch (error) {
    fail(error);
    return false;
  } finally {
    set({ busy: null });
  }
}

export async function setAuthor(
  source: "global" | { name: string; email: string },
) {
  set({ busy: "identity", error: null });
  try {
    const plan =
      source === "global"
        ? await api.git.identityGlobalPlan()
        : await api.git.identityManualPlan(
            source.name.trim(),
            source.email.trim(),
          );
    const result = await api.git.identityApply(plan.planToken);
    if (!result.configured) return false;
    toast.success("Git author saved for this Workspace");
    await refreshGitStatus();
    return true;
  } catch (error) {
    fail(error);
    return false;
  } finally {
    set({ busy: null });
  }
}

/** Step 1 of committing: let the backend compute exactly what will be committed. */
export async function prepareCommit(message: string) {
  const normalized = normalizedGitCommitMessage(message);
  if (normalized === null) {
    set({ error: "Write a one-line message (up to 200 characters)." });
    return;
  }
  if (draftIsDirty()) {
    set({ error: "Save or discard your open edits before committing." });
    return;
  }
  set({ busy: "commit", error: null, commitPlan: null });
  try {
    set({ commitPlan: await api.git.commitPlan(normalized) });
  } catch (error) {
    fail(error);
  } finally {
    set({ busy: null });
  }
}

export async function cancelCommit() {
  const { commitPlan } = get();
  set({ commitPlan: null, error: null });
  if (commitPlan)
    await api.git.commitCancel(commitPlan.planToken).catch(() => undefined);
}

/** Step 2: the backend asks for native confirmation, then commits. */
export async function confirmCommit() {
  const { commitPlan } = get();
  if (commitPlan === null) return false;
  set({ busy: "commit", error: null });
  try {
    const result = await api.git.commitApply(commitPlan.planToken);
    set({ commitPlan: null });
    if (!result.committed) return false;
    toast.success("Committed", {
      description: result.workspaceChanged
        ? "Files changed during the commit; those changes are still uncommitted."
        : undefined,
    });
    if (result.workspaceChanged) await refreshWorkspace();
    await refreshGitStatus();
    return true;
  } catch (error) {
    set({ commitPlan: null });
    fail(error);
    return false;
  } finally {
    set({ busy: null });
  }
}

export async function inspectSync(origin: "manual" | "automatic" = "manual") {
  if (get().busy !== null) return;
  set({ busy: "sync", error: null, inspection: null });
  try {
    const inspection = await api.git.syncInspect();
    set({ inspection });
    if (origin === "automatic") {
      const view = gitSyncPresentation(inspection);
      if (view.canApply || view.canPush) {
        toast.info(view.title, {
          description: "Open Source Control to review it.",
          action: {
            label: "Review",
            onClick: () => useApp.setState({ dialog: "git" }),
          },
        });
      }
    }
  } catch (error) {
    const failure = toNativeError(error);
    if (failure.code !== "cancelled") {
      if (origin === "manual") fail(error);
    }
  } finally {
    set({ busy: null });
  }
}

export async function dismissInspection() {
  const { inspection, busy } = get();
  set({ inspection: null, error: null });
  if (inspection && busy === null)
    await api.git.syncCancel().catch(() => undefined);
}

export async function applyPull() {
  const { inspection } = get();
  if (inspection === null) return;
  set({ busy: "sync", error: null });
  try {
    const result = await api.git.syncApply(inspection.planToken);
    set({ inspection: null });
    if (result.kind === "declined") return;
    adoptWorkspace(
      result.workspace,
      result.changed ? "Pulled the latest changes" : "Already up to date",
    );
  } catch (error) {
    set({ inspection: null });
    fail(error);
  } finally {
    set({ busy: null });
    void refreshGitStatus();
  }
}

export async function applyPush() {
  const { inspection } = get();
  if (inspection === null) return;
  set({ busy: "push", error: null });
  try {
    const result = await api.git.pushApply(inspection.planToken);
    set({ inspection: null });
    if (result.kind === "completed") toast.success("Pushed to remote");
  } catch (error) {
    set({ inspection: null });
    fail(error);
  } finally {
    set({ busy: null });
    void refreshGitStatus();
  }
}

/** Finishes an interrupted sync or commit. Sync recovery is tried first. */
export async function recoverGit() {
  set({ busy: "recover", error: null });
  try {
    try {
      const sync = await api.git.syncRecover();
      if (sync.kind === "completed") {
        adoptWorkspace(sync.workspace, "Finished the interrupted sync");
      }
      return;
    } catch (error) {
      if (toNativeError(error).code !== "recoveryNotRequired") throw error;
    }
    const commit = await api.git.commitRecover();
    if (commit.recovered) {
      toast.success("Finished the interrupted commit");
      if (commit.workspaceChanged) await refreshWorkspace();
    }
  } catch (error) {
    reportError(error, "Couldn't finish the interrupted Git operation");
  } finally {
    set({ busy: null });
    void refreshGitStatus();
  }
}

/** Optional startup and scheduled checks. Both only inspect; never apply. */
export function startGitAutomation() {
  let inspectedSession = "";
  let timer: number | undefined;
  const schedule = () => {
    window.clearInterval(timer);
    const { preferences, workspace } = useApp.getState();
    if (workspace === null || preferences.syncIntervalMinutes === null) return;
    timer = window.setInterval(() => {
      const state = useApp.getState();
      if (
        state.gitStatus?.kind === "ready" &&
        state.gitStatus.upstream &&
        state.draft === null
      )
        void inspectSync("automatic");
    }, preferences.syncIntervalMinutes * 60_000);
  };
  const unsubscribe = useApp.subscribe((state, previous) => {
    if (
      state.preferences !== previous.preferences ||
      state.workspace?.id !== previous.workspace?.id
    )
      schedule();
    const session = state.workspace?.watchSession ?? "";
    if (
      state.preferences.syncOnStart &&
      session !== "" &&
      session !== inspectedSession &&
      state.gitStatus?.kind === "ready" &&
      state.gitStatus.upstream !== null
    ) {
      inspectedSession = session;
      void inspectSync("automatic");
    }
  });
  return () => {
    unsubscribe();
    window.clearInterval(timer);
  };
}
