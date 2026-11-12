// Словарь русских строк. Компоненты не содержат захардкоженных текстов.

export const ru = {
  "app.name": "SplitRoute",
  "app.status.disconnected": "Не подключено",
  "app.status.connecting": "Подключение…",
  "app.status.connected": "Подключено",
  "app.status.disconnecting": "Отключение…",
  "app.status.error": "Ошибка",

  "nav.connect": "Главная",
  "nav.profiles": "Локации",
  "nav.apps": "Приложения",
  "nav.settings": "Настройки",
  "nav.logs": "Логи",

  "common.refresh": "Обновить",
  "common.save": "Сохранить",
  "common.delete": "Удалить",
  "common.add": "Добавить",
  "common.retry": "Повторить",
  "common.copy": "Копировать",
  "common.copied": "Скопировано",
  "common.loading": "Загрузка…",
  "common.none": "—",
  "common.yes": "Да",
  "common.no": "Нет",

  "connect.title": "Подключение",
  "connect.location": "Текущая локация",
  "connect.noLocation": "Локация не выбрана",
  "connect.selectLocation": "Выбрать локацию",
  "connect.connect": "Подключить",
  "connect.disconnect": "Отключить",
  "connect.sessionTime": "Время сессии",
  "connect.helper": "Helper",
  "connect.helper.installed": "Установлен",
  "connect.helper.missing": "Не установлен",
  "connect.dns": "DNS",
  "connect.dns.proxy": "Через прокси",
  "connect.dns.system": "Системный",
  "connect.dns.systemWarning":
    "DNS выбранных приложений уходит в локальный resolver — возможна утечка.",
  "connect.allowlist": "Приложений через прокси",
  "connect.noSubscription": "Подписка не добавлена. Добавьте URL подписки в настройках.",
  "connect.goProfiles": "Перейти к локациям",
  "connect.goSettings": "Открыть настройки",
  "connect.connectFailed": "Не удалось подключиться",
  "connect.disconnectFailed": "Не удалось отключиться",

  "profiles.title": "Локации",
  "profiles.refresh": "Обновить подписку",
  "profiles.empty.title": "Нет локаций",
  "profiles.empty.hint": "Добавьте подписку в настройках, чтобы увидеть список локаций.",
  "profiles.empty.goSettings": "Открыть настройки",
  "profiles.selected": "Выбрано",
  "profiles.select": "Выбрать",
  "profiles.fetchedAt": "Обновлено: {time}",
  "profiles.skipped": "Пропущено профилей: {count}",
  "profiles.refreshing": "Обновление…",

  "apps.title": "Приложения",
  "apps.explanation":
    "Маршрутизация работает по процессу — владельцу сетевого сокета. curl, запущенный в терминале, матчится как curl, а не как Terminal. Браузеры создают дочерние процессы, поэтому для них надёжнее правило по каталогу (.app или папке установки).",
  "apps.add.title": "Добавить правило",
  "apps.add.name": "Название",
  "apps.add.type": "Тип правила",
  "apps.add.type.process": "Имя процесса",
  "apps.add.type.path": "Путь к процессу",
  "apps.add.type.regex": "Regex по пути",
  "apps.add.value": "Значение",
  "apps.add.submit": "Добавить правило",
  "apps.empty": "Правил пока нет. Добавьте приложение, трафик которого должен идти через прокси.",
  "apps.enabled": "Через прокси",
  "apps.delete": "Удалить правило",
  "apps.ruleCount": "Правил: {count}",
  "apps.add.invalid": "Заполните название и значение правила.",

  "settings.title": "Настройки",
  "settings.subscription.title": "Подписка",
  "settings.subscription.url": "URL подписки",
  "settings.subscription.urlHint": "Секрет: не попадает в логи и диагностику.",
  "settings.subscription.hwid": "HWID",
  "settings.subscription.hwidHint": "16 hex-символов. Смена может сбросить привязку подписки.",
  "settings.subscription.regenerate": "Сгенерировать",
  "settings.subscription.userAgent": "User-Agent",
  "settings.subscription.autoUpdate": "Автообновление, ч",
  "settings.network.title": "Сеть",
  "settings.network.mtu": "MTU",
  "settings.network.ipv6": "IPv6 в туннеле",
  "settings.network.ipv6Hint": "По умолчанию IPv6 блокируется для выбранных приложений.",
  "settings.network.mixed": "Локальный mixed-inbound",
  "settings.network.mixedPort": "Порт mixed-inbound",
  "settings.network.dns": "DNS-режим",
  "settings.network.dns.proxy": "Весь DNS через прокси",
  "settings.network.dns.system": "Системный DNS",
  "settings.network.dns.proxyHint":
    "Весь DNS идёт через прокси, утечек нет. DNS невыбранных приложений тоже идёт через прокси; локальные имена (.local) могут не работать.",
  "settings.network.dns.systemHint":
    "DNS операционной системы, максимальная совместимость. DNS выбранных приложений уходит локально — возможна утечка.",
  "settings.network.logLevel": "Уровень логов",
  "settings.app.title": "Приложение",
  "settings.app.autostart": "Запускать при входе в систему",
  "settings.app.launchMinimized": "Запускать свёрнутым",
  "settings.app.connectOnStart": "Подключаться при старте",
  "settings.app.closeToTray": "Закрывать окно в трей",
  "settings.helper.title": "Helper",
  "settings.helper.status": "Статус",
  "settings.helper.install": "Установить",
  "settings.helper.uninstall": "Удалить",
  "settings.helper.hint": "Установка сервиса требует прав администратора и появится на этапе 3.",
  "settings.save": "Сохранить настройки",
  "settings.saved": "Настройки сохранены",

  "logs.title": "Логи",
  "logs.filter": "Уровень",
  "logs.filter.all": "Все",
  "logs.empty": "Логов пока нет.",
  "logs.refresh": "Обновить",
  "logs.copyDiagnostics": "Копировать диагностику",
  "logs.diagnostics": "Диагностика",

  "errors.title": "Ошибка",
  "errors.byCode": "Код: {code}",
  "errors.NETWORK": "Нет сети или сервер недоступен.",
  "errors.UNAUTHORIZED": "Сервер отклонил HWID (401). Проверьте HWID в настройках.",
  "errors.FORBIDDEN": "Доступ к подписке запрещён (403).",
  "errors.NOT_FOUND": "Подписка не найдена (404). Проверьте URL.",
  "errors.HTTP_ERROR": "Сервер вернул ошибку. Попробуйте позже.",
  "errors.INVALID_JSON": "Сервер вернул некорректный JSON.",
  "errors.EMPTY": "Подписка пуста.",
  "errors.NO_PROFILES": "В подписке нет поддерживаемых профилей.",
  "errors.MISSING_URL": "URL подписки не задан.",
  "errors.NO_PROFILE": "Локация не выбрана.",
  "errors.PROFILE_MISSING": "Выбранная локация отсутствует в подписке.",
  "errors.HELPER_NOT_INSTALLED": "Helper не установлен.",
  "errors.HELPER_UNAVAILABLE": "Helper не отвечает.",
  "errors.HELPER_TIMEOUT": "Helper не ответил вовремя.",
  "errors.HELPER_PROTOCOL": "Несовместимая версия helper'а.",
  "errors.HELPER_ERROR": "Helper вернул ошибку.",
  "errors.HELPER_INVALID_RESPONSE": "Helper вернул некорректный ответ.",
  "errors.CONFIG": "Не удалось собрать конфиг.",
  "errors.STORE": "Не удалось сохранить данные.",
  "errors.INSTALLER": "Установка helper'а недоступна.",
  "errors.UNKNOWN": "Неизвестная ошибка.",
} as const;

export type MessageKey = keyof typeof ru;

export function t(
  key: MessageKey,
  vars?: Record<string, string | number>,
): string {
  let text: string = ru[key];
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
