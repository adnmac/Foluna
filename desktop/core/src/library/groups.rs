//! Fichiers d'un même livre (même titre, mêmes auteurs) en plusieurs formats ou exemplaires (`BookGroups`).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::library::book::Book;
use crate::library::names::{self, author_key, author_keys, individuals, without_genre};
use crate::metadata::BookFormat;

const GROUP_PREFIX: &str = "group:";

#[derive(Debug, Clone)]
pub struct BookGroup {
    /// Identifiant de ligne : celui du livre s'il est seul, sinon `group:<chemin du fichier principal>`.
    pub id: String,
    pub books: Vec<Book>,
}

impl BookGroup {
    pub fn is_group(&self) -> bool {
        self.books.len() > 1
    }

    /// Fichier représentant le groupe : le plus directement lisible par la Kindle, complété par les informations
    /// des autres fichiers du livre.
    pub fn primary(&self) -> Book {
        let mut p = self.books.iter().min_by_key(|b| rank(b)).cloned().expect("groupe vide");
        for other in self.books.iter().filter(|b| b.path != p.path) {
            if p.series.is_none() && other.series.is_some() {
                p.series = other.series.clone();
                p.series_index = other.series_index;
            }
            if p.series_index.is_none() && p.series == other.series {
                p.series_index = other.series_index;
            }
            p.isbn = p.isbn.or_else(|| other.isbn.clone());
            p.publisher = p.publisher.or_else(|| other.publisher.clone());
            p.published_date = p.published_date.or_else(|| other.published_date.clone());
            p.language = p.language.or_else(|| other.language.clone());
            p.summary = p.summary.or_else(|| other.summary.clone());
            p.genre = p.genre.or_else(|| other.genre.clone());
        }
        p
    }

    /// Formats distincts, dans l'ordre de préférence.
    pub fn formats(&self) -> Vec<BookFormat> {
        let mut sorted: Vec<&Book> = self.books.iter().collect();
        sorted.sort_by_key(|b| rank(b));
        let mut seen = HashSet::new();
        sorted.into_iter().map(|b| b.format).filter(|f| seen.insert(*f)).collect()
    }
}

/// Clé de regroupement : titre (casse, accents, ponctuation et mention de genre ignorés) + auteurs.
pub fn key(book: &Book) -> String {
    let mut authors: Vec<String> = author_keys(book).into_iter().collect();
    authors.sort();
    format!("{}|{}", names::key(&without_genre(&book.title)), authors.join(";"))
}

/// Regroupe en conservant l'ordre de la liste. `identities` (voir `identities`) rapproche aussi les fichiers
/// d'un même livre dont les auteurs sont écrits différemment ou manquent.
pub fn make(books: &[Book], identities: Option<&HashMap<String, String>>) -> Vec<BookGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut members: HashMap<String, Vec<Book>> = HashMap::new();
    for b in books {
        let k = identities.and_then(|i| i.get(&b.path).cloned()).unwrap_or_else(|| key(b));
        if !members.contains_key(&k) {
            order.push(k.clone());
        }
        members.entry(k).or_default().push(b.clone());
    }
    order
        .into_iter()
        .map(|k| {
            let list = members.remove(&k).unwrap_or_default();
            if list.len() == 1 {
                return BookGroup { id: list[0].path.clone(), books: list };
            }
            let primary = list.iter().min_by_key(|b| rank(b)).map(|b| b.path.clone()).unwrap_or_default();
            BookGroup { id: format!("{GROUP_PREFIX}{primary}"), books: list }
        })
        .collect()
}

pub fn is_group_id(id: &str) -> bool {
    id.starts_with(GROUP_PREFIX)
}

/// Identité de chaque fichier : les fichiers d'un même titre sont réunis quand leurs auteurs concordent, quand ils
/// sont rangés dans le même dossier de livre, ou quand l'auteur manque et qu'un seul auteur connu existe pour ce titre.
pub fn identities(books: &[Book]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut by_title: HashMap<String, Vec<&Book>> = HashMap::new();
    for b in books {
        by_title.entry(names::key(&without_genre(&b.title))).or_default().push(b);
    }
    for (title, files) in by_title {
        let mut strict: HashMap<String, Vec<&Book>> = HashMap::new();
        for b in &files {
            strict.entry(key(b)).or_default().push(b);
        }
        let mut clusters: Vec<Vec<&Book>> = strict.into_values().collect();
        clusters.sort_by_key(|c| c.iter().map(|b| b.path.clone()).min());
        loop {
            let mut merged = false;
            'search: for i in 0..clusters.len() {
                for j in i + 1..clusters.len() {
                    if related(&clusters[i], &clusters[j], &title) {
                        let moved = clusters.remove(j);
                        clusters[i].extend(moved);
                        merged = true;
                        break 'search;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        // Fichiers sans auteur fiable : rattachés au seul livre de ce titre qui en a un, ou réunis entre eux.
        let (reliable, unreliable): (Vec<_>, Vec<_>) =
            clusters.into_iter().partition(|c| c.iter().any(|b| has_reliable_author(b, &title)));
        let clusters = if !unreliable.is_empty() && reliable.len() <= 1 {
            let mut all: Vec<&Book> = reliable.into_iter().flatten().collect();
            all.extend(unreliable.into_iter().flatten());
            vec![all]
        } else {
            reliable.into_iter().chain(unreliable).collect()
        };
        for cluster in clusters {
            let reliable_keys: Vec<String> = cluster.iter().filter(|b| has_reliable_author(b, &title)).map(|b| key(b)).collect();
            let id = if reliable_keys.is_empty() { cluster.iter().map(|b| key(b)).min() } else { reliable_keys.into_iter().min() }
                .unwrap_or_default();
            for b in cluster {
                out.insert(b.path.clone(), id.clone());
            }
        }
    }
    out
}

fn related(a: &[&Book], b: &[&Book], title: &str) -> bool {
    let dirs: HashSet<String> = a.iter().filter_map(|x| book_folder(x)).collect();
    if b.iter().any(|x| book_folder(x).is_some_and(|d| dirs.contains(&d))) {
        return true;
    }
    let words_a: Vec<HashSet<String>> = a.iter().filter(|x| has_reliable_author(x, title)).flat_map(|x| author_word_sets(x)).collect();
    let words_b: Vec<HashSet<String>> = b.iter().filter(|x| has_reliable_author(x, title)).flat_map(|x| author_word_sets(x)).collect();
    words_a.iter().any(|x| words_b.iter().any(|y| x.is_subset(y) || y.is_subset(x)))
}

static BOOK_FOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" \(\d+\)$").unwrap());

/// Dossier propre à un livre (nom terminé par « (numéro) », à la Calibre) ; None pour un dossier ordinaire.
fn book_folder(book: &Book) -> Option<String> {
    let dir = Path::new(&book.path).parent()?;
    let name = dir.file_name()?.to_str()?;
    BOOK_FOLDER.is_match(name).then(|| dir.to_string_lossy().into_owned())
}

/// Mots significatifs de chaque auteur (sans dates ni initiales).
fn author_word_sets(book: &Book) -> Vec<HashSet<String>> {
    book.authors
        .iter()
        .flat_map(|a| individuals(a))
        .map(|name| {
            author_key(&name)
                .split(' ')
                .filter(|w| w.chars().count() > 1 && !w.chars().all(|c| c.is_ascii_digit()))
                .map(String::from)
                .collect::<HashSet<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

const UNKNOWN_AUTHORS: &[&str] = &["unknown", "inconnu", "inconnu e", "inconnue", "anonyme", "anonymous", "auteur inconnu"];

/// Faux si l'auteur manque ou n'en est pas un : « Unknown », titre recopié, nom de fichier téléchargé.
fn has_reliable_author(book: &Book, title: &str) -> bool {
    book.authors.iter().any(|author| {
        let k = names::key(author);
        if k.is_empty() || UNKNOWN_AUTHORS.contains(&k.as_str()) || k.contains("z lib") {
            return false;
        }
        !(k.chars().count() > 8 && (k.contains(title) || title.contains(&k)))
    })
}

/// Fichiers en double : pour un même livre et un même format, un seul est gardé, les autres renvoyés ici (pour
/// être masqués, jamais supprimés). Gardé : le contenu le plus récent, puis celui du dossier source plutôt qu'un
/// import, le moins profond, puis le plus récent.
pub fn duplicates(books: &[Book], identities: Option<&HashMap<String, String>>) -> HashSet<String> {
    let computed;
    let ids = match identities {
        Some(i) => i,
        None => {
            computed = self::identities(books);
            &computed
        }
    };
    let mut same: HashMap<String, Vec<&Book>> = HashMap::new();
    for b in books {
        let k = format!("{}|{}", ids.get(&b.path).cloned().unwrap_or_else(|| key(b)), b.format.extension());
        same.entry(k).or_default().push(b);
    }
    let mut out = HashSet::new();
    for files in same.values().filter(|f| f.len() > 1) {
        let mut content_date: HashMap<&str, f64> = HashMap::new();
        for f in files {
            let d = content_date.entry(f.fingerprint.as_str()).or_insert(f.modified_at);
            *d = d.max(f.modified_at);
        }
        let depth = |b: &Book| b.path.replace('\\', "/").split('/').count();
        let kept = files
            .iter()
            .min_by(|a, b| {
                let (ca, cb) = (content_date[a.fingerprint.as_str()], content_date[b.fingerprint.as_str()]);
                cb.total_cmp(&ca)
                    .then(a.is_imported.cmp(&b.is_imported))
                    .then(depth(a).cmp(&depth(b)))
                    .then(b.modified_at.total_cmp(&a.modified_at))
                    .then(a.path.cmp(&b.path))
            })
            .map(|b| b.path.clone())
            .unwrap_or_default();
        out.extend(files.iter().filter(|f| f.path != kept).map(|f| f.path.clone()));
    }
    out
}

/// Préférence d'envoi à la Kindle.
pub fn rank(book: &Book) -> u8 {
    match book.format {
        BookFormat::Azw3 => 0,
        BookFormat::Mobi => 1,
        BookFormat::Azw => 2,
        BookFormat::Epub => 3,
        BookFormat::Pdf => 4,
    }
}
