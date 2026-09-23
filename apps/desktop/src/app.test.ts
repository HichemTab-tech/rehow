import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  APPLICATION_NAME,
  App,
  AttachmentTextPreviewContent,
  attachmentPreviewOwnerKey,
  automaticRefreshBlocked,
  canMarkGuideTested,
  ExternalGuideComparison,
  filterGuides,
  focusGitCommitReview,
  focusGitSyncAction,
  GuideContext,
  gitFactLabel,
  gitStatusLabel,
  gitSyncPresentation,
  guideLifecycleLabel,
  guideMatchesSearchScope,
  hasUnsavedChanges,
  highlightedExcerptSegments,
  isAttachmentExportPayload,
  isAttachmentRevealPayload,
  isAttachmentTextPreviewPayload,
  isCurrentWorkspaceWatchEvent,
  isDesktopPreferencesPayload,
  isExternalLinkOpenPayload,
  isGitCloneApplyResultPayload,
  isGitCloneCancelResultPayload,
  isGitCloneCompleteResultPayload,
  isGitClonePlanPayload,
  isGitCommitCancelResultPayload,
  isGitCommitPlanPayload,
  isGitCommitRecoveryResultPayload,
  isGitCommitResultPayload,
  isGitInitPlanPayload,
  isGitInitResultPayload,
  isGitPushResultPayload,
  isGitRemotePlanPayload,
  isGitRemoteResultPayload,
  isGitStatusPayload,
  isGitSyncApplyResultPayload,
  isGitSyncCancelResultPayload,
  isGitSyncInspectionPayload,
  isGitSyncRecoveryResultPayload,
  isMarkdownImportApplyPayload,
  isMarkdownImportPreviewPayload,
  isRecentWorkspaceResult,
  isSearchHitPayload,
  isWorkspaceWatchEvent,
  movedCategoryOrder,
  nativeCommandError,
  nextResultIndex,
  normalizedCategoryDetails,
  normalizedCloneUrl,
  normalizedGitCommitMessage,
  normalizedGuideDetails,
  normalizedGuideMetadata,
  normalizedGuideTerms,
  normalizedRemoteName,
  normalizedRemoteUrl,
  normalizedWorkspaceName,
  rankedGuides,
  refreshedGuideId,
  refreshResultMayApply,
  requestGitPush,
  requestGitStatus,
  resultCountLabel,
  searchStatePath,
  WorkspaceIssuesContent,
  workspaceIssuePresentation,
} from "./App";
import { MarkdownDocument } from "./MarkdownDocument";

describe("application identity", () => {
  it("uses the Rehow name", () => {
    expect(APPLICATION_NAME).toBe("Rehow");
  });

  it("formats singular and plural result announcements", () => {
    expect(resultCountLabel(0)).toBe("0 Guides");
    expect(resultCountLabel(1)).toBe("1 Guide");
    expect(resultCountLabel(2)).toBe("2 Guides");
  });

  it("wraps Arrow-key result movement in both directions", () => {
    expect(nextResultIndex(0, 4, 1)).toBe(1);
    expect(nextResultIndex(3, 4, 1)).toBe(0);
    expect(nextResultIndex(0, 4, -1)).toBe(3);
    expect(nextResultIndex(0, 0, 1)).toBe(-1);
  });

  it("accepts only closed desktop preferences", () => {
    expect(
      isDesktopPreferencesPayload({
        syncOnStart: true,
        syncIntervalMinutes: 60,
      }),
    ).toBe(true);
    expect(
      isDesktopPreferencesPayload({
        syncOnStart: false,
        syncIntervalMinutes: null,
      }),
    ).toBe(true);
    expect(
      isDesktopPreferencesPayload({
        syncOnStart: true,
        syncIntervalMinutes: 1,
      }),
    ).toBe(false);
    expect(
      isDesktopPreferencesPayload({
        syncOnStart: true,
        syncIntervalMinutes: null,
        recentWorkspace: "/private/path",
      }),
    ).toBe(false);
  });

  it("accepts only pathless recent Workspace results", () => {
    const workspace = {
      id: "workspace-id",
      name: "Recent Guides",
      categories: [],
      guides: [],
      issueCount: 0,
      issues: [],
      issuesTruncated: false,
      facts: {
        writable: true,
        existingGitRepository: false,
        nestedGitRepository: false,
        gitProbeComplete: true,
        unsafeGitMarker: false,
      },
      watchStatus: "active",
      watchSession: "1",
    };
    expect(isRecentWorkspaceResult({ kind: "none" })).toBe(true);
    expect(isRecentWorkspaceResult({ kind: "restored", workspace })).toBe(true);
    expect(
      isRecentWorkspaceResult({
        kind: "unavailable",
        reason: "The recent Workspace is unavailable.",
      }),
    ).toBe(true);
    expect(
      isRecentWorkspaceResult({
        kind: "unavailable",
        reason: "The recent Workspace is unavailable.",
        path: "/private/path",
      }),
    ).toBe(false);
    expect(
      isRecentWorkspaceResult({
        kind: "unavailable",
        reason: "unsafe\ntext",
      }),
    ).toBe(false);
  });

  it("accepts only bounded pathless Markdown import previews", () => {
    const preview = {
      included: 2,
      normalizationRequired: 1,
      excludedNonMarkdown: 3,
      malformedOrUnsafe: 1,
      oversized: 1,
      aggregateBytes: "2048",
      entries: [
        {
          displayPath: "linux/install.md",
          size: "1024",
          normalizationRequired: false,
        },
        {
          displayPath: "line%0Abreak.md",
          size: "1024",
          normalizationRequired: true,
        },
      ],
    };

    expect(isMarkdownImportPreviewPayload(preview)).toBe(true);
    expect(
      isMarkdownImportPreviewPayload({
        ...preview,
        included: 0,
        normalizationRequired: 0,
        aggregateBytes: "0",
        entries: [],
      }),
    ).toBe(true);
    expect(
      isMarkdownImportPreviewPayload({ ...preview, sourcePath: "/home/user" }),
    ).toBe(false);
    expect(
      isMarkdownImportPreviewPayload({
        ...preview,
        entries: [{ ...preview.entries[0], displayPath: "line\nbreak.md" }],
      }),
    ).toBe(false);
    expect(
      isMarkdownImportPreviewPayload({
        ...preview,
        entries: [preview.entries[0], preview.entries[0]],
      }),
    ).toBe(false);
    expect(
      isMarkdownImportPreviewPayload({
        ...preview,
        included: 10_000,
        excludedNonMarkdown: 10_001,
      }),
    ).toBe(false);
    expect(
      isMarkdownImportPreviewPayload({
        ...preview,
        entries: [{ ...preview.entries[0], size: "2093057" }],
      }),
    ).toBe(false);
  });

  it("accepts only closed Markdown import apply results", () => {
    const workspace = {
      id: "workspace-id",
      name: "Imported Guides",
      categories: [],
      guides: [],
      issueCount: 0,
      issues: [],
      issuesTruncated: false,
      facts: {
        writable: true,
        existingGitRepository: false,
        nestedGitRepository: false,
        gitProbeComplete: true,
        unsafeGitMarker: false,
      },
      watchStatus: "active",
      watchSession: "1",
    };
    const completed = {
      workspace,
      imported: 2,
      total: 2,
      complete: true,
      failure: null,
    };

    expect(isMarkdownImportApplyPayload(completed)).toBe(true);
    expect(
      isMarkdownImportApplyPayload({
        ...completed,
        imported: 1,
        complete: false,
        failure: "A later Guide could not be written.",
      }),
    ).toBe(true);
    expect(
      isMarkdownImportApplyPayload({ ...completed, sourcePath: "/tmp/source" }),
    ).toBe(false);
    expect(
      isMarkdownImportApplyPayload({ ...completed, complete: false }),
    ).toBe(false);
    expect(
      isMarkdownImportApplyPayload({
        ...completed,
        imported: 3,
      }),
    ).toBe(false);
  });

  it("accepts only bounded pathless Attachment text previews", () => {
    const preview = {
      title: "Example config",
      text: "key=value\n",
      sizeBytes: 10,
      sanitized: false,
    };
    expect(isAttachmentTextPreviewPayload(preview)).toBe(true);
    expect(
      isAttachmentTextPreviewPayload({ ...preview, path: "/home/user/file" }),
    ).toBe(false);
    expect(
      isAttachmentTextPreviewPayload({
        ...preview,
        text: "x".repeat(1024 * 1024 + 1),
      }),
    ).toBe(false);
    expect(
      isAttachmentTextPreviewPayload({
        ...preview,
        sizeBytes: 1024 * 1024 + 1,
      }),
    ).toBe(false);
    expect(
      isAttachmentTextPreviewPayload({
        ...preview,
        text: "safe\u202eexe.txt",
        sanitized: true,
      }),
    ).toBe(false);
    for (const invisible of [
      "\u00ad",
      "\u180e",
      "\u2061",
      "\ufe0f",
      "\u{e0001}",
    ]) {
      expect(
        isAttachmentTextPreviewPayload({
          ...preview,
          text: `unsafe${invisible}text`,
          sanitized: true,
        }),
      ).toBe(false);
    }
  });

  it("binds Attachment preview ownership to Workspace session and Guide revision", () => {
    const current = attachmentPreviewOwnerKey(
      "session-1",
      "guide-1",
      "revision-1",
    );
    expect(
      attachmentPreviewOwnerKey("session-2", "guide-1", "revision-1"),
    ).not.toBe(current);
    expect(
      attachmentPreviewOwnerKey("session-1", "guide-1", "revision-2"),
    ).not.toBe(current);
    expect(
      attachmentPreviewOwnerKey("session-1", "guide-1", "revision-1"),
    ).toBe(current);
  });

  it("accepts only closed pathless Attachment export results", () => {
    expect(isAttachmentExportPayload({ kind: "cancelled" })).toBe(true);
    expect(isAttachmentExportPayload({ kind: "exported", sizeBytes: 42 })).toBe(
      true,
    );
    expect(
      isAttachmentExportPayload({
        kind: "exported",
        sizeBytes: 42,
        path: "/home/user/file",
      }),
    ).toBe(false);
    expect(
      isAttachmentExportPayload({
        kind: "exported",
        sizeBytes: 64 * 1024 * 1024 + 1,
      }),
    ).toBe(false);
  });

  it("accepts only a closed pathless Attachment reveal result", () => {
    expect(isAttachmentRevealPayload({ revealed: true })).toBe(true);
    expect(
      isAttachmentRevealPayload({
        revealed: true,
        path: "/home/user/Workspace/attachments",
      }),
    ).toBe(false);
    expect(isAttachmentRevealPayload({ revealed: false })).toBe(false);
  });

  it("accepts only a closed URL-free external-link result", () => {
    expect(isExternalLinkOpenPayload({ requested: true })).toBe(true);
    expect(
      isExternalLinkOpenPayload({
        requested: true,
        destination: "https://example.com",
      }),
    ).toBe(false);
    expect(isExternalLinkOpenPayload({ requested: false })).toBe(false);
  });

  it("describes root, ancestor, and unsafe Git facts", () => {
    const base = {
      writable: true,
      existingGitRepository: false,
      nestedGitRepository: false,
      gitProbeComplete: true,
      unsafeGitMarker: false,
    };
    expect(gitFactLabel({ ...base, existingGitRepository: true })).toBe(
      "Git repository detected",
    );
    expect(gitFactLabel({ ...base, nestedGitRepository: true })).toBe(
      "Inside an existing Git repository",
    );
    expect(gitFactLabel({ ...base, unsafeGitMarker: true })).toBe(
      "Git marker needs attention",
    );
  });

  it("validates and summarizes bounded Git status", () => {
    const status = {
      kind: "ready" as const,
      version: "2.55.0",
      branch: "main",
      detached: false,
      upstream: "origin/main",
      divergenceKnown: true,
      ahead: 2,
      behind: 1,
      changed: 3,
      untracked: 1,
      conflicts: 0,
      remotes: ["origin"],
      remoteCount: 1,
      failure: null,
    };
    expect(isGitStatusPayload(status)).toBe(true);
    expect(gitStatusLabel(status)).toBe("main · 4 changes · ↑2 ↓1");
    expect(isGitStatusPayload({ ...status, unexpected: true })).toBe(false);
    expect(isGitStatusPayload({ ...status, remoteCount: -1 })).toBe(false);
    expect(gitStatusLabel({ ...status, conflicts: 2 })).toBe(
      "main · 2 conflicts",
    );
    const recovery = {
      ...status,
      kind: "recoveryRequired" as const,
      version: null,
      branch: null,
      upstream: null,
      divergenceKnown: false,
      ahead: 0,
      behind: 0,
      changed: 0,
      untracked: 0,
      remotes: [],
      remoteCount: 0,
    };
    expect(isGitStatusPayload(recovery)).toBe(true);
    expect(gitStatusLabel(recovery)).toBe("Git operation needs recovery");
  });

  it("accepts only closed Git initialization plan and result payloads", () => {
    const plan = {
      planToken: `plan:v1:${"A".repeat(43)}`,
      initialBranch: "main",
    };
    expect(isGitInitPlanPayload(plan)).toBe(true);
    expect(isGitInitPlanPayload({ ...plan, path: "/tmp/workspace" })).toBe(
      false,
    );
    expect(isGitInitPlanPayload({ ...plan, initialBranch: "master" })).toBe(
      false,
    );
    expect(isGitInitResultPayload({ initialized: true })).toBe(true);
    expect(
      isGitInitResultPayload({ initialized: true, repositoryPath: "/tmp" }),
    ).toBe(false);
  });

  it("accepts only closed URL-free Git remote payloads and normalizes inputs", () => {
    const plan = {
      planToken: `plan:v1:${"A".repeat(43)}`,
      remoteName: "origin",
      transport: "ssh" as const,
      host: "example.invalid",
    };
    expect(isGitRemotePlanPayload(plan)).toBe(true);
    expect(isGitRemotePlanPayload({ ...plan, url: "ssh://secret" })).toBe(
      false,
    );
    expect(isGitRemotePlanPayload({ ...plan, transport: "file" })).toBe(false);
    expect(isGitRemoteResultPayload({ connected: true })).toBe(true);
    expect(isGitRemoteResultPayload({ connected: true, path: "/tmp" })).toBe(
      false,
    );
    expect(normalizedRemoteName(" origin ")).toBe("origin");
    expect(normalizedRemoteName("Origin")).toBeNull();
    expect(normalizedRemoteName("a..b")).toBeNull();
    expect(normalizedRemoteUrl(" ssh://git@example.invalid/a.git ")).toBe(
      "ssh://git@example.invalid/a.git",
    );
    expect(normalizedRemoteUrl("   ")).toBeNull();
  });

  it("accepts only bounded path-free Git commit payloads", () => {
    const plan = {
      planToken: `plan:v1:${"A".repeat(43)}`,
      branch: "main",
      created: 2,
      updated: 3,
      deleted: 1,
      unrelatedChanges: 4,
    };
    expect(isGitCommitPlanPayload(plan)).toBe(true);
    expect(isGitCommitPlanPayload({ ...plan, path: "/tmp/workspace" })).toBe(
      false,
    );
    expect(isGitCommitPlanPayload({ ...plan, objectId: "a".repeat(40) })).toBe(
      false,
    );
    expect(isGitCommitPlanPayload({ ...plan, created: -1 })).toBe(false);
    expect(isGitCommitPlanPayload({ ...plan, branch: "x\nmain" })).toBe(false);
    expect(
      isGitCommitResultPayload({ committed: true, workspaceChanged: false }),
    ).toBe(true);
    expect(
      isGitCommitResultPayload({ committed: false, workspaceChanged: true }),
    ).toBe(false);
    expect(isGitCommitCancelResultPayload({ cancelled: true })).toBe(true);
    expect(isGitCommitCancelResultPayload({ cancelled: false })).toBe(false);
    expect(
      isGitCommitRecoveryResultPayload({
        recovered: true,
        workspaceChanged: true,
      }),
    ).toBe(true);
    expect(
      isGitCommitRecoveryResultPayload({
        recovered: false,
        workspaceChanged: true,
      }),
    ).toBe(false);
  });

  it("normalizes the same safe one-line Git commit message envelope", () => {
    expect(normalizedGitCommitMessage(" Update Guides ")).toBe("Update Guides");
    expect(normalizedGitCommitMessage(" ")).toBeNull();
    expect(normalizedGitCommitMessage("one\ntwo")).toBeNull();
    expect(
      normalizedGitCommitMessage(`safe${String.fromCodePoint(0x202e)}`),
    ).toBeNull();
    expect(normalizedGitCommitMessage(String.fromCodePoint(0xfdd0))).toBeNull();
    expect(normalizedGitCommitMessage("x".repeat(201))).toBeNull();
  });

  it("moves focus into the commit review after the planned state renders", () => {
    let rendered = false;
    let focused = false;
    let deferred: (() => void) | undefined;
    focusGitCommitReview(
      () => (rendered ? { focus: () => (focused = true) } : null),
      (callback) => {
        deferred = callback;
      },
    );
    expect(focused).toBe(false);
    rendered = true;
    deferred?.();
    expect(focused).toBe(true);
  });

  it("accepts only closed sync payloads and maps every safe result", () => {
    const inspection = {
      planToken: `plan:v1:${"A".repeat(43)}`,
      remoteName: "origin",
      transport: "https" as const,
      host: "example.invalid",
      ancestry: "remoteFastForward" as const,
      localChanges: false,
      workspaceMatchesRemote: false,
    };
    expect(isGitSyncInspectionPayload(inspection)).toBe(true);
    expect(
      isGitSyncInspectionPayload({
        ...inspection,
        remoteUrl: "https://secret",
      }),
    ).toBe(false);
    expect(
      isGitSyncInspectionPayload({ ...inspection, host: "bad\nhost" }),
    ).toBe(false);
    expect(
      isGitSyncInspectionPayload({ ...inspection, ancestry: "unknown" }),
    ).toBe(false);
    expect(isGitSyncCancelResultPayload({ cancellationRequested: true })).toBe(
      true,
    );
    expect(
      isGitSyncCancelResultPayload({
        cancellationRequested: true,
        path: "/tmp",
      }),
    ).toBe(false);
    expect(isGitSyncApplyResultPayload({ kind: "declined" })).toBe(true);
    expect(isGitSyncRecoveryResultPayload({ kind: "declined" })).toBe(true);
    expect(isGitPushResultPayload({ kind: "declined" })).toBe(true);
    expect(isGitPushResultPayload({ kind: "completed", pushed: true })).toBe(
      true,
    );
    expect(isGitPushResultPayload({ kind: "completed", pushed: false })).toBe(
      false,
    );

    expect(gitSyncPresentation(inspection)).toMatchObject({
      title: "Fast-Forward Available",
      canApply: true,
      canPush: false,
    });
    expect(
      gitSyncPresentation({ ...inspection, workspaceMatchesRemote: true })
        .detail,
    ).toContain("metadata");
    expect(
      gitSyncPresentation({ ...inspection, localChanges: true }),
    ).toMatchObject({ title: "Commit Local Changes First", canApply: false });
    for (const [ancestry, title, canPush] of [
      ["upToDate", "Already Up to Date"],
      ["localAhead", "Local Branch Is Ahead", true],
      ["diverged", "Branches Have Diverged"],
      ["historyLimit", "History Needs Review"],
    ] as const) {
      expect(gitSyncPresentation({ ...inspection, ancestry })).toMatchObject({
        title,
        canApply: false,
        canPush: canPush ?? false,
      });
    }

    const workspace = {
      id: "workspace-id",
      name: "Personal",
      categories: [],
      guides: [],
      issueCount: 0,
      issues: [],
      issuesTruncated: false,
      facts: {
        writable: true,
        existingGitRepository: true,
        nestedGitRepository: false,
        gitProbeComplete: true,
        unsafeGitMarker: false,
      },
      watchStatus: "active",
      watchSession: "2",
    };
    expect(
      isGitSyncApplyResultPayload({
        kind: "completed",
        workspace,
        changed: true,
      }),
    ).toBe(true);
    expect(
      isGitSyncApplyResultPayload({
        kind: "completed",
        workspace,
        changed: true,
        objectId: "a".repeat(40),
      }),
    ).toBe(false);
  });

  it("invokes exact push and status commands and restores focus after rendering", async () => {
    const status = {
      kind: "ready",
      version: "2.55.0",
      branch: "main",
      detached: false,
      upstream: "origin/main",
      divergenceKnown: true,
      ahead: 0,
      behind: 0,
      changed: 0,
      untracked: 0,
      conflicts: 0,
      remotes: ["origin"],
      remoteCount: 1,
      failure: null,
    };
    const calls: Array<[string, Record<string, unknown> | undefined]> = [];
    const invoke = async (
      command: string,
      arguments_?: Record<string, unknown>,
    ) => {
      calls.push([command, arguments_]);
      return command === "git_push_apply_open_workspace"
        ? { kind: "completed", pushed: true }
        : status;
    };
    const token = `plan:v1:${"A".repeat(43)}`;
    await expect(requestGitPush(token, invoke)).resolves.toEqual({
      kind: "completed",
      pushed: true,
    });
    await expect(requestGitStatus(invoke)).resolves.toEqual(status);
    expect(calls).toEqual([
      ["git_push_apply_open_workspace", { planToken: token }],
      ["git_status_open_workspace", undefined],
    ]);
    await expect(
      requestGitPush(token, async () => ({ kind: "completed", pushed: false })),
    ).rejects.toThrow("invalid Git push result");

    let rendered = false;
    let focused = false;
    let deferred: (() => void) | undefined;
    focusGitSyncAction(
      () => (rendered ? { focus: () => (focused = true) } : null),
      (callback) => {
        deferred = callback;
      },
    );
    expect(focused).toBe(false);
    rendered = true;
    deferred?.();
    expect(focused).toBe(true);
  });

  it("accepts only closed path-free Git clone lifecycle payloads", () => {
    const plan = {
      planToken: `plan:v1:${"A".repeat(43)}`,
      transport: "https" as const,
      host: "example.invalid",
    };
    expect(isGitClonePlanPayload(plan)).toBe(true);
    expect(
      isGitClonePlanPayload({ ...plan, remoteUrl: "https://secret" }),
    ).toBe(false);
    expect(
      isGitCloneApplyResultPayload({ validated: true, stage: "validated" }),
    ).toBe(true);
    expect(
      isGitCloneApplyResultPayload({ validated: false, stage: "validated" }),
    ).toBe(false);
    expect(isGitCloneCancelResultPayload({ cancellationRequested: true })).toBe(
      true,
    );
    expect(
      isGitCloneCancelResultPayload({
        cancellationRequested: true,
        path: "/tmp/workspace",
      }),
    ).toBe(false);
    expect(isGitCloneCompleteResultPayload({ kind: "declined" })).toBe(true);
    const workspace = {
      id: "workspace-id",
      name: "Personal",
      categories: [],
      guides: [],
      issueCount: 0,
      issues: [],
      issuesTruncated: false,
      facts: {
        writable: true,
        existingGitRepository: true,
        nestedGitRepository: false,
        gitProbeComplete: true,
        unsafeGitMarker: false,
      },
      watchStatus: "active",
      watchSession: "1",
    };
    expect(
      isGitCloneCompleteResultPayload({ kind: "completed", workspace }),
    ).toBe(true);
    expect(
      isGitCloneCompleteResultPayload({ kind: "completed", workspace: {} }),
    ).toBe(false);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: { ...workspace, name: "x".repeat(201) },
      }),
    ).toBe(false);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: { ...workspace, issueCount: 10_001 },
      }),
    ).toBe(false);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: {
          ...workspace,
          issueCount: 1,
          issues: [
            {
              kind: "missingCategoryDescriptor",
              relativePath: "categories/0199a1c2/category.yaml",
            },
          ],
          issuesTruncated: true,
        },
      }),
    ).toBe(false);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: {
          ...workspace,
          issueCount: 1,
          issues: [
            {
              kind: "missingCategoryDescriptor",
              relativePath: "categories/0199a1c2/category.yaml",
            },
          ],
          issuesTruncated: false,
        },
      }),
    ).toBe(true);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: {
          ...workspace,
          issueCount: 1,
          issues: [
            {
              kind: "missingCategoryDescriptor",
              relativePath: "categories/\u202E/category.yaml",
            },
          ],
          issuesTruncated: false,
        },
      }),
    ).toBe(false);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: {
          ...workspace,
          issueCount: 1,
          issues: [{ kind: "unknown", relativePath: "categories/unknown" }],
          issuesTruncated: false,
        },
      }),
    ).toBe(false);
    expect(
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace,
        path: "/tmp/workspace",
      }),
    ).toBe(false);
  });

  it("normalizes bounded secret-free HTTPS and SSH clone inputs for native validation", () => {
    expect(normalizedCloneUrl(" https://example.invalid/team/rehow.git ")).toBe(
      "https://example.invalid/team/rehow.git",
    );
    expect(normalizedCloneUrl("ssh://git@example.invalid/team/rehow.git")).toBe(
      "ssh://git@example.invalid/team/rehow.git",
    );
    expect(normalizedCloneUrl("git@example.invalid:team/rehow.git")).toBe(
      "git@example.invalid:team/rehow.git",
    );
    expect(normalizedCloneUrl("https://user@example.invalid/rehow.git")).toBe(
      "https://user@example.invalid/rehow.git",
    );
    expect(
      normalizedCloneUrl("https://example.invalid/rehow.git?token=x"),
    ).toBeNull();
    expect(normalizedCloneUrl("https://Example.invalid/rehow.git")).toBe(
      "https://Example.invalid/rehow.git",
    );
    expect(normalizedCloneUrl("https://example.invalid/")).toBe(
      "https://example.invalid/",
    );
  });

  it("detects exact Guide editor dirty state", () => {
    expect(hasUnsavedChanges("same\n", "same\n")).toBe(false);
    expect(hasUnsavedChanges("same\n", "changed\n")).toBe(true);
  });

  it("preserves or safely falls back after Workspace refresh", () => {
    const guides = [{ id: "one" }, { id: "two" }];
    expect(refreshedGuideId("two", guides)).toBe("two");
    expect(refreshedGuideId("removed", guides)).toBe("one");
    expect(refreshedGuideId("removed", [])).toBe("");
  });

  it("accepts only bounded path-free native watch events", () => {
    expect(
      isWorkspaceWatchEvent({
        workspaceId: "workspace-id",
        watchSession: "7",
        kind: "changed",
      }),
    ).toBe(true);
    expect(
      isWorkspaceWatchEvent({
        workspaceId: "workspace-id",
        watchSession: "7",
        kind: "degraded",
      }),
    ).toBe(true);
    expect(
      isWorkspaceWatchEvent({
        workspaceId: "workspace-id",
        watchSession: "7",
        kind: "changed",
        path: "/private/file",
      }),
    ).toBe(false);
    expect(
      isWorkspaceWatchEvent({
        workspaceId: "workspace-id",
        watchSession: "7",
        kind: "read",
      }),
    ).toBe(false);
    expect(
      isWorkspaceWatchEvent({
        workspaceId: "x".repeat(65),
        watchSession: "7",
        kind: "changed",
      }),
    ).toBe(false);
  });

  it("rejects stale events from another open-Workspace session", () => {
    const event = {
      workspaceId: "current",
      watchSession: "7",
      kind: "changed" as const,
    };
    expect(isCurrentWorkspaceWatchEvent(event, "current", "7")).toBe(true);
    expect(isCurrentWorkspaceWatchEvent(event, "current", "8")).toBe(false);
    expect(isCurrentWorkspaceWatchEvent(event, "previous", "7")).toBe(false);
    expect(isCurrentWorkspaceWatchEvent(event, "", "7")).toBe(false);
  });

  it("defers automatic refresh for every dirty or busy state", () => {
    const clean = {
      busy: false,
      dirtyGuide: false,
      dirtyGuideDetails: false,
      dirtyCategoryRename: false,
      dirtyCategoryCreate: false,
      dirtyGuideCreate: false,
      dirtyGitCommit: false,
    };
    expect(automaticRefreshBlocked(clean)).toBe(false);
    for (const key of Object.keys(clean) as Array<keyof typeof clean>) {
      expect(automaticRefreshBlocked({ ...clean, [key]: true })).toBe(true);
    }
  });

  it("refuses a refresh result after editing begins in flight", () => {
    expect(refreshResultMayApply(7, 7)).toBe(true);
    expect(refreshResultMayApply(7, 8)).toBe(false);
  });

  it("labels Guide archive and restore states explicitly", () => {
    expect(guideLifecycleLabel("Draft", false)).toBe("Archive Guide");
    expect(guideLifecycleLabel("Stale", true)).toBe("Archiving…");
    expect(guideLifecycleLabel("Archived", false)).toBe("Restore Guide");
    expect(guideLifecycleLabel("Archived", true)).toBe("Restoring…");
  });

  it("offers tested approval only for active unverified Guides", () => {
    expect(canMarkGuideTested("Draft")).toBe(true);
    expect(canMarkGuideTested("Stale")).toBe(true);
    expect(canMarkGuideTested("Tested")).toBe(false);
    expect(canMarkGuideTested("Archived")).toBe(false);
  });

  it("normalizes and bounds Workspace names", () => {
    expect(normalizedWorkspaceName("  Personal Rehow  ")).toBe(
      "Personal Rehow",
    );
    expect(normalizedWorkspaceName("   ")).toBeNull();
    expect(normalizedWorkspaceName("a".repeat(200))).toHaveLength(200);
    expect(normalizedWorkspaceName("a".repeat(201))).toBeNull();
  });

  it("accepts only structured Workspace initialization errors", () => {
    expect(
      nativeCommandError({ code: "conflict", message: "Changed." }),
    ).toEqual({ code: "conflict", message: "Changed." });
    expect(nativeCommandError("hostile content").code).toBe("unknown");
  });

  it("normalizes bounded Category details", () => {
    expect(normalizedCategoryDetails("  Linux  ", "  System work  ")).toEqual({
      name: "Linux",
      summary: "System work",
    });
    expect(normalizedCategoryDetails("Linux", "   ")).toEqual({
      name: "Linux",
      summary: null,
    });
    expect(normalizedCategoryDetails("", "Summary")).toBeNull();
    expect(normalizedCategoryDetails("Linux", "x".repeat(501))).toBeNull();
  });

  it("moves Categories without changing their records", () => {
    const categories = [{ id: "one" }, { id: "two" }, { id: "three" }];
    expect(
      movedCategoryOrder(categories, "two", "up")?.map(({ id }) => id),
    ).toEqual(["two", "one", "three"]);
    expect(
      movedCategoryOrder(categories, "two", "down")?.map(({ id }) => id),
    ).toEqual(["one", "three", "two"]);
    expect(movedCategoryOrder(categories, "one", "up")).toBeNull();
    expect(categories.map(({ id }) => id)).toEqual(["one", "two", "three"]);
  });

  it("normalizes bounded draft Guide details", () => {
    expect(
      normalizedGuideDetails(
        "category-id",
        "  Install Tool  ",
        "  Do it safely  ",
      ),
    ).toEqual({
      categoryId: "category-id",
      title: "Install Tool",
      summary: "Do it safely",
    });
    expect(normalizedGuideDetails("", "Title", "Summary")).toBeNull();
    expect(normalizedGuideDetails("category-id", "", "Summary")).toBeNull();
    expect(normalizedGuideDetails("category-id", "Title", "")).toBeNull();
    expect(
      normalizedGuideDetails("category-id", "Title", "x".repeat(501)),
    ).toBeNull();
  });

  it("normalizes bounded Guide metadata edits", () => {
    expect(
      normalizedGuideMetadata(
        "  New Title  ",
        "  New summary  ",
        " linux\nwayland ",
        "manual install\n",
        "category-id",
      ),
    ).toEqual({
      title: "New Title",
      summary: "New summary",
      tags: ["linux", "wayland"],
      aliases: ["manual install"],
      categoryId: "category-id",
    });
    expect(
      normalizedGuideMetadata("", "Summary", "", "", "category-id"),
    ).toBeNull();
    expect(
      normalizedGuideMetadata("Title", "", "", "", "category-id"),
    ).toBeNull();
    expect(
      normalizedGuideMetadata(
        "x".repeat(201),
        "Summary",
        "",
        "",
        "category-id",
      ),
    ).toBeNull();
    expect(normalizedGuideMetadata("Title", "Summary", "", "", "")).toBeNull();
  });

  it("normalizes bounded one-per-line Tags and aliases", () => {
    expect(normalizedGuideTerms(" linux \n\nWayland\n", 64)).toEqual([
      "linux",
      "Wayland",
    ]);
    expect(normalizedGuideTerms("Linux\nlinux", 64)).toEqual([
      "Linux",
      "linux",
    ]);
    expect(normalizedGuideTerms(`${"x".repeat(65)}`, 64)).toBeNull();
    expect(normalizedGuideTerms(`${"x\n".repeat(65)}`, 64)).toBeNull();
  });
});

describe("application shell semantics", () => {
  it("renders named search, live results, navigation toggle, and Workspace onboarding", () => {
    const markup = renderToStaticMarkup(createElement(App));
    expect(markup).toContain("Search Guides</span>");
    expect(markup).toContain("Guide search scope</span>");
    expect(markup).toContain('<option value="archived">Archived</option>');
    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('aria-controls="workspace-navigation"');
    expect(markup).toContain("Open Workspace");
    expect(markup).toContain("Choose a Rehow Workspace");
    expect(markup).toContain("No Workspace open");
    expect(markup).toContain('aria-label="Refresh Workspace"');
    expect(markup).toContain('translate="no"');
  });
});

describe("Guide search", () => {
  const guides = [
    {
      id: "one",
      title: "Create a launcher",
      summary: "Desktop entry",
      categoryId: "desktop",
      category: "Desktop",
      status: "Draft" as const,
      updatedAt: "2026-09-18T00:00:00Z",
      tags: ["desktop"],
      aliases: ["launcher"],
      attachments: [],
      matches: "Exec icon",
    },
    {
      id: "two",
      title: "Add a udev rule",
      summary: "USB device",
      categoryId: "hardware",
      category: "Hardware",
      status: "Tested" as const,
      updatedAt: "2026-09-18T00:00:00Z",
      tags: ["hardware"],
      aliases: ["usb rule"],
      attachments: [],
      matches: "vendor id",
    },
    {
      id: "three",
      title: "Old keyboard map",
      summary: "Archived layout",
      categoryId: "hardware",
      category: "Hardware",
      status: "Archived" as const,
      updatedAt: "2026-09-17T00:00:00Z",
      tags: ["keyboard"],
      aliases: ["key map"],
      attachments: [],
      matches: "xkb",
    },
  ];

  it("matches all case-insensitive terms across searchable fields", () => {
    expect(
      filterGuides(guides, "DESKTOP exec").map((guide) => guide.id),
    ).toEqual(["one"]);
  });

  it("returns the full list for whitespace and none for an absent term", () => {
    expect(filterGuides(guides, "   ")).toHaveLength(3);
    expect(filterGuides(guides, "limine")).toEqual([]);
  });

  it("applies backend rank order and ignores stale IDs", () => {
    expect(
      rankedGuides(guides, "body match", [
        {
          guideId: "two",
          excerpt: "vendor id",
          excerptField: "body",
          matchedFields: ["body"],
          highlights: [{ start: 0, end: 6 }],
        },
        {
          guideId: "missing",
          excerpt: "missing",
          excerptField: "body",
          matchedFields: ["body"],
          highlights: [{ start: 0, end: 7 }],
        },
        {
          guideId: "one",
          excerpt: "exec icon",
          excerptField: "body",
          matchedFields: ["body"],
          highlights: [{ start: 0, end: 4 }],
        },
      ]).map((guide) => guide.id),
    ).toEqual(["two", "one"]);
  });

  it("applies explicit status scopes", () => {
    expect(
      guides.filter((guide) => guideMatchesSearchScope(guide.status, "active")),
    ).toHaveLength(2);
    expect(
      guides
        .filter((guide) => guideMatchesSearchScope(guide.status, "tested"))
        .map((guide) => guide.id),
    ).toEqual(["two"]);
    expect(
      guides
        .filter((guide) => guideMatchesSearchScope(guide.status, "archived"))
        .map((guide) => guide.id),
    ).toEqual(["three"]);
  });

  it("keeps query and non-default scope in the restorable route", () => {
    expect(
      searchStatePath(
        "/index.html",
        "?theme=dark&q=old",
        "#guide-content",
        "permission denied",
        "archived",
      ),
    ).toBe(
      "/index.html?theme=dark&q=permission+denied&scope=archived#guide-content",
    );
    expect(
      searchStatePath("/index.html", "?scope=stale&q=x", "", "", "active"),
    ).toBe("/index.html");
  });

  it("validates bounded plain excerpts and slices Unicode scalar highlights", () => {
    const hit = {
      guideId: "guide-id",
      excerpt: "café permission denied",
      excerptField: "error" as const,
      matchedFields: ["error" as const, "body" as const],
      highlights: [
        { start: 0, end: 4 },
        { start: 5, end: 22 },
      ],
    };
    expect(isSearchHitPayload(hit)).toBe(true);
    expect(highlightedExcerptSegments(hit)).toEqual([
      { start: 0, end: 4, text: "café", highlighted: true },
      { start: 4, end: 5, text: " ", highlighted: false },
      { start: 5, end: 22, text: "permission denied", highlighted: true },
    ]);
    expect(isSearchHitPayload({ ...hit, html: "<mark>unsafe</mark>" })).toBe(
      false,
    );
    expect(
      isSearchHitPayload({ ...hit, highlights: [{ start: 0, end: 99 }] }),
    ).toBe(false);
    expect(isSearchHitPayload({ ...hit, excerptField: "filesystem" })).toBe(
      false,
    );
  });

  it("keeps 5,000-Guide scope and 100-excerpt transformation below one main-thread budget", () => {
    const corpus = Array.from({ length: 5_000 }, (_, index) => {
      const guide = guides[index % guides.length];
      if (guide === undefined) throw new Error("fixture is empty");
      return { ...guide, id: `guide-${index}` };
    });
    const hit = {
      guideId: "guide",
      excerpt:
        "run tar -xf archive.tar.gz then verify the installed application",
      excerptField: "command" as const,
      matchedFields: ["command" as const],
      highlights: [{ start: 4, end: 26 }],
    };
    const started = performance.now();
    corpus.filter((guide) => guideMatchesSearchScope(guide.status, "active"));
    for (let index = 0; index < 100; index += 1) {
      highlightedExcerptSegments(hit);
    }
    expect(performance.now() - started).toBeLessThan(50);
  });
});

describe("safe Guide rendering", () => {
  it("renders Guide context as escaped text and guarded actions", () => {
    const markup = renderToStaticMarkup(
      createElement(GuideContext, {
        guide: {
          id: "guide-id",
          title: "Install a Tarball",
          summary: "Install a downloaded archive.",
          categoryId: "category-id",
          tags: ["linux"],
          aliases: ["manual install"],
          revision: `rev:v1:${"11".repeat(32)}`,
          body: "",
          document: { version: 1, events: [] },
          createdAt: "2026-09-05T10:03:00Z",
          lastVerifiedAt: "2026-09-23T10:03:00Z",
          appliesTo: [
            {
              platform: "Arch <Linux>",
              versions: "2026",
              notes: "Wayland & Hyprland",
            },
          ],
          prerequisites: ["Back up <config>"],
          warnings: ["Never run <script>alert(1)</script>"],
          filesAffected: ["~/.config/<unsafe>"],
          relatedGuideIds: ["related-id", "missing-id"],
          references: [
            {
              title: "Reference <manual>",
              url: "https://example.com/?q=<unsafe>",
            },
          ],
        },
        guides: [
          {
            id: "related-id",
            title: "Related <Guide>",
            summary: "Summary",
            categoryId: "category-id",
            category: "Linux",
            status: "Draft" as const,
            updatedAt: "2026-09-23T10:03:00Z",
            tags: [],
            aliases: [],
            attachments: [],
            matches: "",
          },
        ],
        openingExternalLink: "",
        onOpenExternalLink: () => undefined,
        onOpenRelatedGuide: () => undefined,
      }),
    );

    expect(markup).toContain("Warnings");
    expect(markup).toContain("Never run &lt;script&gt;alert(1)&lt;/script&gt;");
    expect(markup).toContain("~/.config/&lt;unsafe&gt;");
    expect(markup).toContain("Related &lt;Guide&gt;");
    expect(markup).toContain("Reference &lt;manual&gt;");
    expect(markup).toContain("Unavailable Guide");
    expect(markup).not.toContain("<script>");
    expect(markup).not.toContain("href=");
  });

  it("compares an external Guide with the local draft as inert text", () => {
    const markup = renderToStaticMarkup(
      createElement(ExternalGuideComparison, {
        draft: {
          title: "Draft <title>",
          summary: "Local summary",
          category: "Linux",
          tags: ["local"],
          aliases: [],
          body: '<script src="file:///tmp/local">local()</script>',
        },
        disk: {
          id: "guide-id",
          title: "Disk <title>",
          summary: "Disk summary",
          categoryId: "category-id",
          tags: ["disk"],
          aliases: [],
          revision: `rev:v1:${"22".repeat(32)}`,
          body: '<img src="file:///tmp/disk">',
          document: { version: 1, events: [] },
          createdAt: "2026-09-05T10:03:00Z",
          lastVerifiedAt: null,
          appliesTo: [],
          prerequisites: [],
          warnings: [],
          filesAffected: [],
          relatedGuideIds: [],
          references: [],
        },
        diskCategory: "Linux",
        bodyChanged: true,
        detailsChanged: true,
      }),
    );
    expect(markup).toContain("Draft &lt;title&gt;");
    expect(markup).toContain("Disk &lt;title&gt;");
    expect(markup).toContain("Your Draft Content");
    expect(markup).toContain("Current on Disk");
    expect(markup).not.toContain("<script");
    expect(markup).not.toContain("<img");
  });

  it("renders bounded Workspace issue recovery guidance as inert text", () => {
    const issue = {
      kind: "missingCategoryDescriptor" as const,
      relativePath: "categories/<interrupted>/category.yaml",
    };
    const markup = renderToStaticMarkup(
      createElement(WorkspaceIssuesContent, {
        issueCount: 101,
        issues: [issue],
        issuesTruncated: true,
        refreshing: false,
        onClose: () => undefined,
        onRefresh: () => undefined,
      }),
    );
    expect(workspaceIssuePresentation(issue.kind).detail).toContain(
      "interrupted Category creation",
    );
    expect(markup).toContain("Category Descriptor Missing");
    expect(markup).toContain("categories/&lt;interrupted&gt;/category.yaml");
    expect(markup).toContain("Fix these first, then refresh for more.");
    expect(markup).toContain("Refresh Workspace");
    expect(markup).not.toContain("dangerouslySetInnerHTML");
  });

  it("renders Attachment preview content as inert text", () => {
    const markup = renderToStaticMarkup(
      createElement(AttachmentTextPreviewContent, {
        preview: {
          title: "Hostile <title>",
          text: '<script src="file:///tmp/leak">run()</script>',
          sizeBytes: 48,
          sanitized: true,
        },
        onClose: () => undefined,
      }),
    );
    expect(markup).toContain("Hostile &lt;title&gt;");
    expect(markup).toContain(
      "&lt;script src=&quot;file:///tmp/leak&quot;&gt;run()&lt;/script&gt;",
    );
    expect(markup).toContain("replacement symbols");
    expect(markup).not.toContain("<script");
    expect(markup).not.toContain("<a ");
    expect(markup).not.toContain("<img");
  });

  it("fails closed for an unsupported safe-document version", () => {
    const markup = renderToStaticMarkup(
      createElement(MarkdownDocument, {
        document: { version: 2, events: [] },
        onAnnouncement: () => undefined,
        onOpenExternalLink: () => undefined,
        openingExternalLink: "",
      }),
    );
    expect(markup).toContain('role="alert"');
    expect(markup).toContain("unsupported content version");
  });

  it("escapes HTML, blocks navigation and media, and exposes code copy controls", () => {
    const markup = renderToStaticMarkup(
      createElement(MarkdownDocument, {
        document: {
          version: 1,
          events: [
            { kind: "start", node: { kind: "htmlBlock" } },
            { kind: "htmlLiteral", value: "<script>alert(1)</script>" },
            { kind: "end" },
            {
              kind: "start",
              node: {
                kind: "link",
                destination: "https://example.com/docs",
                title: null,
              },
            },
            { kind: "text", value: "safe link" },
            { kind: "end" },
            {
              kind: "start",
              node: { kind: "link", destination: null, title: null },
            },
            { kind: "text", value: "bad link" },
            { kind: "end" },
            { kind: "start", node: { kind: "image", title: null } },
            { kind: "text", value: "tracker" },
            { kind: "end" },
            {
              kind: "start",
              node: { kind: "codeBlock", language: "sh" },
            },
            { kind: "text", value: "printf ok" },
            { kind: "end" },
          ],
        },
        onAnnouncement: () => undefined,
        onOpenExternalLink: () => undefined,
        openingExternalLink: "",
      }),
    );
    expect(markup).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(markup).toContain("Unsafe or unsupported link blocked");
    expect(markup).toContain('type="button"');
    expect(markup).toContain(
      "Open https://example.com/docs in the desktop handler",
    );
    expect(markup).toContain("Image blocked: ");
    expect(markup).toContain('aria-label="Copy code"');
    expect(markup).not.toContain("dangerouslySetInnerHTML");
    expect(markup).not.toContain("href=");
    expect(markup).not.toContain("src=");
  });
});
