//! Images : type d'après la signature (`Util/ImageTools.swift`).

/// Type MIME d'après les premiers octets.
pub fn mime_type(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some("image/png")
    } else if b.starts_with(b"GIF8") {
        Some("image/gif")
    } else if b.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

pub fn file_extension(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

/// Redimensionne (sans agrandir) et encode en JPEG ; None si l'image est illisible.
pub fn jpeg(data: &[u8], max_pixel_size: u32) -> Option<Vec<u8>> {
    let img = ::image::load_from_memory(data).ok()?;
    let img = if img.width().max(img.height()) > max_pixel_size { img.thumbnail(max_pixel_size, max_pixel_size) } else { img };
    let mut out = std::io::Cursor::new(Vec::new());
    let encoder = ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    img.to_rgb8().write_with_encoder(encoder).ok()?;
    Some(out.into_inner())
}
