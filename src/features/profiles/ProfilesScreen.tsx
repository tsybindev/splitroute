import { RefreshCw } from "lucide-react";
import { t } from "../../shared/i18n/ru";
import { useAppStore } from "../../shared/state/appStore";
import { Badge } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { EmptyState } from "../../shared/ui/EmptyState";
import { ErrorBox } from "../../shared/ui/ErrorBox";

function formatTime(timestamp: number | null): string {
  if (!timestamp) return t("common.none");
  return new Date(timestamp * 1000).toLocaleString("ru-RU");
}

export function ProfilesScreen() {
  const profiles = useAppStore((state) => state.profiles);
  const selectedProfileId = useAppStore((state) => state.selectedProfileId);
  const fetchedAt = useAppStore((state) => state.fetchedAt);
  const skippedCount = useAppStore((state) => state.skippedCount);
  const fetchError = useAppStore((state) => state.fetchError);
  const busy = useAppStore((state) => state.busy);
  const refreshSubscription = useAppStore((state) => state.refreshSubscription);
  const selectProfile = useAppStore((state) => state.selectProfile);
  const setActiveTab = useAppStore((state) => state.setActiveTab);

  const refreshing = Boolean(busy.subscription);

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("profiles.title")}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="text-sm text-neutral-400">
            {fetchedAt
              ? t("profiles.fetchedAt", { time: formatTime(fetchedAt) })
              : t("profiles.empty.hint")}
          </div>
          <Button
            onClick={() => void refreshSubscription()}
            disabled={refreshing}
          >
            <RefreshCw className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
            {refreshing ? t("profiles.refreshing") : t("profiles.refresh")}
          </Button>
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
          {profiles.map((profile) => {
            const selected = profile.id === selectedProfileId;
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
                  {selected ? (
                    <Badge tone="success">{t("profiles.selected")}</Badge>
                  ) : (
                    <span className="text-xs text-neutral-500">
                      {t("profiles.select")}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
