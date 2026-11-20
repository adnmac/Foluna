//! Étagère d'une liseuse et livres présents sur l'appareil (`ShelfSync.swift`).

use std::collections::HashSet;

use unicode_normalization::UnicodeNormalization;

use crate::library::groups::BookGroup;
use crate::library::Book;

/// Titre réduit à ses lettres et chiffres, sans casse ni accents.
pub fn normalize(s: &str) -> String {
    s.nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

/// Un livre est sur l'appareil si l'un de ses fichiers y porte son nom d'envoi, ou si son titre y figure.
pub fn is_on_device(files: &[Book], document_names: &HashSet<String>, device_titles: &HashSet<String>) -> bool {
    files.iter().any(|b| {
        document_names.contains(&b.kindle_filename())
            || b.sent_filename.as_ref().is_some_and(|n| document_names.contains(n))
            || device_titles.contains(&normalize(&b.title))
    })
}

/// Livres de l'étagère à envoyer : absents de l'appareil et envoyables ; un par groupe, son fichier principal.
pub fn missing(shelf: &[BookGroup], document_names: &HashSet<String>, device_titles: &HashSet<String>) -> Vec<Book> {
    shelf
        .iter()
        .filter(|g| !is_on_device(&g.books, document_names, device_titles))
        .map(BookGroup::primary)
        .filter(Book::can_send_to_kindle)
        .collect()
}
