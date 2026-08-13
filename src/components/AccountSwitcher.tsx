import { CheckCircle, PencilSimple, Plus, SignIn, Trash, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  accountCopy,
  beginAccountLogin,
  cancelAccountLogin,
  closeAccountSwitcher,
  deleteAccount,
  getAccountVault,
  getAccountWeeklyQuotas,
  listenAccountEvents,
  pollAccountLogin,
  renameAccount,
  saveCurrentAccount,
  setAccountSwitcherExpanded,
  switchAccount,
  type AccountLoginStatus,
  type AccountWindowTheme,
  type AccountWeeklyQuota,
  type AccountVault,
} from "../lib/accounts";
import { getPreferences } from "../lib/bridge";
import { normalizeLanguage } from "../lib/i18n";
import { quotaThemeStyle } from "../lib/quotaTheme";
import type { Language } from "../types";

export function AccountSwitcher() {
  const [vault, setVault] = useState<AccountVault | null>(null);
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [alias, setAlias] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [login, setLogin] = useState<AccountLoginStatus | null>(null);
  const [notice, setNotice] = useState<{ id: number; kind: "error" | "info" | "success"; message: string } | null>(null);
  const [weeklyQuotas, setWeeklyQuotas] = useState<Map<string, AccountWeeklyQuota>>(() => new Map());
  const [windowTheme, setWindowTheme] = useState<AccountWindowTheme | null>(null);
  const [windowOpen, setWindowOpen] = useState(false);
  const noticeSequence = useRef(0);
  const loginRef = useRef<AccountLoginStatus | null>(null);
  const t = useMemo(() => accountCopy(language), [language]);
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const quotaScope = vault ? `${vault.activeProfileId ?? ""}:${vault.profiles.map((profile) => `${profile.id}:${profile.credentialStatus}`).join("|")}` : "";
  const showNotice = useCallback((message: string, kind: "error" | "info" | "success" = "error") => {
    setNotice({ id: ++noticeSequence.current, kind, message });
  }, []);

  useEffect(() => {
    void getPreferences().then((value) => setLanguage(normalizeLanguage(value.language)));
    void getAccountVault().then(setVault).catch((error) => showNotice(String(error)));
    let cancelled = false;
    let cleanup = () => {};
    void listenAccountEvents({
      onVault: setVault,
      onSwitched: () => showNotice(t.switched, "success"),
      onOpened: (theme) => {
        const loginRunning = loginRef.current?.status === "running";
        setAlias("");
        setAddOpen(loginRunning);
        setNotice(loginRunning
          ? { id: ++noticeSequence.current, kind: "info", message: t.browser }
          : null);
        setWeeklyQuotas(new Map());
        setLogin((current) => current?.status === "running" ? current : null);
        setWindowTheme(theme);
        setWindowOpen(true);
      },
      onClosed: () => {
        setWindowOpen(false);
        setWeeklyQuotas(new Map());
      },
      onThemeChanged: setWindowTheme,
      onError: (message) => showNotice(message),
    }).then((unlisten) => { if (cancelled) unlisten(); else cleanup = unlisten; });
    return () => { cancelled = true; cleanup(); };
  }, [showNotice, t.browser, t.switched]);

  useEffect(() => {
    loginRef.current = login;
  }, [login]);

  useEffect(() => {
    if (notice?.kind !== "success") return;
    const noticeId = notice.id;
    const timer = window.setTimeout(() => {
      setNotice((current) => current?.id === noticeId ? null : current);
    }, 10_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    if (!windowOpen) return;
    let disposed = false;
    let inFlight = false;
    const refreshWeeklyQuotas = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const values = await getAccountWeeklyQuotas();
        if (!disposed) setWeeklyQuotas(new Map(values.map((value) => [value.profileId, value])));
      } catch {
        // Keep the last successful values; account operations remain independently usable.
      } finally {
        inFlight = false;
      }
    };
    void refreshWeeklyQuotas();
    const timer = window.setInterval(() => void refreshWeeklyQuotas(), 10_000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [quotaScope, windowOpen]);

  useEffect(() => {
    void setAccountSwitcherExpanded(addOpen || Boolean(notice), reducedMotion);
  }, [addOpen, notice, reducedMotion]);

  useEffect(() => {
    if (!login || login.status !== "running") return;
    let disposed = false;
    let timer: number | null = null;
    let inFlight = false;
    const poll = async () => {
      if (disposed || inFlight) return;
      inFlight = true;
      try {
        const status = await pollAccountLogin(login.taskId);
        if (disposed) return;
        setLogin(status);
        if (status.status === "completed") {
          setAlias("");
          setAddOpen(false);
          setNotice(null);
          void getAccountVault().then(setVault);
        } else if (status.status === "failed") {
          showNotice(status.message ?? "Codex login failed.");
        } else {
          timer = window.setTimeout(() => void poll(), 750);
        }
      } catch (error) {
        if (!disposed) {
          showNotice(String(error));
          timer = window.setTimeout(() => void poll(), 750);
        }
      } finally {
        inFlight = false;
      }
    };
    timer = window.setTimeout(() => void poll(), 750);
    return () => {
      disposed = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [login, showNotice]);

  const run = useCallback(async (key: string, action: () => Promise<AccountVault>) => {
    setBusy(key);
    setNotice(null);
    try {
      setVault(await action());
    } catch (error) {
      showNotice(String(error));
    } finally {
      setBusy(null);
    }
  }, [showNotice]);

  const beginLogin = useCallback(async (replaceProfileId: string | null = null) => {
    if (!alias.trim() && !replaceProfileId) return;
    setBusy(replaceProfileId ?? "login");
    showNotice(t.browser, "info");
    try {
      const profile = replaceProfileId ? vault?.profiles.find((item) => item.id === replaceProfileId) : null;
      const status = await beginAccountLogin(profile?.alias ?? alias, replaceProfileId);
      setLogin(status);
    } catch (error) {
      showNotice(String(error));
    } finally {
      setBusy(null);
    }
  }, [alias, showNotice, t.browser, vault?.profiles]);

  const close = () => void closeAccountSwitcher().catch((error) => showNotice(String(error)));

  return (
    <main
      className={`account-switcher${windowTheme?.percent == null ? " account-switcher--neutral" : ""}`}
      style={windowTheme?.percent == null ? undefined : quotaThemeStyle(windowTheme.percent, windowTheme.colors)}
      aria-label={t.title}
    >
      <header className="account-switcher__header">
        <div><strong>{t.title}</strong><small>macOS Keychain</small></div>
        <div className="account-switcher__header-actions">
          <button type="button" onClick={() => { setAddOpen((value) => !value); setNotice(null); }} aria-label={t.add} aria-expanded={addOpen}><Plus /></button>
          <button type="button" onClick={close} aria-label={t.close}><X /></button>
        </div>
      </header>

      {notice ? <p className="account-switcher__notice" role="status">{notice.message}</p> : null}

      <section className="account-list" aria-live="polite">
        {vault?.profiles.map((profile) => {
          const quota = weeklyQuotas.get(profile.id);
          const needsLogin = profile.credentialStatus === "invalid" || profile.credentialStatus === "missing" || (profile.credentialStatus === "ready" && quota?.status === "signed_out");
          const credentialBlocked = profile.credentialStatus !== "ready" || quota?.status === "signed_out";
          const credentialMessage = profile.credentialStatus === "locked"
            ? t.locked
            : profile.credentialStatus === "denied"
              ? t.denied
              : profile.credentialStatus === "unavailable"
                ? t.unavailable
                : t.invalid;
          return (
          <article className={`account-row${profile.isActive ? " account-row--active" : ""}`} key={profile.id}>
            <span className="account-row__state" aria-hidden="true">{profile.isActive ? <CheckCircle weight="fill" /> : <SignIn />}</span>
            <div className="account-row__identity">
              <strong>{profile.alias}</strong>
              <small>{profile.maskedEmail ?? (profile.credentialStatus === "ready" ? "Codex" : credentialMessage)}</small>
              <span className="account-row__quota" title={quota?.message ?? undefined}>{credentialBlocked ? credentialMessage : quota?.remainingPercent != null ? `${t.weekly} ${Math.round(quota.remainingPercent)}%` : t.quotaUnavailable}</span>
            </div>
            {profile.isActive ? <span className="account-row__current">{t.current}</span> : (
              <button type="button" className="account-row__switch" disabled={busy !== null || credentialBlocked} onClick={() => void run(profile.id, async () => { await switchAccount(profile.id); return getAccountVault(); })}>{t.switch}</button>
            )}
            {needsLogin ? <button type="button" className="account-icon-button" onClick={() => void beginLogin(profile.id)} aria-label={t.invalid}><SignIn /></button> : null}
            <button type="button" className="account-icon-button" onClick={() => {
              const next = window.prompt(t.alias, profile.alias);
              if (next) void run(`rename-${profile.id}`, () => renameAccount(profile.id, next));
            }} aria-label={t.rename}><PencilSimple /></button>
            <button type="button" className="account-icon-button" disabled={profile.isActive} onClick={() => {
              if (window.confirm(`${t.remove} “${profile.alias}”?`)) void run(`delete-${profile.id}`, () => deleteAccount(profile.id));
            }} aria-label={t.remove}><Trash /></button>
          </article>
          );
        })}
        {vault && vault.profiles.length === 0 ? <p className="account-list__empty">{t.empty}</p> : null}
      </section>

      {addOpen ? (
        <form className="account-add" onSubmit={(event) => { event.preventDefault(); void beginLogin(); }}>
          <label><span>{t.alias}</span><input value={alias} maxLength={32} autoFocus onChange={(event) => setAlias(event.target.value)} /></label>
          <div>
            {vault?.hasCurrentLogin && !vault.currentLoginSaved ? <button type="button" disabled={!alias.trim() || busy !== null} onClick={() => void run("save", () => saveCurrentAccount(alias))}>{t.saveCurrent}</button> : null}
            <button type="submit" disabled={!alias.trim() || busy !== null || login?.status === "running"}>{t.add}</button>
            {login?.status === "running" ? <button type="button" onClick={() => void cancelAccountLogin(login.taskId).then(() => setLogin(null))}>{t.cancel}</button> : null}
          </div>
        </form>
      ) : null}
      <footer>{t.localTokens}</footer>
    </main>
  );
}
