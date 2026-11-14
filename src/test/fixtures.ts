// Синтетические данные для тестов UI. Без реальных URL, HWID и UUID.

import type {
  AppRule,
  AppSettings,
  HelperStatus,
  InstalledApp,
  Profile,
} from "../shared/api/types";

export function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    subscription: {
      url: "",
      hwid: "0123456789abcdef",
      user_agent: "SplitRoute/test",
      auto_update_hours: 12,
      ...overrides.subscription,
    },
    network: {
      mtu: 1500,
      ipv6_enabled: false,
      mixed_inbound_enabled: true,
      mixed_port: 10808,
      dns_mode: "proxy",
      log_level: "info",
      kill_switch: false,
      ...overrides.network,
    },
    preferences: {
      launch_at_login: false,
      launch_minimized: false,
      connect_on_start: false,
      close_to_tray: true,
      language: "ru",
      auto_select: {
        enabled: false,
        auto_switch: false,
        degradation_ms: 100,
        interval_secs: 60,
        confirmations: 2,
        failures_to_switch: 2,
        ...overrides.preferences?.auto_select,
      },
      updates: {
        check_on_start: false,
        channel: "stable",
        ...overrides.preferences?.updates,
      },
      ...overrides.preferences,
    },
  };
}

export function makeProfile(overrides: Partial<Profile> = {}): Profile {
  return {
    id: "profile-1",
    name: "Тестовая локация A",
    protocol: "vless",
    server: "203.0.113.10",
    port: 443,
    uuid: "d811a5ee-1d55-4dca-bd7f-f68b768a2241",
    flow: "xtls-rprx-vision",
    tls: {
      kind: "reality",
      server_name: "example.com",
      public_key: "public-key",
      short_id: "00e5e640a81a9ac1",
      fingerprint: "chrome",
    },
    transport: "tcp",
    ...overrides,
  };
}

export function makeRule(overrides: Partial<AppRule> = {}): AppRule {
  return {
    id: "rule-1",
    name: "Google Chrome",
    platform: "macos",
    process_name: ["Google Chrome"],
    process_path: [],
    path_regex: ["^/Applications/Google Chrome\\.app/"],
    enabled: true,
    ...overrides,
  };
}

export function makeInstalledApp(
  overrides: Partial<InstalledApp> = {},
): InstalledApp {
  return {
    name: "Google Chrome",
    path: "/Applications/Google Chrome.app",
    platform: "macos",
    ...overrides,
  };
}

export const stoppedStatus: HelperStatus = { state: "stopped" };
export const runningStatus: HelperStatus = {
  state: "running",
  pid: 42,
  version: "mock sing-box 1.14.4",
  started_at: Math.floor(Date.now() / 1000) - 65,
};
