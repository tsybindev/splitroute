import { useState } from "react";
import { ClipboardCopy, RefreshCw } from "lucide-react";
import type { LogLevel } from "../../shared/api/types";
import { locale, t } from "../../shared/i18n";
import { useAppStore } from "../../shared/state/appStore";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { EmptyState } from "../../shared/ui/EmptyState";
import { inputClass } from "../../shared/ui/Field";

type Filter = LogLevel | "all";

const levels: Filter[] = ["all", "trace", "debug", "info", "warn", "error"];

function formatTimestamp(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleTimeString(locale());
}

export function LogsScreen() {
  const logs = useAppStore((state) => state.logs);
  const loadLogs = useAppStore((state) => state.loadLogs);
  const loadDiagnostics = useAppStore((state) => state.loadDiagnostics);
  const [filter, setFilter] = useState<Filter>("all");
  const [copied, setCopied] = useState(false);

  const changeFilter = (value: Filter) => {
    setFilter(value);
    void loadLogs(value === "all" ? null : value);
  };

  const copyDiagnostics = async () => {
    const text = await loadDiagnostics();
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("logs.title")}>
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-sm text-neutral-300">
            {t("logs.filter")}
            <select
              aria-label={t("logs.filter")}
              className={inputClass}
              value={filter}
              onChange={(event) => changeFilter(event.target.value as Filter)}
            >
              {levels.map((level) => (
                <option key={level} value={level}>
                  {level === "all" ? t("logs.filter.all") : level}
                </option>
              ))}
            </select>
          </label>
          <Button onClick={() => void loadLogs(filter === "all" ? null : filter)}>
            <RefreshCw className="size-4" />
            {t("logs.refresh")}
          </Button>
          <Button onClick={() => void copyDiagnostics()}>
            <ClipboardCopy className="size-4" />
            {copied ? t("common.copied") : t("logs.copyDiagnostics")}
          </Button>
        </div>
      </Card>

      {logs.length === 0 ? (
        <EmptyState title={t("logs.empty")} />
      ) : (
        <div className="max-h-[60vh] overflow-auto rounded-lg border border-neutral-800 bg-black/40 p-3">
          <ul className="flex flex-col gap-0.5 font-mono text-xs">
            {logs.map((entry, index) => (
              <li
                key={`${entry.timestamp}-${index}`}
                className="flex gap-2 whitespace-pre-wrap break-all"
              >
                <span className="shrink-0 text-neutral-600">
                  {formatTimestamp(entry.timestamp)}
                </span>
                <span
                  className={`shrink-0 uppercase ${
                    entry.level === "error"
                      ? "text-red-400"
                      : entry.level === "warn"
                        ? "text-amber-400"
                        : "text-sky-400"
                  }`}
                >
                  {entry.level}
                </span>
                <span className="text-neutral-300">{entry.message}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
