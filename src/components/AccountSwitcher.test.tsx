// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AccountSwitcher } from "./AccountSwitcher";

const mocks = vi.hoisted(() => ({
  saveCurrentAccount: vi.fn(),
  getAccountVault: vi.fn(),
  setAccountSwitcherExpanded: vi.fn(async () => undefined),
  accountHandlers: null as null | { onOpened?: () => void; onSwitched?: () => void },
}));

vi.mock("../lib/accounts", () => ({
  accountCopy: () => ({
    title: "Codex 账号", close: "关闭", saveCurrent: "保存当前账号", alias: "账号名称",
    add: "添加账号", cancel: "取消登录", current: "当前使用", switch: "切换", rename: "重命名", remove: "删除",
    empty: "请先显式保存当前 Codex 登录，再添加其他账号。", browser: "请在浏览器中完成官方 Codex 登录。",
    switched: "凭据已切换；重启 Codex 后完整生效。", invalid: "重新登录", restart: "切换并重启", restartConfirm: "重启确认", localTokens: "Token 统计继续显示本机全部 Codex 会话累计。",
  }),
  beginAccountLogin: vi.fn(), cancelAccountLogin: vi.fn(), closeAccountSwitcher: vi.fn(), deleteAccount: vi.fn(),
  getAccountVault: mocks.getAccountVault,
  listenAccountEvents: vi.fn(async (handlers: { onOpened?: () => void; onSwitched?: () => void }) => { mocks.accountHandlers = handlers; return () => {}; }), pollAccountLogin: vi.fn(), renameAccount: vi.fn(),
  saveCurrentAccount: mocks.saveCurrentAccount.mockResolvedValue({
    profiles: [{ id: "personal", alias: "个人号", maskedEmail: "p***@example.com", isActive: true, credentialStatus: "ready" }],
    activeProfileId: "personal", hasCurrentLogin: true, currentLoginSaved: true,
  }),
  switchAccount: vi.fn(),
}));

vi.mock("../lib/bridge", () => ({
  getPreferences: vi.fn(async () => ({ language: "zh-CN" })),
  setAccountSwitcherExpanded: mocks.setAccountSwitcherExpanded,
}));

describe("AccountSwitcher", () => {
  beforeEach(() => {
    cleanup();
    mocks.accountHandlers = null;
    mocks.saveCurrentAccount.mockClear();
    mocks.getAccountVault.mockReset().mockResolvedValue({ profiles: [], activeProfileId: null, hasCurrentLogin: true, currentLoginSaved: false });
    mocks.setAccountSwitcherExpanded.mockClear();
  });

  it("requires an explicit user action before saving the current login", async () => {
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(view.getByText("请先显式保存当前 Codex 登录，再添加其他账号。")).not.toBeNull());
    expect(mocks.saveCurrentAccount).not.toHaveBeenCalled();
    const formShell = view.container.querySelector(".account-switcher__form-shell");
    const aliasInput = view.getByLabelText("账号名称") as HTMLInputElement;
    expect(formShell?.getAttribute("aria-hidden")).toBe("true");
    expect(aliasInput.disabled).toBe(true);
    const addButton = view.getByRole("button", { name: "添加账号" });
    expect(addButton.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(addButton);
    expect(addButton.getAttribute("aria-expanded")).toBe("true");
    expect(formShell?.getAttribute("aria-hidden")).toBe("false");
    expect(aliasInput.disabled).toBe(false);
    expect(document.activeElement).toBe(aliasInput);
    fireEvent.change(aliasInput, { target: { value: "未提交" } });
    fireEvent.click(addButton);
    expect(addButton.getAttribute("aria-expanded")).toBe("false");
    expect(formShell?.getAttribute("aria-hidden")).toBe("true");
    expect(aliasInput.disabled).toBe(true);
    expect(aliasInput.value).toBe("");
    fireEvent.click(addButton);
    fireEvent.change(aliasInput, { target: { value: "个人号" } });
    fireEvent.click(view.getByRole("button", { name: "保存当前账号" }));
    await waitFor(() => expect(mocks.saveCurrentAccount).toHaveBeenCalledWith("个人号"));
    expect(view.getByText("p***@example.com")).not.toBeNull();
  });

  it("resets the add form whenever the account window is reopened", async () => {
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(mocks.accountHandlers).not.toBeNull());
    const addButton = view.getByRole("button", { name: "添加账号" });
    fireEvent.click(addButton);
    const aliasInput = view.getByLabelText("账号名称") as HTMLInputElement;
    fireEvent.change(aliasInput, { target: { value: "未提交" } });
    act(() => mocks.accountHandlers?.onOpened?.());
    expect(addButton.getAttribute("aria-expanded")).toBe("false");
    expect(view.container.querySelector(".account-switcher__form-shell")?.getAttribute("aria-hidden")).toBe("true");
    expect(aliasInput.disabled).toBe(true);
    expect(aliasInput.value).toBe("");
  });

  it("reserves native window space for switch notices without showing a restart button", async () => {
    mocks.getAccountVault.mockResolvedValue({
      profiles: [
        { id: "personal", alias: "个人号", maskedEmail: "p***@example.com", isActive: true, credentialStatus: "ready" },
        { id: "work", alias: "工作号", maskedEmail: "w***@example.com", isActive: false, credentialStatus: "ready" },
      ],
      activeProfileId: "personal", hasCurrentLogin: true, currentLoginSaved: true,
    });
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(view.getByRole("button", { name: "切换" })).not.toBeNull());
    expect(view.queryByRole("button", { name: "切换并重启" })).toBeNull();

    act(() => mocks.accountHandlers?.onSwitched?.());

    await waitFor(() => expect(view.getByRole("status").textContent).toContain("重启 Codex 后完整生效"));
    await waitFor(() => expect(mocks.setAccountSwitcherExpanded).toHaveBeenLastCalledWith(false, false, true));
  });
});
