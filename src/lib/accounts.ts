import type { Language } from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export type AccountCredentialStatus = "ready" | "invalid";

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
  credentialsSwitched: boolean;
  restartRecommended: boolean;
}

export interface AccountLoginStatus {
  taskId: string;
  status: "running" | "completed" | "failed";
  message: string | null;
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
  if (!isTauri()) return { profile: mockVault.profiles.find((profile) => profile.id === profileId) ?? mockVault.profiles[0], credentialsSwitched: true, restartRecommended: true };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountSwitchOutcome>("switch_account", { profileId });
}

export async function switchAccountAndRestartCodex(profileId: string, confirmed: boolean): Promise<AccountSwitchOutcome> {
  if (!isTauri()) return { profile: mockVault.profiles[0], credentialsSwitched: true, restartRecommended: false };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountSwitchOutcome>("switch_account_and_restart_codex", { profileId, confirmed });
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

export async function openAccountSwitcher(): Promise<AccountVault> {
  if (!isTauri()) return mockVault;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AccountVault>("open_account_switcher");
}

export async function closeAccountSwitcher(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("close_account_switcher");
}

export async function listenAccountEvents(handlers: {
  onVault: (vault: AccountVault) => void;
  onSwitched?: (outcome: AccountSwitchOutcome) => void;
  onClosed?: () => void;
  onError?: (message: string) => void;
}): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlistenVault = await listen<AccountVault>("account-vault-changed", (event) => handlers.onVault(event.payload));
  const unlistenSwitched = await listen<AccountSwitchOutcome>("account-switch-completed", (event) => handlers.onSwitched?.(event.payload));
  const unlistenClosed = await listen("account-switcher-closed", () => handlers.onClosed?.());
  const unlistenError = await listen<string>("account-operation-error", (event) => handlers.onError?.(event.payload));
  return () => { unlistenVault(); unlistenSwitched(); unlistenClosed(); unlistenError(); };
}

export function accountCopy(language: Language) {
  return language === "en" ? {
    title: "Codex accounts", close: "Close", saveCurrent: "Save current account", alias: "Account name",
    add: "Add account", cancel: "Cancel login", current: "Current", switch: "Switch", rename: "Rename", remove: "Delete",
    empty: "Save the current Codex login before adding another account.", browser: "Complete the official Codex sign-in in your browser.",
    switched: "Credentials switched. Restart Codex for full effect.", invalid: "Sign in again", restart: "Switch + restart", restartConfirm: "Restarting Codex can interrupt active tasks. Continue?", localTokens: "Token totals remain cumulative for this device.",
  } : {
    title: "Codex 账号", close: "关闭", saveCurrent: "保存当前账号", alias: "账号名称",
    add: "添加账号", cancel: "取消登录", current: "当前使用", switch: "切换", rename: "重命名", remove: "删除",
    empty: "请先显式保存当前 Codex 登录，再添加其他账号。", browser: "请在浏览器中完成官方 Codex 登录。",
    switched: "凭据已切换；重启 Codex 后完整生效。", invalid: "重新登录", restart: "切换并重启", restartConfirm: "重启 Codex 可能中断正在运行的任务，确定继续吗？", localTokens: "Token 统计继续显示本机全部 Codex 会话累计。",
  };
}
