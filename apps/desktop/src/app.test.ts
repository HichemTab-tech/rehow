import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  APPLICATION_NAME,
  App,
  filterGuides,
  nextResultIndex,
  resultCountLabel,
} from "./App";

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
});

describe("application shell semantics", () => {
  it("renders named search, live results, navigation toggle, and Workspace onboarding", () => {
    const markup = renderToStaticMarkup(createElement(App));
    expect(markup).toContain("Search Guides</span>");
    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('aria-controls="workspace-navigation"');
    expect(markup).toContain("Open Workspace");
    expect(markup).toContain("No Workspace open");
    expect(markup).toContain('translate="no"');
  });
});

describe("Guide search", () => {
  const guides = [
    {
      id: "one",
      title: "Create a launcher",
      summary: "Desktop entry",
      category: "Desktop",
      status: "Draft" as const,
      updatedAt: "2026-09-18T00:00:00Z",
      matches: "Exec icon",
    },
    {
      id: "two",
      title: "Add a udev rule",
      summary: "USB device",
      category: "Hardware",
      status: "Tested" as const,
      updatedAt: "2026-09-18T00:00:00Z",
      matches: "vendor id",
    },
  ];

  it("matches all case-insensitive terms across searchable fields", () => {
    expect(
      filterGuides(guides, "DESKTOP exec").map((guide) => guide.id),
    ).toEqual(["one"]);
  });

  it("returns the full list for whitespace and none for an absent term", () => {
    expect(filterGuides(guides, "   ")).toHaveLength(2);
    expect(filterGuides(guides, "limine")).toEqual([]);
  });
});
