import { describe, expect, it } from "vitest";
import { WORKSPACE_ISSUE_KINDS } from "@/api/payloads";
import { ISSUE_TEXT } from "./IssuesDialog";

describe("Workspace issue copy", () => {
  it("has a title and fix for every Workspace issue kind", () => {
    expect(Object.keys(ISSUE_TEXT).sort()).toEqual(
      [...WORKSPACE_ISSUE_KINDS].sort(),
    );
    for (const kind of WORKSPACE_ISSUE_KINDS) {
      expect(ISSUE_TEXT[kind].title.trim()).not.toBe("");
      expect(ISSUE_TEXT[kind].fix.trim()).not.toBe("");
    }
  });
});
