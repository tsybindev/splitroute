//! Иконки каталога: диспетчеризация по платформам, нормализация и кэш (S4-T2).
//!
//! Наружу отдаётся готовый data URL: UI подставляет строку в `<img src>` без
//! второго вызова и без доступа к файловой системе. Кэш по `app.path` держит
//! диск прочитанным один раз — каталог из ~100 приложений иначе читался бы
//! заново при каждом перерисовывании списка.
//!
//! Негативный результат кэшируется тоже: приложение без иконки не должно
//! заставлять повторно сканировать каталоги при каждом запросе.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use image::codecs::png::{PngDecoder, PngEncoder};
use image::{DynamicImage, ExtendedColorType, ImageEncoder};
use splitroute_protocol::{InstalledApp, Platform};

/// Максимальная сторона иконки: 64 px достаточно для списка и retina-экрана.
const MAX_ICON_SIDE: u32 = 64;
/// Предел закодированной Base64-строки: крупнее в UI всё равно не нужно.
const MAX_ENCODED_BYTES: usize = 256 * 1024;
/// Предел чтения файла иконки (SVG и ICNS бывают неожиданно большими).
const MAX_ICON_FILE_BYTES: u64 = 8 * 1024 * 1024;

type IconCache = HashMap<String, Option<String>>;

/// Иконка приложения как data URL; `None` — честный fallback без иконки.
pub fn icon_for(app: &InstalledApp) -> Option<String> {
    cached_icon(&app.path, || load_icon(app))
}

/// Кэш по ключу (путь приложения). Загрузка идёт под локом: параллельные
/// запросы одной иконки не читают диск дважды.
pub(crate) fn cached_icon<F: FnOnce() -> Option<String>>(key: &str, loader: F) -> Option<String> {
    let mut guard = match cache().lock() {
        Ok(guard) => guard,
        // Отравленный мьютекс не повод падать: данные кэша — лишь строки.
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(cached) = guard.get(key) {
        return cached.clone();
    }
    let value = loader();
    guard.insert(key.to_string(), value.clone());
    value
}

fn cache() -> &'static Mutex<IconCache> {
    static CACHE: OnceLock<Mutex<IconCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_icon(app: &InstalledApp) -> Option<String> {
    match app.platform {
        Platform::Macos => {
            let png = super::icns::bundle_icon_png(Path::new(&app.path))?;
            png_data_url(&png)
        }
        Platform::Linux => {
            let file = super::linux_icons::icon_file(app)?;
            file_data_url(&file)
        }
        Platform::Windows => windows_data_url(&app.path),
    }
}

/// Windows-иконка: Win32 отдаёт сырые RGBA, PNG кодируется здесь, чтобы
/// изолированная проверка Win32-части не тянула `image`.
#[cfg(windows)]
fn windows_data_url(path: &str) -> Option<String> {
    let raw = super::windows_icons::exe_icon(path)?;
    let image = image::RgbaImage::from_raw(raw.width, raw.height, raw.rgba)?;
    let resized = resize_to_fit(DynamicImage::ImageRgba8(image), MAX_ICON_SIDE);
    let rgba = resized.to_rgba8();
    let png = encode_png(rgba.width(), rgba.height(), rgba.as_raw())?;
    data_url("image/png", &png)
}

/// На других ОС запись `Platform::Windows` может прийти только из состояния
/// другого пользователя: молча возвращаем fallback.
#[cfg(not(windows))]
fn windows_data_url(_path: &str) -> Option<String> {
    None
}

fn file_data_url(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let bytes = read_limited(path)?;
    match extension.as_str() {
        "png" => png_data_url(&bytes),
        // SVG отдаём как есть: растеризатор потребовал бы отдельного движка.
        "svg" => data_url("image/svg+xml", &bytes),
        // XPM не декодируем: это текстовый формат без готового растра, а
        // полноценный парсер ради устаревших иконок не нужен.
        _ => None,
    }
}

/// PNG-иконка -> data URL: декодирование, resize до [`MAX_ICON_SIDE`],
/// перекодирование и Base64 с лимитом.
pub fn png_data_url(png: &[u8]) -> Option<String> {
    let normalized = normalize_png(png)?;
    data_url("image/png", &normalized)
}

/// Декодирует PNG, уменьшает до [`MAX_ICON_SIDE`] и кодирует заново.
pub fn normalize_png(png: &[u8]) -> Option<Vec<u8>> {
    let decoder = PngDecoder::new(Cursor::new(png)).ok()?;
    let image = DynamicImage::from_decoder(decoder).ok()?;
    encode_dynamic(image)
}

fn encode_dynamic(image: DynamicImage) -> Option<Vec<u8>> {
    let resized = resize_to_fit(image, MAX_ICON_SIDE);
    let rgba = resized.to_rgba8();
    encode_png(rgba.width(), rgba.height(), rgba.as_raw())
}

/// Кодирует RGBA в PNG.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }
    let mut buffer = Vec::new();
    PngEncoder::new(&mut buffer)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .ok()?;
    Some(buffer)
}

/// Уменьшает картинку до `max_side` по длинной стороне; не растягивает.
pub fn resize_to_fit(image: DynamicImage, max_side: u32) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    let longest = width.max(height);
    if longest == 0 || longest <= max_side {
        return image;
    }
    let scale = f64::from(max_side) / f64::from(longest);
    let new_width = ((f64::from(width) * scale).round() as u32).max(1);
    let new_height = ((f64::from(height) * scale).round() as u32).max(1);
    image.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
}

/// Собирает data URL с общим лимитом.
pub fn data_url(mime: &str, bytes: &[u8]) -> Option<String> {
    data_url_with_limit(mime, bytes, MAX_ENCODED_BYTES)
}

/// Сборка data URL с явным лимитом на закодированную строку.
pub fn data_url_with_limit(mime: &str, bytes: &[u8], limit: usize) -> Option<String> {
    let encoded = base64_encode(bytes);
    if encoded.len() > limit {
        return None;
    }
    Some(format!("data:{mime};base64,{encoded}"))
}

/// Base64 (RFC 4648, с паддингом): свой на 20 строк вместо новой зависимости.
pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = u32::from(chunk.get(1).copied().unwrap_or_default());
        let third = u32::from(chunk.get(2).copied().unwrap_or_default());
        let triple = (first << 16) | (second << 8) | third;
        output.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        output.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    output
}

fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_ICON_FILE_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splitroute_protocol::Platform;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "sr-catalog-icons-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn test_png(width: u32, height: u32) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[(x % 251) as u8, (y % 241) as u8, 120, 255]);
            }
        }
        encode_png(width, height, &rgba).expect("PNG кодируется")
    }

    fn decode(png: &[u8]) -> DynamicImage {
        let decoder = PngDecoder::new(Cursor::new(png)).expect("PNG декодируется");
        DynamicImage::from_decoder(decoder).expect("PNG декодируется")
    }

    fn app(name: &str, path: &str, platform: Platform) -> InstalledApp {
        InstalledApp {
            name: name.to_string(),
            path: path.to_string(),
            platform,
        }
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn data_url_has_expected_prefix() {
        assert_eq!(
            data_url("image/png", b"").as_deref(),
            Some("data:image/png;base64,")
        );
        assert_eq!(
            data_url("image/svg+xml", b"foobar").as_deref(),
            Some("data:image/svg+xml;base64,Zm9vYmFy")
        );
        let png = png_data_url(&test_png(32, 32)).expect("data URL для PNG");
        assert!(png.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn encoded_size_limit_gives_none() {
        let png = test_png(8, 8);
        assert!(data_url_with_limit("image/png", &png, 8).is_none());
        assert!(data_url_with_limit("image/png", &png, png.len() * 2).is_some());
    }

    #[test]
    fn large_image_is_resized_to_64() {
        let normalized = normalize_png(&test_png(128, 128)).expect("нормализация");
        let image = decode(&normalized);
        assert_eq!((image.width(), image.height()), (64, 64));

        // Пропорции сохраняются.
        let wide = decode(&normalize_png(&test_png(128, 32)).expect("нормализация"));
        assert_eq!((wide.width(), wide.height()), (64, 16));

        // Маленькая иконка не растягивается.
        let small = decode(&normalize_png(&test_png(32, 32)).expect("нормализация"));
        assert_eq!((small.width(), small.height()), (32, 32));
    }

    #[test]
    fn broken_png_gives_none() {
        assert!(normalize_png(b"not a png").is_none());
        assert!(normalize_png(&[]).is_none());
        let mut truncated = test_png(64, 64);
        truncated.truncate(40);
        assert!(normalize_png(&truncated).is_none());
    }

    #[test]
    fn cache_loads_once_including_negative_result() {
        let dir = temp_dir("cache");
        let calls = AtomicUsize::new(0);
        let key = dir.join("positive").to_string_lossy().into_owned();
        let first = cached_icon(&key, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Some("data:image/png;base64,AAAA".to_string())
        });
        let second = cached_icon(&key, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Some("data:image/png;base64,BBBB".to_string())
        });
        assert_eq!(first, second);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "диск читается один раз");

        let negative_calls = AtomicUsize::new(0);
        let negative_key = dir.join("negative").to_string_lossy().into_owned();
        assert!(cached_icon(&negative_key, || {
            negative_calls.fetch_add(1, Ordering::SeqCst);
            None
        })
        .is_none());
        assert!(cached_icon(&negative_key, || {
            negative_calls.fetch_add(1, Ordering::SeqCst);
            Some("data:image/png;base64,CCCC".to_string())
        })
        .is_none());
        assert_eq!(
            negative_calls.load(Ordering::SeqCst),
            1,
            "негативный результат тоже кэшируется"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn macos_bundle_icon_becomes_data_url() {
        let dir = temp_dir("macos");
        let bundle = dir.join("Example.app");
        let resources = bundle.join("Contents").join("Resources");
        std::fs::create_dir_all(&resources).unwrap();

        let png = test_png(128, 128);
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"ic07");
        chunk.extend_from_slice(&((png.len() + 8) as u32).to_be_bytes());
        chunk.extend_from_slice(&png);
        let mut icns = Vec::new();
        icns.extend_from_slice(b"icns");
        icns.extend_from_slice(&((chunk.len() + 8) as u32).to_be_bytes());
        icns.extend_from_slice(&chunk);
        std::fs::write(resources.join("app.icns"), &icns).unwrap();

        let icon = icon_for(&app("Example", &bundle.to_string_lossy(), Platform::Macos));
        let url = icon.expect("иконка бандла");
        let encoded = url
            .strip_prefix("data:image/png;base64,")
            .expect("префикс data URL");
        assert!(!encoded.is_empty());

        // Несуществующий бандл — fallback, а не паника.
        assert!(icon_for(&app("Ghost", "/nonexistent/Ghost.app", Platform::Macos)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Замер на реальном каталоге ОС: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore = "требует реального каталога приложений и ~2 с"]
    fn real_catalog_icons_are_found() {
        let scan_started = std::time::Instant::now();
        let report = crate::catalog::scan();
        let scan_time = scan_started.elapsed();

        let cold_started = std::time::Instant::now();
        let icons: Vec<String> = report.apps.iter().filter_map(icon_for).collect();
        let cold_time = cold_started.elapsed();

        let warm_started = std::time::Instant::now();
        let warm_found = report
            .apps
            .iter()
            .filter(|app| icon_for(app).is_some())
            .count();
        let warm_time = warm_started.elapsed();

        assert_eq!(icons.len(), warm_found, "кэш не меняет результат");
        let total: usize = icons.iter().map(String::len).sum();
        let longest = icons.iter().map(String::len).max().unwrap_or_default();
        eprintln!(
            "каталог: {} приложений, иконок {}; суммарный размер data URL {} КиБ, максимум {} КиБ; \
             скан {:?}, холодные иконки {:?}, кэш {:?}",
            report.apps.len(),
            icons.len(),
            total / 1024,
            longest / 1024,
            scan_time,
            cold_time,
            warm_time
        );
    }
}
