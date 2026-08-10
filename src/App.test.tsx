// @vitest-environment jsdom

import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import { fetchSnapshots } from "./lib/bridge";
import type { ProviderSnapshot, WidgetPreferences } from "./types";

const preferences: WidgetPreferences = {
  locked: false,
  alwaysOnTop: true,
  pinnedProvider: null,
  autoRotateSeconds: 12,
  language: "zh-CN",
  paletteColors: ["#eb5b58", "#f1a06f", "#f5d98f", "#e3f4b8", "#b9e4c9"],
};

const snapshot: ProviderSnapshot = {
  provider: "codex",
  displayName: "CODEX",
  plan: "PRO",
  shortWindow: { remainingPercent: 74, resetsAt: null, windowSeconds: 18_000 },
  weeklyWindow: { remainingPercent: 42, resetsAt: null, windowSeconds: 604_800 },
  resetCredits: 0,
  resetCreditExpiresAt: [],
  updatedAt: new Date().toISOString(),
  status: "ok",
  message: null,
};

vi.mock("./lib/bridge", () => ({
  closePalettePreview: vi.fn(async () => {}),
  fetchSnapshots: vi.fn(),
  fetchTokenUsage: vi.fn(async () => ({
    inputTokens: 0,
    cachedInputTokens: 0,
    outputTokens: 0,
    reasoningOutputTokens: 0,
    totalTokens: 0,
    sessionCount: 0,
    updatedAt: new Date().toISOString(),
  })),
  getPreferences: vi.fn(async () => preferences),
  listenDesktopEvents: vi.fn(async () => () => {}),
  listenPalettePreview: vi.fn(async () => () => {}),
  openPalettePreview: vi.fn(async () => {}),
  setAlwaysOnTop: vi.fn(async () => preferences),
  setWidgetClip: vi.fn(async () => {}),
  setWidgetExpanded: vi.fn(async () => {}),
  startDragging: vi.fn(async () => {}),
  syncWidgetCssScale: vi.fn(async () => {}),
  updatePreferences: vi.fn(async () => {}),
}));

describe("quota refresh coordination", () => {
  it("coalesces focus refreshes and does not refresh quota on hover", async () => {
    let resolveSnapshots!: (value: ProviderSnapshot[]) => void;
    const pending = new Promise<ProviderSnapshot[]>((resolve) => {
      resolveSnapshots = resolve;
    });
    vi.mocked(fetchSnapshots).mockReturnValueOnce(pending);

    const view = render(<App />);
    await waitFor(() => expect(fetchSnapshots).toHaveBeenCalledTimes(1));

    fireEvent.focus(window);
    fireEvent.focus(window);
    expect(fetchSnapshots).toHaveBeenCalledTimes(1);

    await act(async () => resolveSnapshots([snapshot]));
    const card = view.container.querySelector("main");
    expect(card).not.toBeNull();
    fireEvent.mouseEnter(card!);
    expect(fetchSnapshots).toHaveBeenCalledTimes(1);
    view.unmount();
  });
});
