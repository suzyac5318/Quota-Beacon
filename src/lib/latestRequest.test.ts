import { describe, expect, it } from "vitest";
import { createLatestRequestGate } from "./latestRequest";

describe("latest request gate", () => {
  it("invalidates an older resize when a newer target arrives", () => {
    const gate = createLatestRequestGate();
    const expand = gate.begin();
    const collapse = gate.begin();

    expect(gate.isCurrent(expand)).toBe(false);
    expect(gate.isCurrent(collapse)).toBe(true);
  });
});
