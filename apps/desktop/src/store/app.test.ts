import { describe, expect, it } from "vitest";
import type {
  GuideBodyPayload,
  GuideSummary,
  SearchHitPayload,
  WorkspacePayload,
} from "@/api/payloads";
import { draftIsDirty, useApp, visibleGuides } from "./app";

type VisibleState = Parameters<typeof visibleGuides>[0];

function guide(
  id: string,
  overrides: Partial<GuideSummary> = {},
): GuideSummary {
  return {
    id,
    origin: "managed",
    title: id,
    summary: "",
    categoryId: "linux",
    category: "Linux",
    status: "Draft",
    updatedAt: "2026-09-01T00:00:00Z",
    tags: [],
    aliases: [],
    attachments: [],
    matches: "",
    ...overrides,
  };
}

function workspace(guides: GuideSummary[]): WorkspacePayload {
  return {
    id: "workspace",
    name: "Workspace",
    categories: [],
    guides,
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
    watchSession: "session",
  };
}

function hit(guideId: string): SearchHitPayload {
  return {
    guideId,
    excerpt: guideId,
    excerptField: "body",
    matchedFields: ["body"],
    highlights: [],
  };
}

const guides = [
  guide("launcher", {
    title: "Create a launcher",
    summary: "Desktop entry",
    categoryId: "desktop",
    category: "Desktop",
    status: "Draft",
    updatedAt: "2026-09-18T00:00:00Z",
    tags: ["desktop"],
    matches: "Exec icon",
  }),
  guide("udev", {
    title: "Add a udev rule",
    summary: "USB device",
    categoryId: "hardware",
    category: "Hardware",
    status: "Tested",
    updatedAt: "2026-09-20T00:00:00Z",
    tags: ["hardware", "usb"],
    matches: "vendor id",
  }),
  guide("keyboard", {
    title: "Old keyboard map",
    summary: "Archived layout",
    categoryId: "hardware",
    category: "Hardware",
    status: "Archived",
    updatedAt: "2026-09-25T00:00:00Z",
    tags: ["hardware"],
    matches: "xkb",
  }),
  guide("bluetooth", {
    title: "Pair bluetooth",
    summary: "Headset",
    categoryId: "hardware",
    category: "Hardware",
    status: "Stale",
    updatedAt: "2026-09-10T00:00:00Z",
    tags: ["usb"],
    matches: "bluetoothctl",
  }),
];

function state(overrides: Partial<VisibleState> = {}): VisibleState {
  return {
    workspace: workspace(guides),
    query: "",
    scope: "active",
    categoryFilter: null,
    tagFilter: null,
    sort: "updated",
    hits: null,
    ...overrides,
  };
}

const ids = (list: GuideSummary[]) => list.map((item) => item.id);

describe("visibleGuides", () => {
  it("returns nothing without a Workspace", () => {
    expect(visibleGuides(state({ workspace: null }))).toEqual([]);
  });

  it("excludes Archived Guides from the active scope", () => {
    expect(ids(visibleGuides(state()))).toEqual([
      "udev",
      "launcher",
      "bluetooth",
    ]);
    expect(ids(visibleGuides(state({ scope: "archived" })))).toEqual([
      "keyboard",
    ]);
    expect(ids(visibleGuides(state({ scope: "all" })))).toHaveLength(4);
    expect(ids(visibleGuides(state({ scope: "stale" })))).toEqual([
      "bluetooth",
    ]);
  });

  it("applies Category and Tag filters together", () => {
    expect(ids(visibleGuides(state({ categoryFilter: "hardware" })))).toEqual([
      "udev",
      "bluetooth",
    ]);
    expect(ids(visibleGuides(state({ tagFilter: "usb" })))).toEqual([
      "udev",
      "bluetooth",
    ]);
    expect(
      ids(
        visibleGuides(state({ categoryFilter: "desktop", tagFilter: "usb" })),
      ),
    ).toEqual([]);
  });

  it("keeps backend rank order and ignores stale or filtered IDs", () => {
    expect(
      ids(
        visibleGuides(
          state({
            query: "rule",
            hits: [
              hit("bluetooth"),
              hit("missing"),
              hit("keyboard"),
              hit("udev"),
            ],
          }),
        ),
      ),
    ).toEqual(["bluetooth", "udev"]);
    expect(
      ids(
        visibleGuides(
          state({
            query: "rule",
            tagFilter: "usb",
            hits: [hit("launcher"), hit("udev")],
          }),
        ),
      ),
    ).toEqual(["udev"]);
  });

  it("ignores hits when the query is empty", () => {
    expect(
      ids(visibleGuides(state({ query: "  ", hits: [hit("bluetooth")] }))),
    ).toEqual(["udev", "launcher", "bluetooth"]);
  });

  it("filters on the client while search hits are unavailable", () => {
    expect(
      ids(visibleGuides(state({ query: "DESKTOP exec", hits: null }))),
    ).toEqual(["launcher"]);
    expect(
      ids(visibleGuides(state({ query: "hardware", hits: null }))),
    ).toEqual(["udev", "bluetooth"]);
    expect(visibleGuides(state({ query: "limine", hits: null }))).toEqual([]);
  });

  it("sorts by title, status, and most recently updated", () => {
    expect(ids(visibleGuides(state({ scope: "all", sort: "title" })))).toEqual([
      "udev",
      "launcher",
      "keyboard",
      "bluetooth",
    ]);
    expect(ids(visibleGuides(state({ scope: "all", sort: "status" })))).toEqual(
      ["bluetooth", "launcher", "udev", "keyboard"],
    );
    expect(
      ids(visibleGuides(state({ scope: "all", sort: "updated" }))),
    ).toEqual(["keyboard", "udev", "launcher", "bluetooth"]);
  });

  it("breaks status ties by most recent update", () => {
    const tied = workspace([
      guide("older", { status: "Draft", updatedAt: "2026-01-01T00:00:00Z" }),
      guide("newer", { status: "Draft", updatedAt: "2026-02-01T00:00:00Z" }),
    ]);
    expect(
      ids(visibleGuides(state({ workspace: tied, sort: "status" }))),
    ).toEqual(["newer", "older"]);
  });
});

describe("draftIsDirty", () => {
  const body: GuideBodyPayload = {
    id: "launcher",
    origin: "managed",
    title: "Create a launcher",
    summary: "Desktop entry",
    categoryId: "desktop",
    tags: ["desktop"],
    aliases: ["launcher"],
    revision: `rev:v1:${"11".repeat(32)}`,
    body: "# Launcher\n",
    document: { version: 1, events: [] },
    createdAt: "2026-09-01T00:00:00Z",
    lastVerifiedAt: null,
    appliesTo: [],
    prerequisites: [],
    warnings: [],
    filesAffected: [],
    relatedGuideIds: [],
    references: [],
  };
  const cleanDraft = {
    guideId: body.id,
    title: body.title,
    summary: body.summary,
    categoryId: body.categoryId,
    tags: [...body.tags],
    aliases: [...body.aliases],
    body: body.body,
  };
  const base = {
    ...useApp.getInitialState(),
    workspace: workspace(guides),
    guide: body,
  };

  it("is clean without a draft or loaded Guide", () => {
    expect(draftIsDirty({ ...base, draft: null })).toBe(false);
    expect(draftIsDirty({ ...base, guide: null, draft: cleanDraft })).toBe(
      false,
    );
  });

  it("is clean when the draft matches the Guide", () => {
    expect(draftIsDirty({ ...base, draft: cleanDraft })).toBe(false);
  });

  it("detects edits to every editable field", () => {
    for (const patch of [
      { body: "# Launcher\nchanged\n" },
      { title: "Renamed" },
      { summary: "Changed" },
      { categoryId: "hardware" },
      { tags: ["desktop", "new"] },
      { aliases: [] },
    ]) {
      expect(
        draftIsDirty({ ...base, draft: { ...cleanDraft, ...patch } }),
      ).toBe(true);
    }
  });

  it("compares the Category with the Workspace summary when available", () => {
    const moved = workspace([guide("launcher", { categoryId: "hardware" })]);
    expect(
      draftIsDirty({
        ...base,
        workspace: moved,
        draft: { ...cleanDraft, categoryId: "hardware" },
      }),
    ).toBe(false);
  });
});
