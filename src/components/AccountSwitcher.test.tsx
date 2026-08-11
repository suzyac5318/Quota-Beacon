// @vitest-environment jsdom

import { fireEvent, render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AccountSwitcher } from "./AccountSwitcher";

const mocks = vi.hoisted(() => ({ saveCurrentAccount: vi.fn() }));

vi.mock("../lib/accounts", () => ({
  accountCopy: () => ({
    title: "Codex 账号", close: "关闭", saveCurrent: "保存当前账号", alias: "账号名称",
    add: "添加账号", cancel: "取消登录", current: "当前使用", switch: "切换", rename: "重命名", remove: "删除",
    empty: "请先显式保存当前 Codex 登录，再添加其他账号。", browser: "请在浏览器中完成官方 Codex 登录。",
    switched: "凭据已切换；重启 Codex 后完整生效。", invalid: "重新登录", restart: "切换并重启", restartConfirm: "重启确认", localTokens: "Token 统计继续显示本机全部 Codex 会话累计。",
  }),
  beginAccountLogin: vi.fn(), cancelAccountLogin: vi.fn(), closeAccountSwitcher: vi.fn(), deleteAccount: vi.fn(),
  getAccountVault: vi.fn(async () => ({ profiles: [], activeProfileId: null, hasCurrentLogin: true, currentLoginSaved: false })),
  listenAccountEvents: vi.fn(async () => () => {}), pollAccountLogin: vi.fn(), renameAccount: vi.fn(),
  saveCurrentAccount: mocks.saveCurrentAccount.mockResolvedValue({
    profiles: [{ id: "personal", alias: "个人号", maskedEmail: "p***@example.com", isActive: true, credentialStatus: "ready" }],
    activeProfileId: "personal", hasCurrentLogin: true, currentLoginSaved: true,
  }),
  switchAccount: vi.fn(), switchAccountAndRestartCodex: vi.fn(),
}));

vi.mock("../lib/bridge", () => ({
  getPreferences: vi.fn(async () => ({ language: "zh-CN" })),
}));

describe("AccountSwitcher", () => {
  it("requires an explicit user action before saving the current login", async () => {
    const view = render(<AccountSwitcher />);
    await waitFor(() => expect(view.getByText("请先显式保存当前 Codex 登录，再添加其他账号。")).not.toBeNull());
    expect(mocks.saveCurrentAccount).not.toHaveBeenCalled();
    expect(view.queryByLabelText("账号名称")).toBeNull();
    const addButton = view.getByRole("button", { name: "添加账号" });
    expect(addButton.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(addButton);
    expect(addButton.getAttribute("aria-expanded")).toBe("true");
    fireEvent.change(view.getByLabelText("账号名称"), { target: { value: "个人号" } });
    fireEvent.click(view.getByRole("button", { name: "保存当前账号" }));
    await waitFor(() => expect(mocks.saveCurrentAccount).toHaveBeenCalledWith("个人号"));
    expect(view.getByText("p***@example.com")).not.toBeNull();
  });
});
