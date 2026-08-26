// @vitest-environment jsdom

import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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

describe("QuotaCard content layers", () => {
  it("keeps collapsed and expanded content mounted while the card reverses direction", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    const callbacks = {
      onPrevious: vi.fn(),
      onNext: vi.fn(),
      onTogglePin: vi.fn(),
      onLock: vi.fn(),
      onLanguage: vi.fn(),
      onDrag: vi.fn(),
      onHover: vi.fn(),
    };
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
});
