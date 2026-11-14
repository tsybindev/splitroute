import { afterEach, describe, expect, it } from "vitest";
import { en } from "./en";
import { ru } from "./ru";
import { errorText, getLanguage, setLanguage, t } from "./index";

afterEach(() => setLanguage("ru"));

describe("i18n", () => {
  it("наборы ключей ru и en совпадают", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(ru).sort());
  });

  it("в en нет пустых значений и русского текста", () => {
    for (const [key, value] of Object.entries(en)) {
      expect(value.trim(), `пустой ключ ${key}`).not.toBe("");
      expect(value, `непереведённый ключ ${key}`).not.toMatch(/[А-Яа-яЁё]/);
    }
  });

  it("по умолчанию русский", () => {
    expect(getLanguage()).toBe("ru");
    expect(t("nav.connect")).toBe("Главная");
    expect(errorText("NETWORK")).toBe("Нет сети или сервер недоступен.");
  });

  it("переключение языка меняет видимый текст и ошибки", () => {
    setLanguage("en");
    expect(getLanguage()).toBe("en");
    expect(t("nav.connect")).toBe("Home");
    expect(t("connect.connect")).toBe("Connect");
    expect(errorText("NETWORK")).toBe("No network or server unavailable.");
    expect(errorText("UNKNOWN_CODE")).toBe("Unknown error.");
  });

  it("подставляет переменные в обоих языках", () => {
    expect(t("apps.ruleCount", { count: 2 })).toBe("Правил: 2");
    setLanguage("en");
    expect(t("apps.ruleCount", { count: 2 })).toBe("Rules: 2");
  });

  it("неизвестный код использует переданный fallback", () => {
    setLanguage("en");
    expect(errorText("MYSTERY", "detail")).toBe("detail");
  });
});
