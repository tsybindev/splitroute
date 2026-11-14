//! macOS: разбор Apple ICNS (`.icns`) без внешних крейтов.
//!
//! Контейнер: сигнатура `icns`, big-endian u32 полного размера, далее чанки
//! (тип — 4 байта, длина — big-endian u32 включая 8 байт заголовка). Берутся
//! только PNG-чанки: macOS начиная с 10.7 кладёт PNG во все современные
//! размеры, а RLE/JPEG2000-варианты потребовали бы отдельных декодеров.
//!
//! `Info.plist` (`CFBundleIconFile`) намеренно не читается: на практике иконка
//! лежит в `Contents/Resources/*.icns` под предсказуемыми именами, а бинарный
//! plist — это отдельный парсер ради одного имени файла.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Предел чтения файла иконки: гигантские `.icns` пропускаются.
const MAX_ICON_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Минимальная сторона, ниже которой иконка в списке выглядит размытой.
pub const PREFERRED_SIDE: u32 = 64;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// PNG-чанк контейнера с шириной из `IHDR`.
struct PngChunk<'a> {
    width: u32,
    bytes: &'a [u8],
}

/// Выбирает PNG из ICNS: минимальная ширина >= [`PREFERRED_SIDE`] (тай-брейк —
/// меньший размер в байтах), иначе самая крупная картинка.
pub fn select_png(icns: &[u8]) -> Option<&[u8]> {
    let chunks = png_chunks(icns)?;
    if chunks.is_empty() {
        return None;
    }
    let big_enough = chunks
        .iter()
        .filter(|chunk| chunk.width >= PREFERRED_SIDE)
        .min_by_key(|chunk| (chunk.width, chunk.bytes.len()));
    match big_enough.or_else(|| {
        chunks
            .iter()
            .max_by_key(|chunk| (chunk.width, chunk.bytes.len()))
    }) {
        Some(chunk) => Some(chunk.bytes),
        None => None,
    }
}

/// PNG-содержимое иконки бандла: поиск файла в `Contents/Resources` и разбор.
pub fn bundle_icon_png(bundle: &Path) -> Option<Vec<u8>> {
    let path = bundle_icns_path(bundle)?;
    let bytes = read_limited(&path)?;
    select_png(&bytes).map(|png| png.to_vec())
}

/// Файл иконки бандла: `app.icns`, `<stem>.icns`, `AppIcon.icns`, затем первый
/// `.icns` по алфавиту.
pub fn bundle_icns_path(bundle: &Path) -> Option<PathBuf> {
    let resources = bundle.join("Contents").join("Resources");
    let stem = bundle.file_stem()?.to_str()?.trim().to_string();

    let mut preferred = vec![resources.join("app.icns")];
    if !stem.is_empty() {
        preferred.push(resources.join(format!("{stem}.icns")));
    }
    preferred.push(resources.join("AppIcon.icns"));
    if let Some(found) = preferred.into_iter().find(|path| path.is_file()) {
        return Some(found);
    }

    // Запасной вариант: единственная иконка под произвольным именем.
    let mut rest: Vec<PathBuf> = std::fs::read_dir(&resources)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("icns"))
        })
        .collect();
    rest.sort();
    rest.into_iter().next()
}

/// Собирает список PNG-чанков; битый контейнер или обрезанный чанк — `None`.
fn png_chunks(icns: &[u8]) -> Option<Vec<PngChunk<'_>>> {
    if icns.len() < 12 || &icns[0..4] != b"icns" {
        return None;
    }
    let total = u32::from_be_bytes(icns[4..8].try_into().ok()?) as usize;
    // Обрезанный файл: заголовок обещает больше, чем прочитано.
    if total < 12 || total > icns.len() {
        return None;
    }

    let mut chunks = Vec::new();
    let mut offset = 8usize;
    while offset + 8 <= total {
        let tag = &icns[offset..offset + 4];
        let length = u32::from_be_bytes(icns[offset + 4..offset + 8].try_into().ok()?) as usize;
        if length < 8 {
            return None;
        }
        let end = offset.checked_add(length)?;
        if end > total {
            return None;
        }
        let data = &icns[offset + 8..end];
        offset = end;

        if tag == b"TOC " || !data.starts_with(&PNG_SIGNATURE) || data.len() < 24 {
            continue;
        }
        if &data[12..16] != b"IHDR" {
            continue;
        }
        let width = u32::from_be_bytes(data[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(data[20..24].try_into().ok()?);
        if width == 0 || height == 0 {
            continue;
        }
        chunks.push(PngChunk { width, bytes: data });
    }
    Some(chunks)
}

/// Читает файл с ограничением размера.
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
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Минимальный «PNG» нужного размера: сигнатура + `IHDR` + добивка, чтобы
    /// различать чанки по длине. Кодировать настоящий PNG здесь не нужно.
    fn png(width: u32, height: u32, padding: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&PNG_SIGNATURE);
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend(std::iter::repeat_n(0xab, padding));
        bytes
    }

    fn container(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (tag, data) in chunks {
            body.extend_from_slice(*tag);
            body.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
            body.extend_from_slice(data);
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"icns");
        bytes.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(&body);
        bytes
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "sr-catalog-icns-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn smallest_png_at_least_64_is_selected() {
        let bytes = container(&[
            (b"ic11", png(32, 32, 10)),
            (b"ic07", png(128, 128, 30)),
            (b"ic12", png(64, 64, 20)),
        ]);
        let selected = select_png(&bytes).expect("PNG-чанк найден");
        assert_eq!(u32::from_be_bytes(selected[16..20].try_into().unwrap()), 64);
    }

    #[test]
    fn largest_png_is_used_when_no_small_enough_size() {
        let bytes = container(&[(b"ic11", png(32, 32, 10)), (b"ic07", png(128, 128, 30))]);
        let selected = select_png(&bytes).expect("PNG-чанк найден");
        assert_eq!(
            u32::from_be_bytes(selected[16..20].try_into().unwrap()),
            128
        );
    }

    #[test]
    fn tie_break_prefers_smaller_bytes() {
        let smaller = png(64, 64, 5);
        let bytes = container(&[
            (b"ic12", png(64, 64, 40)),
            (b"ic13", smaller.clone()),
            (b"ic07", png(128, 128, 80)),
        ]);
        assert_eq!(select_png(&bytes), Some(smaller.as_slice()));
    }

    #[test]
    fn non_png_chunks_and_broken_container_give_none() {
        let raw = container(&[(b"ic04", vec![0u8; 32]), (b"ic05", vec![1u8; 16])]);
        assert!(select_png(&raw).is_none());

        assert!(select_png(b"nope").is_none());
        assert!(select_png(&[]).is_none());
        assert!(select_png(b"icns\x00\x00\x00\x08").is_none());
    }

    #[test]
    fn truncated_chunk_gives_none() {
        let mut bytes = container(&[(b"ic12", png(64, 64, 4))]);
        bytes.truncate(bytes.len() - 4);
        // Объявленный размер контейнера больше фактического.
        assert!(select_png(&bytes).is_none());

        let mut bytes = container(&[(b"ic12", png(64, 64, 4))]);
        // Портится длина единственного чанка: он выходит за контейнер.
        let chunk_len = bytes.len() - 8;
        bytes[12..16].copy_from_slice(&((chunk_len + 4) as u32).to_be_bytes());
        assert!(select_png(&bytes).is_none());
    }

    #[test]
    fn bundle_icns_priority_and_alphabetical_fallback() {
        let dir = temp_dir("bundle");
        let bundle = dir.join("Example.app");
        let resources = bundle.join("Contents").join("Resources");
        std::fs::create_dir_all(&resources).unwrap();

        // Только иконки под произвольными именами: берётся первая по алфавиту.
        std::fs::write(resources.join("Zeta.icns"), b"icns").unwrap();
        std::fs::write(resources.join("Alpha.icns"), b"icns").unwrap();
        assert_eq!(
            bundle_icns_path(&bundle).and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some("Alpha.icns".into())
        );

        // AppIcon.icns приоритетнее алфавитного порядка.
        std::fs::write(resources.join("AppIcon.icns"), b"icns").unwrap();
        assert_eq!(
            bundle_icns_path(&bundle).and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some("AppIcon.icns".into())
        );

        // <stem>.icns приоритетнее AppIcon.icns.
        std::fs::write(resources.join("Example.icns"), b"icns").unwrap();
        assert_eq!(
            bundle_icns_path(&bundle).and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some("Example.icns".into())
        );

        // app.icns приоритетнее всех.
        std::fs::write(resources.join("app.icns"), b"icns").unwrap();
        assert_eq!(
            bundle_icns_path(&bundle).and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some("app.icns".into())
        );

        // Не-иконки игнорируются, отсутствующий бандл не паникует.
        assert!(bundle_icns_path(Path::new("/nonexistent/Example.app")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_icon_png_reads_real_container() {
        let dir = temp_dir("read");
        let bundle = dir.join("Example.app");
        let resources = bundle.join("Contents").join("Resources");
        std::fs::create_dir_all(&resources).unwrap();
        let bytes = container(&[(b"ic12", png(64, 64, 3))]);
        std::fs::write(resources.join("app.icns"), &bytes).unwrap();

        let png = bundle_icon_png(&bundle).expect("PNG из бандла");
        assert!(png.starts_with(&PNG_SIGNATURE));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
