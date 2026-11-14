import { useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { api, toCommandError } from "../../shared/api/client";
import type { AppSettings, CommandError, Language } from "../../shared/api/types";
import { t } from "../../shared/i18n";
import { helperBadge } from "../../shared/lib/helperState";
import { useAppStore } from "../../shared/state/appStore";
import { Badge } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { ErrorBox } from "../../shared/ui/ErrorBox";
import { Field, inputClass } from "../../shared/ui/Field";

function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-neutral-200">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="size-4 accent-emerald-600"
      />
      {label}
    </label>
  );
}

export function SettingsScreen() {
  const stored = useAppStore((state) => state.settings);

  if (!stored) {
    return (
      <Card title={t("settings.title")}>
        <p className="flex items-center gap-2 text-sm text-neutral-400">
          <Loader2 className="size-4 animate-spin" />
          {t("common.loading")}
        </p>
      </Card>
    );
  }

  return <SettingsForm initial={stored} />;
}

function SettingsForm({ initial }: { initial: AppSettings }) {
  const helperStatus = useAppStore((state) => state.helperStatus);
  const helperStatusError = useAppStore((state) => state.helperStatusError);
  const actionError = useAppStore((state) => state.actionError);
  const notice = useAppStore((state) => state.notice);
  const busy = useAppStore((state) => state.busy);
  const saveSettings = useAppStore((state) => state.saveSettings);
  const language = useAppStore(
    (state) => state.settings?.preferences.language ?? "ru",
  );
  const setLanguagePreference = useAppStore(
    (state) => state.setLanguagePreference,
  );
  const regenerateHwid = useAppStore((state) => state.regenerateHwid);
  const refreshHelperStatus = useAppStore((state) => state.refreshHelperStatus);
  const [form, setForm] = useState<AppSettings>(initial);
  const [helperError, setHelperError] = useState<CommandError | null>(null);
  const [helperNotice, setHelperNotice] = useState<string | null>(null);
  const [helperBusy, setHelperBusy] = useState(false);
  const badge = helperBadge(helperStatus, helperStatusError);

  const updateSubscription = (patch: Partial<AppSettings["subscription"]>) =>
    setForm({
      ...form,
      subscription: { ...form.subscription, ...patch },
    });

  const updateNetwork = (patch: Partial<AppSettings["network"]>) =>
    setForm({ ...form, network: { ...form.network, ...patch } });

  const updatePreferences = (patch: Partial<AppSettings["preferences"]>) =>
    setForm({ ...form, preferences: { ...form.preferences, ...patch } });

  const runHelperAction = async (action: "install" | "uninstall") => {
    setHelperError(null);
    setHelperNotice(null);
    setHelperBusy(true);
    try {
      if (action === "install") {
        await api.helperInstall();
        setHelperNotice(t("settings.helper.installed"));
      } else {
        await api.helperUninstall();
        setHelperNotice(t("settings.helper.removed"));
      }
    } catch (error) {
      // Показываем причину установщика: права, отмену, отсутствующий бинарь
      // или неподдерживаемую платформу — не общую строку.
      setHelperError(toCommandError(error));
    } finally {
      setHelperBusy(false);
      // Статус берём только из helper_status: пароля и локального «успеха» нет.
      await refreshHelperStatus();
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("settings.subscription.title")}>
        <div className="grid gap-3">
          <Field
            label={t("settings.subscription.url")}
            hint={t("settings.subscription.urlHint")}
            htmlFor="subscription-url"
          >
            <input
              id="subscription-url"
              type="url"
              className={inputClass}
              value={form.subscription.url}
              onChange={(event) =>
                updateSubscription({ url: event.target.value })
              }
            />
          </Field>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field
              label={t("settings.subscription.hwid")}
              hint={t("settings.subscription.hwidHint")}
              htmlFor="subscription-hwid"
            >
              <div className="flex gap-2">
                <input
                  id="subscription-hwid"
                  className={inputClass}
                  value={form.subscription.hwid}
                  onChange={(event) =>
                    updateSubscription({ hwid: event.target.value })
                  }
                />
                <Button
                  onClick={() => {
                    void regenerateHwid().then((hwid) => {
                      if (hwid) updateSubscription({ hwid });
                    });
                  }}
                >
                  <RefreshCw className="size-4" />
                  {t("settings.subscription.regenerate")}
                </Button>
              </div>
            </Field>
            <Field
              label={t("settings.subscription.userAgent")}
              htmlFor="subscription-agent"
            >
              <input
                id="subscription-agent"
                className={inputClass}
                value={form.subscription.user_agent}
                onChange={(event) =>
                  updateSubscription({ user_agent: event.target.value })
                }
              />
            </Field>
          </div>
          <Field
            label={t("settings.subscription.autoUpdate")}
            htmlFor="subscription-interval"
          >
            <input
              id="subscription-interval"
              type="number"
              min={1}
              max={168}
              className={inputClass}
              value={form.subscription.auto_update_hours}
              onChange={(event) =>
                updateSubscription({
                  auto_update_hours: Number(event.target.value),
                })
              }
            />
          </Field>
        </div>
      </Card>

      <Card title={t("settings.network.title")}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t("settings.network.mtu")} htmlFor="network-mtu">
            <input
              id="network-mtu"
              type="number"
              min={576}
              max={9000}
              className={inputClass}
              value={form.network.mtu}
              onChange={(event) =>
                updateNetwork({ mtu: Number(event.target.value) })
              }
            />
          </Field>
          <Field
            label={t("settings.network.logLevel")}
            htmlFor="network-loglevel"
          >
            <select
              id="network-loglevel"
              className={inputClass}
              value={form.network.log_level}
              onChange={(event) =>
                updateNetwork({
                  log_level: event.target.value as AppSettings["network"]["log_level"],
                })
              }
            >
              {["trace", "debug", "info", "warn", "error"].map((level) => (
                <option key={level} value={level}>
                  {level}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("settings.network.dns")} htmlFor="network-dns">
            <select
              id="network-dns"
              className={inputClass}
              value={form.network.dns_mode}
              onChange={(event) =>
                updateNetwork({
                  dns_mode: event.target.value as AppSettings["network"]["dns_mode"],
                })
              }
            >
              <option value="proxy">
                {t("settings.network.dns.proxy")}
              </option>
              <option value="system">{t("settings.network.dns.system")}</option>
            </select>
            {form.network.dns_mode === "system" ? (
              <span className="text-xs text-amber-300">
                {t("settings.network.dns.systemHint")}
              </span>
            ) : (
              <span className="text-xs text-neutral-500">
                {t("settings.network.dns.proxyHint")}
              </span>
            )}
          </Field>
          <Field
            label={t("settings.network.mixedPort")}
            htmlFor="network-mixedport"
          >
            <input
              id="network-mixedport"
              type="number"
              min={1}
              max={65535}
              className={inputClass}
              value={form.network.mixed_port}
              disabled={!form.network.mixed_inbound_enabled}
              onChange={(event) =>
                updateNetwork({ mixed_port: Number(event.target.value) })
              }
            />
          </Field>
        </div>
        <div className="mt-3 flex flex-col gap-2">
          <Checkbox
            checked={form.network.ipv6_enabled}
            onChange={(value) => updateNetwork({ ipv6_enabled: value })}
            label={t("settings.network.ipv6")}
          />
          <p className="pl-6 text-xs text-neutral-500">
            {t("settings.network.ipv6Hint")}
          </p>
          <Checkbox
            checked={form.network.mixed_inbound_enabled}
            onChange={(value) =>
              updateNetwork({ mixed_inbound_enabled: value })
            }
            label={t("settings.network.mixed")}
          />
        </div>
      </Card>

      <Card title={t("settings.killSwitch.title")}>
        <Checkbox
          checked={form.network.kill_switch}
          onChange={(value) => updateNetwork({ kill_switch: value })}
          label={t("settings.killSwitch.enable")}
        />
        <p className="mt-2 pl-6 text-xs text-amber-300">
          {t("settings.killSwitch.hint")}
        </p>
        <p className="mt-1 pl-6 text-xs text-neutral-500">
          {t("settings.killSwitch.applies")}
        </p>
      </Card>

      <Card title={t("settings.app.title")}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field
            label={t("settings.app.language")}
            hint={t("settings.app.languageHint")}
            htmlFor="app-language"
          >
            <select
              id="app-language"
              className={inputClass}
              value={language}
              disabled={Boolean(busy.language)}
              onChange={(event) =>
                void setLanguagePreference(event.target.value as Language)
              }
            >
              <option value="ru">{t("settings.app.language.ru")}</option>
              <option value="en">{t("settings.app.language.en")}</option>
            </select>
          </Field>
        </div>
        <div className="mt-3 flex flex-col gap-2">
          <Checkbox
            checked={form.preferences.launch_at_login}
            onChange={(value) =>
              updatePreferences({ launch_at_login: value })
            }
            label={t("settings.app.autostart")}
          />
          <Checkbox
            checked={form.preferences.launch_minimized}
            onChange={(value) =>
              updatePreferences({ launch_minimized: value })
            }
            label={t("settings.app.launchMinimized")}
          />
          <Checkbox
            checked={form.preferences.connect_on_start}
            onChange={(value) =>
              updatePreferences({ connect_on_start: value })
            }
            label={t("settings.app.connectOnStart")}
          />
          <Checkbox
            checked={form.preferences.close_to_tray}
            onChange={(value) => updatePreferences({ close_to_tray: value })}
            label={t("settings.app.closeToTray")}
          />
        </div>
      </Card>

      <Card title={t("settings.helper.title")}>
        <div className="flex flex-wrap items-center gap-3">
          <Badge tone={badge.tone}>
            {helperStatus?.version ?? badge.label}
          </Badge>
          <Button
            onClick={() => void runHelperAction("install")}
            disabled={helperBusy}
          >
            {helperStatus
              ? t("settings.helper.reinstall")
              : t("settings.helper.install")}
          </Button>
          <Button
            variant="danger"
            onClick={() => void runHelperAction("uninstall")}
            disabled={helperBusy}
          >
            {t("settings.helper.uninstall")}
          </Button>
        </div>
        <p className="mt-2 text-xs text-neutral-500">
          {t("settings.helper.hint")}
        </p>
        {helperNotice ? (
          <p className="mt-3 text-sm text-emerald-300">{helperNotice}</p>
        ) : null}
        {helperError ? (
          <div className="mt-3">
            <ErrorBox error={helperError} showDetail />
          </div>
        ) : null}
      </Card>

      {actionError ? <ErrorBox error={actionError} /> : null}

      <div className="flex items-center gap-3">
        <Button
          variant="primary"
          disabled={Boolean(busy.settings)}
          onClick={() => void saveSettings(form)}
        >
          {t("settings.save")}
        </Button>
        {notice ? (
          <span className="text-sm text-emerald-300">{t("settings.saved")}</span>
        ) : null}
      </div>
    </div>
  );
}
