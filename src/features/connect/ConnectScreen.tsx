import { useEffect, useState } from "react";
import { Power } from "lucide-react";
import { t } from "../../shared/i18n/ru";
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

export function ConnectScreen() {
  const profiles = useAppStore((state) => state.profiles);
  const selectedProfileId = useAppStore((state) => state.selectedProfileId);
  const connection = useAppStore((state) => state.connection);
  const settings = useAppStore((state) => state.settings);
  const rules = useAppStore((state) => state.rules);
  const helperStatus = useAppStore((state) => state.helperStatus);
  const actionError = useAppStore((state) => state.actionError);
  const busy = useAppStore((state) => state.busy);
  const toggleConnection = useAppStore((state) => state.toggleConnection);
  const setActiveTab = useAppStore((state) => state.setActiveTab);

  const selected = profiles.find((profile) => profile.id === selectedProfileId);
  const session = useSessionTime(helperStatus?.started_at);
  const activeRules = rules.filter((rule) => rule.enabled).length;
  const connected = connection === "connected";
  const pending = connection === "connecting" || connection === "disconnecting";
  const canConnect = Boolean(selected) && !pending && !busy.connect;

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

          <div className="flex items-center gap-6 text-sm text-neutral-400">
            <span>
              {t("connect.sessionTime")}:{" "}
              <span className="font-mono text-neutral-200">{session}</span>
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

      {actionError ? <ErrorBox error={actionError} /> : null}

      {profiles.length === 0 ? (
        <p className="rounded-lg border border-neutral-800 bg-neutral-900/60 px-4 py-3 text-sm text-neutral-400">
          {t("connect.noSubscription")}
        </p>
      ) : null}

      <div className="grid gap-3 sm:grid-cols-3">
        <Card title={t("connect.helper")}>
          <Badge tone={helperStatus ? "success" : "warn"}>
            {helperStatus
              ? helperStatus.version ?? t("connect.helper.installed")
              : t("connect.helper.missing")}
          </Badge>
        </Card>
        <Card title={t("connect.dns")}>
          <Badge
            tone={settings?.network.dns_mode === "system" ? "info" : "warn"}
          >
            {settings?.network.dns_mode === "system"
              ? t("connect.dns.system")
              : t("connect.dns.process")}
          </Badge>
          {settings?.network.dns_mode === "process" ? (
            <p className="mt-2 text-xs text-amber-300">
              {t("connect.dns.processWarning")}
            </p>
          ) : null}
        </Card>
        <Card title={t("connect.allowlist")}>
          <Badge tone={activeRules > 0 ? "success" : "neutral"}>
            {activeRules}
          </Badge>
        </Card>
      </div>
    </div>
  );
}
