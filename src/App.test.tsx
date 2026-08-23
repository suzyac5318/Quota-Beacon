// @vitest-environment jsdom

import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

const accountHarness = vi.hoisted(() => ({
  handlers: null as null | { onSwitched?: () => void; onClosed?: () => void },
  desktopHandlers: null as null | { onRefresh: (mode: "auto" | "manual" | "account-relogin") => void; onFocusLost: () => void },
}));

vi.mock("./lib/accounts", () => ({
  closeAccountSwitcher: vi.fn(async () => {}),
  getAccountVault: vi.fn(async () => ({ profiles: [], activeProfileId: null, hasCurrentLogin: true, currentLoginSaved: false })),
  listenAccountEvents: vi.fn(async (handlers: { onSwitched?: () => void; onClosed?: () => void }) => {
    accountHarness.handlers = handlers;
    return () => {};
  }),
  openAccountSwitcher: vi.fn(async () => ({ profiles: [], activeProfileId: null, hasCurrentLogin: true, currentLoginSaved: false })),
  updateAccountSwitcherTheme: vi.fn(async () => {}),
}));

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
  listenDesktopEvents: vi.fn(async (handlers: { onRefresh: (mode: "auto" | "manual" | "account-relogin") => void; onFocusLost: () => void }) => {
    accountHarness.desktopHandlers = handlers;
    return () => {};
  }),
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
  beforeEach(() => {
    vi.clearAllMocks();
    accountHarness.handlers = null;
    accountHarness.desktopHandlers = null;
  });

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

  it("clears the previous account quota and refreshes after an account switch", async () => {
    let resolveNext!: (value: ProviderSnapshot[]) => void;
    const nextSnapshot = { ...snapshot, shortWindow: { ...snapshot.shortWindow!, remainingPercent: 63 } };
    vi.mocked(fetchSnapshots)
      .mockResolvedValueOnce([snapshot])
      .mockReturnValueOnce(new Promise<ProviderSnapshot[]>((resolve) => { resolveNext = resolve; }));

    const view = render(<App />);
    await waitFor(() => expect(view.getAllByText("74")).toHaveLength(2));
    await waitFor(() => expect(accountHarness.handlers).not.toBeNull());

    act(() => accountHarness.handlers?.onSwitched?.());
    await waitFor(() => expect(view.container.querySelector(".loading-card")).not.toBeNull());
    expect(fetchSnapshots).toHaveBeenCalledTimes(2);

    await act(async () => resolveNext([nextSnapshot]));
    await waitFor(() => expect(view.getAllByText("63")).toHaveLength(2));
    view.unmount();
  });

  it("queues a forced refresh after an active account relogin", async () => {
    let resolveInitial!: (value: ProviderSnapshot[]) => void;
    const refreshedSnapshot = { ...snapshot, shortWindow: { ...snapshot.shortWindow!, remainingPercent: 63 } };
    vi.mocked(fetchSnapshots)
      .mockReturnValueOnce(new Promise<ProviderSnapshot[]>((resolve) => { resolveInitial = resolve; }))
      .mockResolvedValueOnce([refreshedSnapshot]);

    const view = render(<App />);
    await waitFor(() => expect(fetchSnapshots).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(accountHarness.desktopHandlers).not.toBeNull());

    act(() => accountHarness.desktopHandlers?.onRefresh("account-relogin"));
    expect(fetchSnapshots).toHaveBeenCalledTimes(1);

    await act(async () => resolveInitial([snapshot]));
    await waitFor(() => expect(fetchSnapshots).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(view.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("63"));
    view.unmount();
  });

  it("toggles the account switcher from the account button", async () => {
    const { closeAccountSwitcher, openAccountSwitcher } = await import("./lib/accounts");
    vi.mocked(fetchSnapshots).mockResolvedValue([snapshot]);
    const view = render(<App />);
    await waitFor(() => expect(view.getAllByText("74")).toHaveLength(2));

    const accountButton = view.getByRole("button", { name: /CODEX/i });
    fireEvent.click(accountButton);
    await waitFor(() => expect(openAccountSwitcher).toHaveBeenCalledTimes(1));
    expect(openAccountSwitcher).toHaveBeenCalledWith({ percent: 74, colors: preferences.paletteColors });
    fireEvent.click(accountButton);
    await waitFor(() => expect(closeAccountSwitcher).toHaveBeenCalledTimes(1));
    view.unmount();
  });

  it("closes the account switcher when the app loses focus", async () => {
    const { closeAccountSwitcher } = await import("./lib/accounts");
    vi.mocked(fetchSnapshots).mockResolvedValue([snapshot]);
    const view = render(<App />);
    await waitFor(() => expect(view.getAllByText("74")).toHaveLength(2));
    await waitFor(() => expect(accountHarness.desktopHandlers).not.toBeNull());

    fireEvent.click(view.getByRole("button", { name: /CODEX/i }));
    await waitFor(() => expect(accountHarness.handlers).not.toBeNull());
    act(() => accountHarness.desktopHandlers?.onFocusLost());
    await waitFor(() => expect(closeAccountSwitcher).toHaveBeenCalledTimes(1));
    view.unmount();
  });

  it("closes the account switcher when another widget area is pressed", async () => {
    const { closeAccountSwitcher, openAccountSwitcher } = await import("./lib/accounts");
    vi.mocked(fetchSnapshots).mockResolvedValue([snapshot]);
    const view = render(<App />);
    await waitFor(() => expect(view.getAllByText("74")).toHaveLength(2));

    fireEvent.click(view.getByRole("button", { name: /CODEX/i }));
    await waitFor(() => expect(openAccountSwitcher).toHaveBeenCalledTimes(1));
    fireEvent.pointerDown(view.getByRole("progressbar"));
    await waitFor(() => expect(closeAccountSwitcher).toHaveBeenCalledTimes(1));
    view.unmount();
  });
});
