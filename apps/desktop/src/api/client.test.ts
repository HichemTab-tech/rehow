import { describe, expect, it } from "vitest";
import { toNativeError } from "./client";

describe("toNativeError", () => {
  it("keeps structured native errors", () => {
    const error = toNativeError({ code: "busy", message: "Try again." });
    expect(error.code).toBe("busy");
    expect(error.message).toBe("Try again.");
  });

  it("wraps bounded string errors as failures", () => {
    const error = toNativeError("disk full");
    expect(error.code).toBe("failed");
    expect(error.message).toBe("disk full");
  });

  it("hides unknown and unbounded errors behind generic copy", () => {
    for (const value of [
      undefined,
      null,
      42,
      "",
      "x".repeat(500),
      new Error("leak /home/user"),
    ]) {
      const error = toNativeError(value);
      expect(error.code).toBe("unknown");
      expect(error.message).not.toContain("/home/user");
    }
  });

  it("returns an existing native error unchanged", () => {
    const first = toNativeError("disk full");
    expect(toNativeError(first)).toBe(first);
  });
});
