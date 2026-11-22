//! Автообновление приложения (S4-T8).
//!
//! Проверка выполняется вручную или при запуске, если включена настройка
//! `check_on_start`; установка — только по явному действию пользователя.
//! Артефакты подписываются приватным ключом, который хранится вне репозитория;
//! здесь используется только публичный ключ из `tauri.conf.json`.
//!
//! Домен обновлений: GitHub Releases (решение по PRD §13.3). Манифест канала
//! лежит ассетом последнего опубликованного релиза (не draft, не prerelease);
//! апдейтер следует редиректам. Ассеты GitHub Releases плоские, поэтому канал
//! кодируется в имени файла (`latest.json` — stable, `beta-latest.json` — beta),
//! а не в подпапке. Для локальной проверки в debug-сборке базу можно задать
//! `SPLITROUTE_UPDATE_ENDPOINT` (только HTTPS).

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Url};
use tauri_plugin_updater::{Updater, UpdaterExt};

use crate::store::{UpdateChannel, UpdateSettings};

/// База канала обновлений: `releases/latest/download` раздаёт манифест
/// последнего опубликованного (не draft, не prerelease) релиза.
const UPDATE_BASE: &str = "https://github.com/tsybindev/splitroute/releases/latest/download";
/// Таймаут проверки: сеть может подвиснуть, окно не блокируется.
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// Таймаут загрузки артефакта.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Событие прогресса загрузки: `{ downloaded, total? }`.
pub const PROGRESS_EVENT: &str = "update-progress";
/// Событие завершения загрузки перед установкой.
pub const DOWNLOADED_EVENT: &str = "update-downloaded";

/// Ошибка обновления: стабильный код и безопасный русский текст.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Код остаётся в API сборки, но с реальным endpoint не возникает.
    NotConfigured,
    Config,
    CheckFailed,
    Signature,
    DownloadFailed,
    InstallFailed,
    NotAvailable,
}

impl UpdateError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "UPDATE_NOT_CONFIGURED",
            Self::Config => "UPDATE_CONFIG",
            Self::CheckFailed => "UPDATE_CHECK_FAILED",
            Self::Signature => "UPDATE_SIGNATURE",
            Self::DownloadFailed => "UPDATE_DOWNLOAD_FAILED",
            Self::InstallFailed => "UPDATE_INSTALL_FAILED",
            Self::NotAvailable => "UPDATE_NOT_AVAILABLE",
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::NotConfigured => "канал обновлений не настроен в этой сборке",
            Self::Config => "некорректная настройка канала обновлений",
            Self::CheckFailed => "не удалось проверить обновления",
            Self::Signature => "подпись обновления недействительна, установка отменена",
            Self::DownloadFailed => "не удалось скачать обновление",
            Self::InstallFailed => "не удалось установить обновление",
            Self::NotAvailable => "обновление больше не доступно",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for UpdateError {}

/// Состояние обновлений для экрана настроек.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub channel: UpdateChannel,
    /// Поле осталось в API UI: приложение собирается с настроенным каналом.
    pub configured: bool,
}

/// Результат проверки: доступная версия и заметки релиза.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateCheck {
    pub available: bool,
    pub current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// Прогресс загрузки в байтах; `total` может быть неизвестен.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Состояние канала обновлений без сетевого запроса.
pub fn status(app: &AppHandle, settings: &UpdateSettings) -> UpdateStatus {
    UpdateStatus {
        current_version: app.package_info().version.to_string(),
        channel: settings.channel,
        // Endpoint вшит в сборку (см. UPDATE_BASE), канал всегда настроен.
        configured: true,
    }
}

/// Проверяет обновление в выбранном канале.
pub async fn check(app: &AppHandle, channel: UpdateChannel) -> Result<UpdateCheck, UpdateError> {
    let updater = build_updater(app, endpoint(channel)?, CHECK_TIMEOUT)?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheck {
            available: true,
            current_version: update.current_version,
            version: Some(update.version),
            // Заметки релиза показываются как текст, без HTML.
            notes: update.body,
            date: update.date.map(|date| date.to_string()),
        }),
        Ok(None) => Ok(UpdateCheck {
            available: false,
            current_version: app.package_info().version.to_string(),
            version: None,
            notes: None,
            date: None,
        }),
        Err(error) => Err(classify_check(error)),
    }
}

/// Скачивает и устанавливает обновление, затем перезапускает приложение.
///
/// `before_install` вызывается только после успешной проверки и прямо перед
/// загрузкой: приложение останавливает ядро, чтобы TUN и правила kill switch не
/// пережили замену. Ошибка проверки не трогает работающий туннель.
pub async fn install<F: FnOnce()>(
    app: &AppHandle,
    channel: UpdateChannel,
    before_install: F,
) -> Result<(), UpdateError> {
    let updater = build_updater(app, endpoint(channel)?, DOWNLOAD_TIMEOUT)?;
    let update = updater
        .check()
        .await
        .map_err(classify_check)?
        .ok_or(UpdateError::NotAvailable)?;

    before_install();

    let mut downloaded = 0u64;
    let progress_app = app.clone();
    let finish_app = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                let _ = progress_app.emit(PROGRESS_EVENT, UpdateProgress { downloaded, total });
            },
            move || {
                let _ = finish_app.emit(DOWNLOADED_EVENT, ());
            },
        )
        .await
        .map_err(classify_install)?;

    // `restart` завершает процесс: на macOS/Linux запускает новую версию,
    // на Windows установщик уже мог завершить приложение.
    app.restart();
}

fn build_updater(
    app: &AppHandle,
    endpoint: Url,
    timeout: Duration,
) -> Result<Updater, UpdateError> {
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|_| UpdateError::Config)?
        .timeout(timeout)
        .build()
        .map_err(|_| UpdateError::Config)
}

fn endpoint(channel: UpdateChannel) -> Result<Url, UpdateError> {
    let mut base = UPDATE_BASE.to_string();
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("SPLITROUTE_UPDATE_ENDPOINT") {
        let value = value.trim().trim_end_matches('/');
        if !value.is_empty() {
            base = value.to_string();
        }
    }
    manifest_url(&base, channel)
}

/// Имя манифеста канала: ассеты GitHub Releases плоские, канал — в имени файла.
fn manifest_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => "latest.json",
        UpdateChannel::Beta => "beta-latest.json",
    }
}

/// Собирает URL манифеста. Обновления принимаются только по HTTPS.
fn manifest_url(base: &str, channel: UpdateChannel) -> Result<Url, UpdateError> {
    let base = base.trim().trim_end_matches('/');
    let url = format!("{base}/{}", manifest_name(channel));
    let url = Url::parse(&url).map_err(|_| UpdateError::Config)?;
    if url.scheme() != "https" {
        return Err(UpdateError::Config);
    }
    Ok(url)
}

/// Ошибка проверки: подпись/конфигурация различаются, остальное — сеть/сервер.
fn classify_check(error: tauri_plugin_updater::Error) -> UpdateError {
    use tauri_plugin_updater::Error as Source;
    match error {
        Source::Minisign(_)
        | Source::Base64(_)
        | Source::SignatureUtf8(_)
        | Source::SignedVersionMismatch { .. }
        | Source::MissingSignedVersion => UpdateError::Signature,
        Source::InsecureTransportProtocol | Source::UrlParse(_) | Source::EmptyEndpoints => {
            UpdateError::Config
        }
        _ => UpdateError::CheckFailed,
    }
}

/// Ошибка установки: отдельно подпись, сеть и всё остальное.
fn classify_install(error: tauri_plugin_updater::Error) -> UpdateError {
    use tauri_plugin_updater::Error as Source;
    match error {
        Source::Minisign(_)
        | Source::Base64(_)
        | Source::SignatureUtf8(_)
        | Source::SignedVersionMismatch { .. }
        | Source::MissingSignedVersion => UpdateError::Signature,
        Source::Reqwest(_) | Source::Network(_) => UpdateError::DownloadFailed,
        Source::InsecureTransportProtocol | Source::UrlParse(_) | Source::EmptyEndpoints => {
            UpdateError::Config
        }
        _ => UpdateError::InstallFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_url_uses_channel_file_names() {
        let stable = manifest_url("https://updates.example.test", UpdateChannel::Stable).unwrap();
        assert_eq!(stable.as_str(), "https://updates.example.test/latest.json");
        let beta = manifest_url("https://updates.example.test/", UpdateChannel::Beta).unwrap();
        assert_eq!(
            beta.as_str(),
            "https://updates.example.test/beta-latest.json"
        );
    }

    #[test]
    fn manifest_url_rejects_insecure_transport() {
        assert_eq!(
            manifest_url("http://updates.example.test", UpdateChannel::Stable),
            Err(UpdateError::Config)
        );
        assert_eq!(
            manifest_url("notaurl", UpdateChannel::Stable),
            Err(UpdateError::Config)
        );
    }

    #[test]
    fn endpoint_defaults_to_github_releases() {
        // Без env-переопределения база — реальный канал GitHub Releases.
        // Только URL, сеть не трогаем.
        if std::env::var("SPLITROUTE_UPDATE_ENDPOINT").is_err() {
            let stable = endpoint(UpdateChannel::Stable).unwrap();
            assert_eq!(
                stable.as_str(),
                "https://github.com/tsybindev/splitroute/releases/latest/download/latest.json"
            );
            let beta = endpoint(UpdateChannel::Beta).unwrap();
            assert_eq!(
                beta.as_str(),
                "https://github.com/tsybindev/splitroute/releases/latest/download/beta-latest.json"
            );
        }
    }

    #[test]
    fn signature_errors_are_not_reported_as_network_errors() {
        for error in [
            tauri_plugin_updater::Error::SignatureUtf8("bad".to_string()),
            tauri_plugin_updater::Error::MissingSignedVersion,
        ] {
            assert_eq!(classify_install(error), UpdateError::Signature);
        }
    }

    #[test]
    fn network_errors_are_typed() {
        assert_eq!(
            classify_install(tauri_plugin_updater::Error::Network("timeout".to_string())),
            UpdateError::DownloadFailed
        );
        assert_eq!(
            classify_check(tauri_plugin_updater::Error::Network("timeout".to_string())),
            UpdateError::CheckFailed
        );
    }

    #[test]
    fn config_errors_are_typed() {
        assert_eq!(
            classify_check(tauri_plugin_updater::Error::InsecureTransportProtocol),
            UpdateError::Config
        );
        assert_eq!(
            classify_install(tauri_plugin_updater::Error::EmptyEndpoints),
            UpdateError::Config
        );
    }

    #[test]
    fn error_codes_are_stable() {
        let cases = [
            (UpdateError::NotConfigured, "UPDATE_NOT_CONFIGURED"),
            (UpdateError::Config, "UPDATE_CONFIG"),
            (UpdateError::CheckFailed, "UPDATE_CHECK_FAILED"),
            (UpdateError::Signature, "UPDATE_SIGNATURE"),
            (UpdateError::DownloadFailed, "UPDATE_DOWNLOAD_FAILED"),
            (UpdateError::InstallFailed, "UPDATE_INSTALL_FAILED"),
            (UpdateError::NotAvailable, "UPDATE_NOT_AVAILABLE"),
        ];
        for (error, code) in cases {
            assert_eq!(error.code(), code);
        }
    }

    #[test]
    fn update_check_serializes_without_secrets() {
        let check = UpdateCheck {
            available: true,
            current_version: "0.1.0".to_string(),
            version: Some("0.2.0".to_string()),
            notes: Some("исправления".to_string()),
            date: Some("2026-11-14T00:00:00Z".to_string()),
        };
        let json = serde_json::to_value(&check).unwrap();
        assert_eq!(json["available"], true);
        assert_eq!(json["version"], "0.2.0");
        assert!(json.get("signature").is_none());
        assert!(json.get("download_url").is_none());
    }
}
