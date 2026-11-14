// Типы обмена с Rust-ядром. Зеркалят модели crates/protocol и команды.

export type ConnectionState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type CoreState = "running" | "stopped";

export interface HelperStatus {
  state: CoreState;
  pid?: number;
  version?: string;
  started_at?: number;
}

export type Protocol = "vless";
export type Transport = "tcp" | "ws" | "grpc" | "http";
export type TlsKind = "tls" | "reality";

export interface TlsConfig {
  kind: TlsKind;
  server_name: string;
  public_key?: string;
  short_id?: string;
  fingerprint?: string;
  alpn?: string[];
}

export interface Profile {
  id: string;
  name: string;
  protocol: Protocol;
  server: string;
  port: number;
  uuid: string;
  flow?: string;
  tls: TlsConfig;
  transport: Transport;
}

export type Platform = "macos" | "windows" | "linux";

export interface AppRule {
  id: string;
  name: string;
  platform: Platform;
  process_name?: string[];
  process_path?: string[];
  path_regex?: string[];
  enabled: boolean;
}

/** Запись каталога установленных приложений (PRD §6.3). Иконок пока нет. */
export interface InstalledApp {
  name: string;
  path: string;
  platform: Platform;
}

export type DnsMode = "proxy" | "system";
export type Language = "ru" | "en";
export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export interface NetworkSettings {
  mtu: number;
  ipv6_enabled: boolean;
  mixed_inbound_enabled: boolean;
  mixed_port: number;
  dns_mode: DnsMode;
  log_level: LogLevel;
}

export interface SubscriptionSettings {
  url: string;
  hwid: string;
  user_agent: string;
  auto_update_hours: number;
}

export interface AppPreferences {
  launch_at_login: boolean;
  launch_minimized: boolean;
  connect_on_start: boolean;
  close_to_tray: boolean;
  language: Language;
}

export interface AppSettings {
  subscription: SubscriptionSettings;
  network: NetworkSettings;
  preferences: AppPreferences;
}

export interface SkippedProfile {
  index: number;
  name: string;
  reason: string;
}

export interface ProfilesResponse {
  profiles: Profile[];
  selected_profile_id: string | null;
  fetched_at: number | null;
}

export interface SubscriptionFetchResponse extends ProfilesResponse {
  skipped: SkippedProfile[];
}

export interface LogEntry {
  timestamp: number;
  level: LogLevel;
  message: string;
}

export interface CommandError {
  code: string;
  message: string;
}
