// Выбор языка и локализованные функции. Текущий язык — внешнее состояние:
// Rust хранит его в настройках (`preferences.language`), UI подписывается через
// `useLanguage`, чтобы переключение применялось без перезапуска окна.

import { useSyncExternalStore } from "react";
import { en } from "./en";
import { ru, type MessageKey } from "./ru";

export type { MessageKey };

export type Language = "ru" | "en";

const dictionaries: Record<Language, Record<MessageKey, string>> = { ru, en };

let currentLanguage: Language = "ru";
const listeners = new Set<() => void>();

export function getLanguage(): Language {
  return currentLanguage;
}

export function setLanguage(language: Language): void {
  if (language !== "ru" && language !== "en") return;
  if (currentLanguage === language) return;
  currentLanguage = language;
  for (const listener of listeners) listener();
}

export function subscribeLanguage(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Подписка компонента на смену языка: меняет язык — перерисовывается дерево. */
export function useLanguage(): Language {
  return useSyncExternalStore(subscribeLanguage, getLanguage, getLanguage);
}

export function t(
  key: MessageKey,
  vars?: Record<string, string | number>,
): string {
  let text = dictionaries[currentLanguage][key];
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}

/** Текст ошибки по стабильному коду команды. */
export function errorText(code: string, fallback?: string): string {
  const key = `errors.${code}` as MessageKey;
  if (key in ru) {
    return t(key);
  }
  return fallback ?? t("errors.UNKNOWN");
}

/** Локаль для дат и времени, соответствующая выбранному языку. */
export function locale(): string {
  return currentLanguage === "en" ? "en-US" : "ru-RU";
}
