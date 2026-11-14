// Типы обмена с Rust-ядром. Зеркалят модели crates/protocol и команды.

export type ConnectionState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type CoreState = "running" | "stopped";

/** Состояние правил kill switch в helper'е (протокол 2). */
export interface KillSwitchStatus {
  enabled: boolean;
  error?: string;
}

export interface HelperStatus {
  state: CoreState;
  pid?: number;
  version?: string;
  started_at?: number;
  kill_switch?: KillSwitchStatus;
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

/** Запущенный процесс (S4-T11): путь — исполняемый файл, на macOS внутри `.app`. */
export interface RunningApp {
  pid: number;
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
  /** Fail-closed: при обрыве туннеля helper блокирует прямой выход. */
  kill_switch: boolean;
}

export interface SubscriptionSettings {
  url: string;
  hwid: string;
  user_agent: string;
  auto_update_hours: number;
}

/** Автовыбор и автопереключение локации (PRD §6.1, §6.6). */
export interface AutoSelectSettings {
  enabled: boolean;
  auto_switch: boolean;
  degradation_ms: number;
  interval_secs: number;
  confirmations: number;
  failures_to_switch: number;
}

/** Канал обновлений приложения (S4-T8). */
export type UpdateChannel = "stable" | "beta";

export interface UpdateSettings {
  /** Проверять обновления при запуске; установка всегда вручную. */
  check_on_start: boolean;
  channel: UpdateChannel;
}

export interface AppPreferences {
  launch_at_login: boolean;
  launch_minimized: boolean;
  connect_on_start: boolean;
  close_to_tray: boolean;
  language: Language;
  auto_select: AutoSelectSettings;
  updates: UpdateSettings;
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

/** Счётчики трафика сессии в байтах (S4-T4). */
export interface TrafficStats {
  available: boolean;
  upload_total: number;
  download_total: number;
}

export type PingErrorCode = "PING_TIMEOUT" | "PING_UNREACHABLE" | "PING_DNS";

/** Замер одной локации: TCP-connect до `server:port` (S4-T6). */
export interface LatencyResult {
  profile_id: string;
  latency_ms: number | null;
  error?: PingErrorCode;
}

export interface LatencyResponse {
  results: LatencyResult[];
  best_profile_id: string | null;
  switch_to: string | null;
  checked_at: number;
}

export interface CommandError {
  code: string;
  message: string;
}

/** Состояние канала обновлений (S4-T8). */
export interface UpdateStatus {
  current_version: string;
  channel: UpdateChannel;
  configured: boolean;
}

/** Результат проверки обновления. */
export interface UpdateCheckResult {
  available: boolean;
  current_version: string;
  version?: string;
  notes?: string;
  date?: string;
}

/** Прогресс загрузки артефакта обновления. */
export interface UpdateProgress {
  downloaded: number;
  total?: number | null;
}
