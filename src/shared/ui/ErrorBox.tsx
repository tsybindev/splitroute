import { AlertTriangle } from "lucide-react";
import type { CommandError } from "../api/types";
import { errorText, t } from "../i18n";
import { Button } from "./Button";

interface ErrorBoxProps {
  error: CommandError;
  onRetry?: () => void;
  /** Показать исходный текст ошибки под заголовком (причина установщика). */
  showDetail?: boolean;
}

export function ErrorBox({ error, onRetry, showDetail }: ErrorBoxProps) {
  const headline = errorText(error.code, error.message);
  // Точная причина (путь, подпись, текст установщика) показывается отдельной
  // строкой, чтобы не теряться за локализованной формулировкой.
  const detail =
    showDetail && error.message && error.message !== headline
      ? error.message
      : null;
  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-lg border border-red-900 bg-red-950/30 p-3 text-sm text-red-100"
    >
      <AlertTriangle className="mt-0.5 size-4 shrink-0 text-red-400" />
      <div className="flex-1">
        <p className="font-medium">{headline}</p>
        {detail ? <p className="mt-0.5 text-sm text-red-200">{detail}</p> : null}
        <p className="mt-0.5 text-xs text-red-300/70">
          {t("errors.byCode", { code: error.code })}
        </p>
      </div>
      {onRetry ? (
        <Button variant="ghost" onClick={onRetry}>
          {t("common.retry")}
        </Button>
      ) : null}
    </div>
  );
}
