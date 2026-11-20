//! Marque d'une liseuse : où vont les livres, quels formats elle lit, ce que Foluna lui envoie (`ReaderBrand.swift`).
//! Le transport (disque USB) est à part : voir `volume`.

use serde::{Deserialize, Serialize};

use crate::library::Book;
use crate::metadata::BookFormat;
use crate::util::bytes::safe_filename;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReaderBrand {
    /// Amazon Kindle : AZW3, MOBI, PDF ; l'EPUB est converti en AZW3.
    Kindle,
    /// Rakuten Kobo : lit l'EPUB tel quel (et le PDF).
    Kobo,
    /// Autre liseuse qui lit l'EPUB (PocketBook, Tolino…), ajoutée par l'utilisateur.
    Generic,
}

impl ReaderBrand {
    pub fn label(self) -> &'static str {
        match self {
            ReaderBrand::Kindle => "Kindle",
            ReaderBrand::Kobo => "Kobo",
            ReaderBrand::Generic => "Liseuse USB",
        }
    }

    /// Lit l'EPUB : un livre lui est envoyé en EPUB plutôt qu'en AZW3.
    pub fn reads_epub(self) -> bool {
        self != ReaderBrand::Kindle
    }

    /// Dossier d'envoi des livres, relatif à la racine du disque.
    pub fn default_books_folder(self) -> &'static str {
        match self {
            ReaderBrand::Kindle => "documents",
            ReaderBrand::Kobo => "Books",
            ReaderBrand::Generic => "",
        }
    }

    /// Dossier parcouru pour la liste des livres : la Kobo range ses livres n'importe où sur le disque.
    pub fn list_folder(self, books_folder: &str) -> String {
        if self == ReaderBrand::Kobo { String::new() } else { books_folder.to_string() }
    }

    /// Miniature de couverture déposée à côté du livre (la Kobo et les autres la tirent du fichier).
    pub fn sends_thumbnails(self) -> bool {
        self == ReaderBrand::Kindle
    }

    /// Extensions des livres affichés dans la liste de l'appareil.
    pub fn book_extensions(self) -> &'static [&'static str] {
        match self {
            ReaderBrand::Kindle => &["azw3", "azw", "mobi", "pdf", "kfx", "txt", "prc", "azw8"],
            ReaderBrand::Kobo => &["epub", "pdf", "mobi", "txt", "cbz", "cbr", "rtf", "html"],
            ReaderBrand::Generic => &["epub", "pdf", "mobi", "fb2", "txt", "djvu", "cbz", "rtf"],
        }
    }

    /// Dossiers jamais parcourus (données de l'appareil).
    pub fn hidden_folders(self) -> &'static [&'static str] {
        match self {
            ReaderBrand::Kindle => &[],
            ReaderBrand::Kobo => &[".kobo", ".kobo-images", ".adobe-digital-editions", "System Volume Information"],
            ReaderBrand::Generic => &["System Volume Information", "system"],
        }
    }

    /// Fichier à présenter comme un livre (exclut le fichier de notes de la Kindle).
    pub fn is_book(self, name: &str) -> bool {
        if name.starts_with('.') || name.eq_ignore_ascii_case("My Clippings.txt") {
            return false;
        }
        let ext = std::path::Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
        self.book_extensions().contains(&ext.as_str())
    }
}

impl Book {
    /// Format du fichier envoyé à une liseuse de cette marque. Kindle : AZW3 pour un EPUB ; autres : EPUB pour un
    /// livre Kindle (MOBI, AZW3, AZW) ; le PDF tel quel.
    pub fn sent_extension(&self, brand: ReaderBrand) -> &'static str {
        if !brand.reads_epub() {
            return if self.becomes_azw3() { "azw3" } else { self.format.extension() };
        }
        if self.format == BookFormat::Pdf { "pdf" } else { "epub" }
    }

    /// Une conversion précède l'envoi : AZW3 produit pour la Kindle, EPUB réécrit ou converti pour les autres
    /// (un PDF part tel quel).
    pub fn needs_conversion(&self, brand: ReaderBrand) -> bool {
        if brand.reads_epub() { self.format != BookFormat::Pdf } else { self.becomes_azw3() }
    }

    /// Peut partir sur une liseuse de cette marque. Un livre Kindle sous DRM ne se lit que sur une Kindle ; un EPUB
    /// sous DRM (Adobe) part tel quel sur une liseuse EPUB, jamais converti.
    pub fn can_send(&self, brand: ReaderBrand) -> bool {
        if !self.is_drm_protected {
            return true;
        }
        if brand.reads_epub() {
            matches!(self.format, BookFormat::Epub | BookFormat::Pdf)
        } else {
            self.format.is_kindle_native()
        }
    }

    /// Peut partir sur au moins une marque de liseuse.
    pub fn can_send_to_any_reader(&self) -> bool {
        [ReaderBrand::Kindle, ReaderBrand::Kobo, ReaderBrand::Generic].into_iter().any(|b| self.can_send(b))
    }

    /// Nom proposé pour la copie sur une liseuse de cette marque.
    pub fn filename(&self, brand: ReaderBrand) -> String {
        let author = self.authors.first().map(|a| format!(" - {a}")).unwrap_or_default();
        let ext = if self.is_drm_protected { self.format.extension() } else { self.sent_extension(brand) };
        format!("{}.{ext}", safe_filename(&format!("{}{author}", self.title)))
    }
}
