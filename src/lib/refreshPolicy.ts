export const NORMAL_REFRESH_DELAY_MS = 10_000;
export const STARTUP_FAILURE_REFRESH_DELAYS_MS = [2_000, 5_000, 10_000] as const;
export const FAILURE_REFRESH_DELAYS_MS = [30_000, 60_000, 120_000] as const;

export interface RefreshSchedule {
  failures: number;
  delayMs: number;
}

export function nextRefreshSchedule(previousFailures: number, failed: boolean, coldStart = false): RefreshSchedule {
  if (!failed) return { failures: 0, delayMs: NORMAL_REFRESH_DELAY_MS };
  const delays = coldStart
    ? [...STARTUP_FAILURE_REFRESH_DELAYS_MS, ...FAILURE_REFRESH_DELAYS_MS]
    : FAILURE_REFRESH_DELAYS_MS;
  const failures = Math.min(Math.max(0, previousFailures) + 1, delays.length);
  return {
    failures,
    delayMs: delays[failures - 1],
  };
}
