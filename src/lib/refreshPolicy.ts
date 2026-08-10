export const NORMAL_REFRESH_DELAY_MS = 10_000;
export const FAILURE_REFRESH_DELAYS_MS = [30_000, 60_000, 120_000] as const;

export interface RefreshSchedule {
  failures: number;
  delayMs: number;
}

export function nextRefreshSchedule(previousFailures: number, failed: boolean): RefreshSchedule {
  if (!failed) return { failures: 0, delayMs: NORMAL_REFRESH_DELAY_MS };
  const failures = Math.min(Math.max(0, previousFailures) + 1, FAILURE_REFRESH_DELAYS_MS.length);
  return {
    failures,
    delayMs: FAILURE_REFRESH_DELAYS_MS[failures - 1],
  };
}
