import { describe, expect, it } from "vitest";
import {
  gitSyncPresentation,
  highlightedExcerptSegments,
  isAttachmentExportPayload,
  isAttachmentRevealPayload,
  isAttachmentTextPreviewPayload,
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
  isGitIdentityPlanPayload,
  isGitIdentityResultPayload,
  isGitInitPlanPayload,
  isGitInitResultPayload,
  isGitPushResultPayload,
  isGitRemotePlanPayload,
  isGitRemoteResultPayload,
  isGitStatusPayload,
  isGitSyncApplyResultPayload,
  isGitSyncCancelResultPayload,
  isGitSyncInspectionPayload,
  isMarkdownImportApplyPayload,
  isMarkdownImportPreviewPayload,
  isRecentWorkspaceResult,
  isSearchHitPayload,
  isWorkspaceWatchEvent,
  normalizedCloneUrl,
  normalizedGitCommitMessage,
  normalizedRemoteName,
  normalizedRemoteUrl,
} from "./payloads";

describe("presentation helpers and native payload validators", () => {
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

  it("validates bounded Git status", () => {
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
      identity: "configured" as const,
      globalIdentity: "available" as const,
      failure: null,
      sandbox: "bubblewrap" as const,
    };
    expect(isGitStatusPayload(status)).toBe(true);
    expect(isGitStatusPayload({ ...status, sandbox: "unsandboxed" })).toBe(
      true,
    );
    expect(isGitStatusPayload({ ...status, sandbox: "chroot" })).toBe(false);
    expect(isGitStatusPayload({ ...status, unexpected: true })).toBe(false);
    expect(isGitStatusPayload({ ...status, remoteCount: -1 })).toBe(false);
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

  it("accepts only closed Git identity payloads", () => {
    const token = `plan:v1:${"A".repeat(43)}`;
    expect(
      isGitIdentityPlanPayload({ planToken: token, source: "global" }),
    ).toBe(true);
    expect(
      isGitIdentityPlanPayload({
        planToken: token,
        source: "global",
        email: "private@example.test",
      }),
    ).toBe(false);
    expect(isGitIdentityResultPayload({ configured: true })).toBe(true);
    expect(
      isGitIdentityResultPayload({ configured: true, path: "/private" }),
    ).toBe(false);
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
    const diagnosed = (diagnostic: unknown) =>
      isGitCloneCompleteResultPayload({
        kind: "completed",
        workspace: {
          ...workspace,
          issueCount: 1,
          issues: [
            {
              kind: "malformedGuide",
              relativePath: "categories/net/notes.md",
              diagnostic,
              field: "title",
              line: 3,
              column: 7,
            },
          ],
          issuesTruncated: false,
        },
      });
    expect(diagnosed("yamlSyntax")).toBe(true);
    expect(diagnosed("<script>")).toBe(false);
    expect(diagnosed(42)).toBe(false);

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
              diagnostic: null,
              field: null,
              line: null,
              column: null,
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
              diagnostic: null,
              field: null,
              line: null,
              column: null,
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
});

describe("Guide search", () => {
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

  it("keeps 100-excerpt transformation below one main-thread budget", () => {
    const hit = {
      guideId: "guide",
      excerpt:
        "run tar -xf archive.tar.gz then verify the installed application",
      excerptField: "command" as const,
      matchedFields: ["command" as const],
      highlights: [{ start: 4, end: 26 }],
    };
    const started = performance.now();
    for (let index = 0; index < 100; index += 1) {
      highlightedExcerptSegments(hit);
    }
    expect(performance.now() - started).toBeLessThan(50);
  });
});
