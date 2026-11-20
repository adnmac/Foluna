//! PDF : titre, auteur, mots-clés et protection (`Library/PDFMetadata.swift`), couverture rendue depuis la première
//! page par pdfium. La bibliothèque pdfium (`pdfium.dll`, livrée avec l'app) est chargée à la demande : sans elle,
//! un PDF n'a simplement pas de couverture.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::{PdfRenderConfig, Pdfium};

use lopdf::{Document, Object};

use crate::error::{BookError, Result};
use crate::metadata::BookMetadata;

pub fn metadata(path: &Path) -> Result<BookMetadata> {
    let doc = match Document::load(path) {
        Ok(doc) => doc,
        // Un PDF chiffré qui ne s'ouvre pas sans mot de passe est un livre protégé, pas un fichier illisible.
        Err(lopdf::Error::Decryption(_)) => {
            let mut meta = BookMetadata::new("");
            meta.is_drm_protected = true;
            return Ok(meta);
        }
        Err(_) => return Err(BookError::Unreadable("PDF invalide".into())),
    };
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| match o {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        });
    let field = |key: &[u8]| -> Option<String> {
        let value = info?.get(key).ok()?;
        let s = match value {
            Object::String(bytes, _) => decode_text(bytes),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::String(bytes, _) => decode_text(bytes),
                _ => return None,
            },
            _ => return None,
        };
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let mut meta = BookMetadata::new(field(b"Title").unwrap_or_default());
    meta.authors = field(b"Author")
        .map(|a| a.split([';', '&']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    meta.subjects = field(b"Keywords")
        .map(|k| k.split([',', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    // Chiffré et pas ouvert sans mot de passe (un PDF chiffré pour ses seules permissions s'ouvre) : protégé.
    if doc.trailer.has(b"Encrypt") && doc.encryption_state.is_none() {
        meta = BookMetadata::new("");
        meta.is_drm_protected = true;
        return Ok(meta);
    }
    meta.cover_data = cover(path);
    Ok(meta)
}

static LIBRARY_PATH: OnceLock<PathBuf> = OnceLock::new();
static PDFIUM: OnceLock<Option<Pdfium>> = OnceLock::new();

/// Dossier qui contient la bibliothèque pdfium (`pdfium.dll` ou `libpdfium.so`), donné par l'app au lancement
/// (ressources de l'installeur). Sinon : `FOLUNA_PDFIUM` (chemin du fichier), puis le dossier de l'exécutable.
pub fn set_library_dir(dir: &Path) {
    let _ = LIBRARY_PATH.set(dir.join(Pdfium::pdfium_platform_library_name()));
}

fn pdfium() -> Option<&'static Pdfium> {
    PDFIUM
        .get_or_init(|| {
            let candidates: Vec<PathBuf> = LIBRARY_PATH
                .get()
                .cloned()
                .into_iter()
                .chain(std::env::var_os("FOLUNA_PDFIUM").map(PathBuf::from))
                .chain(std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(Pdfium::pdfium_platform_library_name()))))
                .collect();
            candidates.iter().filter(|p| p.exists()).find_map(|p| Pdfium::bind_to_library(p).ok()).map(Pdfium::new)
        })
        .as_ref()
}

/// Vrai si pdfium est disponible (couvertures des PDF).
pub fn has_renderer() -> bool {
    pdfium().is_some()
}

/// Couverture : première page rendue en 600 px de large, en JPEG.
pub fn cover(path: &Path) -> Option<Vec<u8>> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, None).ok()?;
    let page = doc.pages().get(0).ok()?;
    let image = page.render_with_config(&PdfRenderConfig::new().set_target_width(600)).ok()?.as_image().ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    image.to_rgb8().write_to(&mut out, image::ImageFormat::Jpeg).ok()?;
    Some(out.into_inner())
}

/// Chaîne de texte PDF : UTF-16 (avec BOM) ou PDFDocEncoding (proche du Latin-1).
fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    bytes.iter().map(|&b| b as char).collect()
}
