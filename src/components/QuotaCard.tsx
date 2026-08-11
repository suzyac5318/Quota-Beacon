import { ArrowClockwise, ArrowDown, ArrowUp, ClockCounterClockwise, CloudSlash, PushPin, PushPinSlash, SignIn, WarningCircle } from "@phosphor-icons/react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { clampPercent, formatCompactTokens, formatDateTime, formatExactTokens, formatResetDate, formatResetTime, getPrimaryQuota } from "../lib/format";
import { copy, normalizeLanguage } from "../lib/i18n";
import { quotaThemeStyle } from "../lib/quotaTheme";
import type { ConversationTokenUsage, Language, ProviderSnapshot, TokenUsageStatus, TokenUsageSummary, WidgetPreferences } from "../types";
import { AnimatedNumber } from "./AnimatedNumber";
import { ProviderMark } from "./ProviderMark";

interface Props {
  snapshot: ProviderSnapshot;
  preferences: WidgetPreferences;
  providerCount: number;
  onPrevious: () => void;
  onNext: () => void;
  onTogglePin: () => void;
  onLock: () => void;
  onLanguage: () => void;
  onDrag: () => void;
  onHover: (hovered: boolean) => void;
  onRefresh?: () => void;
  isConsuming?: boolean;
  notice?: string | null;
  initialShowCreditTip?: boolean;
  palettePreviewActive?: boolean;
  onPalettePreview?: () => void;
  accountAlias?: string | null;
  onAccount?: () => void;
  tokenUsage?: TokenUsageSummary | null;
  tokenUsageStatus?: TokenUsageStatus;
  conversationTokenUsage?: ConversationTokenUsage;
  paletteColors?: readonly string[];
  compact?: boolean;
  hovered?: boolean;
}

function StatusIcon({ status, expired = false }: { status: ProviderSnapshot["status"]; expired?: boolean }) {
  if (status === "signed_out") return <SignIn weight="duotone" />;
  if (status === "stale" || expired) return <ClockCounterClockwise weight="duotone" />;
  if (status === "unavailable") return <CloudSlash weight="duotone" />;
  return <WarningCircle weight="duotone" />;
}

function localizedBackendMessage(message: string | null, language: Language): string | null {
  if (!message) return null;
  if (language === "en") return message;
  const normalized = message.toLowerCase();
  if (normalized.includes("sign in") || normalized.includes("login")) return "Codex 登录已失效，请重新登录。";
  if (normalized.includes("rate limited")) return "请求过于频繁，将稍后自动重试。";
  if (normalized.includes("network")) return "网络不可用，将自动重试。";
  if (normalized.includes("format")) return "额度响应格式已变化。";
  if (normalized.includes("missing the 5h")) return "额度响应缺少 5 小时窗口。";
  if (normalized.includes("refresh is already running")) return "额度正在刷新，请稍候。";
  return message;
}

export const QuotaCard = memo(function QuotaCard({
  snapshot,
  preferences,
  providerCount,
  onPrevious,
  onNext,
  onTogglePin: _onTogglePin,
  onLock,
  onLanguage,
  onDrag,
  onHover,
  onRefresh,
  isConsuming = false,
  notice = null,
  initialShowCreditTip = false,
  palettePreviewActive = false,
  onPalettePreview,
  accountAlias,
  onAccount,
  tokenUsage = null,
  tokenUsageStatus = "loading",
  conversationTokenUsage = { conversationId: null, totalTokens: null },
  paletteColors,
  compact = false,
  hovered = false,
}: Props) {
  const [showCreditTip, setShowCreditTip] = useState(initialShowCreditTip);
  const language = normalizeLanguage(preferences.language);
  const t = copy[language];
  const primaryQuota = getPrimaryQuota(snapshot);
  const primary = primaryQuota ? clampPercent(primaryQuota.window.remainingPercent) : null;
  const primaryLabel = primary === null
    ? t.unavailableStatus
    : primaryQuota?.kind === "weekly" ? t.weeklyAvailableLabel(primary) : t.availableLabel(primary);
  const weeklyOnly = primaryQuota?.kind === "weekly";
  const weekly = snapshot.weeklyWindow ? clampPercent(snapshot.weeklyWindow.remainingPercent) : null;
  const staleAge = Date.now() - new Date(snapshot.updatedAt).getTime();
  const staleExpired = snapshot.status === "stale" && staleAge > 30 * 60_000;
  const available = snapshot.status === "ok" || (snapshot.status === "stale" && !staleExpired);
  const indicatorState = isConsuming ? "active" : snapshot.status === "ok" ? "ok" : snapshot.status === "stale" ? "stale" : "error";
  const indicatorLabel = isConsuming
    ? t.active
    : snapshot.status === "ok"
      ? t.dataSynced
      : snapshot.status === "stale"
        ? t.dataStale
        : snapshot.status === "signed_out"
          ? t.notSignedIn
          : t.unavailableStatus;
  const message = localizedBackendMessage(snapshot.message, language);
  const compactTokenTotal = tokenUsageStatus === "ready" && tokenUsage
    ? formatCompactTokens(tokenUsage.totalTokens)
    : tokenUsageStatus === "loading" ? "…" : "--";
  const tokenMetricLabel = tokenUsageStatus === "ready" && tokenUsage
    ? t.tokenTotalAria(formatExactTokens(tokenUsage.totalTokens, language))
    : tokenUsageStatus === "loading" ? t.tokenLoading : t.tokenUnavailable;
  const compactConversationTotal = conversationTokenUsage.totalTokens === null
    ? "null"
    : formatCompactTokens(conversationTokenUsage.totalTokens);
  const conversationMetricLabel = conversationTokenUsage.totalTokens === null
    ? t.tokenCurrentWindowUnavailable
    : t.tokenCurrentWindowAria(formatExactTokens(conversationTokenUsage.totalTokens, language));
  const creditExpirations = useMemo(() => (snapshot.resetCreditExpiresAt ?? []).map((value, index) => {
    return t.creditItem(index, formatDateTime(value, language));
  }), [language, snapshot.resetCreditExpiresAt, t]);

  return (
    <main
      className={`quota-card quota-card--${snapshot.status}${compact ? " quota-card--compact" : ""}${hovered ? " quota-card--hovered" : ""}`}
      style={primary === null ? undefined : quotaThemeStyle(primary, paletteColors)}
      onMouseEnter={() => onHover(true)}
      onMouseLeave={() => onHover(false)}
      onMouseDown={(event) => { if (event.button === 0) void onDrag(); }}
    >
      <div className="aurora" aria-hidden="true" />
      <span className="sr-only" aria-live="polite">{available && primary !== null ? primaryLabel : message}</span>
      <div className="card-content-clip">
        <div className="collapsed-content" aria-hidden="true">
          {available && primary !== null ? (
            <section className="collapsed-metric"><AnimatedNumber value={primary} /><small>%</small></section>
          ) : (
            <section className="collapsed-unavailable"><StatusIcon status={snapshot.status} expired={staleExpired} /></section>
          )}
        </div>

        <div className="expanded-content">
          {notice ? <p className="operation-notice" role="status">{notice}</p> : null}
          <header className="card-header quota-reveal quota-reveal--0">
            <div className="account-heading">
              <button
                type="button"
                className="account-chip"
                onMouseDown={(event) => event.stopPropagation()}
                onClick={onAccount}
                disabled={!onAccount}
                aria-label={`${accountAlias ?? snapshot.displayName} · ${snapshot.plan ?? t.accountFallback}`}
              >
                <span>{accountAlias ?? snapshot.displayName}</span>
                <small>· {snapshot.plan ?? t.accountFallback} ▾</small>
              </button>
              {snapshot.status !== "stale" ? <p className="updated">{weeklyOnly ? t.weeklyRemaining : t.shortRemaining}</p> : null}
            </div>
            {!preferences.locked ? (
              <nav className="card-actions" aria-label={t.controls} onMouseDown={(event) => event.stopPropagation()}>
                {providerCount > 1 ? <button onClick={onPrevious} aria-label={t.servicePrevious}><ArrowUp /></button> : null}
                {providerCount > 1 ? <button onClick={onNext} aria-label={t.serviceNext}><ArrowDown /></button> : null}
                <span className={`usage-indicator usage-indicator--${indicatorState}`} role="status" aria-label={indicatorLabel} title={indicatorLabel}><i /></span>
                <button className="language-button" onClick={onLanguage} aria-label={t.switchLanguage} title={t.switchLanguage}>{language === "en" ? "中" : "EN"}</button>
                <button onClick={onLock} aria-label={preferences.alwaysOnTop ? t.pinOff : t.pinOn} title={preferences.alwaysOnTop ? t.pinOff : t.pinOn}>{preferences.alwaysOnTop ? <PushPin /> : <PushPinSlash />}</button>
              </nav>
            ) : null}
          </header>

          {available && primary !== null ? (
            <>
              <div className="metric-row">
                <section className="primary-metric expanded-primary" aria-label={primaryLabel}><AnimatedNumber value={primary} /><small>%</small></section>
                <section className={`token-total token-total--${tokenUsageStatus} quota-reveal quota-reveal--1`} aria-label={tokenMetricLabel} title={tokenMetricLabel}>
                  <div className="token-current" aria-label={conversationMetricLabel} title={conversationMetricLabel}>
                    <span>{t.tokenCurrentWindow}</span>
                    <strong>{compactConversationTotal}</strong>
                  </div>
                  <div className="token-cumulative">
                    <span>{t.tokenTotal}</span>
                    <strong>{compactTokenTotal}</strong>
                  </div>
                  <small>{t.tokensUnit}</small>
                </section>
              </div>
              <div className="progress quota-reveal quota-reveal--2" role="progressbar" aria-label={primaryLabel} aria-valuemin={0} aria-valuemax={100} aria-valuenow={primary}><span style={{ width: `${primary}%` }} /></div>
              <p className="reset-time quota-reveal quota-reveal--3">{formatResetTime(primaryQuota?.window.resetsAt ?? null, new Date(), language)}</p>
              <footer className="card-footer quota-reveal quota-reveal--4">
                <div className="weekly-metric">
                  {!weeklyOnly ? <p>{t.weeklyUntil(formatResetDate(snapshot.weeklyWindow?.resetsAt ?? null, language))}</p> : null}
                  {!weeklyOnly ? <strong>{weekly ?? "--"}<small>{weekly === null ? "" : "%"}</small></strong> : null}
                  <div className="reset-credit-row" onMouseDown={(event) => event.stopPropagation()}>
                    <span>{snapshot.resetCredits === null ? t.resetCreditUnknown : t.resetCredits(snapshot.resetCredits)}</span>
                    {snapshot.resetCredits !== null && snapshot.resetCredits > 0 ? <button type="button" className="reset-credit-button" onClick={() => setShowCreditTip((value) => !value)} aria-expanded={showCreditTip} aria-label={t.view}>{t.view}</button> : null}
                  </div>
                  {showCreditTip ? <div className="reset-credit-tip" role="status" onMouseDown={(event) => event.stopPropagation()}>{creditExpirations.length > 0 ? creditExpirations.map((item) => <p key={item}>{item}</p>) : <p>{t.noCreditExpiration}</p>}</div> : null}
                </div>
                <ProviderMark active={palettePreviewActive} onClick={onPalettePreview} />
              </footer>
            </>
          ) : (
            <section className="error-state" aria-live="polite">
              <div className="status-icon quota-reveal quota-reveal--0" aria-hidden="true"><StatusIcon status={snapshot.status} expired={staleExpired} /></div>
              <strong className="quota-reveal quota-reveal--1">{snapshot.status === "signed_out" ? t.signedInRequired : staleExpired ? t.staleExpired : t.temporarilyUnavailable}</strong>
              <p className="quota-reveal quota-reveal--2">{message ?? t.errorUnavailable}</p>
              {snapshot.status === "stale" ? <button type="button" className="error-refresh-button quota-reveal quota-reveal--3" onMouseDown={(event) => event.stopPropagation()} onClick={onRefresh} disabled={!onRefresh} aria-label={t.refreshQuota}><ArrowClockwise /><span>{t.refresh}</span></button> : null}
            </section>
          )}
        </div>
      </div>
    </main>
  );
});

export const QuotaOrb = memo(function QuotaOrb({ snapshot, onDrag, onHover, language = "zh-CN", paletteColors }: Pick<Props, "snapshot" | "onDrag" | "onHover" | "paletteColors"> & { language?: Language }) {
  const [idle, setIdle] = useState(false);
  const idleTimer = useRef<number | null>(null);
  const activeLanguage = normalizeLanguage(language);
  const t = copy[activeLanguage];
  const primaryQuota = getPrimaryQuota(snapshot);
  const primary = primaryQuota ? clampPercent(primaryQuota.window.remainingPercent) : null;
  const primaryLabel = primary === null
    ? t.unavailableStatus
    : primaryQuota?.kind === "weekly" ? t.weeklyAvailableLabel(primary) : t.availableLabel(primary);
  const available = snapshot.status === "ok" && primary !== null;

  useEffect(() => {
    idleTimer.current = window.setTimeout(() => setIdle(true), 2000);
    return () => {
      if (idleTimer.current !== null) window.clearTimeout(idleTimer.current);
    };
  }, []);

  const handleMouseEnter = () => {
    if (idleTimer.current !== null) window.clearTimeout(idleTimer.current);
    setIdle(false);
    onHover(true);
  };

  return (
    <main
      className={`quota-orb quota-card--${snapshot.status}${idle ? " quota-orb--idle" : ""}`}
      style={primary === null ? undefined : quotaThemeStyle(primary, paletteColors)}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={() => onHover(false)}
      onMouseDown={(event) => { if (event.button === 0) void onDrag(); }}
      aria-label={available ? primaryLabel : localizedBackendMessage(snapshot.message, activeLanguage) ?? t.unavailableStatus}
    >
      <div className="aurora" aria-hidden="true" />
      {available ? (
        <section className="orb-metric">
          <span>{primary}</span>
          <small>%</small>
        </section>
      ) : (
        <section className="orb-unavailable">
          <StatusIcon status={snapshot.status} />
        </section>
      )}
    </main>
  );
});
