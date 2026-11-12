# Фикстура подписки (S1-T2)

## Откуда формат

Подписка провайдера — это **Happ-совместимый JSON**: массив полных конфигов Xray.
Такой ответ отдаёт сервер при запросе с заголовками `User-Agent: Happ/...`,
`X-HWID: <hwid>`, `Accept: application/json` (см. `docs/research.md` §4 и PRD §6.1).
Внутри каждого элемента — почти готовый `config.json` Xray: `remarks`, `dns`,
`inbounds`, `outbounds`, `routing`, `burstObservatory`.

`subscription.json` — **санитизированная копия структуры**, а не реальный ответ.
Она повторяет форму подписки (2 профиля, 4 outbound'а, `burstObservatory`),
чтобы на ней можно было отлаживать парсер и конвертер без доступа к провайдеру.

## Что в файле

| Элемент | Значение | Зачем |
|---|---|---|
| Профилей | 2 | подписка провайдера тоже отдаёт два |
| `remarks` | `🇬🇧 Тестовая локация A`, `🇳🇱 Тестовая локация B [Fragment]` | эмодзи-флаг и суффикс `[Fragment]` — проверка slug'а и нормализации имени |
| `inbounds` | `socks` на `127.0.0.1:10808`, `http` на `127.0.0.1:10809` | как в шаблоне провайдера; в sing-box не переносятся |
| `outbounds[proxy]` | `vless` + `reality` + `network: tcp`, `flow: xtls-rprx-vision` | обязательный для MVP набор (PRD §6.1) |
| `outbounds[direct]` | `freedom` | служебный outbound подписки |
| `outbounds[block]` | `blackhole` | служебный outbound подписки |
| `outbounds[fragment-out]` | `freedom` с `fragment` в settings и `sockopt` в streamSettings | Xray-трюк против DPI; проверяем, что конвертер его не ломает и не тащит в sing-box |
| `routing.rules` | `geosite:private`, `geosite:category-ads-all`; во втором профиле дополнительно правило в `fragment-out` | во втором профиле `fragment-out` реально используется правилом |
| `dns` | два IP-сервера и `queryStrategy` | как в подписке; DNS-политику sing-box задаёт сам |
| `burstObservatory` | `subjectSelector: ["proxy"]` + `pingConfig` | поле есть в подписке, в sing-box не переносится |

## Все значения синтетические

Фикстура не содержит реальных URL подписки, HWID, UUID, ключей и адресов серверов:

- домены — только `example.com` / `example.net`;
- адреса — только TEST-NET-3 (`203.0.113.0/24`), это зарезервированный диапазон для документации;
- UUID, Reality public key, `short_id` — сгенерированы специально для спайка и нигде не используются как рабочие;
- приватного ключа Reality в фикстуре нет и быть не может — клиенту он не нужен.

Маркеров вида `fake`/`spike-fixture` в значениях нет специально: структура
должна быть максимально похожа на настоящую подписку, чтобы парсер и конвертер
проверялись на реальной форме данных.

## Как заменить на реальную подписку (без вставки значений в файлы)

Реальные значения берутся **из окружения**, в репозиторий и в файлы не попадают:

```bash
# переменные задаются только в текущей сессии, в файлы не пишутся
export SUBSCRIPTION_URL='...'      # URL подписки
export SUBSCRIPTION_HWID='...'     # 16 hex, из настроек приложения

# скачать во временный файл вне репозитория и сразу прогнать конвертер
curl -fsS "$SUBSCRIPTION_URL" \
  -H "X-HWID: $SUBSCRIPTION_HWID" \
  -H "User-Agent: Happ/4.1.0" \
  -H 'Accept: application/json' \
  -o "$TMPDIR/splitroute-subscription.json"

python3 spike/convert.py \
  --input "$TMPDIR/splitroute-subscription.json" \
  --out-dir "$TMPDIR/splitroute-configs" --check

rm -f "$TMPDIR/splitroute-subscription.json"
```

Правила:

- скачанную подписку и сгенерированные из неё конфиги держим в `$TMPDIR`, не в `spike/configs/`;
- после проверки временные файлы удаляем — в них есть реальные UUID и адреса;
- в отчёты, коммиты и `docs/` попадают только обезличенные наблюдения
  («профиль 1 — VLESS/Reality/TCP, порт 443»), без значений.

Перегенерировать фикстуру с нуля не требуется: она лежит в git и должна
меняться только осознанно — когда меняется структура реальной подписки.
