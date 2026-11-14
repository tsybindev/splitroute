import { useEffect, useMemo, useState } from "react";
import { Plus, RefreshCw, Search, Trash2 } from "lucide-react";
import type { InstalledApp, RunningApp } from "../../shared/api/types";
import { t, type MessageKey } from "../../shared/i18n";
import { useAppStore, type NewRuleInput } from "../../shared/state/appStore";
import { Badge } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { EmptyState } from "../../shared/ui/EmptyState";
import { ErrorBox } from "../../shared/ui/ErrorBox";
import { Field, inputClass } from "../../shared/ui/Field";
import { AppIcon } from "./AppIcon";

type AppsTab = "installed" | "running" | "rules";

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

// Правило сохранено, но пути на диске нет: Rust проверяет только однозначные
// пути (PRD §6.3), поэтому пометка не бывает ложной. Включение и удаление
// правила остаются доступны.
function MissingPathBadge({ paths }: { paths: string[] }) {
  if (paths.length === 0) return null;
  return (
    <span title={t("apps.inactive.hint", { paths: paths.join(", ") })}>
      <Badge tone="warn">{t("apps.inactive")}</Badge>
    </span>
  );
}

// Порядок вкладок — как в PRD §7; по умолчанию открыт каталог.
const tabLabels: Record<AppsTab, MessageKey> = {
  running: "apps.tabs.running",
  installed: "apps.tabs.installed",
  rules: "apps.tabs.rules",
};

export function AppsScreen() {
  const [tab, setTab] = useState<AppsTab>("installed");

  return (
    <div className="flex flex-col gap-4">
      <h2 className="text-base font-medium">{t("apps.title")}</h2>
      <div role="tablist" className="flex gap-1">
        {(Object.keys(tabLabels) as AppsTab[]).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={`rounded-md px-3 py-1.5 text-sm transition-colors ${
              tab === id
                ? "bg-neutral-800 text-neutral-100"
                : "text-neutral-400 hover:text-neutral-200"
            }`}
          >
            {t(tabLabels[id])}
          </button>
        ))}
      </div>

      {tab === "installed" ? <InstalledApps /> : null}
      {tab === "running" ? <RunningApps /> : null}
      {tab === "rules" ? <RulesTab /> : null}
    </div>
  );
}

function InstalledApps() {
  const apps = useAppStore((state) => state.installedApps);
  const loading = useAppStore((state) => state.installedLoading);
  const error = useAppStore((state) => state.installedError);
  const busy = useAppStore((state) => state.busy);
  const loadInstalledApps = useAppStore((state) => state.loadInstalledApps);
  const addRuleFromInstalled = useAppStore(
    (state) => state.addRuleFromInstalled,
  );
  const [query, setQuery] = useState("");
  const [addedPath, setAddedPath] = useState<string | null>(null);

  useEffect(() => {
    void loadInstalledApps();
  }, [loadInstalledApps]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return apps;
    return apps.filter((app) => app.name.toLowerCase().includes(needle));
  }, [apps, query]);

  const add = async (app: InstalledApp) => {
    await addRuleFromInstalled(app);
    setAddedPath(app.path);
  };

  return (
    <div className="flex flex-col gap-3">
      <Card title={t("apps.installed.title")}>
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-neutral-500" />
              <input
                id="installed-search"
                aria-label={t("apps.installed.search")}
                className={`${inputClass} pl-8`}
                placeholder={t("apps.installed.search")}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <Button
              onClick={() => void loadInstalledApps()}
              disabled={loading}
            >
              <RefreshCw className={`size-4 ${loading ? "animate-spin" : ""}`} />
              {t("apps.installed.refresh")}
            </Button>
          </div>

          {error ? (
            <ErrorBox
              error={error}
              showDetail
              onRetry={() => void loadInstalledApps()}
            />
          ) : null}

          {loading && apps.length === 0 ? (
            <p className="text-sm text-neutral-400">
              {t("apps.installed.loading")}
            </p>
          ) : null}

          {!loading && apps.length === 0 && !error ? (
            <EmptyState
              title={t("apps.installed.empty.title")}
              hint={t("apps.installed.empty.hint")}
            />
          ) : null}

          {apps.length > 0 && filtered.length === 0 ? (
            <EmptyState
              title={t("apps.installed.empty.title")}
              hint={t("apps.installed.noMatches", { query: query.trim() })}
            />
          ) : null}

          {filtered.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {filtered.map((app) => {
                const added = addedPath === app.path;
                return (
                  <li
                    key={app.path}
                    className="flex items-center justify-between gap-3 rounded-lg border border-neutral-800 bg-neutral-900/60 px-4 py-3"
                  >
                    <div className="flex min-w-0 items-center gap-3">
                      <AppIcon app={app} />
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium text-neutral-100">
                          {app.name}
                        </p>
                        <p className="truncate font-mono text-xs text-neutral-500">
                          {app.path}
                        </p>
                      </div>
                    </div>
                    <Button
                      variant={added ? "ghost" : "secondary"}
                      aria-label={`${t("apps.installed.add")}: ${app.name}`}
                      disabled={Boolean(busy[`rule-${app.path}`]) || added}
                      onClick={() => void add(app)}
                      className="shrink-0"
                    >
                      <Plus className="size-4" />
                      {added ? t("apps.installed.added") : t("apps.installed.add")}
                    </Button>
                  </li>
                );
              })}
            </ul>
          ) : null}

          <p className="text-xs text-neutral-500">
            {t("apps.installed.count", { count: filtered.length })}
          </p>
          {addedPath ? (
            <p className="text-xs text-emerald-300">
              {t("apps.installed.addedHint")}
            </p>
          ) : null}
        </div>
      </Card>
    </div>
  );
}

// Иконки здесь не запрашиваются: процессов сотни, а чтение иконок по одному
// пути замедлило бы список. Каталог с иконками — на вкладке «Установленные».
function RunningApps() {
  const apps = useAppStore((state) => state.runningApps);
  const loading = useAppStore((state) => state.runningLoading);
  const error = useAppStore((state) => state.runningError);
  const busy = useAppStore((state) => state.busy);
  const loadRunningApps = useAppStore((state) => state.loadRunningApps);
  const addRuleFromRunning = useAppStore((state) => state.addRuleFromRunning);
  const [query, setQuery] = useState("");
  const [addedPath, setAddedPath] = useState<string | null>(null);

  useEffect(() => {
    void loadRunningApps();
  }, [loadRunningApps]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return apps;
    return apps.filter((app) => app.name.toLowerCase().includes(needle));
  }, [apps, query]);

  const add = async (app: RunningApp) => {
    await addRuleFromRunning(app);
    setAddedPath(app.path);
  };

  return (
    <div className="flex flex-col gap-3">
      <Card title={t("apps.running.title")}>
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-neutral-500" />
              <input
                id="running-search"
                aria-label={t("apps.running.search")}
                className={`${inputClass} pl-8`}
                placeholder={t("apps.running.search")}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <Button
              onClick={() => void loadRunningApps()}
              disabled={loading}
            >
              <RefreshCw className={`size-4 ${loading ? "animate-spin" : ""}`} />
              {t("apps.running.refresh")}
            </Button>
          </div>

          {error ? (
            <ErrorBox
              error={error}
              showDetail
              onRetry={() => void loadRunningApps()}
            />
          ) : null}

          {loading && apps.length === 0 ? (
            <p className="text-sm text-neutral-400">
              {t("apps.running.loading")}
            </p>
          ) : null}

          {!loading && apps.length === 0 && !error ? (
            <EmptyState
              title={t("apps.running.empty.title")}
              hint={t("apps.running.empty.hint")}
            />
          ) : null}

          {apps.length > 0 && filtered.length === 0 ? (
            <EmptyState
              title={t("apps.running.empty.title")}
              hint={t("apps.running.noMatches", { query: query.trim() })}
            />
          ) : null}

          {filtered.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {filtered.map((app) => {
                const added = addedPath === app.path;
                return (
                  <li
                    key={app.path}
                    className="flex items-center justify-between gap-3 rounded-lg border border-neutral-800 bg-neutral-900/60 px-4 py-3"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <p className="truncate text-sm font-medium text-neutral-100">
                          {app.name}
                        </p>
                        <Badge tone="neutral">
                          {t("apps.running.pid", { pid: app.pid })}
                        </Badge>
                      </div>
                      <p className="truncate font-mono text-xs text-neutral-500">
                        {app.path}
                      </p>
                    </div>
                    <Button
                      variant={added ? "ghost" : "secondary"}
                      aria-label={`${t("apps.running.add")}: ${app.name}`}
                      disabled={Boolean(busy[`rule-${app.path}`]) || added}
                      onClick={() => void add(app)}
                      className="shrink-0"
                    >
                      <Plus className="size-4" />
                      {added ? t("apps.running.added") : t("apps.running.add")}
                    </Button>
                  </li>
                );
              })}
            </ul>
          ) : null}

          <p className="text-xs text-neutral-500">
            {t("apps.running.count", { count: filtered.length })}
          </p>
          {addedPath ? (
            <p className="text-xs text-emerald-300">
              {t("apps.running.addedHint")}
            </p>
          ) : null}
        </div>
      </Card>
    </div>
  );
}

function RulesTab() {
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
                <div className="flex flex-wrap items-center gap-2">
                  <p className="truncate text-sm font-medium text-neutral-100">
                    {rule.name}
                  </p>
                  <MissingPathBadge paths={rule.missing_paths ?? []} />
                </div>
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
                    {t("apps.enabled")}:{" "}
                    {t(rule.enabled ? "common.yes" : "common.no")}
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
