// Единое состояние UI. Все действия ходят в Rust-ядро через shared/api.

import { create } from "zustand";
import {
  api,
  onAutoconnectFailed,
  onConnectionChanged,
  onReconnectFailed,
  onReconnectProgress,
  onSubscriptionUpdated,
  onUpdateProgress,
  toCommandError,
} from "../api/client";
import type {
  AppRule,
  AppRuleView,
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
  ReconnectStatus,
  RunningApp,
  SubscriptionFetchResponse,
  SubscriptionInput,
  SubscriptionView,
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
  /** Подписки в порядке добавления: локации и расписание по группам. */
  subscriptions: SubscriptionView[];
  /** Все локации плоским списком: задержки и выбор не зависят от подписки. */
  profiles: Profile[];
  selectedProfileId: string | null;
  /** Сколько профилей было пропущено при последней загрузке каждой подписки. */
  skippedCounts: Record<string, number>;
  latency: LatencyResponse | null;
  rules: AppRuleView[];
  installedApps: InstalledApp[];
  installedLoading: boolean;
  installedError: CommandError | null;
  runningApps: RunningApp[];
  runningLoading: boolean;
  runningError: CommandError | null;
  helperStatus: HelperStatus | null;
  helperStatusError: CommandError | null;
  connection: ConnectionState;
  /**
   * Идущая серия автореконнекта (S4-T15): номер попытки и лимит. `null` —
   * серии нет (не началась, завершилась успехом или отменена).
   */
  reconnect: ReconnectStatus | null;
  /** Ошибка последнего обновления по каждой подписке. */
  fetchErrors: Record<string, CommandError>;
  actionError: CommandError | null;
  /** Автоподключение при старте не удалось (B2): причина для ErrorBox. */
  startupError: CommandError | null;
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
  refreshSubscription: (id: string) => Promise<void>;
  addSubscription: (input: SubscriptionInput) => Promise<void>;
  updateSubscription: (id: string, input: SubscriptionInput) => Promise<void>;
  deleteSubscription: (id: string) => Promise<void>;
  setProfilesEnabled: (
    subscriptionId: string,
    profileIds: string[],
    enabled: boolean,
  ) => Promise<void>;
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
  toggleRule: (rule: AppRuleView) => Promise<void>;
  deleteRule: (id: string) => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  setLanguagePreference: (language: Language) => Promise<void>;
  regenerateHwid: () => Promise<string | null>;
  refreshHelperStatus: () => Promise<void>;
  pollConnection: () => Promise<void>;
  loadLogs: (level: LogLevel | null) => Promise<void>;
  loadDiagnostics: () => Promise<string>;
  loadStartupError: () => Promise<void>;
  loadReconnectStatus: () => Promise<void>;
  loadUpdateStatus: () => Promise<void>;
  checkForUpdates: (channel: UpdateChannel, silent?: boolean) => Promise<void>;
  installUpdate: (channel: UpdateChannel) => Promise<void>;
  dismissUpdate: () => void;
  clearNotice: () => void;
}

const initialState = {
  activeTab: "connect" as TabId,
  settings: null,
  subscriptions: [] as SubscriptionView[],
  profiles: [] as Profile[],
  selectedProfileId: null,
  skippedCounts: {} as Record<string, number>,
  latency: null as LatencyResponse | null,
  rules: [] as AppRuleView[],
  installedApps: [] as InstalledApp[],
  installedLoading: false,
  installedError: null,
  runningApps: [] as RunningApp[],
  runningLoading: false,
  runningError: null,
  helperStatus: null,
  helperStatusError: null,
  connection: "disconnected" as ConnectionState,
  reconnect: null as ReconnectStatus | null,
  fetchErrors: {} as Record<string, CommandError>,
  actionError: null,
  startupError: null,
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

/** Плоский список локаций из всех подписок. */
function flattenProfiles(subscriptions: SubscriptionView[]): Profile[] {
  return subscriptions.flatMap((entry) => entry.profiles);
}

/**
 * Результат успешной загрузки одной подписки в общее состояние: обновляет
 * её группу (параметры запроса сохраняются — в ответе их нет), плоский
 * список, выбор и счётчик пропущенных; ошибка этой подписки снимается.
 */
function applyFetchResponse(
  state: Pick<AppState, "subscriptions" | "skippedCounts" | "fetchErrors">,
  response: SubscriptionFetchResponse,
): Partial<AppState> {
  const subscriptions = state.subscriptions.map((entry) =>
    entry.id === response.subscription_id
      ? {
          ...entry,
          name: response.name,
          profiles: response.profiles,
          fetched_at: response.fetched_at,
          next_update_at: response.next_update_at,
        }
      : entry,
  );
  const fetchErrors = { ...state.fetchErrors };
  delete fetchErrors[response.subscription_id];
  return {
    subscriptions,
    profiles: flattenProfiles(subscriptions),
    selectedProfileId: response.selected_profile_id,
    skippedCounts: {
      ...state.skippedCounts,
      [response.subscription_id]: response.skipped.length,
    },
    fetchErrors,
  };
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

  /// Ошибка правки правила. RELOAD_FAILED — правка уже сохранена в ядре,
  /// но не применена к активной сессии (S10-T1, применится при
  /// переподключении): список синхронизируется с persist-состоянием, чтобы
  /// UI не показывал то, чего в хранилище нет.
  const setRuleActionError = async (error: unknown) => {
    const commandError = toCommandError(error);
    set({ actionError: commandError });
    if (commandError.code === "RELOAD_FAILED") {
      try {
        set({ rules: await api.rulesList() });
      } catch {
        // Список обновится при следующей загрузке вкладки.
      }
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
            subscriptions: response.subscriptions,
            profiles: response.profiles,
            selectedProfileId: response.selected_profile_id,
          });
        })(),
        (async () => {
          await get().refreshHelperStatus();
        })(),
        get().loadLogs(null),
        get().loadUpdateStatus(),
      ]);
      // Автоподключение при старте могло завершиться до появления окна: забираем
      // сохранённую причину сбоя (B2). Более поздний сбой придёт событием.
      await get().loadStartupError();
      // Серия автореконнекта могла идти или исчерпаться при закрытом окне:
      // читаем состояние, чтобы не потерять прогресс (аналог B2).
      await get().loadReconnectStatus();
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

    subscribe: async () => {
      const stopConnection = await onConnectionChanged((status) => {
        applyStatus(status);
        // Ядро снова работает: серия автореконнекта завершена, а с ней
        // уходят показанные причины падения (CORE_STOPPED / RECONNECT_FAILED).
        if (status.state === "running") {
          set((state) => ({
            reconnect: null,
            actionError:
              state.actionError?.code === "CORE_STOPPED" ||
              state.actionError?.code === "RECONNECT_FAILED"
                ? null
                : state.actionError,
          }));
        }
        void get().loadLogs(null);
      });
      // Поздний сбой автоподключения: окно не блокируется, показываем причину.
      const stopStartup = await onAutoconnectFailed((error) => {
        set({ startupError: error });
        void get().loadLogs(null);
      });
      // Серия автореконнекта (S4-T15): детект и попытки делает Rust, UI
      // показывает прогресс и исчерпание серии.
      const stopReconnectProgress = await onReconnectProgress((status) => {
        set({ reconnect: status });
        void get().loadLogs(null);
      });
      const stopReconnectFailed = await onReconnectFailed((error) => {
        set({
          reconnect: null,
          connection: "error",
          actionError: error,
        });
        void get().loadLogs(null);
      });
      // Фоновое автообновление подписок (M1): локации и расписание обновляются
      // без участия пользователя, окно не перезагружается.
      const stopSubscription = await onSubscriptionUpdated((response) => {
        set(applyFetchResponse(get(), response));
        void get().loadLogs(null);
      });
      return () => {
        stopConnection();
        stopStartup();
        stopReconnectProgress();
        stopReconnectFailed();
        stopSubscription();
      };
    },

    refreshSubscription: async (id) => {
      await withBusy(`subscription-${id}`, async () => {
        try {
          const response = await api.subscriptionFetch(id);
          set(applyFetchResponse(get(), response));
          // Автовыбор: после успешного обновления список локаций уже известен.
          if (get().settings?.preferences.auto_select.enabled) {
            void get().pingProfiles();
          }
        } catch (error) {
          const commandError = toCommandError(error);
          // Параллельная загрузка (планировщик) — не ошибка для пользователя.
          if (commandError.code === "FETCH_IN_PROGRESS") return;
          set({
            fetchErrors: { ...get().fetchErrors, [id]: commandError },
          });
        }
      });
    },

    addSubscription: async (input) => {
      await withBusy("subscription-add", async () => {
        try {
          const entry = await api.subscriptionAdd(input);
          set({
            subscriptions: [...get().subscriptions, entry],
            actionError: null,
          });
          // Локации появляются сразу после добавления: грузим без клика.
          void get().refreshSubscription(entry.id);
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    updateSubscription: async (id, input) => {
      await withBusy(`subscription-update-${id}`, async () => {
        try {
          const updated = await api.subscriptionUpdate(id, input);
          const subscriptions = get().subscriptions.map((entry) =>
            entry.id === id ? updated : entry,
          );
          set({
            subscriptions,
            profiles: flattenProfiles(subscriptions),
            actionError: null,
          });
          // После смены URL кэш пуст: локации появляются сами, без клика.
          if (!updated.profiles.length) {
            void get().refreshSubscription(id);
          }
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    deleteSubscription: async (id) => {
      await withBusy(`subscription-delete-${id}`, async () => {
        try {
          const response = await api.subscriptionDelete(id);
          set({
            subscriptions: response.subscriptions,
            profiles: response.profiles,
            selectedProfileId: response.selected_profile_id,
            actionError: null,
          });
        } catch (error) {
          set({ actionError: toCommandError(error) });
        }
      });
    },

    /// Включает/выключает локации подписки. Выключение выбранной при
    /// активной сессии переподключает туннель внутри команды: новый статус
    /// придёт событием подключения, здесь меняется только список.
    setProfilesEnabled: async (subscriptionId, profileIds, enabled) => {
      await withBusy(`profiles-enabled-${subscriptionId}`, async () => {
        try {
          const response = await api.profilesSetEnabled(
            subscriptionId,
            profileIds,
            enabled,
          );
          set({
            subscriptions: response.subscriptions,
            profiles: response.profiles,
            selectedProfileId: response.selected_profile_id,
            actionError: null,
          });
        } catch (error) {
          set({ actionError: toCommandError(error) });
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
      set({ connection: "connecting", actionError: null, startupError: null });
      try {
        const status = await api.connect();
        applyStatus(status);
        // Успешный старт сбрасывает серию автореконнекта (как в Rust).
        set((state) => ({
          reconnect: null,
          actionError:
            state.actionError?.code === "CORE_STOPPED" ||
            state.actionError?.code === "RECONNECT_FAILED"
              ? null
              : state.actionError,
        }));
      } catch (error) {
        set({ connection: "error", actionError: toCommandError(error) });
      }
    },

    disconnect: async () => {
      set({
        connection: "disconnecting",
        actionError: null,
        startupError: null,
        // Ручное отключение отменяет серию: бейдж прогресса больше не нужен.
        reconnect: null,
      });
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
          await setRuleActionError(error);
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
          await setRuleActionError(error);
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
          await setRuleActionError(error);
        }
      });
    },

    /// Вычисленный признак `missing_paths` обратно в правило не возвращается:
    /// ядро хранит только данные правила и проверяет пути заново при чтении.
    toggleRule: async (rule) => {
      const payload: AppRule = {
        id: rule.id,
        name: rule.name,
        platform: rule.platform,
        process_name: rule.process_name,
        process_path: rule.process_path,
        path_regex: rule.path_regex,
        enabled: !rule.enabled,
      };
      await withBusy(`rule-${rule.id}`, async () => {
        try {
          const rules = await api.rulesUpsert(payload);
          set({ rules, actionError: null });
        } catch (error) {
          await setRuleActionError(error);
        }
      });
    },

    deleteRule: async (id) => {
      await withBusy(`rule-${id}`, async () => {
        try {
          const rules = await api.rulesDelete(id);
          set({ rules, actionError: null });
        } catch (error) {
          await setRuleActionError(error);
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

    /// Ошибка автоподключения при запуске, если она уже случилась (B2).
    /// Подключение к сети и ядру здесь не выполняется.
    loadStartupError: async () => {
      try {
        set({ startupError: await api.autoconnectStatus() });
      } catch {
        // Автоподключение не критично: окно и ручное подключение работают.
      }
    },

    /// Состояние серии автореконнекта (S4-T15) при открытии окна: попытки и
    /// исчерпание, случившиеся при закрытом окне, показываются по статусу.
    loadReconnectStatus: async () => {
      try {
        const status = await api.reconnectStatus();
        if (status.exhausted) {
          // Серия исчерпалась без окна: показываем причину и ошибку сразу.
          set({
            reconnect: null,
            connection: "error",
            actionError: status.exhausted,
          });
        } else if (status.attempt > 0) {
          set({ reconnect: status });
        }
      } catch {
        // Статус серии не критичен: окно и ручное подключение работают.
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

    clearNotice: () => set({ notice: null, actionError: null }),
  };
});

/** Сброс состояния между тестами. */
export function resetAppStore(): void {
  useAppStore.setState(initialState);
}
