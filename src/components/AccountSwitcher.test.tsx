// @vitest-environment jsdom

import { fireEvent, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

vi.mock("../lib/bridge", () => ({
  getPreferences: vi.fn(async () => ({ language: "zh-CN" })),
}));

vi.mock("../lib/accounts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/accounts")>();
  return {
    ...actual,
    getAccountVault: vi.fn(async () => vault),
    listenAccountEvents: vi.fn(async () => () => {}),
    setAccountSwitcherExpanded: vi.fn(async () => {}),
    closeAccountSwitcher: vi.fn(async () => {}),
    switchAccount: vi.fn(async () => ({ profile: vault.profiles[1], credentialsSwitched: true, restartRecommended: false })),
  };
});

describe("AccountSwitcher", () => {
  beforeEach(() => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
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
});
