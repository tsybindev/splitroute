import { afterEach, describe, expect, it } from "vitest";
import { setLanguage } from "../i18n";
import { formatBytes } from "./format";

afterEach(() => setLanguage("ru"));

describe("formatBytes", () => {
  it("показывает байты и КиБ", () => {
    expect(formatBytes(0)).toBe("0 Б");
    expect(formatBytes(512)).toBe("512 Б");
    expect(formatBytes(1024)).toBe("1 КиБ");
  });

  it("переходит к МиБ и ГиБ с одним знаком после запятой", () => {
    expect(formatBytes(1536)).toBe("1,5 КиБ");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5 МиБ");
    expect(formatBytes(1.5 * 1024 ** 3)).toBe("1,5 ГиБ");
  });

  it("в английском использует латинские единицы и точку", () => {
    setLanguage("en");
    expect(formatBytes(1024)).toBe("1 KiB");
    expect(formatBytes(1536)).toBe("1.5 KiB");
  });

  it("отрицательные и NaN не дают мусора", () => {
    expect(formatBytes(-1024)).toBe("0 Б");
    expect(formatBytes(Number.NaN)).toBe("0 Б");
  });
});
