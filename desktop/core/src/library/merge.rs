//! Rapprochement des fichiers trouvés et de l'index existant lors d'un parcours (`LibraryMerge`).

use std::collections::{HashMap, HashSet};

use crate::library::book::{Book, CURRENT_METADATA_VERSION};
use crate::library::scanner::{fingerprint, ScannedFile};

pub struct Plan {
    /// Livres repris tels quels (chemin mis à jour s'ils ont été déplacés ou renommés).
    pub kept: Vec<Book>,
    /// Fichiers à relire, avec le livre indexé qu'ils remplacent s'il existe.
    pub changed: Vec<(ScannedFile, Option<Book>)>,
}

/// Un fichier est retrouvé par son chemin, sinon par son empreinte parmi les livres disparus : un livre déplacé
/// ou renommé garde ainsi ses retouches.
pub fn plan(files: &[ScannedFile], known: &[Book], imports_path: &str) -> Plan {
    let by_path: HashMap<&str, &Book> = known.iter().map(|b| (b.path.as_str(), b)).collect();
    let present: HashSet<String> = files.iter().map(ScannedFile::path_string).collect();
    let mut vanished: HashMap<String, Vec<Book>> = HashMap::new();
    for b in known.iter().filter(|b| !present.contains(&b.path)) {
        vanished.entry(b.fingerprint.clone()).or_default().push(b.clone());
    }
    let mut out = Plan { kept: Vec::new(), changed: Vec::new() };
    for f in files {
        let path = f.path_string();
        let mut previous = by_path.get(path.as_str()).map(|b| (*b).clone());
        if previous.is_none()
            && let Some(candidates) = vanished.get_mut(&fingerprint(&f.path, f.size)).filter(|c| !c.is_empty()) {
                let mut moved = candidates.remove(0);
                moved.path = path.clone();
                moved.is_imported = path.starts_with(imports_path);
                // Même contenu : seule la date de modification peut différer (copie, synchronisation).
                moved.modified_at = f.modified_at;
                previous = Some(moved);
            }
        match previous {
            Some(mut b)
                if b.file_size == f.size
                    && (b.modified_at - f.modified_at).abs() < 1.0
                    && b.metadata_version == Some(CURRENT_METADATA_VERSION) =>
            {
                // La date d'ajout suit le fichier.
                b.added_at = f.added_at;
                out.kept.push(b);
            }
            other => out.changed.push((f.clone(), other)),
        }
    }
    out
}

/// Fichier modifié ou index plus ancien : les retouches faites dans l'app restent prioritaires.
pub fn carry_over(previous: &Book, book: &mut Book) {
    book.title = previous.title.clone();
    book.authors = previous.authors.clone();
    book.publisher = previous.publisher.clone().or(book.publisher.take());
    book.language = previous.language.clone().or(book.language.take());
    book.summary = previous.summary.clone().or(book.summary.take());
    book.published_date = previous.published_date.clone().or(book.published_date.take());
    book.isbn = previous.isbn.clone().or(book.isbn.take());
    book.genre = previous.genre.clone().or(book.genre.take());
    book.tags = previous.tags.clone();
    book.asin = previous.asin.clone();
    book.last_sent_at = previous.last_sent_at;
    book.sent_filename = previous.sent_filename.clone();
    book.pending_send_at = previous.pending_send_at;
    book.pending_for = previous.pending_for.clone();
    book.sent_to = previous.sent_to.clone();
    book.trashed_at = previous.trashed_at;
    book.shelves = previous.shelves.clone();
    // Notice du catalogue, mode PDF… (champs gérés par l'app Mac) : gardés.
    for (k, v) in &previous.extra {
        book.extra.entry(k.clone()).or_insert_with(|| v.clone());
    }
}
