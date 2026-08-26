// @vitest-environment jsdom

import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ProviderSnapshot, WidgetPreferences } from "../types";
import { QuotaCard } from "./QuotaCard";

const snapshot: ProviderSnapshot = {
  provider: "codex",
  displayName: "CODEX",
  plan: "PLUS",
  shortWindow: { remainingPercent: 72, resetsAt: null, windowSeconds: 18_000 },
  weeklyWindow: { remainingPercent: 84, resetsAt: null, windowSeconds: 604_800 },
  resetCredits: 0,
  updatedAt: "2026-07-13T00:00:00Z",
  status: "ok",
  message: null,
};

const preferences: WidgetPreferences = {
  locked: false,
  alwaysOnTop: true,
  pinnedProvider: null,
  autoRotateSeconds: 12,
  language: "zh-CN",
  paletteColors: ["#ff0000", "#ff9900", "#ffee00", "#aadd88", "#33aa66"],
};

const callbacks = {
  onPrevious: vi.fn(),
  onNext: vi.fn(),
  onTogglePin: vi.fn(),
  onLock: vi.fn(),
  onLanguage: vi.fn(),
  onDrag: vi.fn(),
  onHover: vi.fn(),
};

afterEach(cleanup);

describe("QuotaCard content layers", () => {
  it("keeps collapsed and expanded content mounted while the card reverses direction", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    const conversationTokenUsage = { conversationId: "thread-1", totalTokens: 12_345 };
    const view = render(<QuotaCard snapshot={snapshot} preferences={preferences} providerCount={1} conversationTokenUsage={conversationTokenUsage} compact {...callbacks} />);
    const collapsed = view.container.querySelector(".collapsed-content");
    const expanded = view.container.querySelector(".expanded-content");

    expect(collapsed).not.toBeNull();
    expect(expanded).not.toBeNull();
    expect(view.container.querySelector(".quota-card--compact")).not.toBeNull();
    expect(view.getByText("当前窗口使用量")).not.toBeNull();
    expect(view.getByText("12.3K")).not.toBeNull();

    view.rerender(<QuotaCard snapshot={snapshot} preferences={preferences} providerCount={1} conversationTokenUsage={conversationTokenUsage} compact={false} {...callbacks} />);

    expect(view.container.querySelector(".collapsed-content")).toBe(collapsed);
    expect(view.container.querySelector(".expanded-content")).toBe(expanded);
    expect(view.container.querySelector(".quota-card--compact")).toBeNull();
  });

  it("uses a semantic account chip without starting a window drag", () => {
    const onAccount = vi.fn();
    const onDrag = vi.fn();
    const view = render(<QuotaCard
      snapshot={snapshot}
      preferences={preferences}
      providerCount={1}
      onPrevious={vi.fn()}
      onNext={vi.fn()}
      onTogglePin={vi.fn()}
      onLock={vi.fn()}
      onLanguage={vi.fn()}
      onDrag={onDrag}
      onHover={vi.fn()}
      accountAlias="个人号"
      onAccount={onAccount}
    />);
    const button = view.getByRole("button", { name: "个人号 · PLUS" });
    fireEvent.mouseDown(button, { button: 0 });
    fireEvent.click(button);
    expect(onAccount).toHaveBeenCalledTimes(1);
    expect(onDrag).not.toHaveBeenCalled();
  });

  it("shows the five-hour and weekly quotas together when both windows exist", () => {
    const view = render(<QuotaCard snapshot={snapshot} preferences={preferences} providerCount={1} {...callbacks} />);
    const card = view.container.querySelector(".quota-card");
    const progress = view.getByRole("progressbar", { name: "5 小时额度剩余 72%" });

    expect(card?.classList.contains("quota-card--dual-quota")).toBe(true);
    expect(card?.classList.contains("quota-card--weekly-only")).toBe(false);
    expect(view.getByText("5 小时剩余")).not.toBeNull();
    expect(view.getByText(/^本周剩余/)).not.toBeNull();
    expect(progress.getAttribute("aria-valuenow")).toBe("72");
    expect(view.getByText("84")).not.toBeNull();
  });

  it("uses the weekly quota as the only quota for accounts without a five-hour window", () => {
    const weeklyOnlySnapshot: ProviderSnapshot = {
      ...snapshot,
      plan: "PRO",
      shortWindow: null,
      weeklyWindow: { remainingPercent: 86, resetsAt: null, windowSeconds: 604_800 },
    };
    const view = render(<QuotaCard snapshot={weeklyOnlySnapshot} preferences={preferences} providerCount={1} {...callbacks} />);
    const card = view.container.querySelector(".quota-card");
    const progress = view.getByRole("progressbar", { name: "本周额度剩余 86%" });

    expect(card?.classList.contains("quota-card--weekly-only")).toBe(true);
    expect(card?.classList.contains("quota-card--dual-quota")).toBe(false);
    expect(view.getByText("本周剩余")).not.toBeNull();
    expect(view.queryByText("5 小时剩余")).toBeNull();
    expect(progress.getAttribute("aria-valuenow")).toBe("86");
    expect(view.queryByText(/^本周剩余 · 至/)).toBeNull();
  });
});
