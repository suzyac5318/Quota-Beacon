import { describe, expect, it } from "vitest";
import { nextRefreshSchedule, NORMAL_REFRESH_DELAY_MS } from "./refreshPolicy";

describe("quota refresh policy", () => {
  it("keeps successful refreshes on the normal ten-second cadence", () => {
    expect(nextRefreshSchedule(3, false)).toEqual({ failures: 0, delayMs: NORMAL_REFRESH_DELAY_MS });
  });

  it("backs failures off to thirty, sixty, and one hundred twenty seconds", () => {
    expect(nextRefreshSchedule(0, true)).toEqual({ failures: 1, delayMs: 30_000 });
    expect(nextRefreshSchedule(1, true)).toEqual({ failures: 2, delayMs: 60_000 });
    expect(nextRefreshSchedule(2, true)).toEqual({ failures: 3, delayMs: 120_000 });
  });

  it("retries cold-start failures quickly before entering the normal backoff", () => {
    expect(nextRefreshSchedule(0, true, true)).toEqual({ failures: 1, delayMs: 2_000 });
    expect(nextRefreshSchedule(1, true, true)).toEqual({ failures: 2, delayMs: 5_000 });
    expect(nextRefreshSchedule(2, true, true)).toEqual({ failures: 3, delayMs: 10_000 });
    expect(nextRefreshSchedule(3, true, true)).toEqual({ failures: 4, delayMs: 30_000 });
  });

  it("caps repeated failures at the longest delay", () => {
    expect(nextRefreshSchedule(99, true)).toEqual({ failures: 3, delayMs: 120_000 });
    expect(nextRefreshSchedule(99, true, true)).toEqual({ failures: 6, delayMs: 120_000 });
  });
});
