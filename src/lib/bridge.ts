import type { ConversationTokenUsage, ProviderSnapshot, TokenUsageSummary, WidgetPreferences } from "../types";
import { DEFAULT_PALETTE_COLORS } from "./quotaTheme";

const defaultPreferences: WidgetPreferences = { locked: false, alwaysOnTop: true, pinnedProvider: null, autoRotateSeconds: 12, language: "zh-CN", paletteColors: [...DEFAULT_PALETTE_COLORS] };

export interface PaletteSessionPayload {
  percent: number;
  colors: string[];
}

export type RefreshRequestMode = "auto" | "manual" | "account-relogin";

export function normalizeRefreshRequestMode(value: unknown): RefreshRequestMode {
  return value === "manual" || value === "account-relogin" ? value : "auto";
}

const mockSnapshot: ProviderSnapshot = {
  provider: "codex",
  displayName: "CODEX",
  plan: "PRO",
  shortWindow: { remainingPercent: 74, resetsAt: new Date(Date.now() + 78 * 60_000).toISOString(), windowSeconds: 18_000 },
  weeklyWindow: { remainingPercent: 42, resetsAt: new Date(Date.now() + 3.2 * 86_400_000).toISOString(), windowSeconds: 604_800 },
  resetCredits: 1,
  resetCreditExpiresAt: [new Date(Date.now() + 9 * 86_400_000).toISOString()],
  updatedAt: new Date().toISOString(),
  status: "ok",
  message: null,
};

const mockTokenUsage: TokenUsageSummary = {
  inputTokens: 12_328_760,
  cachedInputTokens: 8_102_400,
  outputTokens: 129_560,
  reasoningOutputTokens: 34_280,
  totalTokens: 12_458_320,
  sessionCount: 92,
  updatedAt: new Date().toISOString(),
};

export const isTauri = () => "__TAURI_INTERNALS__" in window;

export async function fetchSnapshots(force = false): Promise<ProviderSnapshot[]> {
  if (!isTauri()) return [mockSnapshot];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProviderSnapshot[]>(force ? "refresh_snapshots" : "get_snapshots");
}

export async function fetchCachedSnapshots(): Promise<ProviderSnapshot[]> {
  if (!isTauri()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProviderSnapshot[]>("get_cached_snapshots");
}

export async function fetchTokenUsage(): Promise<TokenUsageSummary> {
  if (!isTauri()) return mockTokenUsage;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<TokenUsageSummary>("get_token_usage");
}

export async function getPreferences(): Promise<WidgetPreferences> {
  if (!isTauri()) return defaultPreferences;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<WidgetPreferences>("get_preferences");
}

export async function updatePreferences(value: WidgetPreferences): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_preferences", { preferences: value });
}

export async function setClickThrough(locked: boolean): Promise<WidgetPreferences> {
  if (!isTauri()) return { ...defaultPreferences, locked };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<WidgetPreferences>("set_widget_locked", { locked });
}

export async function setAlwaysOnTop(alwaysOnTop: boolean): Promise<WidgetPreferences> {
  if (!isTauri()) return { ...defaultPreferences, alwaysOnTop };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<WidgetPreferences>("set_widget_always_on_top", { alwaysOnTop });
}

export async function startDragging(): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().startDragging();
}

export async function setWidgetExpanded(expanded: boolean): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
  const size = expanded ? new LogicalSize(320, 320) : new LogicalSize(100, 100);
  await getCurrentWindow().setSize(size);
}

export const ACCOUNT_SWITCHER_COMPACT_HEIGHT = 190;
export const ACCOUNT_SWITCHER_EXPANDED_HEIGHT = 240;
export const ACCOUNT_SWITCHER_NOTICE_EXTRA_HEIGHT = 24;
const ACCOUNT_SWITCHER_RESIZE_MS = 240;
let accountSwitcherResizeGeneration = 0;

export async function setAccountSwitcherExpanded(expanded: boolean, reducedMotion = false, noticeVisible = false): Promise<void> {
  if (!isTauri()) return;
  const generation = ++accountSwitcherResizeGeneration;
  const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
  const appWindow = getCurrentWindow();
  const [physicalSize, scaleFactor] = await Promise.all([appWindow.innerSize(), appWindow.scaleFactor()]);
  const startHeight = physicalSize.height / scaleFactor;
  const baseHeight = expanded ? ACCOUNT_SWITCHER_EXPANDED_HEIGHT : ACCOUNT_SWITCHER_COMPACT_HEIGHT;
  const targetHeight = baseHeight + (noticeVisible ? ACCOUNT_SWITCHER_NOTICE_EXTRA_HEIGHT : 0);

  if (reducedMotion || Math.abs(startHeight - targetHeight) < 1) {
    await appWindow.setSize(new LogicalSize(320, targetHeight));
    return;
  }

  const startedAt = performance.now();
  while (generation === accountSwitcherResizeGeneration) {
    const progress = Math.min(1, (performance.now() - startedAt) / ACCOUNT_SWITCHER_RESIZE_MS);
    const eased = 1 - Math.pow(1 - progress, 3);
    const height = startHeight + (targetHeight - startHeight) * eased;
    await appWindow.setSize(new LogicalSize(320, height));
    if (progress >= 1) return;
    await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
  }
}

export async function setWidgetClip(expanded: boolean): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_widget_clip", { expanded });
}

export async function syncWidgetCssScale(cssViewportWidth: number): Promise<void> {
  if (!isTauri() || !Number.isFinite(cssViewportWidth) || cssViewportWidth <= 0) return;
  const [{ invoke }, { getCurrentWindow }] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/api/window"),
  ]);
  // `outerSize()` includes the invisible Win32 frame around a transparent
  // window. The WebView viewport occupies the client area, so using the outer
  // width overestimates the CSS-to-physical scale and lets the native blur
  // surface protrude beyond the card's antialiased corners.
  const physicalSize = await getCurrentWindow().innerSize();
  const scale = physicalSize.width / cssViewportWidth;
  if (!Number.isFinite(scale) || scale <= 0) return;
  await invoke("set_widget_css_scale", { scale });
}

export async function openPalettePreview(percent: number): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_palette_preview", { percent });
}

export async function updatePalettePreview(percent: number): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("update_palette_preview", { percent });
}

export async function updatePaletteColors(colors: readonly string[]): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("update_palette_colors", { colors: [...colors] });
}

export async function savePaletteColors(colors: readonly string[]): Promise<WidgetPreferences> {
  if (!isTauri()) return { ...defaultPreferences, paletteColors: [...colors] };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<WidgetPreferences>("save_palette_colors", { colors: [...colors] });
}

export async function closePalettePreview(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("close_palette_preview");
}

export async function listenPalettePreview(handlers: {
  onChanged: (percent: number) => void;
  onColorsChanged: (colors: string[]) => void;
  onClosed: () => void;
}): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlistenChanged = await listen<number>("palette-preview-changed", (event) => handlers.onChanged(event.payload));
  const unlistenColors = await listen<string[]>("palette-colors-changed", (event) => handlers.onColorsChanged(event.payload));
  const unlistenClosed = await listen("palette-preview-closed", handlers.onClosed);
  return () => { unlistenChanged(); unlistenColors(); unlistenClosed(); };
}

export async function listenPaletteController(handlers: {
  onOpened: (payload: PaletteSessionPayload) => void;
  onClosing: () => void;
  onColorsChanged: (colors: string[]) => void;
  onPercentChanged?: (percent: number) => void;
}): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlistenOpened = await listen<PaletteSessionPayload>("palette-preview-opened", (event) => handlers.onOpened(event.payload));
  const unlistenClosing = await listen("palette-preview-closing", handlers.onClosing);
  const unlistenColors = await listen<string[]>("palette-colors-changed", (event) => handlers.onColorsChanged(event.payload));
  const unlistenPercent = await listen<number>("palette-preview-changed", (event) => handlers.onPercentChanged?.(event.payload));
  return () => { unlistenOpened(); unlistenClosing(); unlistenColors(); unlistenPercent(); };
}

export async function listenDesktopEvents(handlers: {
  onPreferences: (value: WidgetPreferences) => void;
  onRefresh: (mode: RefreshRequestMode) => void;
  onFocusLost: () => void;
  onConversationTokenUsage: (value: ConversationTokenUsage) => void;
}): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlistenPreferences = await listen<WidgetPreferences>("preferences-changed", (event) => handlers.onPreferences(event.payload));
  const unlistenRefresh = await listen<RefreshRequestMode>("refresh-requested", (event) => {
    handlers.onRefresh(normalizeRefreshRequestMode(event.payload));
  });
  const unlistenFocusLost = await listen("widget-focus-lost", handlers.onFocusLost);
  const unlistenConversationTokenUsage = await listen<ConversationTokenUsage>("conversation-token-usage", (event) => handlers.onConversationTokenUsage(event.payload));
  return () => { unlistenPreferences(); unlistenRefresh(); unlistenFocusLost(); unlistenConversationTokenUsage(); };
}
