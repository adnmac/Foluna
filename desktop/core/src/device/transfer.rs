//! Préparation du fichier envoyé à une liseuse (`KindleTransfer`), avec les corrections de l'app. Les fichiers
//! produits sont mis en cache dans `Conversions/`.

use std::collections::HashSet;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::convert::{epub_to_azw3, mobi_to_epub};
use crate::device::brand::ReaderBrand;
use crate::epub::metadata_writer;
use crate::error::{BookError, Result};
use crate::library::{AppDirectories, Book};
use crate::metadata::BookFormat;
use crate::mobi::metadata_writer as mobi_writer;

/// Fichier à copier sur la liseuse :
/// - Kindle : AZW3 converti pour un EPUB, copie aux informations réécrites pour un MOBI/AZW3, PDF tel quel ;
/// - Kobo et liseuse générique : EPUB aux informations réécrites, EPUB converti pour un MOBI/AZW3, PDF tel quel ;
/// - livre sous DRM : l'original, tel quel.
pub fn prepare(book: &Book, dirs: &AppDirectories, brand: ReaderBrand) -> Result<PathBuf> {
    if book.is_drm_protected || book.format == BookFormat::Pdf {
        return Ok(book.file_path());
    }
    let out = cached_copy(book, dirs, brand);
    if out.exists() {
        return Ok(out);
    }
    let edits = book.edits();
    let source = book.file_path();
    match (book.format, brand.reads_epub()) {
        (BookFormat::Epub, true) => metadata_writer::rewrite(&source, &edits, &out)?,
        (BookFormat::Epub, false) => {
            epub_to_azw3::convert(&source, &out, Some(&edits), Some(&book.asin))?;
        }
        (_, true) => mobi_to_epub::convert(&source, &out, Some(&edits))?,
        (_, false) => mobi_writer::rewrite_file(&source, &edits, &out)?,
    }
    if !out.exists() {
        return Err(BookError::ConversionFailed("aucun fichier produit".into()));
    }
    Ok(out)
}

/// Copie en cache pour la liseuse. Clé stable : toute correction des informations force une nouvelle copie.
/// La Kobo et les liseuses génériques reçoivent la même copie (EPUB).
pub fn cached_copy(book: &Book, dirs: &AppDirectories, brand: ReaderBrand) -> PathBuf {
    let mut key = serde_json::to_vec(&serde_json::to_value(book.edits()).unwrap_or_default()).unwrap_or_default();
    if brand.reads_epub() {
        key.extend_from_slice(b"liseuse-epub");
    }
    let digest = Sha256::digest(&key);
    let variant: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    dirs.conversions().join(format!("{}-{variant}.{}", book.fingerprint, book.sent_extension(brand)))
}

/// Noms des copies en cache encore utiles (ménage des conversions).
pub fn cache_names(books: &[Book], dirs: &AppDirectories) -> HashSet<String> {
    books
        .iter()
        .flat_map(|b| {
            [cached_copy(b, dirs, ReaderBrand::Kindle), cached_copy(b, dirs, ReaderBrand::Kobo), exported_epub(b, dirs)]
        })
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

pub fn exported_epub(book: &Book, dirs: &AppDirectories) -> PathBuf {
    dirs.conversions().join(format!("{}.epub", book.fingerprint))
}

/// Conversion MOBI/AZW3 → EPUB vers le cache (export par l'utilisateur).
pub fn export_epub(book: &Book, dirs: &AppDirectories) -> Result<PathBuf> {
    let out = exported_epub(book, dirs);
    mobi_to_epub::convert(&book.file_path(), &out, Some(&book.edits()))?;
    Ok(out)
}
