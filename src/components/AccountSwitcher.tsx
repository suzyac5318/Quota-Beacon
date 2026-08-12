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
  type AccountWeeklyQuota,
  type AccountVault,
} from "../lib/accounts";
import { getPreferences } from "../lib/bridge";
import { normalizeLanguage } from "../lib/i18n";
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
  const noticeSequence = useRef(0);
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
      onError: (message) => showNotice(message),
    }).then((unlisten) => { if (cancelled) unlisten(); else cleanup = unlisten; });
    return () => { cancelled = true; cleanup(); };
  }, [showNotice, t.switched]);

  useEffect(() => {
    if (notice?.kind !== "success") return;
    const noticeId = notice.id;
    const timer = window.setTimeout(() => {
      setNotice((current) => current?.id === noticeId ? null : current);
    }, 10_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

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
        // Keep the last successful values; account operations remain independently usable.
      } finally {
        inFlight = false;
      }
    };
    void refreshWeeklyQuotas();
    const timer = window.setInterval(() => void refreshWeeklyQuotas(), 10_000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [quotaScope]);

  useEffect(() => {
    void setAccountSwitcherExpanded(addOpen || Boolean(notice), reducedMotion);
  }, [addOpen, notice, reducedMotion]);

  useEffect(() => {
    if (!login || login.status !== "running") return;
    const id = window.setInterval(() => {
      void pollAccountLogin(login.taskId).then((status) => {
        setLogin(status);
        if (status.status === "completed") {
          setAlias("");
          setAddOpen(false);
          setNotice(null);
          void getAccountVault().then(setVault);
        } else if (status.status === "failed") {
          showNotice(status.message ?? "Codex login failed.");
        }
      }).catch((error) => showNotice(String(error)));
    }, 750);
    return () => window.clearInterval(id);
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
    <main className="account-switcher" aria-label={t.title}>
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
          const needsLogin = profile.credentialStatus === "invalid" || quota?.status === "signed_out";
          return (
          <article className={`account-row${profile.isActive ? " account-row--active" : ""}`} key={profile.id}>
            <span className="account-row__state" aria-hidden="true">{profile.isActive ? <CheckCircle weight="fill" /> : <SignIn />}</span>
            <div className="account-row__identity">
              <strong>{profile.alias}</strong>
              <small>{profile.maskedEmail ?? (profile.credentialStatus === "invalid" ? t.invalid : "Codex")}</small>
              <span className="account-row__quota" title={quota?.message ?? undefined}>{needsLogin ? t.invalid : quota?.remainingPercent != null ? `${t.weekly} ${Math.round(quota.remainingPercent)}%` : t.quotaUnavailable}</span>
            </div>
            {profile.isActive ? <span className="account-row__current">{t.current}</span> : (
              <button type="button" className="account-row__switch" disabled={busy !== null || needsLogin} onClick={() => void run(profile.id, async () => { await switchAccount(profile.id); return getAccountVault(); })}>{t.switch}</button>
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
