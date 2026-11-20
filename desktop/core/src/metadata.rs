//! Formats de livre et informations extraites d'un fichier (`Library/BookMetadata.swift`).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Formats de la première version Windows : ceux de l'app Mac 0.6.0, mêmes noms dans l'index JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    Epub,
    Mobi,
    Azw3,
    Azw,
    Pdf,
}

impl BookFormat {
    pub const ALL: [BookFormat; 5] = [BookFormat::Epub, BookFormat::Mobi, BookFormat::Azw3, BookFormat::Azw, BookFormat::Pdf];

    /// Format d'après l'extension du fichier.
    pub fn from_path(path: &Path) -> Option<BookFormat> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|f| f.extension() == ext)
    }

    pub fn extension(self) -> &'static str {
        match self {
            BookFormat::Epub => "epub",
            BookFormat::Mobi => "mobi",
            BookFormat::Azw3 => "azw3",
            BookFormat::Azw => "azw",
            BookFormat::Pdf => "pdf",
        }
    }

    pub fn label(self) -> String {
        self.extension().to_ascii_uppercase()
    }

    /// Lisible par une Kindle chargée par USB (l'EPUB est converti en AZW3).
    pub fn is_kindle_native(self) -> bool {
        self != BookFormat::Epub
    }

    /// Formats Mobipocket, lus par `MobiBook`.
    pub fn is_mobi(self) -> bool {
        matches!(self, BookFormat::Mobi | BookFormat::Azw3 | BookFormat::Azw)
    }
}

/// Informations extraites d'un fichier de livre.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BookMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub summary: Option<String>,
    pub identifier: Option<String>,
    /// Date de publication brute (« 1759 », « 2024-03-15 »…).
    pub published_date: Option<String>,
    /// ISBN-13 valide trouvé dans les identifiants du fichier.
    pub isbn: Option<String>,
    pub subjects: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub cover_data: Option<Vec<u8>>,
    pub is_drm_protected: bool,
}

impl BookMetadata {
    pub fn new(title: impl Into<String>) -> Self {
        BookMetadata { title: title.into(), ..Default::default() }
    }

    pub fn edits(&self) -> MetadataEdits {
        MetadataEdits {
            title: self.title.clone(),
            authors: self.authors.clone(),
            publisher: self.publisher.clone(),
            language: self.language.clone(),
            summary: self.summary.clone(),
            published_date: self.published_date.clone(),
            isbn: self.isbn.clone(),
            series: self.series.clone(),
            series_index: self.series_index,
            genre: None,
        }
    }

    pub fn apply(&mut self, e: &MetadataEdits) {
        self.title = e.title.clone();
        self.authors = e.authors.clone();
        self.publisher = e.publisher.clone();
        self.language = e.language.clone();
        self.summary = e.summary.clone();
        self.published_date = e.published_date.clone();
        self.isbn = e.isbn.clone();
        self.series = e.series.clone();
        self.series_index = e.series_index;
    }
}

/// Informations corrigées dans l'app, appliquées aux copies envoyées (`MetadataEdits`, mêmes clés JSON).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataEdits {
    pub title: String,
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isbn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_index: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
}

/// Entrée de table des matières, indépendante du format.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TocNode {
    pub title: String,
    /// Chemin dans l'archive (ou dans le livre généré).
    pub path: String,
    pub fragment: Option<String>,
    pub children: Vec<TocNode>,
}

impl TocNode {
    pub fn new(title: impl Into<String>, path: impl Into<String>) -> Self {
        TocNode { title: title.into(), path: path.into(), ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(BookFormat::from_path(Path::new("C:/Livres/Candide.EPUB")), Some(BookFormat::Epub));
        assert_eq!(BookFormat::from_path(Path::new("notes.txt")), None);
        assert_eq!(serde_json_like(BookFormat::Azw3), "azw3");
    }

    fn serde_json_like(f: BookFormat) -> String {
        f.extension().to_string()
    }
}
