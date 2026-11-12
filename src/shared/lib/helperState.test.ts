import { describe, expect, it } from "vitest";
import { helperBadge, helperState } from "./helperState";
import { stoppedStatus } from "../../test/fixtures";

describe("helperState", () => {
  it("считает helper установленным по успешному helper_status", () => {
    expect(helperState(stoppedStatus, null)).toBe("installed");
    expect(helperBadge(stoppedStatus, null)).toMatchObject({
      state: "installed",
      tone: "success",
      label: "Установлен",
    });
  });

  it("не установлен, если сокета нет", () => {
    expect(
      helperState(null, {
        code: "HELPER_NOT_INSTALLED",
        message: "helper не установлен или сокет не найден",
      }),
    ).toBe("missing");
  });

  it("устарел при несовпадении версии протокола", () => {
    expect(
      helperState(null, {
        code: "HELPER_PROTOCOL",
        message: "несовместимая версия",
      }),
    ).toBe("outdated");
    expect(
      helperBadge(null, { code: "HELPER_PROTOCOL", message: "версия" }).tone,
    ).toBe("danger");
  });

  it("не отвечает при таймауте или недоступности", () => {
    for (const code of ["HELPER_TIMEOUT", "HELPER_UNAVAILABLE"]) {
      expect(helperState(null, { code, message: "нет ответа" })).toBe(
        "unavailable",
      );
    }
  });

  it("не установлен по умолчанию и при неизвестной ошибке", () => {
    expect(helperState(null, null)).toBe("missing");
    expect(helperState(null, { code: "UNKNOWN", message: "" })).toBe("missing");
  });
});
