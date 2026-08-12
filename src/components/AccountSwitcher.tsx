import { Check, PencilSimple, Plus, Trash } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  accountCopy,
  beginAccountLogin,
  cancelAccountLogin,
  deleteAccount,
  getAccountVault,
  getAccountWeeklyQuotas,
  listenAccountEvents,
  pollAccountLogin,
  renameAccount,
  saveCurrentAccount,
  switchAccount,
  type AccountLoginStatus,
  type AccountWindowTheme,
  type AccountWeeklyQuota,
  type AccountVault,
} from "../lib/accounts";
import { getPreferences, setAccountSwitcherExpanded } from "../lib/bridge";
import { normalizeLanguage } from "../lib/i18n";
import { quotaThemeStyle } from "../lib/quotaTheme";
import type { Language } from "../types";

export function AccountSwitcher() {
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [vault, setVault] = useState<AccountVault | null>(null);
  const [alias, setAlias] = useState("");
  const [login, setLogin] = useState<AccountLoginStatus | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [addFormOpen, setAddFormOpen] = useState(false);
  const [weeklyQuotas, setWeeklyQuotas] = useState<Map<string, AccountWeeklyQuota>>(() => new Map());
  const [windowTheme, setWindowTheme] = useState<AccountWindowTheme | null>(null);
  const aliasInputRef = useRef<HTMLInputElement>(null);
  const t = useMemo(() => accountCopy(language), [language]);

  useEffect(() => {
    void getPreferences().then((preferences) => setLanguage(normalizeLanguage(preferences.language))).catch(() => undefined);
    void getAccountVault().then(setVault).catch((error) => setNotice(String(error)));
    let cleanup = () => {};
    void listenAccountEvents({
      onVault: setVault,
      onSwitched: () => setNotice(t.switched),
      onOpened: (theme) => { setWindowTheme(theme); setAddFormOpen(false); setAlias(""); },
      onThemeChanged: setWindowTheme,
      onError: setNotice,
    }).then((value) => { cleanup = value; });
    return () => cleanup();
  }, [t.switched]);

  useEffect(() => {
    if (notice !== t.switched) return;
    const timer = window.setTimeout(() => {
      setNotice((current) => current === t.switched ? null : current);
    }, 10_000);
    return () => window.clearTimeout(timer);
  }, [notice, t.switched]);

  useEffect(() => {
    let disposed = false;
    let inFlight = false;
    const refreshWeeklyQuotas = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const values = await getAccountWeeklyQuotas();
        if (!disposed) setWeeklyQuotas(new Map(values.map((value) => [value.profileId, value])));
      } catch {
        // Preserve the last successful values; account operations remain independently usable.
      } finally {
        inFlight = false;
      }
    };
    void refreshWeeklyQuotas();
    const timer = window.setInterval(() => void refreshWeeklyQuotas(), 10_000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, []);

  useEffect(() => {
    if (!login || login.status !== "running") return;
    const timer = window.setInterval(() => {
      void pollAccountLogin(login.taskId).then((status) => {
        setLogin(status);
        if (status.status === "completed") {
          setAlias("");
          setAddFormOpen(false);
          void getAccountVault().then(setVault);
        } else if (status.status === "failed") {
          setNotice(status.message);
        }
      }).catch((error) => setNotice(String(error)));
    }, 800);
    return () => window.clearInterval(timer);
  }, [login]);

  const run = async (profileId: string, operation: () => Promise<AccountVault>) => {
    setBusyId(profileId);
    setNotice(null);
    try { setVault(await operation()); } catch (error) { setNotice(String(error)); } finally { setBusyId(null); }
  };

  const handleSaveCurrent = async () => {
    setBusyId("save-current");
    setNotice(null);
    try { setVault(await saveCurrentAccount(alias)); setAlias(""); setAddFormOpen(false); } catch (error) { setNotice(String(error)); } finally { setBusyId(null); }
  };

  const handleAdd = async (replaceProfileId: string | null = null, nextAlias = alias) => {
    setNotice(null);
    try { setLogin(await beginAccountLogin(nextAlias, replaceProfileId)); } catch (error) { setNotice(String(error)); }
  };

  const handleCancel = async () => {
    if (!login) return;
    await cancelAccountLogin(login.taskId).catch((error) => setNotice(String(error)));
    setLogin(null);
  };

  const toggleAddForm = () => {
    if (login?.status === "running") return;
    if (addFormOpen) setAlias("");
    setAddFormOpen((open) => !open);
  };

  const addFormVisible = addFormOpen || login?.status === "running";
  const noticeVisible = login?.status === "running" || notice !== null;

  useEffect(() => {
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    void setAccountSwitcherExpanded(addFormVisible, reducedMotion, noticeVisible).catch((error) => setNotice(String(error)));
    if (addFormVisible) aliasInputRef.current?.focus();
  }, [addFormVisible, noticeVisible]);

  return (
    <main
      className={`account-switcher${addFormVisible ? " account-switcher--expanded" : ""}${windowTheme?.percent == null ? " account-switcher--neutral" : ""}`}
      style={windowTheme?.percent == null ? undefined : quotaThemeStyle(windowTheme.percent, windowTheme.colors)}
      aria-label={t.title}
    >
      <header className="account-switcher__header">
        <div><h1>{t.title}</h1><p>{t.localTokens}</p></div>
        <button type="button" onClick={toggleAddForm} disabled={login?.status === "running"} aria-label={t.add} title={t.add} aria-expanded={addFormVisible} aria-controls="account-add-form"><Plus /></button>
      </header>

      <section className="account-switcher__body" aria-live="polite">
        {!vault ? <p className="account-empty">…</p> : null}
        {vault && !vault.currentLoginSaved ? <p className="account-empty">{t.empty}</p> : null}
        {vault?.profiles.map((profile) => {
          const quota = weeklyQuotas.get(profile.id);
          const needsLogin = profile.credentialStatus === "invalid" || quota?.status === "signed_out";
          return (
          <article className={`account-row${profile.isActive ? " account-row--active" : ""}`} key={profile.id}>
            <span className="account-row__status" aria-hidden="true">{profile.isActive ? <Check /> : null}</span>
            <div className="account-row__identity"><strong>{profile.alias}</strong><small>{profile.maskedEmail ?? "—"}</small><span className="account-row__quota" title={quota?.message ?? undefined}>{(() => {
              if (needsLogin) return t.quotaExpired;
              if (quota?.remainingPercent != null) return `${t.weekly} ${Math.round(quota.remainingPercent)}%`;
              return t.quotaUnavailable;
            })()}</span></div>
            <span className="account-row__label">{profile.isActive ? t.current : ""}</span>
            {!profile.isActive && !needsLogin ? <button type="button" disabled={busyId !== null} onClick={() => { setBusyId(profile.id); setNotice(null); void switchAccount(profile.id).then(() => getAccountVault()).then(setVault).catch((error) => setNotice(String(error))).finally(() => setBusyId(null)); }}>{t.switch}</button> : null}
            {needsLogin ? <button type="button" disabled={login?.status === "running"} onClick={() => void handleAdd(profile.id, profile.alias)}>{t.invalid}</button> : null}
            <button type="button" className="account-icon-button account-icon-button--rename" disabled={busyId !== null} aria-label={`${t.rename} ${profile.alias}`} title={t.rename} onClick={() => { const next = window.prompt(t.alias, profile.alias); if (next !== null) void run(profile.id, () => renameAccount(profile.id, next)); }}><PencilSimple /></button>
            <button type="button" className="account-icon-button account-icon-button--delete account-icon-button--danger" disabled={busyId !== null} aria-label={`${t.remove} ${profile.alias}`} title={t.remove} onClick={() => { if (window.confirm(`${t.remove} ${profile.alias}?`)) void run(profile.id, () => deleteAccount(profile.id)); }}><Trash /></button>
          </article>
          );
        })}
      </section>

      <div className={`account-switcher__form-shell${addFormVisible ? " account-switcher__form-shell--open" : ""}`} aria-hidden={!addFormVisible} inert={!addFormVisible}>
        <footer className="account-switcher__footer" id="account-add-form">
          <label><span>{t.alias}</span><input ref={aliasInputRef} value={alias} maxLength={32} disabled={!addFormVisible} onChange={(event) => setAlias(event.target.value)} /></label>
          {login?.status === "running" ? (
            <button type="button" className="account-secondary" onClick={() => void handleCancel()}>{t.cancel}</button>
          ) : !vault?.currentLoginSaved ? (
            <button type="button" disabled={!alias.trim() || busyId !== null} onClick={() => void handleSaveCurrent()}>{t.saveCurrent}</button>
          ) : (
            <button type="button" disabled={!alias.trim()} onClick={() => void handleAdd()}><Plus />{t.add}</button>
          )}
        </footer>
      </div>
      {login?.status === "running" ? <p className="account-notice" role="status">{t.browser}</p> : notice ? <p className="account-notice" role="status">{notice}</p> : null}
    </main>
  );
}
