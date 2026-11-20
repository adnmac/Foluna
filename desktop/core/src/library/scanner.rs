//! Parcours des dossiers de livres et création des fiches (`FolderScanner`).

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::convert::epub_to_azw3::make_asin;
use crate::extract::extract;
use crate::library::book::{from_system_time, now, Book, Timestamp, CURRENT_METADATA_VERSION};
use crate::library::dirs::CoverStore;
use crate::library::names::normalized_series;
use crate::metadata::{BookFormat, BookMetadata};

/// Fichier trouvé sur disque, avant extraction des métadonnées.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub size: i64,
    pub modified_at: Timestamp,
    /// Arrivée du fichier dans son dossier (copie, téléchargement, import) : sert de date d'ajout.
    pub added_at: Timestamp,
}

impl ScannedFile {
    pub fn path_string(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

/// Fichier caché : nom commençant par un point, ou attribut « caché » / « système » de Windows.
fn is_hidden(path: &Path, meta: &std::fs::Metadata) -> bool {
    if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        if meta.file_attributes() & (HIDDEN | SYSTEM) != 0 {
            return true;
        }
    }
    let _ = meta;
    false
}

/// Parcours récursif des livres, sans les fichiers cachés ni les liens symboliques.
pub fn scan(roots: &[PathBuf]) -> Vec<ScannedFile> {
    let mut out = Vec::new();
    for root in roots {
        walk(root, &mut out, 0);
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<ScannedFile>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() || is_hidden(&path, &meta) {
            continue;
        }
        if meta.is_dir() {
            walk(&path, out, depth + 1);
        } else if meta.is_file() && BookFormat::from_path(&path).is_some() {
            let modified = meta.modified().map(from_system_time).unwrap_or(f64::MIN);
            let added = meta.created().map(from_system_time).unwrap_or_else(|_| now());
            out.push(ScannedFile { path, size: meta.len() as i64, modified_at: modified, added_at: added });
        }
    }
}

/// Empreinte rapide : SHA-256 des 64 premiers Ko (12 octets en hexadécimal) + taille, comme sur Mac.
pub fn fingerprint(path: &Path, size: i64) -> String {
    let Ok(file) = std::fs::File::open(path) else { return size.to_string() };
    let mut head = Vec::with_capacity(65_536);
    if file.take(65_536).read_to_end(&mut head).is_err() {
        return size.to_string();
    }
    let digest = Sha256::digest(&head);
    let hex: String = digest[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("{hex}-{size}")
}

/// Fiche d'un livre : informations du fichier, couverture enregistrée, ASIN tiré au sort.
pub fn make_book(file: &ScannedFile, imported: bool, covers: &CoverStore) -> Book {
    let format = BookFormat::from_path(&file.path).unwrap_or(BookFormat::Epub);
    let fingerprint = fingerprint(&file.path, file.size);
    let meta = extract(&file.path)
        .unwrap_or_else(|_| BookMetadata::new(file.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default()));
    let has_cover = meta.cover_data.as_deref().is_some_and(|c| covers.store(c, &fingerprint));
    let (series, series_index) = normalized_series(meta.series.as_deref(), meta.series_index);
    Book {
        path: file.path_string(),
        format,
        fingerprint,
        file_size: file.size,
        modified_at: file.modified_at,
        added_at: file.added_at,
        title: meta.title,
        authors: meta.authors,
        language: meta.language,
        publisher: meta.publisher,
        summary: meta.summary,
        published_date: meta.published_date,
        isbn: meta.isbn,
        series,
        series_index,
        genre: None,
        tags: meta.subjects,
        has_cover,
        is_drm_protected: meta.is_drm_protected,
        is_imported: imported,
        asin: make_asin(),
        last_sent_at: None,
        sent_filename: None,
        pending_send_at: None,
        pending_for: None,
        sent_to: None,
        trashed_at: None,
        shelves: None,
        metadata_version: Some(CURRENT_METADATA_VERSION),
        extra: Default::default(),
    }
}
