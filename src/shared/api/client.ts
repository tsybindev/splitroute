// Обёртки Tauri commands и событий. UI не вызывает invoke напрямую и не
// формирует конфиг sing-box.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t } from "../i18n";
import type {
  AppRule,
  AppRuleView,
  AppSettings,
  CommandError,
  HelperStatus,
  InstalledApp,
  LatencyResponse,
  LogEntry,
  LogLevel,
  ProfilesResponse,
  RunningApp,
  SubscriptionFetchResponse,
  SubscriptionInput,
  SubscriptionView,
  TrafficStats,
  UpdateChannel,
  UpdateCheckResult,
  UpdateProgress,
  UpdateStatus,
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
    message: typeof error === "string" ? error : t("errors.UNKNOWN"),
  };
}

export const api = {
  helperStatus: () => invoke<HelperStatus>("helper_status"),
  // Автоподключение при старте (B2): причина сбоя или null.
  autoconnectStatus: () => invoke<CommandError | null>("autoconnect_status"),
  connect: () => invoke<HelperStatus>("connect"),
  disconnect: () => invoke<HelperStatus>("disconnect"),
  helperInstall: () => invoke<void>("helper_install"),
  helperUninstall: () => invoke<void>("helper_uninstall"),
  // Ручное обновление одной подписки: сеть в Rust, окно не ждёт ответа.
  subscriptionFetch: (id: string) =>
    invoke<SubscriptionFetchResponse>("subscription_fetch", { id }),
  // Подписки: список приходит в profiles_list, эти команды управляют записями.
  subscriptionAdd: (input: SubscriptionInput) =>
    invoke<SubscriptionView>("subscription_add", { ...input }),
  subscriptionUpdate: (id: string, input: SubscriptionInput) =>
    invoke<SubscriptionView>("subscription_update", { id, ...input }),
  // Удаление возвращает общий список: выбор локации мог сброситься.
  subscriptionDelete: (id: string) =>
    invoke<ProfilesResponse>("subscription_delete", { id }),
  profilesList: () => invoke<ProfilesResponse>("profiles_list"),
  // Задержки меряет Rust: TCP-connect с таймаутом, без ядра и секретов.
  profilesPing: () => invoke<LatencyResponse>("profiles_ping"),
  profileSelect: (id: string) =>
    invoke<ProfilesResponse>("profile_select", { id }),
  // Переключение с переподключением: stop → выбор → start.
  profileSwitch: (id: string) =>
    invoke<ProfilesResponse>("profile_switch", { id }),
  // Пути проверяет Rust: отсутствующий путь приходит как `missing_paths`.
  rulesList: () => invoke<AppRuleView[]>("rules_list"),
  rulesUpsert: (rule: AppRule) =>
    invoke<AppRuleView[]>("rules_upsert", { rule }),
  rulesDelete: (id: string) => invoke<AppRuleView[]>("rules_delete", { id }),
  appsInstalled: () => invoke<InstalledApp[]>("apps_installed"),
  appRuleFromInstalled: (app: InstalledApp) =>
    invoke<AppRule>("app_rule_from_installed", { app }),
  // Процессы читает Rust из ОС без привилегий; список уже отфильтрован и отсортирован.
  appsRunning: () => invoke<RunningApp[]>("apps_running"),
  appRuleFromRunning: (app: RunningApp) =>
    invoke<AppRule>("app_rule_from_running", { app }),
  // Иконка приходит готовым data URL или null: UI не разбирает форматы файлов.
  appIcon: (app: InstalledApp) => invoke<string | null>("app_icon", { app }),
  settingsGet: () => invoke<AppSettings>("settings_get"),
  settingsUpdate: (settings: AppSettings) =>
    invoke<AppSettings>("settings_update", { settings }),
  hwidGenerate: () => invoke<string>("hwid_generate"),
  logsList: (level: LogLevel | null, limit = 200) =>
    invoke<LogEntry[]>("logs_list", { level, limit }),
  diagnosticsGet: () => invoke<string>("diagnostics_get"),
  // Счётчики читает Rust у локального Clash API; секрет и адрес ядра в UI не попадают.
  trafficStats: () => invoke<TrafficStats>("traffic_stats"),
  // Обновления: подпись проверяет Rust, UI не получает URL или ключи.
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: (channel: UpdateChannel) =>
    invoke<UpdateCheckResult>("update_check", { channel }),
  updateInstall: (channel: UpdateChannel) =>
    invoke<void>("update_install", { channel }),
};

export function onConnectionChanged(
  handler: (status: HelperStatus) => void,
): Promise<UnlistenFn> {
  return listen<HelperStatus>("connection-changed", (event) =>
    handler(event.payload),
  );
}

/** Автоподключение при старте не удалось: причина для ErrorBox (B2). */
export function onAutoconnectFailed(
  handler: (error: CommandError) => void,
): Promise<UnlistenFn> {
  return listen<CommandError>("autoconnect-failed", (event) =>
    handler(event.payload),
  );
}

/** Подписка обновлена планировщиком (M1): список локаций без участия UI. */
export function onSubscriptionUpdated(
  handler: (response: SubscriptionFetchResponse) => void,
): Promise<UnlistenFn> {
  return listen<SubscriptionFetchResponse>("subscription-updated", (event) =>
    handler(event.payload),
  );
}

/** Прогресс загрузки обновления: байты и, если сервер сообщил, общий размер. */
export function onUpdateProgress(
  handler: (progress: UpdateProgress) => void,
): Promise<UnlistenFn> {
  return listen<UpdateProgress>("update-progress", (event) =>
    handler(event.payload),
  );
}
