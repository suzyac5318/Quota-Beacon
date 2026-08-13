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
  beginAccountLogin: vi.fn(),
  pollAccountLogin: vi.fn(),
  accountHandlers: null as null | {
    onOpened?: (theme: { percent: number | null; colors: string[] }) => void;
    onThemeChanged?: (theme: { percent: number | null; colors: string[] }) => void;
    onClosed?: () => void;
    onSwitched?: () => void;
    onError?: (message: string) => void;
  },
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
    beginAccountLogin: mocks.beginAccountLogin,
    pollAccountLogin: mocks.pollAccountLogin,
    listenAccountEvents: vi.fn(async (handlers: {
      onOpened?: (theme: { percent: number | null; colors: string[] }) => void;
      onThemeChanged?: (theme: { percent: number | null; colors: string[] }) => void;
      onClosed?: () => void;
      onSwitched?: () => void;
      onError?: (message: string) => void;
    }) => { mocks.accountHandlers = handlers; return () => {}; }),
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
    mocks.beginAccountLogin.mockReset().mockResolvedValue({ taskId: "login-task", status: "running", message: null });
    mocks.pollAccountLogin.mockReset().mockResolvedValue({ taskId: "login-task", status: "running", message: null });
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
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));

    expect(await view.findByText("周 65%")).not.toBeNull();
    expect(view.getAllByText("重新登录").length).toBeGreaterThan(0);
    expect(view.queryByText(/5\s*小时|分钟前|已过期/)).toBeNull();
    expect((view.getByRole("button", { name: "切换" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("does not query weekly quotas until the hidden account window opens", async () => {
    render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    expect(mocks.getAccountWeeklyQuotas).not.toHaveBeenCalled();

    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));
    await waitFor(() => expect(mocks.getAccountWeeklyQuotas).toHaveBeenCalledTimes(1));
    act(() => mocks.accountHandlers?.onClosed?.());
  });

  it("uses the active five-hour quota theme and falls back to neutral glass", async () => {
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    const shell = view.container.querySelector(".account-switcher") as HTMLElement;
    expect(shell.classList.contains("account-switcher--neutral")).toBe(true);

    act(() => mocks.accountHandlers?.onOpened?.({
      percent: 60,
      colors: ["#eb5b58", "#f1a06f", "#f5d98f", "#e3f4b8", "#b9e4c9"],
    }));
    expect(shell.classList.contains("account-switcher--neutral")).toBe(false);
    expect(shell.style.getPropertyValue("--card-base")).toBe("#e3f4b8");

    act(() => mocks.accountHandlers?.onThemeChanged?.({ percent: null, colors: [] }));
    expect(shell.classList.contains("account-switcher--neutral")).toBe(true);
    expect(shell.style.getPropertyValue("--card-base")).toBe("");
  });

  it("resets finished form state and notices when the window reopens", async () => {
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));
    fireEvent.click(view.getByLabelText("添加账号"));
    fireEvent.change(view.getByLabelText("账号名称"), { target: { value: "临时名称" } });
    act(() => mocks.accountHandlers?.onError?.("旧提示"));
    expect(view.getByText("旧提示")).not.toBeNull();

    act(() => mocks.accountHandlers?.onClosed?.());
    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));
    expect(view.queryByLabelText("账号名称")).toBeNull();
    expect(view.queryByText("旧提示")).toBeNull();
  });

  it("keeps only one login poll in flight during a slow operation", async () => {
    let resolvePoll: ((value: { taskId: string; status: "running"; message: null }) => void) | undefined;
    mocks.pollAccountLogin.mockImplementation(() => new Promise((resolve) => { resolvePoll = resolve; }));
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));
    fireEvent.click(view.getByLabelText("添加账号"));
    const aliasInput = view.getByLabelText("账号名称");
    fireEvent.change(aliasInput, { target: { value: "新账号" } });
    fireEvent.submit(aliasInput.closest("form") as HTMLFormElement);
    await waitFor(() => expect(mocks.beginAccountLogin).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => window.setTimeout(resolve, 1_650));
    expect(mocks.pollAccountLogin).toHaveBeenCalledTimes(1);
    await act(async () => resolvePoll?.({ taskId: "login-task", status: "running", message: null }));
  });

  it("keeps a running browser login visible after the account window reopens", async () => {
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));
    fireEvent.click(view.getByLabelText("添加账号"));
    const aliasInput = view.getByLabelText("账号名称");
    fireEvent.change(aliasInput, { target: { value: "新账号" } });
    fireEvent.submit(aliasInput.closest("form") as HTMLFormElement);
    await waitFor(() => expect(mocks.beginAccountLogin).toHaveBeenCalledTimes(1));

    act(() => mocks.accountHandlers?.onClosed?.());
    act(() => mocks.accountHandlers?.onOpened?.({ percent: null, colors: [] }));
    expect(view.getByText("请在浏览器中完成官方 Codex 登录。")).not.toBeNull();
    expect(view.getByRole("button", { name: "取消登录" })).not.toBeNull();
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
