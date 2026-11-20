//! Point d'entrée unique : informations d'un fichier selon son format (`MetadataExtractor`).

use std::path::Path;

use crate::epub::EpubReader;
use crate::error::{BookError, Result};
use crate::metadata::{BookFormat, BookMetadata};
use crate::mobi::MobiBook;
use crate::pdf;
use crate::util::html;

pub fn extract(path: &Path) -> Result<BookMetadata> {
    let format = BookFormat::from_path(path).ok_or_else(|| {
        BookError::Unsupported(format!("extension .{}", path.extension().and_then(|e| e.to_str()).unwrap_or_default()))
    })?;
    let mut meta = match format {
        BookFormat::Epub => EpubReader::open(path)?.book_metadata(),
        BookFormat::Mobi | BookFormat::Azw3 | BookFormat::Azw => MobiBook::open(path)?.metadata(),
        BookFormat::Pdf => pdf::metadata(path)?,
    };
    if meta.title.trim().is_empty() {
        meta.title = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    }
    meta.summary = meta.summary.as_deref().map(html::strip_html).filter(|s| !s.is_empty());
    // « 2003-11-01T05:00:00+00:00 » → « 2003-11-01 » : l'heure n'a pas de sens pour une publication.
    if let Some(d) = &meta.published_date
        && d.find('T') == Some(10) {
            meta.published_date = Some(d[..10].to_string());
        }
    Ok(meta)
}
