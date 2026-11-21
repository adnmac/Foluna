//! Comparaison des noms : clés sans accents (`Matching`), auteurs (`AuthorNames`), séries (`SeriesNames`),
//! genres (`GenreNames`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::library::book::Book;
use crate::library::groups;

/// Minuscules, sans accents ni ponctuation, mots séparés par une espace.
pub fn key(s: &str) -> String {
    let folded: String = s.nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect::<String>().to_lowercase();
    folded.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Nom comparable quel que soit l'ordre (« Heller, Peter » = « Peter Heller »).
pub fn person_key(name: &str) -> String {
    let k = key(name);
    let mut words: Vec<&str> = k.split(' ').filter(|w| !w.is_empty()).collect();
    words.sort_unstable();
    words.join(" ")
}

static GENRE: LazyLock<Regex> = LazyLock::new(|| {
    let genres = [
        "romans?", "nouvelles?", "récits?", "contes?", "essais?", "poèmes?", "poésies?", "théâtre", "pièces?", "comédie", "tragédie",
        "drame", "fables?", "chroniques?", "thriller", "polar", "témoignage", "biographie", "autobiographie", "mémoires", "journal",
        "document", "bande dessinée", "roman graphique", "manga",
    ];
    let qualifiers = [
        "policiers?", "policières?", "historiques?", "noirs?", "fantastiques?", "psychologiques?", "graphiques?", "d'aventures?",
        "d'amour", "inédite?s?", "complète?s?", "choisie?s?",
    ];
    Regex::new(&format!(r"(?i)\s*:\s*(?:{})(?:\s+(?:{}))?\s*\.?$", genres.join("|"), qualifiers.join("|"))).unwrap()
});

/// Titre sans mention de genre ajoutée par la BnF (« … : roman »).
pub fn without_genre(title: &str) -> String {
    match GENRE.find(title) {
        Some(m) => title[..m.start()].trim().to_string(),
        None => title.trim().to_string(),
    }
}

/// Ordre « naturel » des noms affichés, sans tenir compte de la casse ni des accents.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    key(a).cmp(&key(b)).then_with(|| a.cmp(b))
}

// Auteurs

/// Clé d'auteur : indépendante de la casse, des accents, de l'ordre prénom/nom et des lettres doublées.
pub fn author_key(name: &str) -> String {
    let mut out = String::new();
    for c in person_key(name).chars() {
        if !(c.is_alphabetic() && out.ends_with(c)) {
            out.push(c);
        }
    }
    out
}

/// Sépare un champ qui cite plusieurs personnes : « A B & C D », « Edwy Plenel, Lisa Fittko ».
/// « Nom, Prénom » (un seul mot de part et d'autre de la virgule) reste une seule personne.
pub fn individuals(author: &str) -> Vec<String> {
    author
        .split(" & ")
        .flat_map(|part| {
            let pieces: Vec<String> = part.split(", ").map(|p| p.trim().to_string()).collect();
            let all_full = pieces.len() > 1 && pieces.iter().all(|p| p.split(' ').filter(|w| !w.is_empty()).count() >= 2);
            if all_full { pieces } else { vec![part.trim().to_string()] }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn author_keys(book: &Book) -> HashSet<String> {
    book.authors.iter().flat_map(|a| individuals(a)).map(|a| author_key(&a)).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct NameGroup {
    pub key: String,
    /// Forme affichée.
    pub name: String,
    pub variants: Vec<String>,
    pub book_count: usize,
}

pub fn author_groups(books: &[Book]) -> Vec<NameGroup> {
    let mut variants: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for book in books {
        let mut seen = HashSet::new();
        for author in book.authors.iter().flat_map(|a| individuals(a)) {
            let k = author_key(&author);
            if k.is_empty() {
                continue;
            }
            *variants.entry(k.clone()).or_default().entry(author).or_default() += 1;
            if seen.insert(k.clone()) {
                *counts.entry(k).or_default() += 1;
            }
        }
    }
    let mut out: Vec<NameGroup> = variants
        .into_iter()
        .map(|(k, v)| {
            let mut names: Vec<String> = v.keys().cloned().collect();
            names.sort();
            NameGroup { name: display_name(&v), variants: names, book_count: counts.get(&k).copied().unwrap_or(0), key: k }
        })
        .collect();
    out.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    out
}

/// Nom affiché de chaque auteur (clé de `author_key` → nom de la barre latérale).
pub fn author_names(books: &[Book]) -> HashMap<String, String> {
    author_groups(books).into_iter().map(|g| (g.key, g.name)).collect()
}

/// Auteurs d'un livre tels que la barre latérale les nomme, chacun une seule fois : « Bauer Alain » s'affiche
/// « Alain Bauer » si c'est le nom retenu pour le groupe. Vide si le livre n'a pas d'auteur.
pub fn author_line(authors: &[String], names: &HashMap<String, String>) -> String {
    let mut seen = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for author in authors.iter().flat_map(|a| individuals(a)) {
        let k = author_key(&author);
        if !k.is_empty() && seen.insert(k.clone()) {
            out.push(names.get(&k).cloned().unwrap_or(author));
        }
    }
    out.join(", ")
}

/// « Nom, Prénom » indique le nom de famille sans ambiguïté : retenu (et remis dans l'ordre). Sinon : sans
/// parenthèses ni MAJUSCULES, puis la plus fréquente.
fn display_name(variants: &HashMap<String, usize>) -> String {
    let with_comma: Vec<&String> = variants.keys().filter(|v| v.contains(',')).collect();
    let pool: Vec<&String> = if with_comma.is_empty() { variants.keys().collect() } else { with_comma };
    let score = |v: &str| -> (i32, usize) {
        let mut s = 0;
        if !v.contains('(') {
            s += 2;
        }
        let shouting = v
            .split([' ', ','])
            .any(|w| w.chars().count() > 1 && w == w.to_uppercase() && w.chars().any(char::is_alphabetic));
        if !shouting {
            s += 1;
        }
        if !v.contains(',') || v.contains(", ") {
            s += 1;
        }
        (s, variants.get(v).copied().unwrap_or(0))
    };
    let best = pool
        .into_iter()
        .max_by(|a, b| score(a).cmp(&score(b)).then_with(|| b.cmp(a)))
        .cloned()
        .unwrap_or_default();
    readable(&best)
}

/// « Murakami,Haruki » → « Haruki Murakami » ; « HARARI » → « Harari ».
pub fn readable(name: &str) -> String {
    let mut n = name.trim_matches(|c: char| c.is_whitespace() || c == '.').to_string();
    let parts: Vec<String> = n.splitn(2, ',').map(|p| p.trim().to_string()).collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() && (!parts[1].contains(' ') || !parts[0].contains(' ')) {
        n = format!("{} {}", parts[1], parts[0]);
    }
    n.split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            if w.chars().count() > 1 && w == w.to_uppercase() && w.chars().any(char::is_alphabetic) {
                w.to_lowercase()
                    .split('-')
                    .map(|p| {
                        let mut c = p.chars();
                        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join("-")
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// Séries

const GENERIC_WORDS: &[&str] = &[
    "commissaire", "inspecteur", "inspectrice", "capitaine", "detective", "enquete", "enquetes", "les", "le", "la", "l", "de", "du",
    "des", "d", "une", "un", "the", "of", "a", "saga", "serie", "cycle", "tome", "tomes", "volume", "vol", "n", "no",
];

static SERIES_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.*?\S)(?:[\s,]*(?:#|n°|no\.|tome|t\.|vol\.?)\s*|[\s,]+)(\d+(?:[.,]\d+)?)$").unwrap()
});

/// « Commissaire Brunetti #32 » → (« Commissaire Brunetti », 32) ; « Brunetti 8 » → (« Brunetti », 8).
pub fn split_series(series: &str) -> (String, Option<f64>) {
    let s = series.trim();
    if let Some(c) = SERIES_NUMBER.captures(s)
        && let Ok(n) = c[2].replace(',', ".").parse::<f64>()
            && n < 1000.0 {
                return (c[1].to_string(), Some(n));
            }
    (s.to_string(), None)
}

/// Nom et numéro rangés (le numéro collé au nom est séparé si le numéro manque).
pub fn normalized_series(series: Option<&str>, index: Option<f64>) -> (Option<String>, Option<f64>) {
    let Some(series) = series.filter(|s| !s.trim().is_empty()) else { return (None, index) };
    let (name, n) = split_series(series);
    match (n, index) {
        (Some(n), None) => (Some(name), Some(n)),
        (_, None) => (Some(series.to_string()), None),
        (_, Some(i)) => (Some(name), Some(i)),
    }
}

/// Clé de regroupement d'un nom de série (« Commissaire Brunetti » = « Brunetti »).
pub fn series_key(series: &str) -> String {
    let k = key(&split_series(series).0);
    let words: Vec<&str> = k.split(' ').filter(|w| !w.is_empty()).collect();
    let significant: Vec<&str> = words.iter().copied().filter(|w| !GENERIC_WORDS.contains(w)).collect();
    if significant.is_empty() { words.join(" ") } else { significant.join(" ") }
}

pub fn series_groups(books: &[Book]) -> Vec<NameGroup> {
    let mut variants: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut titles: HashMap<String, HashSet<String>> = HashMap::new();
    for b in books {
        let Some(s) = b.series.as_deref().filter(|s| !s.is_empty()) else { continue };
        let name = split_series(s).0;
        let k = series_key(&name);
        *variants.entry(k.clone()).or_default().entry(name).or_default() += 1;
        titles.entry(k).or_default().insert(groups::key(b));
    }
    let mut out: Vec<NameGroup> = variants
        .into_iter()
        .map(|(k, v)| {
            // La forme la plus fréquente ; à égalité, la plus courte.
            let name = v
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.chars().count().cmp(&a.0.chars().count())))
                .map(|(n, _)| n.clone())
                .unwrap_or_default();
            let mut names: Vec<String> = v.keys().cloned().collect();
            names.sort();
            NameGroup { name, variants: names, book_count: titles.get(&k).map_or(0, HashSet::len), key: k }
        })
        .collect();
    out.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    out
}

/// Numéro affichable : « 8 », « 2.5 ».
pub fn format_series_index(index: f64) -> String {
    crate::epub::writer::format_index(index)
}

// Genres

pub fn genre_groups(books: &[Book], identities: &HashMap<String, String>) -> Vec<NameGroup> {
    let mut spellings: BTreeMap<String, HashMap<String, usize>> = BTreeMap::new();
    let mut members: HashMap<String, HashSet<String>> = HashMap::new();
    for b in books {
        let Some(genre) = b.genre.as_deref().map(str::trim).filter(|g| !g.is_empty()) else { continue };
        let k = key(genre);
        *spellings.entry(k.clone()).or_default().entry(genre.to_string()).or_default() += 1;
        members.entry(k).or_default().insert(identities.get(b.id()).cloned().unwrap_or_else(|| groups::key(b)));
    }
    let mut out: Vec<NameGroup> = spellings
        .into_iter()
        .map(|(k, v)| {
            let name = v.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(n, _)| n.clone()).unwrap_or_default();
            NameGroup { name: name.clone(), variants: vec![name], book_count: members.get(&k).map_or(0, HashSet::len), key: k }
        })
        .collect();
    out.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn author_line_uses_group_names() {
        use crate::library::book::Book;
        let book = |authors: &[&str]| -> Book {
            serde_json::from_value(serde_json::json!({
                "path": "/l.epub", "format": "epub", "fingerprint": "f", "fileSize": 1, "modifiedAt": 0.0, "addedAt": 0.0,
                "title": "T", "authors": authors, "tags": [], "hasCover": false, "isDRMProtected": false,
                "isImported": false, "asin": ""
            }))
            .unwrap()
        };
        let books = vec![book(&["Alain Bauer"]), book(&["Bauer Alain"]), book(&["Alain Bauer", "Bauer Alain"]), book(&["Giono Jean"])];
        let names = super::author_names(&books);
        assert_eq!(super::author_line(&books[1].authors, &names), "Alain Bauer");
        assert_eq!(super::author_line(&books[2].authors, &names), "Alain Bauer", "un seul nom pour deux graphies");
        assert_eq!(super::author_line(&books[3].authors, &names), "Giono Jean", "un seul sens : inchangé");
        assert_eq!(super::author_line(&[], &names), "");
        assert_eq!(super::author_line(&["Inconnu Ailleurs".into()], &names), "Inconnu Ailleurs");
    }

    use super::*;

    #[test]
    fn keys() {
        assert_eq!(key("L'Été — À l'œil!"), "l ete a l œil");
        assert_eq!(author_key("Murakami, Haruki"), author_key("Haruki Murakami"));
        assert_eq!(author_key("Banks Russel"), author_key("Russell Banks"));
        assert_eq!(without_genre("Brunetti et le mauvais augure : roman policier"), "Brunetti et le mauvais augure");
        assert_eq!(without_genre("Le Fleuve : une vie"), "Le Fleuve : une vie");
    }

    #[test]
    fn authors() {
        assert_eq!(individuals("Edwy Plenel, Lisa Fittko"), ["Edwy Plenel", "Lisa Fittko"]);
        assert_eq!(individuals("Murakami, Haruki"), ["Murakami, Haruki"]);
        assert_eq!(individuals("A B & C D"), ["A B", "C D"]);
        assert_eq!(readable("Murakami,Haruki"), "Haruki Murakami");
        assert_eq!(readable("HARARI"), "Harari");
    }

    #[test]
    fn series() {
        assert_eq!(split_series("Commissaire Brunetti #32"), ("Commissaire Brunetti".into(), Some(32.0)));
        assert_eq!(split_series("Brunetti 8"), ("Brunetti".into(), Some(8.0)));
        assert_eq!(normalized_series(Some("Brunetti 8"), None), (Some("Brunetti".into()), Some(8.0)));
        assert_eq!(series_key("Commissaire Brunetti"), series_key("Brunetti"));
    }
}
