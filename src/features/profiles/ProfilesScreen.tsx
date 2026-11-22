import { Gauge, RefreshCw } from "lucide-react";
import type {
  LatencyResult,
  Profile,
  SubscriptionView,
} from "../../shared/api/types";
import { errorText, locale, t } from "../../shared/i18n";
import { latencyById } from "../../shared/lib/latency";
import { useAppStore } from "../../shared/state/appStore";
import { Badge, type Tone } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { Checkbox } from "../../shared/ui/Checkbox";
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
  const setProfilesEnabled = useAppStore((state) => state.setProfilesEnabled);

  const refreshing = Boolean(busy[`subscription-${entry.id}`]);
  const toggling = Boolean(busy[`profiles-enabled-${entry.id}`]);
  const latencyResults = latencyById(latency);
  // Порядок локаций — как в ответе подписки; «Лучшая» подсвечивается бейджем.
  const title = entry.name.trim().length
    ? entry.name
    : t("profiles.defaultName", { n: index + 1 });
  const allIds = entry.profiles.map((profile) => profile.id);

  const setAll = (enabled: boolean) => {
    void setProfilesEnabled(entry.id, allIds, enabled);
  };

  return (
    <Card title={title}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="text-sm text-neutral-400">
          {entry.fetched_at
            ? t("profiles.fetchedAt", { time: formatTime(entry.fetched_at) })
            : t("profiles.empty.hint")}
        </div>
        <div className="flex gap-2">
          {entry.profiles.length > 0 ? (
            <>
              <Button
                onClick={() => setAll(true)}
                disabled={toggling || entry.disabled_profiles.length === 0}
              >
                {t("profiles.enableAll")}
              </Button>
              <Button
                onClick={() => setAll(false)}
                disabled={toggling || entry.disabled_profiles.length === allIds.length}
              >
                {t("profiles.disableAll")}
              </Button>
            </>
          ) : null}
          <Button
            onClick={() => void refreshSubscription(entry.id)}
            disabled={refreshing}
          >
            <RefreshCw className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
            {refreshing ? t("profiles.refreshing") : t("profiles.refresh")}
          </Button>
        </div>
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
        <>
          {entry.disabled_profiles.length > 0 ? (
            <p className="mt-3 text-xs text-neutral-500">
              {t("profiles.disabledHint")}
            </p>
          ) : null}
          <ul className="mt-3 flex flex-col gap-2">
            {entry.profiles.map((profile) => (
              <ProfileRow
                key={profile.id}
                profile={profile}
                enabled={!entry.disabled_profiles.includes(profile.id)}
                selected={profile.id === selectedProfileId}
                best={latency?.best_profile_id === profile.id}
                result={latencyResults.get(profile.id)}
                busy={toggling || Boolean(busy.profile)}
                onSelect={() => void selectProfile(profile.id)}
                onToggle={(enabled) =>
                  void setProfilesEnabled(entry.id, [profile.id], enabled)
                }
              />
            ))}
          </ul>
        </>
      ) : null}
    </Card>
  );
}

function ProfileRow({
  profile,
  enabled,
  selected,
  best,
  result,
  busy,
  onSelect,
  onToggle,
}: {
  profile: Profile;
  enabled: boolean;
  selected: boolean;
  best: boolean;
  result: LatencyResult | undefined;
  busy: boolean;
  onSelect: () => void;
  onToggle: (enabled: boolean) => void;
}) {
  return (
    <li>
      <div
        className={`flex w-full items-center gap-2 rounded-lg border px-3 py-3 ${
          selected
            ? "border-emerald-700 bg-emerald-950/20"
            : enabled
              ? "border-neutral-800 bg-neutral-900/60"
              : "border-neutral-800 bg-neutral-900/40"
        }`}
      >
        <Checkbox
          checked={enabled}
          onChange={onToggle}
          ariaLabel={`${t("profiles.enableToggle")}: ${profile.name}`}
          disabled={busy}
        />
        <button
          type="button"
          onClick={onSelect}
          disabled={!enabled || busy}
          className={`flex min-w-0 flex-1 items-center justify-between gap-3 text-left transition-colors disabled:opacity-60 ${
            enabled ? "hover:opacity-80" : ""
          }`}
        >
          <span className="min-w-0">
            <span
              className={`block truncate text-sm font-medium text-neutral-100 ${
                enabled ? "" : "opacity-60"
              }`}
            >
              {profile.name}
            </span>
            <span className="block truncate text-xs text-neutral-500">
              {profile.protocol.toUpperCase()} · {profile.tls.kind.toUpperCase()} ·{" "}
              {profile.server}:{profile.port}
            </span>
          </span>
          <span className="flex shrink-0 items-center gap-2">
            {!enabled ? (
              <Badge tone="neutral">{t("profiles.disabledBadge")}</Badge>
            ) : (
              <>
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
              </>
            )}
            {selected ? (
              <Badge tone="success">{t("profiles.selected")}</Badge>
            ) : (
              enabled && (
                <span className="text-xs text-neutral-500">
                  {t("profiles.select")}
                </span>
              )
            )}
          </span>
        </button>
      </div>
    </li>
  );
}
