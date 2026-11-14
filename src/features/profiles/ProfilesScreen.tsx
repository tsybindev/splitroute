import { Gauge, RefreshCw } from "lucide-react";
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
  const profiles = useAppStore((state) => state.profiles);
  const selectedProfileId = useAppStore((state) => state.selectedProfileId);
  const fetchedAt = useAppStore((state) => state.fetchedAt);
  const skippedCount = useAppStore((state) => state.skippedCount);
  const latency = useAppStore((state) => state.latency);
  const fetchError = useAppStore((state) => state.fetchError);
  const busy = useAppStore((state) => state.busy);
  const refreshSubscription = useAppStore((state) => state.refreshSubscription);
  const pingProfiles = useAppStore((state) => state.pingProfiles);
  const selectProfile = useAppStore((state) => state.selectProfile);
  const setActiveTab = useAppStore((state) => state.setActiveTab);

  const refreshing = Boolean(busy.subscription);
  const pinging = Boolean(busy.ping);
  const latencyResults = latencyById(latency);
  const ordered = sortProfilesByLatency(profiles, latency);

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("profiles.title")}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="text-sm text-neutral-400">
            {fetchedAt
              ? t("profiles.fetchedAt", { time: formatTime(fetchedAt) })
              : t("profiles.empty.hint")}
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              onClick={() => void pingProfiles()}
              disabled={pinging || profiles.length === 0}
            >
              <Gauge className={`size-4 ${pinging ? "animate-pulse" : ""}`} />
              {pinging ? t("profiles.pinging") : t("profiles.ping")}
            </Button>
            <Button
              onClick={() => void refreshSubscription()}
              disabled={refreshing}
            >
              <RefreshCw className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
              {refreshing ? t("profiles.refreshing") : t("profiles.refresh")}
            </Button>
          </div>
        </div>
        {skippedCount > 0 ? (
          <p className="mt-3 text-sm text-amber-300">
            {t("profiles.skipped", { count: skippedCount })}
          </p>
        ) : null}
      </Card>

      {fetchError ? (
        <ErrorBox
          error={fetchError}
          onRetry={() => void refreshSubscription()}
        />
      ) : null}

      {profiles.length === 0 ? (
        <EmptyState
          title={t("profiles.empty.title")}
          hint={t("profiles.empty.hint")}
        >
          <Button onClick={() => setActiveTab("settings")}>
            {t("profiles.empty.goSettings")}
          </Button>
        </EmptyState>
      ) : (
        <ul className="flex flex-col gap-2">
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
      )}
    </div>
  );
}
