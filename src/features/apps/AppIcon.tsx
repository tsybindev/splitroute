// Декоративная иконка приложения для каталога (PRD §6.3).
//
// Иконка — это отдельный вызов ядра с чтением файла с диска, поэтому:
// * загрузка ленивая — по появлению строки во вьюпорте (rootMargin 200px),
//   чтобы каталог из сотен записей не дёргал ядро целиком при открытии вкладки;
// * результат кэшируется в модульной Map по app.path (включая «иконки нет»),
//   чтобы повторный рендер, поиск и возврат на вкладку не порождали новых вызовов.
// Кэш живёт вне Zustand: это не состояние приложения, а побочный ресурс.

import { useEffect, useRef, useState } from "react";
import { api } from "../../shared/api/client";
import type { InstalledApp } from "../../shared/api/types";

const ROOT_MARGIN = "200px";

const iconCache = new Map<string, string | null>();
const pendingRequests = new Map<string, Promise<string | null>>();

/** Сброс кэша иконок между тестами. */
export function clearIconCache(): void {
  iconCache.clear();
  pendingRequests.clear();
}

function loadIcon(app: InstalledApp): Promise<string | null> {
  const cached = iconCache.get(app.path);
  if (cached !== undefined) return Promise.resolve(cached);

  const pending = pendingRequests.get(app.path);
  if (pending) return pending;

  const request = api
    .appIcon(app)
    // Ошибка команды (нет доступа, битый файл) — это тоже «иконки нет»:
    // декоративная картинка не должна показывать ErrorBox и ломать список.
    .catch(() => null)
    .then((url) => {
      iconCache.set(app.path, url);
      pendingRequests.delete(app.path);
      return url;
    });

  pendingRequests.set(app.path, request);
  return request;
}

function fallbackLetter(name: string): string {
  const trimmed = name.trim();
  return trimmed ? trimmed[0].toUpperCase() : "?";
}

interface AppIconProps {
  app: InstalledApp;
}

export function AppIcon({ app }: AppIconProps) {
  // Ключ по пути: React переиспользует узлы списка, а иконка привязана к пути,
  // поэтому при смене пути состояние сбрасывается вместе с компонентом.
  return <AppIconView key={app.path} app={app} />;
}

function AppIconView({ app }: AppIconProps) {
  const hostRef = useRef<HTMLSpanElement | null>(null);
  const [iconUrl, setIconUrl] = useState<string | null>(
    () => iconCache.get(app.path) ?? null,
  );

  useEffect(() => {
    if (iconCache.has(app.path)) return;

    let active = true;
    const load = () => {
      void loadIcon(app).then((url) => {
        // Размонтировались до ответа — ничего не обновляем.
        if (active && url) setIconUrl(url);
      });
    };

    const host = hostRef.current;
    // В jsdom и старых WebView IntersectionObserver нет — грузим сразу.
    if (typeof IntersectionObserver === "undefined" || !host) {
      load();
      return () => {
        active = false;
      };
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        // Один раз: после старта загрузки наблюдатель больше не нужен.
        observer.disconnect();
        load();
      },
      { rootMargin: ROOT_MARGIN },
    );
    observer.observe(host);

    return () => {
      active = false;
      observer.disconnect();
    };
  }, [app]);

  return (
    <span
      ref={hostRef}
      // Имя приложения выводится рядом текстом — иконка декоративная.
      aria-hidden="true"
      className="flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-md bg-neutral-800 text-xs font-medium text-neutral-400"
    >
      {iconUrl ? (
        <img src={iconUrl} alt="" className="size-8 object-contain" />
      ) : (
        fallbackLetter(app.name)
      )}
    </span>
  );
}
