// Единое состояние UI. Все действия ходят в Rust-ядро через shared/api.

import { create } from "zustand";
import {
  api,
  onConnectionChanged,
  onUpdateProgress,
  toCommandError,
} from "../api/client";
import type {
  AppRule,
  AppSettings,
  CommandError,
  ConnectionState,
  HelperStatus,
  InstalledApp,
  Language,
  LatencyResponse,
  LogEntry,
  LogLevel,
  Platform,
  Profile,
  RunningApp,
  UpdateChannel,
  UpdateCheckResult,
  UpdateProgress,
  UpdateStatus,
} from "../api/types";
import type { TabId } from "../../app/tabs";
import { setLanguage } from "../i18n";

export interface NewRuleInput {
  name: string;
  kind: "process" | "path" | "regex";
  value: string;
}

interface AppState {
  activeTab: TabId;
  settings: AppSettings | null;
  profiles: Profile[];
  selectedProfileId: string | null;
  fetchedAt: number | null;
  skippedCount: number;
  latency: LatencyResponse | null;
  rules: AppRule[];
  installedApps: InstalledApp[];
  installedLoading: boolean;
  installedError: CommandError | null;
  runningApps: RunningApp[];
  runningLoading: boolean;
  runningError: CommandError | null;
  helperStatus: HelperStatus | null;
  helperStatusError: CommandError | null;
  connection: ConnectionState;
  fetchError: CommandError | null;
  actionError: CommandError | null;
  notice: string | null;
  logs: LogEntry[];
  diagnostics: string;
  busy: Record<string, boolean>;
  updateStatus: UpdateStatus | null;
  updateCheck: UpdateCheckResult | null;
  updateChecking: boolean;
  updateInstalling: boolean;
  updateProgress: UpdateProgress | null;
  updateError: CommandError | null;

  setActiveTab: (tab: TabId) => void;
  initialize: () => Promise<void>;
  subscribe: () => Promise<() => void>;
  refreshSubscription: () => Promise<void>;
  pingProfiles: () => Promise<void>;
  autoSwitchCheck: () => Promise<void>;
  switchProfile: (id: string) => Promise<void>;
  selectProfile: (id: string) => Promise<void>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  toggleConnection: () => Promise<void>;
  loadRules: () => Promise<void>;
  loadInstalledApps: () => Promise<void>;
  loadRunningApps: () => Promise<void>;
  addRule: (input: NewRuleInput) => Promise<void>;
  addRuleFromInstalled: (app: InstalledApp) => Promise<void>;
  addRuleFromRunning: (app: RunningApp) => Promise<void>;
  toggleRule: (rule: AppRule) => Promise<void>;
  deleteRule: (id: string) => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  setLanguagePreference: (language: Language) => Promise<void>;
  regenerateHwid: () => Promise<string | null>;
  refreshHelperStatus: () => Promise<void>;
  pollConnection: () => Promise<void>;
  loadLogs: (level: LogLevel | null) => Promise<void>;
  loadDiagnostics: () => Promise<string>;
  loadUpdateStatus: () => Promise<void>;
  checkForUpdates: (channel: UpdateChannel, silent?: boolean) => Promise<void>;
  installUpdate: (channel: UpdateChannel) => Promise<void>;
  dismissUpdate: () => void;
  clearNotice: () => void;
}

const initialState = {
  activeTab: "connect" as TabId,
  settings: null,
  profiles: [] as Profile[],
  selectedProfileId: null,
  fetchedAt: null,
  skippedCount: 0,
  latency: null as LatencyResponse | null,
  rules: [] as AppRule[],
  installedApps: [] as InstalledApp[],
  installedLoading: false,
  installedError: null,
  runningApps: [] as RunningApp[],
  runningLoading: false,
  runningError: null,
  helperStatus: null,
  helperStatusError: null,
  connection: "disconnected" as ConnectionState,
  fetchError: null,
  actionError: null,
  notice: null,
  logs: [] as LogEntry[],
  diagnostics: "",
  busy: {},
  updateStatus: null as UpdateStatus | null,
  updateCheck: null as UpdateCheckResult | null,
  updateChecking: false,
  updateInstalling: false,
  updateProgress: null as UpdateProgress | null,
  updateError: null,
};

function connectionFromStatus(status: HelperStatus): ConnectionState {
  return status.state === "running" ? "connected" : "disconnected";
}

function detectPlatform(): Platform {
  const agent = navigator.userAgent;
  if (/Mac/i.test(agent)) return "macos";
  if (/Win/i.test(agent)) return "windows";
  return "linux";
}

function newRuleId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `rule-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function ruleFromInput(input: NewRuleInput): AppRule {
  return {
    id: newRuleId(),
    name: input.name.trim(),
    platform: detectPlatform(),
    process_name: input.kind === "process" ? [input.value.trim()] : [],
    process_path: input.kind === "path" ? [input.value.trim()] : [],
    path_regex: input.kind === "regex" ? [input.value.trim()] : [],
    enabled: true,
  };
}

export const useAppStore = create<AppState>((set, get) => {
  const withBusy = async (key: string, action: () => Promise<void>) => {
    set((state) => ({ busy: { ...state.busy, [key]: true } }));
    try {
      await action();
    } finally {
      set((state) => ({ busy: { ...state.busy, [key]: false } }));
    }
  };

  const applyStatus = (status: HelperStatus) => {
    set({
      helperStatus: status,
      helperStatusError: null,
      connection: connectionFromStatus(status),
    });
  };

  return {
    ...initialState,

    setActiveTab: (tab) => set({ activeTab: tab }),

    initialize: async () => {
      await Promise.allSettled([
        get().loadSettings(),
        get().loadRules(),
        (async () => {
          const response = await api.profilesList();
          set({
            profiles: response.profiles,
            selectedProfileId: response.selected_profile_id,
            fetchedAt: response.fetched_at,
          });
        })(),
        (async () => {
          await get().refreshHelperStatus();
        })(),
        get().loadLogs(null),
        get().loadUpdateStatus(),
      ]);
      // Проверка при запуске — только если она включена и канал настроен.
      // Установка никогда не запускается автоматически.
      const settings = get().settings;
      if (
        settings?.preferences.updates.check_on_start &&
        get().updateStatus?.configured
      ) {
        void get().checkForUpdates(
          settings.preferences.updates.channel,
          true,
        );
      }
    },

    subscribe: async () =>
      onConnectionChanged((status) => {
        applyStatus(status);
        void get().loadLogs(null);
      }),

    refreshSubscription: async () => {
      await withBusy("subscription", async () => {
        try {
          const response = await api.subscriptionFetch();
          set({
            profiles: response.profiles,
            selectedProfileId: response.selected_profile_id,
            fetchedAt: response.fetched_at,
            skippedCount: response.skipped.length,
            fetchError: null,
            actionError: null,
          });
          // Автовыбор: после успешного обновления список локаций уже известен.
          if (get().settings?.preferences.auto_select.enabled) {
            void get().pingProfiles();
          }
        } catch (error) {
          set({ fetchError: toCommandError(error) });
        }
      });
    },

    /// Ручная проверка задержек. Если включён автовыбор и туннель не поднят,
    /// сохраняется лучшая локация; при активном подключении выбор не меняется —
    /// этим занимается автопереключение.
    pingProfiles: async () => {
      await withBusy("ping", async () => {
        try {
          const latency = await api.profilesPing();
          set({ latency, actionError: null });
          const settings = get().settings;
          const best = latency.best_profile_id;
          if (
            settings?.preferences.auto_select.enabled &&
            get().connection !== "connected" &&
            best &&
            best !== get().selectedProfileId
          ) {
            const response = await api.profileSelect(best);
            set({ selectedProfileId: response.selected_profile_id });
          }
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    /// Фоновая проверка автопереключения: решение принимает Rust (гистерезис и
    /// серии неудач), UI только выполняет явно рекомендованное переключение.
    autoSwitchCheck: async () => {
      const state = get();
      const auto = state.settings?.preferences.auto_select;
      if (!auto?.auto_switch || state.connection !== "connected") return;
      if (state.busy["auto-switch"]) return;
      await withBusy("auto-switch", async () => {
        try {
          const latency = await api.profilesPing();
          set({ latency });
          if (latency.switch_to && latency.switch_to !== get().selectedProfileId) {
            await get().switchProfile(latency.switch_to);
          }
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    /// Переключение с переподключением. Ядро само возвращается к прежней
    /// локации при ошибке старта, поэтому после сбоя перечитываем состояние.
    switchProfile: async (id) => {
      await withBusy("switch", async () => {
        try {
          const response = await api.profileSwitch(id);
          set({ selectedProfileId: response.selected_profile_id, actionError: null });
        } catch (error) {
          set({ actionError: toCommandError(error) });
          await get().refreshHelperStatus();
          try {
            const response = await api.profilesList();
            set({
              profiles: response.profiles,
              selectedProfileId: response.selected_profile_id,
            });
          } catch {
            // Причина уже показана в actionError.
          }
        }
      });
    },

    selectProfile: async (id) => {
      await withBusy("profile", async () => {
        try {
          const response = await api.profileSelect(id);
          set({
            selectedProfileId: response.selected_profile_id,
            actionError: null,
          });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    connect: async () => {
      set({ connection: "connecting", actionError: null });
      try {
        const status = await api.connect();
        applyStatus(status);
      } catch (error) {
        set({ connection: "error", actionError: toCommandError(error) });
      }
    },

    disconnect: async () => {
      set({ connection: "disconnecting", actionError: null });
      try {
        const status = await api.disconnect();
        applyStatus(status);
      } catch (error) {
        set({ connection: "error", actionError: toCommandError(error) });
      }
    },

    toggleConnection: async () => {
      const state = get();
      if (state.connection === "connected") {
        await state.disconnect();
      } else {
        await state.connect();
      }
    },

    loadRules: async () => {
      try {
        const rules = await api.rulesList();
        set({ rules });
      } catch (error) {
        set({ actionError: toCommandError(error) });
      }
    },

    loadInstalledApps: async () => {
      set({ installedLoading: true });
      try {
        const installedApps = await api.appsInstalled();
        set({ installedApps, installedError: null });
      } catch (error) {
        set({ installedError: toCommandError(error) });
      } finally {
        set({ installedLoading: false });
      }
    },

    /// Список процессов обновляется по кнопке: между вызовами он не опрашивается.
    loadRunningApps: async () => {
      set({ runningLoading: true });
      try {
        const runningApps = await api.appsRunning();
        set({ runningApps, runningError: null });
      } catch (error) {
        set({ runningError: toCommandError(error) });
      } finally {
        set({ runningLoading: false });
      }
    },

    addRule: async (input) => {
      await withBusy("rule", async () => {
        try {
          const rules = await api.rulesUpsert(ruleFromInput(input));
          set({ rules, actionError: null });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    /// Правило из каталога строит ядро: regex по `.app` на macOS и полный путь
    /// на Windows/Linux. UI не собирает regex сам (PRD §6.3).
    addRuleFromInstalled: async (app) => {
      await withBusy(`rule-${app.path}`, async () => {
        try {
          const rule = await api.appRuleFromInstalled(app);
          const rules = await api.rulesUpsert(rule);
          set({ rules, actionError: null });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    /// Правило из запущенного процесса также строит ядро: macOS — regex по
    /// `.app`, остальные — путь к исполняемому файлу (PRD §6.3).
    addRuleFromRunning: async (app) => {
      await withBusy(`rule-${app.path}`, async () => {
        try {
          const rule = await api.appRuleFromRunning(app);
          const rules = await api.rulesUpsert(rule);
          set({ rules, actionError: null });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    toggleRule: async (rule) => {
      await withBusy(`rule-${rule.id}`, async () => {
        try {
          const rules = await api.rulesUpsert({ ...rule, enabled: !rule.enabled });
          set({ rules, actionError: null });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    deleteRule: async (id) => {
      await withBusy(`rule-${id}`, async () => {
        try {
          const rules = await api.rulesDelete(id);
          set({ rules, actionError: null });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    loadSettings: async () => {
      try {
        const settings = await api.settingsGet();
        setLanguage(settings.preferences.language);
        set({ settings });
      } catch (error) {
        set({ actionError: toCommandError(error) });
      }
    },

    saveSettings: async (settings) => {
      // Язык принадлежит Rust-состоянию и уже применён: форма не должна
      // откатывать его к значению, загруженному при открытии экрана.
      const current = get().settings;
      const payload = current
        ? {
            ...settings,
            preferences: {
              ...settings.preferences,
              language: current.preferences.language,
            },
          }
        : settings;
      await withBusy("settings", async () => {
        try {
          const saved = await api.settingsUpdate(payload);
          setLanguage(saved.preferences.language);
          set({ settings: saved, actionError: null, notice: "settings.saved" });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    /// Язык применяется сразу (единое состояние в Rust) и сразу сохраняется,
    /// поэтому переключение не зависит от остальных несохранённых полей формы.
    setLanguagePreference: async (language) => {
      const settings = get().settings;
      if (!settings) return;
      const next: AppSettings = {
        ...settings,
        preferences: { ...settings.preferences, language },
      };
      setLanguage(language);
      set({ settings: next, actionError: null });
      await withBusy("language", async () => {
        try {
          const saved = await api.settingsUpdate(next);
          setLanguage(saved.preferences.language);
          set({ settings: saved });
        } catch (error) {
          setLanguage(settings.preferences.language);
          set({ settings, actionError: toCommandError(error) });
        }
      });
    },

    regenerateHwid: async () => {
      const settings = get().settings;
      if (!settings) return null;
      try {
        const hwid = await api.hwidGenerate();
        set({
          settings: {
            ...settings,
            subscription: { ...settings.subscription, hwid },
          },
        });
        return hwid;
      } catch (error) {
        set({ actionError: toCommandError(error) });
        return null;
      }
    },

    refreshHelperStatus: async () => {
      try {
        applyStatus(await api.helperStatus());
      } catch (error) {
        set({ helperStatus: null, helperStatusError: toCommandError(error) });
      }
    },

    /// Опрос статуса при активном kill switch. Если ядро упало, helper
    /// продолжает удерживать правила: UI обязан показать ошибку, а не
    /// «подключено» или «отключено».
    pollConnection: async () => {
      const before = get();
      try {
        const status = await api.helperStatus();
        const protectedCrash =
          before.connection === "connected" &&
          status.state === "stopped" &&
          ((before.settings?.network.kill_switch ?? false) ||
            (before.helperStatus?.kill_switch?.enabled ?? false));
        if (protectedCrash) {
          set({
            helperStatus: status,
            helperStatusError: null,
            connection: "error",
            actionError: { code: "CORE_STOPPED", message: "" },
          });
          void get().loadLogs(null);
          return;
        }
        applyStatus(status);
      } catch (error) {
        set({ helperStatusError: toCommandError(error) });
      }
    },

    loadLogs: async (level) => {
      try {
        const logs = await api.logsList(level);
        set({ logs });
      } catch {
        // Логи не критичны для работы экрана подключения.
      }
    },

    loadDiagnostics: async () => {
      try {
        const diagnostics = await api.diagnosticsGet();
        set({ diagnostics });
        return diagnostics;
      } catch (error) {
        set({ actionError: toCommandError(error) });
        return "";
      }
    },

    loadUpdateStatus: async () => {
      try {
        set({ updateStatus: await api.updateStatus() });
      } catch (error) {
        set({ updateStatus: null, updateError: toCommandError(error) });
      }
    },

    /// Проверка обновлений. `silent` — стартовая проверка: отсутствие канала
    /// не показывается как ошибка, установка всё равно только вручную.
    checkForUpdates: async (channel, silent = false) => {
      set({ updateChecking: true, updateError: null });
      try {
        const updateCheck = await api.updateCheck(channel);
        set({ updateCheck, updateError: null });
      } catch (error) {
        const commandError = toCommandError(error);
        set({
          updateCheck: null,
          updateError:
            silent && commandError.code === "UPDATE_NOT_CONFIGURED"
              ? null
              : commandError,
        });
      } finally {
        set({ updateChecking: false });
      }
    },

    /// Установка обновления: прогресс приходит событием из Rust. После
    /// успешной установки приложение перезапускается, и промис может не
    /// завершиться — это ожидаемо.
    installUpdate: async (channel) => {
      set({
        updateInstalling: true,
        updateError: null,
        updateProgress: { downloaded: 0, total: null },
      });
      let unlisten: (() => void) | undefined;
      try {
        unlisten = await onUpdateProgress((progress) =>
          set({ updateProgress: progress }),
        );
      } catch {
        // Прогресс не критичен: установка продолжается без него.
      }
      try {
        await api.updateInstall(channel);
        set({ updateInstalling: false, updateCheck: null });
      } catch (error) {
        set({ updateInstalling: false, updateError: toCommandError(error) });
      } finally {
        unlisten?.();
      }
    },

    dismissUpdate: () => set({ updateCheck: null }),

    clearNotice: () => set({ notice: null, actionError: null, fetchError: null }),
  };
});

/** Сброс состояния между тестами. */
export function resetAppStore(): void {
  useAppStore.setState(initialState);
}
