// Единое состояние UI. Все действия ходят в Rust-ядро через shared/api.

import { create } from "zustand";
import {
  api,
  onConnectionChanged,
  toCommandError,
} from "../api/client";
import type {
  AppRule,
  AppSettings,
  CommandError,
  ConnectionState,
  HelperStatus,
  InstalledApp,
  LogEntry,
  LogLevel,
  Platform,
  Profile,
} from "../api/types";
import type { TabId } from "../../app/tabs";

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
  rules: AppRule[];
  installedApps: InstalledApp[];
  installedLoading: boolean;
  installedError: CommandError | null;
  helperStatus: HelperStatus | null;
  helperStatusError: CommandError | null;
  connection: ConnectionState;
  fetchError: CommandError | null;
  actionError: CommandError | null;
  notice: string | null;
  logs: LogEntry[];
  diagnostics: string;
  busy: Record<string, boolean>;

  setActiveTab: (tab: TabId) => void;
  initialize: () => Promise<void>;
  subscribe: () => Promise<() => void>;
  refreshSubscription: () => Promise<void>;
  selectProfile: (id: string) => Promise<void>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  toggleConnection: () => Promise<void>;
  loadRules: () => Promise<void>;
  loadInstalledApps: () => Promise<void>;
  addRule: (input: NewRuleInput) => Promise<void>;
  addRuleFromInstalled: (app: InstalledApp) => Promise<void>;
  toggleRule: (rule: AppRule) => Promise<void>;
  deleteRule: (id: string) => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  regenerateHwid: () => Promise<string | null>;
  refreshHelperStatus: () => Promise<void>;
  loadLogs: (level: LogLevel | null) => Promise<void>;
  loadDiagnostics: () => Promise<string>;
  clearNotice: () => void;
}

const initialState = {
  activeTab: "connect" as TabId,
  settings: null,
  profiles: [] as Profile[],
  selectedProfileId: null,
  fetchedAt: null,
  skippedCount: 0,
  rules: [] as AppRule[],
  installedApps: [] as InstalledApp[],
  installedLoading: false,
  installedError: null,
  helperStatus: null,
  helperStatusError: null,
  connection: "disconnected" as ConnectionState,
  fetchError: null,
  actionError: null,
  notice: null,
  logs: [] as LogEntry[],
  diagnostics: "",
  busy: {},
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
      ]);
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
        } catch (error) {
          set({ fetchError: toCommandError(error) });
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
        set({ settings });
      } catch (error) {
        set({ actionError: toCommandError(error) });
      }
    },

    saveSettings: async (settings) => {
      await withBusy("settings", async () => {
        try {
          const saved = await api.settingsUpdate(settings);
          set({ settings: saved, actionError: null, notice: "settings.saved" });
        } catch (error) {
          set({ actionError: toCommandError(error) });
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

    clearNotice: () => set({ notice: null, actionError: null, fetchError: null }),
  };
});

/** Сброс состояния между тестами. */
export function resetAppStore(): void {
  useAppStore.setState(initialState);
}
