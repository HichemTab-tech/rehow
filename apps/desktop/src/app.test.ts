import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  APPLICATION_NAME,
  App,
  automaticRefreshBlocked,
  canMarkGuideTested,
  filterGuides,
  gitFactLabel,
  gitStatusLabel,
  guideLifecycleLabel,
  guideMatchesSearchScope,
  hasUnsavedChanges,
  highlightedExcerptSegments,
  isCurrentWorkspaceWatchEvent,
  isGitInitPlanPayload,
  isGitInitResultPayload,
  isGitRemotePlanPayload,
  isGitRemoteResultPayload,
  isGitStatusPayload,
  isSearchHitPayload,
  isWorkspaceWatchEvent,
  movedCategoryOrder,
  nativeCommandError,
  nextResultIndex,
  normalizedCategoryDetails,
  normalizedGuideDetails,
  normalizedGuideMetadata,
  normalizedGuideTerms,
  normalizedRemoteName,
  normalizedRemoteUrl,
  normalizedWorkspaceName,
  rankedGuides,
  refreshedGuideId,
  refreshResultMayApply,
  resultCountLabel,
  searchStatePath,
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
  it("fails closed for an unsupported safe-document version", () => {
    const markup = renderToStaticMarkup(
      createElement(MarkdownDocument, {
        document: { version: 2, events: [] },
        onAnnouncement: () => undefined,
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
      }),
    );
    expect(markup).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(markup).toContain("Unsafe or unsupported link blocked");
    expect(markup).toContain("Image blocked: ");
    expect(markup).toContain('aria-label="Copy code"');
    expect(markup).not.toContain("dangerouslySetInnerHTML");
    expect(markup).not.toContain("href=");
    expect(markup).not.toContain("src=");
  });
});
