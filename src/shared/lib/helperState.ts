// Состояние helper'а для UI: единственный источник — ответ `helper_status`.
// Пароль и права установщика здесь не участвуют: только результат проверки.

import type { CommandError, HelperStatus } from "../api/types";
import type { MessageKey } from "../i18n";
import { t } from "../i18n";
import type { Tone } from "../ui/Badge";

export type HelperState =
  | "installed"
  | "missing"
  | "outdated"
  | "unavailable";

/**
 * Определяет состояние helper'а по успешному статусу или ошибке проверки.
 *
 * `helper_status` не различает «не установлен» и «не отвечает» отдельным полем,
 * поэтому причину берём из кода ошибки клиента (crates/protocol + privileged).
 */
export function helperState(
  status: HelperStatus | null,
  error: CommandError | null,
): HelperState {
  if (status) return "installed";
  switch (error?.code) {
    case "HELPER_PROTOCOL":
      return "outdated";
    case "HELPER_TIMEOUT":
    case "HELPER_UNAVAILABLE":
      return "unavailable";
    default:
      return "missing";
  }
}

const labels: Record<HelperState, MessageKey> = {
  installed: "connect.helper.installed",
  missing: "connect.helper.missing",
  outdated: "connect.helper.outdated",
  unavailable: "connect.helper.unavailable",
};

const tones: Record<HelperState, Tone> = {
  installed: "success",
  missing: "warn",
  outdated: "danger",
  unavailable: "danger",
};

export function helperBadge(
  status: HelperStatus | null,
  error: CommandError | null,
): { state: HelperState; tone: Tone; label: string } {
  const state = helperState(status, error);
  return { state, tone: tones[state], label: t(labels[state]) };
}
