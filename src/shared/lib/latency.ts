// Поиск результатов замера задержки (PRD §7).
// Замеряет Rust; UI только раскладывает готовые числа. Список локаций
// показывается в порядке подписки — сортировка по задержке не применяется.

import type { LatencyResponse, LatencyResult } from "../api/types";

export function latencyById(
  latency: LatencyResponse | null,
): Map<string, LatencyResult> {
  return new Map(
    (latency?.results ?? []).map((result) => [result.profile_id, result]),
  );
}
