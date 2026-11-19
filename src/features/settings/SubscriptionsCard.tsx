import { useState } from "react";
import { Plus, RefreshCw, Save, Trash2 } from "lucide-react";
import type { SubscriptionInput, SubscriptionView } from "../../shared/api/types";
import { t } from "../../shared/i18n";
import { useAppStore } from "../../shared/state/appStore";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { ErrorBox } from "../../shared/ui/ErrorBox";
import { Field, inputClass } from "../../shared/ui/Field";

function inputOf(entry: SubscriptionView): SubscriptionInput {
  return {
    name: entry.name,
    url: entry.url,
    user_agent: entry.user_agent,
    auto_update_hours: entry.auto_update_hours,
  };
}

function sameInput(a: SubscriptionInput, b: SubscriptionInput): boolean {
  return (
    a.name === b.name &&
    a.url === b.url &&
    a.user_agent === b.user_agent &&
    a.auto_update_hours === b.auto_update_hours
  );
}

const emptyInput: SubscriptionInput = {
  name: "",
  url: "",
  user_agent: "",
  auto_update_hours: 12,
};

/** Управление подписками: добавление, изменение, обновление, удаление.
 *
 * Изменения сохраняются каждой записью отдельно (subscription_update), а не
 * общей кнопкой настроек: подписки независимы, и смена URL сразу обновляет
 * её локации.
 */
export function SubscriptionsCard() {
  const subscriptions = useAppStore((state) => state.subscriptions);
  const actionError = useAppStore((state) => state.actionError);
  const busy = useAppStore((state) => state.busy);
  // Черновики полей живут отдельно от стора: фоновое обновление профилей
  // не затирает незаконченную правку параметров.
  const [drafts, setDrafts] = useState<Record<string, SubscriptionInput>>({});
  const [adding, setAdding] = useState<SubscriptionInput | null>(null);

  const draftOf = (entry: SubscriptionView) =>
    drafts[entry.id] ?? inputOf(entry);

  const setDraft = (id: string, patch: Partial<SubscriptionInput>) => {
    const entry = subscriptions.find((item) => item.id === id);
    if (!entry) return;
    setDrafts({
      ...drafts,
      [id]: { ...draftOf(entry), ...patch },
    });
  };

  const discardDraft = (id: string) =>
    setDrafts((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });

  return (
    <Card title={t("subscriptions.title")}>
      <p className="text-xs text-neutral-500">{t("subscriptions.hint")}</p>

      {subscriptions.length === 0 ? (
        <p className="mt-3 text-sm text-neutral-400">{t("subscriptions.empty")}</p>
      ) : (
        <div className="mt-3 flex flex-col gap-3">
          {subscriptions.map((entry) => (
            <SubscriptionEntry
              key={entry.id}
              entry={entry}
              draft={draftOf(entry)}
              onChange={(patch) => setDraft(entry.id, patch)}
              onSave={() => {
                void useAppStore
                  .getState()
                  .updateSubscription(entry.id, draftOf(entry))
                  .then(() => discardDraft(entry.id));
              }}
              onDelete={() => {
                void useAppStore.getState().deleteSubscription(entry.id);
              }}
            />
          ))}
        </div>
      )}

      {adding ? (
        <div className="mt-4 rounded-lg border border-neutral-800 p-3">
          <p className="text-sm font-medium text-neutral-200">
            {t("subscriptions.addTitle")}
          </p>
          <div className="mt-3 grid gap-3">
            <Field label={t("subscriptions.name")} htmlFor="subscription-add-name">
              <input
                id="subscription-add-name"
                className={inputClass}
                value={adding.name}
                onChange={(event) =>
                  setAdding({ ...adding, name: event.target.value })
                }
              />
            </Field>
            <Field
              label={t("subscriptions.url")}
              hint={t("subscriptions.urlHint")}
              htmlFor="subscription-add-url"
            >
              <input
                id="subscription-add-url"
                type="url"
                className={inputClass}
                value={adding.url}
                onChange={(event) =>
                  setAdding({ ...adding, url: event.target.value })
                }
              />
            </Field>
            <div className="grid gap-3 sm:grid-cols-2">
              <Field
                label={t("subscriptions.userAgent")}
                htmlFor="subscription-add-agent"
              >
                <input
                  id="subscription-add-agent"
                  className={inputClass}
                  value={adding.user_agent}
                  onChange={(event) =>
                    setAdding({ ...adding, user_agent: event.target.value })
                  }
                />
              </Field>
              <Field
                label={t("subscriptions.autoUpdate")}
                hint={t("subscriptions.autoUpdateHint")}
                htmlFor="subscription-add-interval"
              >
                <input
                  id="subscription-add-interval"
                  type="number"
                  min={0}
                  max={168}
                  className={inputClass}
                  value={adding.auto_update_hours}
                  onChange={(event) =>
                    setAdding({
                      ...adding,
                      auto_update_hours: Number(event.target.value),
                    })
                  }
                />
              </Field>
            </div>
          </div>
          <div className="mt-3 flex gap-2">
            <Button
              variant="primary"
              disabled={Boolean(busy["subscription-add"])}
              onClick={() => {
                void useAppStore.getState().addSubscription(adding).then(() => {
                  // Форма закрывается при успехе: подписка появилась в списке.
                  if (!useAppStore.getState().actionError) setAdding(null);
                });
              }}
            >
              <Plus className="size-4" />
              {t("subscriptions.add")}
            </Button>
            <Button onClick={() => setAdding(null)}>
              {t("subscriptions.cancel")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="mt-4">
          <Button onClick={() => setAdding({ ...emptyInput })}>
            <Plus className="size-4" />
            {t("subscriptions.addTitle")}
          </Button>
        </div>
      )}

      {actionError ? (
        <div className="mt-4">
          <ErrorBox error={actionError} />
        </div>
      ) : null}
    </Card>
  );
}

function SubscriptionEntry({
  entry,
  draft,
  onChange,
  onSave,
  onDelete,
}: {
  entry: SubscriptionView;
  draft: SubscriptionInput;
  onChange: (patch: Partial<SubscriptionInput>) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  const busy = useAppStore((state) => state.busy);
  const fetchError = useAppStore((state) => state.fetchErrors[entry.id]);
  const refreshSubscription = useAppStore((state) => state.refreshSubscription);
  const dirty = !sameInput(draft, inputOf(entry));
  const refreshing = Boolean(busy[`subscription-${entry.id}`]);
  const saving = Boolean(busy[`subscription-update-${entry.id}`]);
  const deleting = Boolean(busy[`subscription-delete-${entry.id}`]);
  const idPrefix = `subscription-${entry.id}`;

  return (
    <div className="rounded-lg border border-neutral-800 p-3">
      <div className="grid gap-3">
        <Field label={t("subscriptions.name")} htmlFor={`${idPrefix}-name`}>
          <input
            id={`${idPrefix}-name`}
            className={inputClass}
            value={draft.name}
            onChange={(event) => onChange({ name: event.target.value })}
          />
        </Field>
        <Field
          label={t("subscriptions.url")}
          hint={t("subscriptions.urlHint")}
          htmlFor={`${idPrefix}-url`}
        >
          <input
            id={`${idPrefix}-url`}
            type="url"
            className={inputClass}
            value={draft.url}
            onChange={(event) => onChange({ url: event.target.value })}
          />
        </Field>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field
            label={t("subscriptions.userAgent")}
            htmlFor={`${idPrefix}-agent`}
          >
            <input
              id={`${idPrefix}-agent`}
              className={inputClass}
              value={draft.user_agent}
              onChange={(event) => onChange({ user_agent: event.target.value })}
            />
          </Field>
          <Field
            label={t("subscriptions.autoUpdate")}
            hint={t("subscriptions.autoUpdateHint")}
            htmlFor={`${idPrefix}-interval`}
          >
            <input
              id={`${idPrefix}-interval`}
              type="number"
              min={0}
              max={168}
              className={inputClass}
              value={draft.auto_update_hours}
              onChange={(event) =>
                onChange({ auto_update_hours: Number(event.target.value) })
              }
            />
          </Field>
        </div>
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button
          onClick={() => void refreshSubscription(entry.id)}
          disabled={refreshing}
        >
          <RefreshCw className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
          {refreshing ? t("profiles.refreshing") : t("profiles.refresh")}
        </Button>
        <Button
          variant="primary"
          onClick={onSave}
          disabled={!dirty || saving || !draft.url.trim()}
        >
          <Save className="size-4" />
          {t("subscriptions.save")}
        </Button>
        <Button variant="danger" onClick={onDelete} disabled={deleting}>
          <Trash2 className="size-4" />
          {t("subscriptions.delete")}
        </Button>
      </div>
      {fetchError ? (
        <div className="mt-3">
          <ErrorBox
            error={fetchError}
            onRetry={() => void refreshSubscription(entry.id)}
          />
        </div>
      ) : null}
    </div>
  );
}
