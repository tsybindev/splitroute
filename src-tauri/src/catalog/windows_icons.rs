//! Windows: иконка `.exe` через Shell API, без внешних крейтов.
//!
//! Иконка извлекается в 64×64 (`PrivateExtractIconsW`, при неудаче —
//! `ExtractIconExW`), пиксели читаются через `GetIconInfo` + `GetDIBits` в
//! top-down 32bpp BGRA. Старые `.exe` отдают полностью нулевую альфу: тогда
//! прозрачность восстанавливается из AND-маски (`hbmMask`), как это делает
//! `tauri::image`.
//!
//! Win32-часть возвращает [`RawIcon`], а PNG кодирует `icons.rs`: так чистые
//! функции собираются и проверяются на macOS, а изолированная проверка
//! Win32-кода не тянет крейт `image`.

/// Сырые пиксели иконки в RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Сторона извлекаемой иконки: совпадает с целевым размером в UI.
pub const ICON_SIDE: u32 = 64;

/// BGRA (порядок `GetDIBits` для 32bpp) -> RGBA. Хвост не кратный 4 байтам
/// отбрасывается: неполный пиксель бессмыслен.
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let (pixels, _rest) = bgra.as_chunks::<4>();
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for pixel in pixels {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    rgba
}

/// Назначает альфу по AND-маске: `alpha = 0` в маске — пиксель непрозрачный,
/// `alpha = 255` — прозрачный (белый пиксель маски закрывает иконку).
pub fn apply_mask_alpha(rgba: &mut [u8], mask_rgba: &[u8]) {
    let (pixels, _rest) = rgba.as_chunks_mut::<4>();
    let (mask, _mask_rest) = mask_rgba.as_chunks::<4>();
    for (pixel, mask) in pixels.iter_mut().zip(mask) {
        pixel[3] = if mask[3] == 0 { 255 } else { 0 };
    }
}

/// Переводит 32bpp-копию AND-маски в RGBA-байты для [`apply_mask_alpha`]:
/// светлый пиксель маски помечается `alpha = 255` (прозрачный).
pub fn mask_rgba_from_bgra(pixels: &[u8]) -> Vec<u8> {
    let (source, _rest) = pixels.as_chunks::<4>();
    let mut mask = vec![0u8; source.len() * 4];
    for (index, pixel) in source.iter().enumerate() {
        // 1bpp-маска после конвертации в 32bpp даёт белый (0xFF) на бит 1.
        let luminance = (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3;
        mask[index * 4 + 3] = if luminance >= 128 { 255 } else { 0 };
    }
    mask
}

/// Извлекает иконку `.exe`. `None` — нет файла, нет ресурса или ошибка Win32.
#[cfg(windows)]
pub fn exe_icon(path: &str) -> Option<RawIcon> {
    use std::ptr;
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, PrivateExtractIconsW, HICON};

    if path.trim().is_empty() {
        return None;
    }
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

    let mut hicon: HICON = ptr::null_mut();
    let mut icon_id = 0u32;
    // SAFETY: строка с завершающим NUL, буферы иконки валидны на время вызова.
    let count = unsafe {
        PrivateExtractIconsW(
            wide.as_ptr(),
            0,
            ICON_SIDE as i32,
            ICON_SIDE as i32,
            &mut hicon,
            &mut icon_id,
            1,
            0,
        )
    };
    if count == 0 || hicon.is_null() {
        if !hicon.is_null() {
            // SAFETY: иконка получена выше и в fallback не участвует.
            unsafe { DestroyIcon(hicon) };
        }
        // Запасной путь: Shell вернёт иконку исходного размера из ресурсов.
        let mut large: HICON = ptr::null_mut();
        let mut small: HICON = ptr::null_mut();
        // SAFETY: указатели на локальные переменные, строка с NUL.
        let extracted = unsafe { ExtractIconExW(wide.as_ptr(), 0, &mut large, &mut small, 1) };
        if extracted == 0 {
            return None;
        }
        if !small.is_null() && small != large {
            // SAFETY: `small` — иконка, полученная от Shell.
            unsafe { DestroyIcon(small) };
        }
        hicon = if large.is_null() { small } else { large };
        if hicon.is_null() {
            return None;
        }
    }

    // SAFETY: `hicon` получен от Shell и ещё не уничтожен.
    let icon = unsafe { copy_icon(hicon) };
    // SAFETY: иконка больше не нужна.
    unsafe { DestroyIcon(hicon) };
    icon
}

/// Копирует пиксели иконки и освобождает битмапы, созданные `GetIconInfo`.
///
/// # Safety
/// `hicon` должен быть валидной иконкой; вызывающий уничтожает её сам.
#[cfg(windows)]
unsafe fn copy_icon(hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON) -> Option<RawIcon> {
    use windows_sys::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut info = ICONINFO::default();
    // SAFETY: `hicon` валиден, `info` — локальная структура.
    if unsafe { GetIconInfo(hicon, &mut info) } == 0 {
        return None;
    }

    // SAFETY: битмапы принадлежат `info` и удаляются ниже.
    let result = unsafe { raw_from_bitmaps(info.hbmColor, info.hbmMask) };

    // SAFETY: оба битмапа созданы `GetIconInfo` и больше не нужны.
    unsafe {
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor as HGDIOBJ);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask as HGDIOBJ);
        }
    }
    result
}

/// Читает цветной битмап и, при полностью нулевой альфе, применяет маску.
///
/// # Safety
/// Битмапы должны быть валидными `HBITMAP` (после `GetIconInfo`).
#[cfg(windows)]
unsafe fn raw_from_bitmaps(
    color: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    mask: windows_sys::Win32::Graphics::Gdi::HBITMAP,
) -> Option<RawIcon> {
    // SAFETY: вызовы Win32 ниже получают валидные дескрипторы.
    let (width, height) = unsafe { bitmap_size(color)? };
    // SAFETY: цветной битмап существует, размеры получены из него.
    let color_pixels = unsafe { read_dib(color, width, height)? };
    let mut rgba = bgra_to_rgba(&color_pixels);
    // Отдельная область: проверка занимает неизменяемую ссылку на `rgba`.
    let alpha_is_empty = {
        let (pixels, _rest) = rgba.as_chunks::<4>();
        pixels.iter().all(|pixel| pixel[3] == 0)
    };
    if alpha_is_empty && !mask.is_null() {
        // Прозрачность берём из AND-маски: цветной слой её не содержит.
        if let Some(mask_pixels) = unsafe { read_dib(mask, width, height) } {
            apply_mask_alpha(&mut rgba, &mask_rgba_from_bgra(&mask_pixels));
        }
    }
    Some(RawIcon {
        rgba,
        width,
        height,
    })
}

/// Размеры битмапа через `GetObjectW`.
///
/// # Safety
/// `bitmap` должен быть валидным `HBITMAP`.
#[cfg(windows)]
unsafe fn bitmap_size(bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP) -> Option<(u32, u32)> {
    use std::mem::size_of;
    use windows_sys::Win32::Graphics::Gdi::{GetObjectW, BITMAP, HGDIOBJ};

    if bitmap.is_null() {
        return None;
    }
    let mut info = BITMAP::default();
    let size = size_of::<BITMAP>() as i32;
    // SAFETY: буфер соответствует типу и размеру, который запрашивает GDI.
    let written = unsafe {
        GetObjectW(
            bitmap as HGDIOBJ,
            size,
            std::ptr::from_mut(&mut info).cast(),
        )
    };
    if written != size || info.bmWidth <= 0 || info.bmHeight <= 0 {
        return None;
    }
    Some((info.bmWidth as u32, info.bmHeight as u32))
}

/// Читает битмап как top-down 32bpp BGRA.
///
/// # Safety
/// `bitmap` должен быть валидным `HBITMAP` нужного размера.
#[cfg(windows)]
unsafe fn read_dib(
    bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS,
    };

    if bitmap.is_null() || width == 0 || height == 0 {
        return None;
    }
    let length = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    // SAFETY: HWND = null допустим для GetDC; дескрипторы освобождаются ниже.
    unsafe {
        let screen = GetDC(ptr::null_mut());
        if screen.is_null() {
            return None;
        }
        let memory = CreateCompatibleDC(screen);
        if memory.is_null() {
            ReleaseDC(ptr::null_mut(), screen);
            return None;
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Отрицательная высота — top-down порядок строк, как в RGBA.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };

        let mut pixels = vec![0u8; length];
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            height,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );
        DeleteDC(memory);
        ReleaseDC(ptr::null_mut(), screen);

        // Частичное копирование строк оставляет нули: такой растр не используем.
        if lines == height as i32 {
            Some(pixels)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_is_swapped_to_rgba() {
        let bgra = [0x10u8, 0x20, 0x30, 0xff, 0x01, 0x02, 0x03, 0x80];
        assert_eq!(
            bgra_to_rgba(&bgra),
            vec![0x30, 0x20, 0x10, 0xff, 0x03, 0x02, 0x01, 0x80]
        );
    }

    #[test]
    fn incomplete_pixel_is_dropped() {
        assert_eq!(
            bgra_to_rgba(&[0x01, 0x02, 0x03, 0x04, 0x05]),
            vec![0x03, 0x02, 0x01, 0x04]
        );
        assert!(bgra_to_rgba(&[]).is_empty());
    }

    #[test]
    fn mask_alpha_zero_is_opaque_and_white_is_transparent() {
        let mut rgba = vec![1u8, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0];
        // Первый пиксель непрозрачный (чёрный в маске), второй — прозрачный.
        let mask = vec![0u8, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 255];
        apply_mask_alpha(&mut rgba, &mask);
        assert_eq!(rgba[3], 255);
        assert_eq!(rgba[7], 0);
        assert_eq!(rgba[11], 0);
        // Цветовые каналы не меняются.
        assert_eq!(&rgba[0..3], &[1, 2, 3]);
    }

    #[test]
    fn mask_bgra_is_converted_to_alpha() {
        // Белый пиксель — бит маски выставлен (прозрачный), чёрный — нет.
        let mask_bgra = [0xffu8, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mask = mask_rgba_from_bgra(&mask_bgra);
        assert_eq!(mask[3], 255);
        assert_eq!(mask[7], 0);

        let mut rgba = vec![10u8, 20, 30, 0, 40, 50, 60, 0];
        apply_mask_alpha(&mut rgba, &mask);
        assert_eq!(rgba[3], 0, "белая маска делает пиксель прозрачным");
        assert_eq!(rgba[7], 255, "чёрная маска делает пиксель непрозрачным");
    }

    #[test]
    fn empty_mask_keeps_pixels_opaque() {
        let mut rgba = vec![1u8, 2, 3, 0];
        apply_mask_alpha(&mut rgba, &[]);
        assert_eq!(rgba[3], 0, "без маски альфа не выдумывается");
    }
}
