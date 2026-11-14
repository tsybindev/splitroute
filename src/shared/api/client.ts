// Обёртки Tauri commands и событий. UI не вызывает invoke напрямую и не
// формирует конфиг sing-box.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppRule,
  AppSettings,
  CommandError,
  HelperStatus,
  InstalledApp,
  LogEntry,
  LogLevel,
  ProfilesResponse,
  SubscriptionFetchResponse,
} from "./types";

export function toCommandError(error: unknown): CommandError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  ) {
    const candidate = error as CommandError;
    return { code: String(candidate.code), message: String(candidate.message) };
  }
  return {
    code: "UNKNOWN",
    message: typeof error === "string" ? error : "Неизвестная ошибка",
  };
}

export const api = {
  helperStatus: () => invoke<HelperStatus>("helper_status"),
  connect: () => invoke<HelperStatus>("connect"),
  disconnect: () => invoke<HelperStatus>("disconnect"),
  helperInstall: () => invoke<void>("helper_install"),
  helperUninstall: () => invoke<void>("helper_uninstall"),
  subscriptionFetch: (url?: string, hwid?: string, userAgent?: string) =>
    invoke<SubscriptionFetchResponse>("subscription_fetch", {
      url: url ?? null,
      hwid: hwid ?? null,
      userAgent: userAgent ?? null,
    }),
  profilesList: () => invoke<ProfilesResponse>("profiles_list"),
  profileSelect: (id: string) =>
    invoke<ProfilesResponse>("profile_select", { id }),
  rulesList: () => invoke<AppRule[]>("rules_list"),
  rulesUpsert: (rule: AppRule) =>
    invoke<AppRule[]>("rules_upsert", { rule }),
  rulesDelete: (id: string) => invoke<AppRule[]>("rules_delete", { id }),
  appsInstalled: () => invoke<InstalledApp[]>("apps_installed"),
  appRuleFromInstalled: (app: InstalledApp) =>
    invoke<AppRule>("app_rule_from_installed", { app }),
  // Иконка приходит готовым data URL или null: UI не разбирает форматы файлов.
  appIcon: (app: InstalledApp) => invoke<string | null>("app_icon", { app }),
  settingsGet: () => invoke<AppSettings>("settings_get"),
  settingsUpdate: (settings: AppSettings) =>
    invoke<AppSettings>("settings_update", { settings }),
  hwidGenerate: () => invoke<string>("hwid_generate"),
  logsList: (level: LogLevel | null, limit = 200) =>
    invoke<LogEntry[]>("logs_list", { level, limit }),
  diagnosticsGet: () => invoke<string>("diagnostics_get"),
};

export function onConnectionChanged(
  handler: (status: HelperStatus) => void,
): Promise<UnlistenFn> {
  return listen<HelperStatus>("connection-changed", (event) =>
    handler(event.payload),
  );
}
