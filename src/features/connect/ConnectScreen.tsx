import { useEffect, useState } from "react";
import { Power } from "lucide-react";
import { api } from "../../shared/api/client";
import type { TrafficStats } from "../../shared/api/types";
import { t } from "../../shared/i18n";
import { formatBytes } from "../../shared/lib/format";
import { helperBadge } from "../../shared/lib/helperState";
import { useAppStore } from "../../shared/state/appStore";
import { Badge } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { ErrorBox } from "../../shared/ui/ErrorBox";
import { StatusBadge } from "../../shared/ui/StatusBadge";

function formatDuration(totalSeconds: number): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return [hours, minutes, seconds]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
}

function useSessionTime(startedAt: number | undefined): string {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    if (!startedAt) return;
    const timer = setInterval(
      () => setNow(Math.floor(Date.now() / 1000)),
      1000,
    );
    return () => clearInterval(timer);
  }, [startedAt]);

  if (!startedAt) return "00:00:00";
  return formatDuration(Math.max(0, now - startedAt));
}

/**
 * Счётчики сессии опрашиваются раз в секунду, только пока идёт подключение.
 * Компонент монтируется при подключении и размонтируется при остановке, поэтому
 * новая сессия всегда начинается с «—», а старые значения не мелькают.
 * Ошибка источника не подмешивается в общую ошибку подключения.
 */
function SessionTraffic() {
  const [stats, setStats] = useState<TrafficStats | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      api
        .trafficStats()
        .then((next) => {
          if (!cancelled) setStats(next);
        })
        .catch(() => {
          if (!cancelled) setStats(null);
        });
    };

    load();
    const timer = setInterval(load, 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  if (!stats?.available) {
    return <span className="text-neutral-500">{t("common.none")}</span>;
  }

  return (
    <>
      <span
        title={t("connect.traffic.upload")}
        aria-label={t("connect.traffic.upload")}
      >
        ↑{" "}
        <span className="font-mono text-neutral-200">
          {formatBytes(stats.upload_total)}
        </span>
      </span>
      <span
        title={t("connect.traffic.download")}
        aria-label={t("connect.traffic.download")}
      >
        ↓{" "}
        <span className="font-mono text-neutral-200">
          {formatBytes(stats.download_total)}
        </span>
      </span>
    </>
  );
}

export function ConnectScreen() {
  const profiles = useAppStore((state) => state.profiles);
  const selectedProfileId = useAppStore((state) => state.selectedProfileId);
  const connection = useAppStore((state) => state.connection);
  const settings = useAppStore((state) => state.settings);
  const rules = useAppStore((state) => state.rules);
  const helperStatus = useAppStore((state) => state.helperStatus);
  const helperStatusError = useAppStore((state) => state.helperStatusError);
  const latency = useAppStore((state) => state.latency);
  const actionError = useAppStore((state) => state.actionError);
  const startupError = useAppStore((state) => state.startupError);
  const busy = useAppStore((state) => state.busy);
  const toggleConnection = useAppStore((state) => state.toggleConnection);
  const disconnect = useAppStore((state) => state.disconnect);
  const pollConnection = useAppStore((state) => state.pollConnection);
  const autoSwitchCheck = useAppStore((state) => state.autoSwitchCheck);
  const setActiveTab = useAppStore((state) => state.setActiveTab);

  const selected = profiles.find((profile) => profile.id === selectedProfileId);
  const badge = helperBadge(helperStatus, helperStatusError);
  const session = useSessionTime(helperStatus?.started_at);
  const activeRules = rules.filter((rule) => rule.enabled).length;
  const connected = connection === "connected";
  const pending = connection === "connecting" || connection === "disconnecting";
  const canConnect = Boolean(selected) && !pending && !busy.connect;
  const killSwitchSetting = settings?.network.kill_switch ?? false;
  const killSwitchActive = helperStatus?.kill_switch?.enabled ?? false;
  const killSwitchError = helperStatus?.kill_switch?.error;
  const autoSelect = settings?.preferences.auto_select;
  const autoSwitchEnabled = autoSelect?.auto_switch ?? false;
  const bestProfile = latency?.best_profile_id
    ? profiles.find((profile) => profile.id === latency.best_profile_id)
    : undefined;
  const bestLatency = latency?.best_profile_id
    ? (latency.results.find(
        (result) => result.profile_id === latency.best_profile_id,
      )?.latency_ms ?? null)
    : null;

  // Пока защита включена, следим за ядром: helper продолжает блокировать
  // трафик и после падения sing-box, поэтому UI не должен показывать «подключено».
  useEffect(() => {
    if (!killSwitchSetting || connection !== "connected") return;
    const timer = setInterval(() => {
      void pollConnection();
    }, 2000);
    return () => clearInterval(timer);
  }, [killSwitchSetting, connection, pollConnection]);

  // Автопереключение — только при явно включённой настройке и активном
  // подключении. Решение принимает Rust, здесь только интервал опроса.
  useEffect(() => {
    if (!autoSwitchEnabled || connection !== "connected") return;
    const interval = Math.max(15, autoSelect?.interval_secs ?? 60) * 1000;
    void autoSwitchCheck();
    const timer = setInterval(() => {
      void autoSwitchCheck();
    }, interval);
    return () => clearInterval(timer);
  }, [
    autoSwitchEnabled,
    autoSelect?.interval_secs,
    connection,
    autoSwitchCheck,
  ]);

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("connect.title")}>
        <div className="flex flex-col items-center gap-5 py-4">
          <StatusBadge />

          <button
            type="button"
            onClick={() => void toggleConnection()}
            disabled={!canConnect}
            aria-label={connected ? t("connect.disconnect") : t("connect.connect")}
            className={`flex size-40 flex-col items-center justify-center gap-2 rounded-full border-4 text-lg font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
              connected
                ? "border-emerald-500 bg-emerald-950/40 text-emerald-200"
                : "border-neutral-700 bg-neutral-900 text-neutral-200 hover:border-neutral-500"
            }`}
          >
            <Power className="size-8" />
            {connected ? t("connect.disconnect") : t("connect.connect")}
          </button>

          <div className="text-center">
            <p className="text-xs uppercase tracking-wide text-neutral-500">
              {t("connect.location")}
            </p>
            <p className="mt-1 text-base font-medium text-neutral-100">
              {selected ? selected.name : t("connect.noLocation")}
            </p>
            {selected ? (
              <p className="text-sm text-neutral-500">
                {selected.server}:{selected.port}
              </p>
            ) : null}
          </div>

          <div className="flex flex-col items-center gap-1 text-sm text-neutral-400">
            <span>
              {t("connect.sessionTime")}:{" "}
              <span className="font-mono text-neutral-200">{session}</span>
            </span>
            <span className="flex items-center gap-3">
              <span className="text-xs uppercase tracking-wide text-neutral-500">
                {t("connect.traffic")}:
              </span>
              {connected ? (
                <SessionTraffic />
              ) : (
                <span className="text-neutral-500">{t("common.none")}</span>
              )}
            </span>
          </div>

          <div className="flex gap-2">
            <Button onClick={() => setActiveTab("profiles")}>
              {t("connect.goProfiles")}
            </Button>
            {profiles.length === 0 ? (
              <Button variant="ghost" onClick={() => setActiveTab("settings")}>
                {t("connect.goSettings")}
              </Button>
            ) : null}
          </div>
        </div>
      </Card>

      {startupError ? <ErrorBox error={startupError} /> : null}

      {actionError ? <ErrorBox error={actionError} /> : null}

      {connection === "error" && killSwitchActive ? (
        <div className="flex justify-end">
          <Button onClick={() => void disconnect()}>
            {t("connect.killSwitch.disarm")}
          </Button>
        </div>
      ) : null}

      {profiles.length === 0 ? (
        <p className="rounded-lg border border-neutral-800 bg-neutral-900/60 px-4 py-3 text-sm text-neutral-400">
          {t("connect.noSubscription")}
        </p>
      ) : null}

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card title={t("connect.helper")}>
          <Badge tone={badge.tone}>{helperStatus?.version ?? badge.label}</Badge>
        </Card>
        <Card title={t("connect.dns")}>
          <Badge
            tone={settings?.network.dns_mode === "system" ? "warn" : "success"}
          >
            {settings?.network.dns_mode === "system"
              ? t("connect.dns.system")
              : t("connect.dns.proxy")}
          </Badge>
          {settings?.network.dns_mode === "system" ? (
            <p className="mt-2 text-xs text-amber-300">
              {t("connect.dns.systemWarning")}
            </p>
          ) : null}
        </Card>
        <Card title={t("connect.allowlist")}>
          <Badge tone={activeRules > 0 ? "success" : "neutral"}>
            {activeRules}
          </Badge>
        </Card>
        <Card title={t("connect.killSwitch")}>
          <Badge
            tone={
              killSwitchError ? "warn" : killSwitchActive ? "success" : "neutral"
            }
          >
            {killSwitchError
              ? t("connect.killSwitch.error")
              : killSwitchActive
                ? t("connect.killSwitch.active")
                : t("connect.killSwitch.off")}
          </Badge>
          {killSwitchError ? (
            <p className="mt-2 text-xs text-amber-300">{killSwitchError}</p>
          ) : null}
          {connection === "error" && killSwitchActive ? (
            <p className="mt-2 text-xs text-amber-300">
              {t("connect.killSwitch.blocked")}
            </p>
          ) : null}
        </Card>
        <Card title={t("connect.autoSelect")}>
          <Badge tone={autoSwitchEnabled ? "success" : "neutral"}>
            {autoSwitchEnabled
              ? t("connect.autoSelect.on")
              : t("connect.autoSelect.off")}
          </Badge>
          {autoSwitchEnabled && autoSelect ? (
            <p className="mt-2 text-xs text-neutral-500">
              {t("connect.autoSelect.interval", {
                seconds: autoSelect.interval_secs,
              })}
            </p>
          ) : null}
          {bestProfile && bestLatency !== null ? (
            <p className="mt-2 text-xs text-neutral-500">
              {t("connect.autoSelect.lastCheck", {
                name: bestProfile.name,
                ms: bestLatency,
              })}
            </p>
          ) : null}
        </Card>
      </div>
    </div>
  );
}
