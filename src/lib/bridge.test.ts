import { describe, expect, it } from "vitest";
import { normalizeRefreshRequestMode } from "./bridge";

describe("desktop refresh events", () => {
  it("preserves forced account relogin refreshes", () => {
    expect(normalizeRefreshRequestMode("account-relogin")).toBe("account-relogin");
    expect(normalizeRefreshRequestMode("manual")).toBe("manual");
    expect(normalizeRefreshRequestMode("unexpected")).toBe("auto");
  });
});
