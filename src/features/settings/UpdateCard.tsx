import { useEffect } from "react";
import { Download, Loader2, RefreshCw } from "lucide-react";
import type { UpdateChannel } from "../../shared/api/types";
import { t } from "../../shared/i18n";
import { useAppStore } from "../../shared/state/appStore";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { Checkbox } from "../../shared/ui/Checkbox";
import { ErrorBox } from "../../shared/ui/ErrorBox";
import { Field, inputClass } from "../../shared/ui/Field";

interface UpdateCardProps {
  channel: UpdateChannel;
  checkOnStart: boolean;
  onChannelChange: (channel: UpdateChannel) => void;
  onCheckOnStartChange: (value: boolean) => void;
}

/** Карточка автообновления: версия, канал, проверка, прогресс и установка. */
export function UpdateCard({
  channel,
  checkOnStart,
  onChannelChange,
  onCheckOnStartChange,
}: UpdateCardProps) {
  const updateStatus = useAppStore((state) => state.updateStatus);
  const updateCheck = useAppStore((state) => state.updateCheck);
  const updateChecking = useAppStore((state) => state.updateChecking);
  const updateInstalling = useAppStore((state) => state.updateInstalling);
  const updateProgress = useAppStore((state) => state.updateProgress);
  const updateError = useAppStore((state) => state.updateError);
  const loadUpdateStatus = useAppStore((state) => state.loadUpdateStatus);
  const checkForUpdates = useAppStore((state) => state.checkForUpdates);
  const installUpdate = useAppStore((state) => state.installUpdate);
  const dismissUpdate = useAppStore((state) => state.dismissUpdate);

  useEffect(() => {
    void loadUpdateStatus();
  }, [loadUpdateStatus]);

  const configured = updateStatus?.configured ?? false;
  const busy = updateChecking || updateInstalling;
  const total = updateProgress?.total ?? null;
  const percent =
    total && total > 0
      ? Math.min(
          100,
          Math.round(((updateProgress?.downloaded ?? 0) / total) * 100),
        )
      : null;

  return (
    <Card title={t("settings.updates.title")}>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field
          label={t("settings.updates.currentVersion")}
          htmlFor="update-version"
        >
          <p
            id="update-version"
            className="rounded-md border border-neutral-800 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-200"
          >
            {updateStatus?.current_version ?? t("common.loading")}
          </p>
        </Field>
        <Field label={t("settings.updates.channel")} htmlFor="update-channel">
          <select
            id="update-channel"
            className={inputClass}
            value={channel}
            disabled={busy}
            onChange={(event) =>
              onChannelChange(event.target.value as UpdateChannel)
            }
          >
            <option value="stable">
              {t("settings.updates.channel.stable")}
            </option>
            <option value="beta">{t("settings.updates.channel.beta")}</option>
          </select>
        </Field>
      </div>

      <div className="mt-3 flex flex-col gap-1">
        <Checkbox
          checked={checkOnStart}
          onChange={onCheckOnStartChange}
          label={t("settings.updates.checkOnStart")}
        />
        <p className="pl-6 text-xs text-neutral-500">
          {t("settings.updates.checkOnStartHint")}
        </p>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-3">
        <Button
          onClick={() => void checkForUpdates(channel)}
          disabled={!configured || busy}
        >
          {updateChecking ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <RefreshCw className="size-4" />
          )}
          {updateChecking
            ? t("settings.updates.checking")
            : t("settings.updates.check")}
        </Button>
        {updateStatus && !configured ? (
          <span className="text-xs text-neutral-500">
            {t("settings.updates.notConfigured")}
          </span>
        ) : null}
      </div>

      {updateCheck && !updateCheck.available ? (
        <p className="mt-3 text-sm text-emerald-300">
          {t("settings.updates.upToDate")}
        </p>
      ) : null}

      {updateCheck?.available ? (
        <div className="mt-3 rounded-lg border border-neutral-800 bg-neutral-900/40 p-3">
          <p className="text-sm font-medium text-neutral-100">
            {t("settings.updates.available", {
              version: updateCheck.version ?? updateCheck.current_version,
            })}
          </p>
          {updateCheck.date ? (
            <p className="mt-1 text-xs text-neutral-500">
              {t("settings.updates.date", { date: updateCheck.date })}
            </p>
          ) : null}
          {updateCheck.notes ? (
            <p className="mt-2 whitespace-pre-line text-sm text-neutral-300">
              {updateCheck.notes}
            </p>
          ) : null}
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              variant="primary"
              disabled={updateInstalling}
              onClick={() => void installUpdate(channel)}
            >
              {updateInstalling ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Download className="size-4" />
              )}
              {updateInstalling
                ? t("settings.updates.installing")
                : t("settings.updates.install")}
            </Button>
            {!updateInstalling ? (
              <Button variant="ghost" onClick={dismissUpdate}>
                {t("settings.updates.later")}
              </Button>
            ) : null}
          </div>
          {updateInstalling ? (
            <div className="mt-3">
              <p className="text-xs text-neutral-400">
                {percent === null
                  ? t("settings.updates.progressUnknown")
                  : t("settings.updates.progress", { percent })}
              </p>
              {percent !== null ? (
                <div
                  role="progressbar"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent}
                  className="mt-1 h-1.5 overflow-hidden rounded-full bg-neutral-800"
                >
                  <div
                    className="h-full bg-emerald-500 transition-all"
                    style={{ width: `${percent}%` }}
                  />
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}

      {updateError ? (
        <div className="mt-3">
          <ErrorBox error={updateError} />
        </div>
      ) : null}

      <p className="mt-3 text-xs text-neutral-500">
        {t("settings.updates.restartHint")}
      </p>
      <p className="mt-1 text-xs text-neutral-500">
        {t("settings.updates.signatureHint")}
      </p>
      <p className="mt-1 text-xs text-neutral-500">
        {t("settings.updates.helperHint")}
      </p>
    </Card>
  );
}
