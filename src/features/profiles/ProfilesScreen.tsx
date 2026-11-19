import { Gauge, RefreshCw } from "lucide-react";
import type { SubscriptionView } from "../../shared/api/types";
import { errorText, locale, t } from "../../shared/i18n";
import { latencyById, sortProfilesByLatency } from "../../shared/lib/latency";
import { useAppStore } from "../../shared/state/appStore";
import { Badge, type Tone } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { EmptyState } from "../../shared/ui/EmptyState";
import { ErrorBox } from "../../shared/ui/ErrorBox";

function formatTime(timestamp: number | null): string {
  if (!timestamp) return t("common.none");
  return new Date(timestamp * 1000).toLocaleString(locale());
}

function latencyTone(milliseconds: number): Tone {
  if (milliseconds <= 100) return "success";
  if (milliseconds <= 250) return "warn";
  return "danger";
}

export function ProfilesScreen() {
  const subscriptions = useAppStore((state) => state.subscriptions);
  const busy = useAppStore((state) => state.busy);
  const pingProfiles = useAppStore((state) => state.pingProfiles);
  const setActiveTab = useAppStore((state) => state.setActiveTab);

  const pinging = Boolean(busy.ping);
  const hasSubscriptions = subscriptions.length > 0;
  const hasProfiles = subscriptions.some((entry) => entry.profiles.length > 0);

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("profiles.title")}>
        {hasSubscriptions ? (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="text-sm text-neutral-400">
              {t("profiles.count", { count: subscriptions.length })}
            </div>
            <Button
              onClick={() => void pingProfiles()}
              disabled={pinging || !hasProfiles}
            >
              <Gauge className={`size-4 ${pinging ? "animate-pulse" : ""}`} />
              {pinging ? t("profiles.pinging") : t("profiles.ping")}
            </Button>
          </div>
        ) : null}
      </Card>

      {hasSubscriptions ? (
        subscriptions.map((entry, index) => (
          <SubscriptionGroup key={entry.id} entry={entry} index={index} />
        ))
      ) : (
        <EmptyState
          title={t("profiles.empty.title")}
          hint={t("profiles.empty.hint")}
        >
          <Button onClick={() => setActiveTab("settings")}>
            {t("profiles.empty.goSettings")}
          </Button>
        </EmptyState>
      )}
    </div>
  );
}

function SubscriptionGroup({
  entry,
  index,
}: {
  entry: SubscriptionView;
  index: number;
}) {
  const selectedProfileId = useAppStore((state) => state.selectedProfileId);
  const skippedCount = useAppStore((state) => state.skippedCounts[entry.id]);
  const latency = useAppStore((state) => state.latency);
  const fetchError = useAppStore((state) => state.fetchErrors[entry.id]);
  const busy = useAppStore((state) => state.busy);
  const refreshSubscription = useAppStore((state) => state.refreshSubscription);
  const selectProfile = useAppStore((state) => state.selectProfile);

  const refreshing = Boolean(busy[`subscription-${entry.id}`]);
  const latencyResults = latencyById(latency);
  const ordered = sortProfilesByLatency(entry.profiles, latency);
  const title = entry.name.trim().length
    ? entry.name
    : t("profiles.defaultName", { n: index + 1 });

  return (
    <Card title={title}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="text-sm text-neutral-400">
          {entry.fetched_at
            ? t("profiles.fetchedAt", { time: formatTime(entry.fetched_at) })
            : t("profiles.empty.hint")}
        </div>
        <Button
          onClick={() => void refreshSubscription(entry.id)}
          disabled={refreshing}
        >
          <RefreshCw className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
          {refreshing ? t("profiles.refreshing") : t("profiles.refresh")}
        </Button>
      </div>
      {entry.auto_update_hours > 0
        ? entry.next_update_at
          ? (
              <p className="mt-2 text-xs text-neutral-500">
                {t("profiles.nextUpdate", { time: formatTime(entry.next_update_at) })}
              </p>
            )
          : null
        : entry.fetched_at
          ? (
              <p className="mt-2 text-xs text-neutral-500">
                {t("profiles.autoUpdateOff")}
              </p>
            )
          : null}
      {skippedCount > 0 ? (
        <p className="mt-3 text-sm text-amber-300">
          {t("profiles.skipped", { count: skippedCount })}
        </p>
      ) : null}
      {fetchError ? (
        <div className="mt-3">
          <ErrorBox
            error={fetchError}
            onRetry={() => void refreshSubscription(entry.id)}
          />
        </div>
      ) : null}

      {entry.profiles.length > 0 ? (
        <ul className="mt-3 flex flex-col gap-2">
          {ordered.map((profile) => {
            const selected = profile.id === selectedProfileId;
            const result = latencyResults.get(profile.id);
            const best = latency?.best_profile_id === profile.id;
            return (
              <li key={profile.id}>
                <button
                  type="button"
                  onClick={() => void selectProfile(profile.id)}
                  disabled={Boolean(busy.profile)}
                  className={`flex w-full items-center justify-between gap-3 rounded-lg border px-4 py-3 text-left transition-colors disabled:opacity-60 ${
                    selected
                      ? "border-emerald-700 bg-emerald-950/20"
                      : "border-neutral-800 bg-neutral-900/60 hover:border-neutral-600"
                  }`}
                >
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-medium text-neutral-100">
                      {profile.name}
                    </span>
                    <span className="block truncate text-xs text-neutral-500">
                      {profile.protocol.toUpperCase()} · {profile.tls.kind.toUpperCase()} ·{" "}
                      {profile.server}:{profile.port}
                    </span>
                  </span>
                  <span className="flex shrink-0 items-center gap-2">
                    {result ? (
                      result.latency_ms !== null ? (
                        <Badge tone={latencyTone(result.latency_ms)}>
                          {t("profiles.latencyMs", { ms: result.latency_ms })}
                        </Badge>
                      ) : (
                        <Badge tone="warn">
                          {errorText(result.error ?? "PING_UNREACHABLE")}
                        </Badge>
                      )
                    ) : null}
                    {best ? <Badge tone="info">{t("profiles.best")}</Badge> : null}
                    {selected ? (
                      <Badge tone="success">{t("profiles.selected")}</Badge>
                    ) : (
                      <span className="text-xs text-neutral-500">
                        {t("profiles.select")}
                      </span>
                    )}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      ) : null}
    </Card>
  );
}
