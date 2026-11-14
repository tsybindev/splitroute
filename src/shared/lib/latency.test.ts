import { describe, expect, it } from "vitest";
import { latencyById, sortProfilesByLatency } from "./latency";
import type { LatencyResponse } from "../api/types";
import { makeProfile } from "../../test/fixtures";

const profiles = [
  makeProfile({ id: "a", name: "A" }),
  makeProfile({ id: "b", name: "B" }),
  makeProfile({ id: "c", name: "C" }),
];

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
  it("без результатов сохраняет порядок подписки", () => {
    expect(sortProfilesByLatency(profiles, null).map((item) => item.id)).toEqual([
      "a",
      "b",
      "c",
    ]);
  });

  it("сортирует по возрастанию, ошибки — в конце", () => {
    expect(
      sortProfilesByLatency(profiles, response()).map((item) => item.id),
    ).toEqual(["b", "a", "c"]);
  });

  it("индексирует результаты по профилю", () => {
    const byId = latencyById(response());
    expect(byId.get("b")?.latency_ms).toBe(40);
    expect(byId.get("missing")).toBeUndefined();
  });
});
