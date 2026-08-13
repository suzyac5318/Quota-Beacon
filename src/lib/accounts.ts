import type { Language } from "../types";
import { isTauri } from "./bridge";
import { createLatestRequestGate } from "./latestRequest";

export type AccountCredentialStatus = "ready" | "invalid" | "missing" | "locked" | "denied" | "unavailable";

export interface AccountProfile {
  id: string;
  alias: string;
  maskedEmail: string | null;
  isActive: boolean;
  credentialStatus: AccountCredentialStatus;
}

export interface AccountVault {
  profiles: AccountProfile[];
  activeProfileId: string | null;
  hasCurrentLogin: boolean;
  currentLoginSaved: boolean;
}

export interface AccountSwitchOutcome {
  profile: AccountProfile;
}

export interface AccountLoginStatus {
  taskId: string;
  status: "running" | "completed" | "failed";
  message: string | null;
}

export interface AccountWeeklyQuota {
  profileId: string;
  remainingPercent: number | null;
  status: "ok" | "loading" | "unavailable" | "signed_out";
  message: string | null;
}

export interface AccountWindowTheme {
  percent: number | null;
  colors: string[];
}

const mockVault: AccountVault = {
  profiles: [
    { id: "demo-personal", alias: "个人号", maskedEmail: "p***@example.com", isActive: true, credentialStatus: "ready" },
    { id: "demo-work", alias: "工作号", maskedEmail: "w***@example.com", isActive: false, credentialStatus: "ready" },
  ],
  activeProfileId: "demo-personal",
  hasCurrentLogin: true,
  currentLoginSaved: true,
};

export async function getAccountVault(): Promise<AccountVault> {
  if (!isTauri()) return mockVault;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountVault>("get_account_vault");
}

export async function getAccountWeeklyQuotas(): Promise<AccountWeeklyQuota[]> {
  if (!isTauri()) return [
    { profileId: "demo-personal", remainingPercent: 68, status: "ok", message: null },
    { profileId: "demo-work", remainingPercent: 42, status: "ok", message: null },
  ];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountWeeklyQuota[]>("get_account_weekly_quotas");
}

export async function saveCurrentAccount(alias: string): Promise<AccountVault> {
  if (!isTauri()) return { ...mockVault, profiles: [{ ...mockVault.profiles[0], alias }] };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountVault>("save_current_account", { alias });
}

export async function renameAccount(profileId: string, alias: string): Promise<AccountVault> {
  if (!isTauri()) return { ...mockVault, profiles: mockVault.profiles.map((profile) => profile.id === profileId ? { ...profile, alias } : profile) };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountVault>("rename_account", { profileId, alias });
}

export async function deleteAccount(profileId: string): Promise<AccountVault> {
  if (!isTauri()) return { ...mockVault, profiles: mockVault.profiles.filter((profile) => profile.id !== profileId) };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountVault>("delete_account", { profileId });
}

export async function switchAccount(profileId: string): Promise<AccountSwitchOutcome> {
  if (!isTauri()) {
    const profile = mockVault.profiles.find((item) => item.id === profileId) ?? mockVault.profiles[0];
    return { profile };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountSwitchOutcome>("switch_account", { profileId });
}

export async function beginAccountLogin(alias: string, replaceProfileId: string | null = null): Promise<AccountLoginStatus> {
  if (!isTauri()) return { taskId: "demo-task", status: "running", message: null };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountLoginStatus>("begin_account_login", { alias, replaceProfileId });
}

export async function pollAccountLogin(taskId: string): Promise<AccountLoginStatus> {
  if (!isTauri()) return { taskId, status: "completed", message: null };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountLoginStatus>("poll_account_login", { taskId });
}

export async function cancelAccountLogin(taskId: string): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("cancel_account_login", { taskId });
}

export async function openAccountSwitcher(theme: AccountWindowTheme): Promise<AccountVault> {
  if (!isTauri()) return mockVault;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountVault>("open_account_switcher", { theme });
}

export async function updateAccountSwitcherTheme(theme: AccountWindowTheme): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("update_account_switcher_theme", { theme });
}

export async function closeAccountSwitcher(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("close_account_switcher");
}

const accountSizeAnimationGate = createLatestRequestGate();

export async function setAccountSwitcherExpanded(expanded: boolean, reducedMotion = false): Promise<void> {
  if (!isTauri()) return;
  const generation = accountSizeAnimationGate.begin();
  const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
  const appWindow = getCurrentWindow();
  const targetHeight = expanded ? 280 : 240;
  if (!accountSizeAnimationGate.isCurrent(generation)) return;
  if (reducedMotion) {
    await appWindow.setSize(new LogicalSize(320, targetHeight));
    return;
  }
  const scale = await appWindow.scaleFactor();
  const start = (await appWindow.innerSize()).height / scale;
  const startedAt = performance.now();
  while (true) {
    if (!accountSizeAnimationGate.isCurrent(generation)) return;
    const progress = Math.min(1, (performance.now() - startedAt) / 220);
    const eased = 1 - Math.pow(1 - progress, 3);
    await appWindow.setSize(new LogicalSize(320, start + (targetHeight - start) * eased));
    if (!accountSizeAnimationGate.isCurrent(generation)) return;
    if (progress >= 1) return;
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }
}

export async function listenAccountEvents(handlers: {
  onVault: (vault: AccountVault) => void;
  onSwitched?: (outcome: AccountSwitchOutcome) => void;
  onOpened?: (theme: AccountWindowTheme) => void;
  onThemeChanged?: (theme: AccountWindowTheme) => void;
  onClosed?: () => void;
  onError?: (message: string) => void;
}): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlistenVault = await listen<AccountVault>("account-vault-changed", (event) => handlers.onVault(event.payload));
  const unlistenSwitched = await listen<AccountSwitchOutcome>("account-switch-completed", (event) => handlers.onSwitched?.(event.payload));
  const unlistenOpened = await listen<AccountWindowTheme>("account-switcher-opened", (event) => handlers.onOpened?.(event.payload));
  const unlistenTheme = await listen<AccountWindowTheme>("account-switcher-theme-changed", (event) => handlers.onThemeChanged?.(event.payload));
  const unlistenClosed = await listen("account-switcher-closed", () => handlers.onClosed?.());
  const unlistenError = await listen<string>("account-operation-error", (event) => handlers.onError?.(event.payload));
  return () => { unlistenVault(); unlistenSwitched(); unlistenOpened(); unlistenTheme(); unlistenClosed(); unlistenError(); };
}

export function accountCopy(language: Language) {
  return language === "en" ? {
    title: "Codex accounts", close: "Close", saveCurrent: "Save current", alias: "Account name",
    add: "Add account", cancel: "Cancel login", current: "Current", switch: "Switch", rename: "Rename", remove: "Delete",
    empty: "Save the current Codex login before adding another account.", browser: "Complete the official Codex sign-in in your browser.",
    switched: "Account switched. Quota is refreshing.", invalid: "Sign in again", locked: "Keychain locked", denied: "Keychain access denied", unavailable: "Keychain unavailable", weekly: "Week", quotaUnavailable: "Week --", localTokens: "Token totals remain cumulative for this Mac.",
  } : {
    title: "Codex 账号", close: "关闭", saveCurrent: "保存当前账号", alias: "账号名称",
    add: "添加账号", cancel: "取消登录", current: "当前使用", switch: "切换", rename: "重命名", remove: "删除",
    empty: "请先显式保存当前 Codex 登录，再添加其他账号。", browser: "请在浏览器中完成官方 Codex 登录。",
    switched: "账号已切换，正在刷新额度。", invalid: "重新登录", locked: "钥匙串已锁定", denied: "钥匙串拒绝访问", unavailable: "钥匙串不可用", weekly: "周", quotaUnavailable: "周 --", localTokens: "Token 统计继续累计此 Mac 上的全部 Codex 会话。",
  };
}
