// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AccountSwitcher } from "./AccountSwitcher";

const vault = {
  profiles: [
    { id: "one", alias: "个人号", maskedEmail: "p***@example.com", isActive: true, credentialStatus: "ready" as const },
    { id: "two", alias: "工作号", maskedEmail: "w***@example.com", isActive: false, credentialStatus: "ready" as const },
  ],
  activeProfileId: "one",
  hasCurrentLogin: true,
  currentLoginSaved: true,
};

const mocks = vi.hoisted(() => ({
  getAccountVault: vi.fn(),
  getAccountWeeklyQuotas: vi.fn(),
  accountHandlers: null as null | { onSwitched?: () => void; onError?: (message: string) => void },
}));

vi.mock("../lib/bridge", () => ({
  getPreferences: vi.fn(async () => ({ language: "zh-CN" })),
}));

vi.mock("../lib/accounts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/accounts")>();
  return {
    ...actual,
    getAccountVault: mocks.getAccountVault,
    getAccountWeeklyQuotas: mocks.getAccountWeeklyQuotas,
    listenAccountEvents: vi.fn(async (handlers: { onSwitched?: () => void; onError?: (message: string) => void }) => { mocks.accountHandlers = handlers; return () => {}; }),
    setAccountSwitcherExpanded: vi.fn(async () => {}),
    closeAccountSwitcher: vi.fn(async () => {}),
    switchAccount: vi.fn(async () => ({ profile: vault.profiles[1], credentialsSwitched: true, restartRecommended: false })),
  };
});

describe("AccountSwitcher", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  beforeEach(() => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    mocks.accountHandlers = null;
    mocks.getAccountVault.mockReset().mockResolvedValue(vault);
    mocks.getAccountWeeklyQuotas.mockReset().mockResolvedValue([]);
  });

  it("shows masked account metadata and keeps add fields collapsed by default", async () => {
    const view = render(<AccountSwitcher />);
    expect(await view.findByText("个人号")).not.toBeNull();
    expect(view.getByText("p***@example.com")).not.toBeNull();
    expect(view.queryByLabelText("账号名称")).toBeNull();
    fireEvent.click(view.getByLabelText("添加账号"));
    expect(view.getByLabelText("账号名称")).not.toBeNull();
    expect(view.container.textContent).not.toContain("access_token");
  });

  it("shows only rounded weekly quota values and isolates an expired account", async () => {
    mocks.getAccountWeeklyQuotas.mockResolvedValue([
      { profileId: "one", remainingPercent: 64.6, status: "ok", message: null },
      { profileId: "two", remainingPercent: null, status: "signed_out", message: "expired" },
    ]);

    const view = render(<AccountSwitcher />);

    expect(await view.findByText("周 65%")).not.toBeNull();
    expect(view.getAllByText("重新登录").length).toBeGreaterThan(0);
    expect(view.queryByText(/5\s*小时|分钟前|已过期/)).toBeNull();
    expect((view.getByRole("button", { name: "切换" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("dismisses switch success after ten seconds without clearing a later error", async () => {
    const timeoutSpy = vi.spyOn(window, "setTimeout");
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());

    act(() => mocks.accountHandlers?.onSwitched?.());
    expect(await view.findByText("账号已切换，正在刷新额度。")).not.toBeNull();
    await waitFor(() => expect(timeoutSpy.mock.calls.some(([, delay]) => delay === 10_000)).toBe(true));
    const dismiss = timeoutSpy.mock.calls.find(([, delay]) => delay === 10_000)?.[0] as (() => void) | undefined;

    act(() => mocks.accountHandlers?.onError?.("后续错误"));
    act(() => dismiss?.());
    expect(view.getByText("后续错误")).not.toBeNull();
    timeoutSpy.mockRestore();
  });
});
