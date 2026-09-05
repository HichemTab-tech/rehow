import { describe, expect, it } from "vitest";
import { APPLICATION_NAME } from "./App";

describe("application identity", () => {
  it("uses the Rehow name", () => {
    expect(APPLICATION_NAME).toBe("Rehow");
  });
});
