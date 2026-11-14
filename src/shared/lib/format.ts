// Форматирование величин для UI. Единицы измерения берутся из словаря,
// разделитель дробной части — из текущей локали (ru/en).

import { locale, t } from "../i18n";
import type { MessageKey } from "../i18n";

const UNITS: readonly MessageKey[] = [
  "units.bytes",
  "units.kib",
  "units.mib",
  "units.gib",
  "units.tib",
];

/** Байты в читаемый вид: Б, КиБ, МиБ, ГиБ, ТиБ. */
export function formatBytes(bytes: number): string {
  let value = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  let index = 0;
  while (value >= 1024 && index < UNITS.length - 1) {
    value /= 1024;
    index += 1;
  }
  const formatted = new Intl.NumberFormat(locale(), {
    maximumFractionDigits: index === 0 ? 0 : 1,
  }).format(value);
  return `${formatted} ${t(UNITS[index])}`;
}
