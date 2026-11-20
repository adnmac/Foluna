//! PDF : titre, auteur, mots-clés et protection (`Library/PDFMetadata.swift`). La couverture (rendu de la
//! première page) viendra avec pdfium ; en attendant, un PDF n'a pas de couverture extraite.

use std::path::Path;

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
    }
    Ok(meta)
}

/// Chaîne de texte PDF : UTF-16 (avec BOM) ou PDFDocEncoding (proche du Latin-1).
fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    bytes.iter().map(|&b| b as char).collect()
}
