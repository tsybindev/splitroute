// Сортировка и поиск результатов замера задержки (PRD §7).
// Замеряет Rust; UI только раскладывает готовые числа.

import type { LatencyResponse, LatencyResult, Profile } from "../api/types";

export function latencyById(
  latency: LatencyResponse | null,
): Map<string, LatencyResult> {
  return new Map(
    (latency?.results ?? []).map((result) => [result.profile_id, result]),
  );
}

/**
 * Сортирует по задержке по возрастанию. Локации без замера и с ошибкой — в
 * конце; их относительный порядок сохраняется.
 */
export function sortProfilesByLatency(
  profiles: Profile[],
  latency: LatencyResponse | null,
): Profile[] {
  if (!latency) return profiles;
  const byId = latencyById(latency);
  return [...profiles].sort((left, right) => {
    const leftLatency = byId.get(left.id)?.latency_ms ?? null;
    const rightLatency = byId.get(right.id)?.latency_ms ?? null;
    if (leftLatency === null && rightLatency === null) return 0;
    if (leftLatency === null) return 1;
    if (rightLatency === null) return -1;
    return leftLatency - rightLatency;
  });
}
