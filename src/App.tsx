import { useCallback, useEffect, useRef, useState } from "react";
import { QuotaCard } from "./components/QuotaCard";
import { closePalettePreview, fetchSnapshots, fetchTokenUsage, getPreferences, listenDesktopEvents, listenPalettePreview, openPalettePreview, setAlwaysOnTop, setWidgetClip, setWidgetExpanded, startDragging, syncWidgetCssScale, updatePreferences } from "./lib/bridge";
import { clampPercent, getPrimaryQuota } from "./lib/format";
import { copy, nextLanguage, normalizeLanguage } from "./lib/i18n";
import { DEFAULT_PALETTE_COLORS, normalizePaletteColors } from "./lib/quotaTheme";
import { mergeSnapshots } from "./lib/snapshots";
import type { ConversationTokenUsage, ProviderSnapshot, TokenUsageStatus, TokenUsageSummary, WidgetPreferences } from "./types";

const DEFAULT_PREFS: WidgetPreferences = { locked: false, alwaysOnTop: true, pinnedProvider: null, autoRotateSeconds: 12, language: "zh-CN", paletteColors: [...DEFAULT_PALETTE_COLORS] };
const REFRESH_INTERVAL_MS = 10_000;
const TOKEN_USAGE_REFRESH_INTERVAL_MS = 60_000;
const HOVER_LIFT_MS = 36;
const COLLAPSE_DELAY_MS = 120;
const COLLAPSE_MORPH_MS = 280;

export default function App() {
  const [snapshots, setSnapshots] = useState<ProviderSnapshot[]>([]);
  const [preferences, setPreferences] = useState(DEFAULT_PREFS);
  const [activeIndex, setActiveIndex] = useState(0);
  const [hovered, setHovered] = useState(false);
  const [compact, setCompact] = useState(() => window.innerWidth <= 120 || window.innerHeight <= 120);
  const [consumingProviders, setConsumingProviders] = useState<Set<string>>(() => new Set());
  const [operationError, setOperationError] = useState<string | null>(null);
  const [palettePercent, setPalettePercent] = useState<number | null>(null);
  const [paletteDraft, setPaletteDraft] = useState<string[] | null>(null);
  const [tokenUsage, setTokenUsage] = useState<TokenUsageSummary | null>(null);
  const [tokenUsageStatus, setTokenUsageStatus] = useState<TokenUsageStatus>("loading");
  const [conversationTokenUsage, setConversationTokenUsage] = useState<ConversationTokenUsage>({ conversationId: null, totalTokens: null });
  const failures = useRef(0);
  const previousPrimary = useRef(new Map<string, number>());
  const consumptionTimers = useRef(new Map<string, number>());
  const paletteActive = useRef(false);
  const paletteOpening = useRef(false);
  const paletteClosing = useRef(false);
  const hoveredRef = useRef(false);
  const hoverExpandTimer = useRef<number | null>(null);
  const collapseDelayTimer = useRef<number | null>(null);
  const collapseResizeTimer = useRef<number | null>(null);
  const language = normalizeLanguage(preferences.language);
  const t = copy[language];

  const clearWidgetMotionTimers = useCallback(() => {
    for (const timer of [hoverExpandTimer, collapseDelayTimer, collapseResizeTimer]) {
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
    }
  }, []);

  useEffect(() => {
    let frame = 0;
    const syncScale = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        void syncWidgetCssScale(window.innerWidth);
      });
    };
    syncScale();
    window.addEventListener("resize", syncScale);
    window.visualViewport?.addEventListener("resize", syncScale);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", syncScale);
      window.visualViewport?.removeEventListener("resize", syncScale);
    };
  }, []);

  const scheduleCollapse = useCallback((withHoverDelay = true) => {
    if (paletteActive.current) return;
    if (collapseDelayTimer.current !== null) window.clearTimeout(collapseDelayTimer.current);
    collapseDelayTimer.current = window.setTimeout(() => {
      collapseDelayTimer.current = null;
      if (hoveredRef.current || paletteActive.current) return;
      void setWidgetClip(false);
      setCompact(true);
      collapseResizeTimer.current = window.setTimeout(() => {
        collapseResizeTimer.current = null;
        if (hoveredRef.current || paletteActive.current) return;
        void setWidgetExpanded(false).catch(() => setOperationError("Widget collapse failed."));
      }, COLLAPSE_MORPH_MS);
    }, withHoverDelay ? COLLAPSE_DELAY_MS : 0);
  }, []);

  const refresh = useCallback(async (force = false) => {
    try {
      const values = await fetchSnapshots(force);
      const hasFailure = values.some((item) => item.status !== "ok");
      if (hasFailure) failures.current += 1;
      else failures.current = 0;
      for (const item of values) {
        const nextPrimary = getPrimaryQuota(item)?.window.remainingPercent;
        const previous = previousPrimary.current.get(item.provider);
        if (nextPrimary !== undefined && previous !== undefined && nextPrimary < previous) {
          setConsumingProviders((current) => new Set(current).add(item.provider));
          const oldTimer = consumptionTimers.current.get(item.provider);
          if (oldTimer !== undefined) window.clearTimeout(oldTimer);
          const timer = window.setTimeout(() => {
            setConsumingProviders((current) => { const next = new Set(current); next.delete(item.provider); return next; });
            consumptionTimers.current.delete(item.provider);
          }, 5 * 60_000);
          consumptionTimers.current.set(item.provider, timer);
        }
        if (nextPrimary !== undefined) previousPrimary.current.set(item.provider, nextPrimary);
      }
      setSnapshots((current) => mergeSnapshots(current, values));
    } catch {
      failures.current += 1;
      setSnapshots((current) => current.length > 0
        ? current.map((item) => ({ ...item, status: "stale", message: "Refresh failed. Please try again later." }))
        : [{ provider: "codex", displayName: "CODEX", plan: null, shortWindow: null, weeklyWindow: null, resetCredits: null, resetCreditExpiresAt: [], updatedAt: new Date().toISOString(), status: "unavailable", message: "Quota is temporarily unavailable. It will retry automatically." }]);
    }
  }, []);

  const refreshTokenUsage = useCallback(async () => {
    try {
      const value = await fetchTokenUsage();
      setTokenUsage(value);
      setTokenUsageStatus("ready");
    } catch {
      setTokenUsage(null);
      setTokenUsageStatus("unavailable");
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    const loadPreferences = async () => {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          const value = await getPreferences();
          if (!cancelled) {
            setPreferences({ ...DEFAULT_PREFS, ...value, language: normalizeLanguage(value.language), paletteColors: normalizePaletteColors(value.paletteColors) });
            setOperationError(null);
          }
          return;
        } catch {
          if (attempt < 2) await new Promise((resolve) => window.setTimeout(resolve, 300));
        }
      }
      if (!cancelled) setOperationError("Unable to read settings. Defaults are in use.");
    };
    void refresh(true);
    void loadPreferences();
    return () => { cancelled = true; for (const timer of consumptionTimers.current.values()) window.clearTimeout(timer); consumptionTimers.current.clear(); };
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    let cleanup = () => {};
    void listenPalettePreview({
      onChanged: (value) => setPalettePercent(clampPercent(value)),
      onColorsChanged: (colors) => setPaletteDraft(normalizePaletteColors(colors)),
      onClosed: () => {
        paletteActive.current = false;
        paletteOpening.current = false;
        paletteClosing.current = false;
        setPalettePercent(null);
        setPaletteDraft(null);
        if (!hoveredRef.current) scheduleCollapse(false);
      },
    }).then((unlisten) => { if (cancelled) unlisten(); else cleanup = unlisten; });
    return () => { cancelled = true; cleanup(); };
  }, [scheduleCollapse]);

  useEffect(() => () => clearWidgetMotionTimers(), [clearWidgetMotionTimers]);

  useEffect(() => {
    let cancelled = false;
    let cleanup: () => void = () => {};
    void listenDesktopEvents({
      onPreferences: (value) => { setPreferences({ ...DEFAULT_PREFS, ...value, language: normalizeLanguage(value.language), paletteColors: normalizePaletteColors(value.paletteColors) }); setOperationError(null); },
      onRefresh: () => void refresh(true),
      onFocusLost: () => {
        hoveredRef.current = false;
        setHovered(false);
        if (paletteActive.current) {
          if (paletteClosing.current) return;
          paletteClosing.current = true;
          void closePalettePreview().catch(() => {
            paletteClosing.current = false;
            setOperationError("Unable to close color preview.");
          });
          return;
        }
        scheduleCollapse(false);
      },
      onConversationTokenUsage: setConversationTokenUsage,
    }).then((value) => {
      if (cancelled) value(); else cleanup = value;
    }).catch(() => setOperationError("Desktop event listener failed to start."));
    return () => { cancelled = true; cleanup(); };
  }, [refresh, scheduleCollapse]);

  useEffect(() => {
    const id = window.setInterval(() => void refresh(true), REFRESH_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    void refreshTokenUsage();
    const id = window.setInterval(() => void refreshTokenUsage(), TOKEN_USAGE_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [refreshTokenUsage]);

  useEffect(() => {
    const refreshWhenActive = () => { if (document.visibilityState === "visible") void refresh(true); };
    window.addEventListener("focus", refreshWhenActive);
    document.addEventListener("visibilitychange", refreshWhenActive);
    return () => {
      window.removeEventListener("focus", refreshWhenActive);
      document.removeEventListener("visibilitychange", refreshWhenActive);
    };
  }, [refresh]);

  useEffect(() => {
    if (hovered || preferences.pinnedProvider || snapshots.length < 2) return;
    const id = window.setInterval(() => setActiveIndex((value) => (value + 1) % snapshots.length), preferences.autoRotateSeconds * 1000);
    return () => window.clearInterval(id);
  }, [hovered, preferences.autoRotateSeconds, preferences.pinnedProvider, snapshots.length]);

  const current = preferences.pinnedProvider
    ? snapshots.find((item) => item.provider === preferences.pinnedProvider) ?? snapshots[0]
    : snapshots[activeIndex % Math.max(1, snapshots.length)];
  const displayed = (() => {
    if (!current || palettePercent === null) return current;
    const primary = getPrimaryQuota(current);
    if (primary?.kind === "short") return { ...current, shortWindow: { ...primary.window, remainingPercent: palettePercent } };
    if (primary?.kind === "weekly") return { ...current, weeklyWindow: { ...primary.window, remainingPercent: palettePercent } };
    return current;
  })();
  const activePaletteColors = paletteDraft ?? preferences.paletteColors;

  const savePreferences = useCallback((next: WidgetPreferences) => {
    const previous = preferences;
    setPreferences(next);
    setOperationError(null);
    void updatePreferences(next).catch(() => { setPreferences(previous); setOperationError("Settings could not be saved. Previous state restored."); });
  }, [preferences]);

  const handleHover = useCallback((value: boolean) => {
    hoveredRef.current = value;
    setHovered(value);
    if (value) {
      clearWidgetMotionTimers();
      void setWidgetExpanded(true).catch(() => setOperationError("Widget expand failed."));
      hoverExpandTimer.current = window.setTimeout(() => {
        hoverExpandTimer.current = null;
        if (hoveredRef.current || paletteActive.current) {
          void setWidgetClip(true);
          setCompact(false);
        }
      }, HOVER_LIFT_MS);
      void refresh(true);
      return;
    }
    if (hoverExpandTimer.current !== null) {
      window.clearTimeout(hoverExpandTimer.current);
      hoverExpandTimer.current = null;
    }
    scheduleCollapse(true);
  }, [clearWidgetMotionTimers, refresh, scheduleCollapse]);

  const handlePalettePreview = useCallback(() => {
    const requestClose = () => {
      void closePalettePreview().catch(() => {
        paletteClosing.current = false;
        setOperationError("Unable to close color preview.");
      });
    };
    if (paletteActive.current) {
      if (paletteClosing.current) return;
      paletteClosing.current = true;
      if (!paletteOpening.current) requestClose();
      return;
    }
    const primary = current ? getPrimaryQuota(current) : null;
    const initial = primary ? clampPercent(primary.window.remainingPercent) : 100;
    paletteActive.current = true;
    paletteOpening.current = true;
    paletteClosing.current = false;
    clearWidgetMotionTimers();
    setPalettePercent(initial);
    void setWidgetClip(true);
    setCompact(false);
    void setWidgetExpanded(true);
    void openPalettePreview(initial).then(() => {
      paletteOpening.current = false;
      if (paletteClosing.current) requestClose();
    }).catch(() => {
      paletteActive.current = false;
      paletteOpening.current = false;
      paletteClosing.current = false;
      setPalettePercent(null);
      setOperationError("Unable to open color preview.");
    });
  }, [clearWidgetMotionTimers, current]);

  if (!displayed) return <div className="loading-card" aria-label={t.loadingQuota}><span /><span /><span /></div>;

  return (
    <QuotaCard
      snapshot={displayed}
      preferences={preferences}
      providerCount={snapshots.length}
      onPrevious={() => setActiveIndex((value) => (value - 1 + snapshots.length) % snapshots.length)}
      onNext={() => setActiveIndex((value) => (value + 1) % snapshots.length)}
      onTogglePin={() => savePreferences({ ...preferences, pinnedProvider: preferences.pinnedProvider ? null : current.provider })}
      onLanguage={() => savePreferences({ ...preferences, language: nextLanguage(language) })}
      onLock={() => { setOperationError(null); void setAlwaysOnTop(!preferences.alwaysOnTop).then((value) => setPreferences({ ...DEFAULT_PREFS, ...value, language: normalizeLanguage(value.language) })).catch(() => setOperationError("Always-on-top toggle failed.")); }}
      onDrag={() => startDragging()}
      onHover={handleHover}
      onRefresh={() => refresh(true)}
      isConsuming={consumingProviders.has(current.provider)}
      notice={operationError}
      palettePreviewActive={palettePercent !== null}
      onPalettePreview={handlePalettePreview}
      tokenUsage={tokenUsage}
      tokenUsageStatus={tokenUsageStatus}
      conversationTokenUsage={conversationTokenUsage}
      paletteColors={activePaletteColors}
      compact={compact}
      hovered={hovered}
    />
  );
}
