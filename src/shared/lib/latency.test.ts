import { describe, expect, it } from "vitest";
import { latencyById } from "./latency";
import type { LatencyResponse } from "../api/types";

function response(overrides: Partial<LatencyResponse> = {}): LatencyResponse {
  return {
    results: [
      { profile_id: "a", latency_ms: 180 },
      { profile_id: "b", latency_ms: 40 },
      { profile_id: "c", latency_ms: null, error: "PING_TIMEOUT" },
    ],
    best_profile_id: "b",
    switch_to: null,
    checked_at: 1,
    ...overrides,
  };
}

describe("latency", () => {
  it("индексирует результаты по профилю", () => {
    const byId = latencyById(response());
    expect(byId.get("b")?.latency_ms).toBe(40);
    expect(byId.get("missing")).toBeUndefined();
  });

  it("без результатов индекс пуст", () => {
    expect(latencyById(null).size).toBe(0);
  });
});
