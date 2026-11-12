import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { t } from "../../shared/i18n/ru";
import { useAppStore, type NewRuleInput } from "../../shared/state/appStore";
import { Badge } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { EmptyState } from "../../shared/ui/EmptyState";
import { Field, inputClass } from "../../shared/ui/Field";

function ruleValues(rule: {
  process_name?: string[];
  process_path?: string[];
  path_regex?: string[];
}): string {
  return [
    ...(rule.process_name ?? []),
    ...(rule.process_path ?? []),
    ...(rule.path_regex ?? []),
  ].join(", ");
}

const emptyInput: NewRuleInput = { name: "", kind: "process", value: "" };

export function AppsScreen() {
  const rules = useAppStore((state) => state.rules);
  const busy = useAppStore((state) => state.busy);
  const addRule = useAppStore((state) => state.addRule);
  const toggleRule = useAppStore((state) => state.toggleRule);
  const deleteRule = useAppStore((state) => state.deleteRule);
  const [input, setInput] = useState<NewRuleInput>(emptyInput);
  const [invalid, setInvalid] = useState(false);

  const submit = async () => {
    if (!input.name.trim() || !input.value.trim()) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    await addRule(input);
    setInput(emptyInput);
  };

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("apps.title")}>
        <p className="text-sm leading-relaxed text-neutral-400">
          {t("apps.explanation")}
        </p>
      </Card>

      <Card title={t("apps.add.title")}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t("apps.add.name")} htmlFor="rule-name">
            <input
              id="rule-name"
              className={inputClass}
              value={input.name}
              onChange={(event) =>
                setInput({ ...input, name: event.target.value })
              }
            />
          </Field>
          <Field label={t("apps.add.type")} htmlFor="rule-kind">
            <select
              id="rule-kind"
              className={inputClass}
              value={input.kind}
              onChange={(event) =>
                setInput({
                  ...input,
                  kind: event.target.value as NewRuleInput["kind"],
                })
              }
            >
              <option value="process">{t("apps.add.type.process")}</option>
              <option value="path">{t("apps.add.type.path")}</option>
              <option value="regex">{t("apps.add.type.regex")}</option>
            </select>
          </Field>
          <Field label={t("apps.add.value")} htmlFor="rule-value">
            <input
              id="rule-value"
              className={inputClass}
              value={input.value}
              onChange={(event) =>
                setInput({ ...input, value: event.target.value })
              }
            />
          </Field>
        </div>
        <div className="mt-3 flex items-center gap-3">
          <Button
            variant="primary"
            onClick={() => void submit()}
            disabled={Boolean(busy.rule)}
          >
            <Plus className="size-4" />
            {t("apps.add.submit")}
          </Button>
          {invalid ? (
            <span className="text-xs text-amber-300">
              {t("apps.add.invalid")}
            </span>
          ) : null}
        </div>
      </Card>

      {rules.length === 0 ? (
        <EmptyState title={t("apps.empty")} />
      ) : (
        <ul className="flex flex-col gap-2">
          {rules.map((rule) => (
            <li
              key={rule.id}
              className="flex items-center justify-between gap-3 rounded-lg border border-neutral-800 bg-neutral-900/60 px-4 py-3"
            >
              <div className="min-w-0">
                <p className="truncate text-sm font-medium text-neutral-100">
                  {rule.name}
                </p>
                <p className="truncate font-mono text-xs text-neutral-500">
                  {ruleValues(rule)}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  aria-pressed={rule.enabled}
                  onClick={() => void toggleRule(rule)}
                  className="cursor-pointer"
                >
                  <Badge tone={rule.enabled ? "success" : "neutral"}>
                    {t("apps.enabled")}: {t(rule.enabled ? "common.yes" : "common.no")}
                  </Badge>
                </button>
                <Button
                  variant="ghost"
                  aria-label={t("apps.delete")}
                  onClick={() => void deleteRule(rule.id)}
                >
                  <Trash2 className="size-4" />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}

      <p className="text-xs text-neutral-500">
        {t("apps.ruleCount", { count: rules.length })}
      </p>
    </div>
  );
}
