import { Check, PencilSimple, Plus, Trash } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  accountCopy,
  beginAccountLogin,
  cancelAccountLogin,
  deleteAccount,
  getAccountVault,
  listenAccountEvents,
  pollAccountLogin,
  renameAccount,
  saveCurrentAccount,
  switchAccount,
  switchAccountAndRestartCodex,
  type AccountLoginStatus,
  type AccountVault,
} from "../lib/accounts";
import { getPreferences, setAccountSwitcherExpanded } from "../lib/bridge";
import { normalizeLanguage } from "../lib/i18n";
import type { Language } from "../types";

export function AccountSwitcher() {
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [vault, setVault] = useState<AccountVault | null>(null);
  const [alias, setAlias] = useState("");
  const [login, setLogin] = useState<AccountLoginStatus | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [addFormOpen, setAddFormOpen] = useState(false);
  const aliasInputRef = useRef<HTMLInputElement>(null);
  const t = useMemo(() => accountCopy(language), [language]);

  useEffect(() => {
    void getPreferences().then((preferences) => setLanguage(normalizeLanguage(preferences.language))).catch(() => undefined);
    void getAccountVault().then(setVault).catch((error) => setNotice(String(error)));
    let cleanup = () => {};
    void listenAccountEvents({
      onVault: setVault,
      onSwitched: () => setNotice(t.switched),
      onOpened: () => { setAddFormOpen(false); setAlias(""); },
      onError: setNotice,
    }).then((value) => { cleanup = value; });
    return () => cleanup();
  }, [t.switched]);

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

  useEffect(() => {
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    void setAccountSwitcherExpanded(addFormVisible, reducedMotion).catch((error) => setNotice(String(error)));
    if (addFormVisible) aliasInputRef.current?.focus();
  }, [addFormVisible]);

  return (
    <main className={`account-switcher${addFormVisible ? " account-switcher--expanded" : ""}`} aria-label={t.title}>
      <header className="account-switcher__header">
        <div><h1>{t.title}</h1><p>{t.localTokens}</p></div>
        <button type="button" onClick={toggleAddForm} disabled={login?.status === "running"} aria-label={t.add} title={t.add} aria-expanded={addFormVisible} aria-controls="account-add-form"><Plus /></button>
      </header>

      <section className="account-switcher__body" aria-live="polite">
        {!vault ? <p className="account-empty">…</p> : null}
        {vault && !vault.currentLoginSaved ? <p className="account-empty">{t.empty}</p> : null}
        {vault?.profiles.map((profile) => (
          <article className={`account-row${profile.isActive ? " account-row--active" : ""}`} key={profile.id}>
            <span className="account-row__status" aria-hidden="true">{profile.isActive ? <Check /> : null}</span>
            <div><strong>{profile.alias}</strong><small>{profile.maskedEmail ?? "—"}</small></div>
            <span className="account-row__label">{profile.isActive ? t.current : profile.credentialStatus === "invalid" ? t.invalid : ""}</span>
            {!profile.isActive && profile.credentialStatus === "ready" ? <button type="button" disabled={busyId !== null} onClick={() => { setBusyId(profile.id); setNotice(null); void switchAccount(profile.id).then(() => getAccountVault()).then(setVault).catch((error) => setNotice(String(error))).finally(() => setBusyId(null)); }}>{t.switch}</button> : null}
            {!profile.isActive && profile.credentialStatus === "ready" ? <button type="button" className="account-restart-button" disabled={busyId !== null} onClick={() => { if (!window.confirm(t.restartConfirm)) return; setBusyId(profile.id); setNotice(null); void switchAccountAndRestartCodex(profile.id, true).catch((error) => setNotice(String(error))).finally(() => setBusyId(null)); }}>{t.restart}</button> : null}
            {profile.credentialStatus === "invalid" ? <button type="button" disabled={login?.status === "running"} onClick={() => void handleAdd(profile.id, profile.alias)}>{t.invalid}</button> : null}
            <button type="button" className="account-icon-button account-icon-button--rename" disabled={busyId !== null} aria-label={`${t.rename} ${profile.alias}`} title={t.rename} onClick={() => { const next = window.prompt(t.alias, profile.alias); if (next !== null) void run(profile.id, () => renameAccount(profile.id, next)); }}><PencilSimple /></button>
            <button type="button" className="account-icon-button account-icon-button--delete account-icon-button--danger" disabled={busyId !== null} aria-label={`${t.remove} ${profile.alias}`} title={t.remove} onClick={() => { if (window.confirm(`${t.remove} ${profile.alias}?`)) void run(profile.id, () => deleteAccount(profile.id)); }}><Trash /></button>
          </article>
        ))}
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
