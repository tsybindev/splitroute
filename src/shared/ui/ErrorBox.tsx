import { AlertTriangle } from "lucide-react";
import type { CommandError } from "../api/types";
import { errorText, t } from "../i18n/ru";
import { Button } from "./Button";

interface ErrorBoxProps {
  error: CommandError;
  onRetry?: () => void;
}

export function ErrorBox({ error, onRetry }: ErrorBoxProps) {
  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-lg border border-red-900 bg-red-950/30 p-3 text-sm text-red-100"
    >
      <AlertTriangle className="mt-0.5 size-4 shrink-0 text-red-400" />
      <div className="flex-1">
        <p className="font-medium">{errorText(error.code, error.message)}</p>
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
