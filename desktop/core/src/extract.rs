//! Point d'entrée unique : informations d'un fichier selon son format (`MetadataExtractor`).

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

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
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if meta.title.trim().is_empty() {
        // PDF sans informations : titre et auteur lus dans le nom du fichier (« Titre - Auteur »).
        if format == BookFormat::Pdf && meta.authors.is_empty() {
            let (title, author) = title_and_author(stem);
            meta.title = title;
            meta.authors = author.into_iter().collect();
        } else {
            meta.title = stem.to_string();
        }
    }
    meta.summary = meta.summary.as_deref().map(html::strip_html).filter(|s| !s.is_empty());
    meta.language = meta.language.as_deref().map(language_code).filter(|l| !l.is_empty());
    // « 2003-11-01T05:00:00+00:00 » → « 2003-11-01 » : l'heure n'a pas de sens pour une publication.
    if let Some(d) = &meta.published_date
        && d.find('T') == Some(10)
    {
        meta.published_date = Some(d[..10].to_string());
    }
    Ok(meta)
}

/// Mentions ajoutées par les sites de téléchargement : « (z-library.sk, 1lib.sk, z-lib.sk) », « (libgen.rs) ».
static SOURCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s*[(\[][^)\]]*(z-?lib|1lib|libgen|annas?-archive|epubs?\.\w+)[^)\]]*[)\]]").unwrap());
/// Auteur entre parenthèses à la fin : « Astérix et Cléopâtre (René Goscinny) ».
static TRAILING_PARENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(.*?)[\s\-–—]*\(([^()]+)\)\s*$").unwrap());

/// Titre et auteur d'après un nom de fichier : « Titre - Auteur » (si la fin ressemble à un nom de personne) ou
/// « Titre (Auteur) » ; sinon le nom nettoyé, sans auteur.
pub fn title_and_author(stem: &str) -> (String, Option<String>) {
    let cleaned = SOURCE.replace_all(stem, "").replace('_', " ");
    let cleaned = cleaned.trim();
    let tidy = |s: &str| s.trim().trim_end_matches(['-', '–', '—', ',']).trim().to_string();
    if let Some(c) = TRAILING_PARENS.captures(cleaned)
        && looks_like_name(&c[2])
        && !c[1].trim().is_empty()
    {
        return (tidy(&c[1]), Some(c[2].trim().to_string()));
    }
    for sep in [" - ", " – ", " — "] {
        if let Some(i) = cleaned.rfind(sep) {
            let (title, author) = (&cleaned[..i], &cleaned[i + sep.len()..]);
            if looks_like_name(author) && !title.trim().is_empty() {
                return (tidy(title), Some(author.trim().to_string()));
            }
        }
    }
    (tidy(cleaned), None)
}

/// 2 à 4 mots commençant par une majuscule, sans chiffre (« Anna Funder », « Jean-Paul Sartre »).
fn looks_like_name(s: &str) -> bool {
    let words: Vec<&str> = s.split_whitespace().collect();
    (2..=4).contains(&words.len())
        && !s.chars().any(|c| c.is_ascii_digit())
        && words.iter().all(|w| {
            let w = w.trim_matches(|c: char| !c.is_alphabetic());
            w.chars().next().is_some_and(char::is_uppercase) || ["de", "du", "van", "von", "da", "le", "la"].contains(&w)
        })
}

/// Code de langue sur 2 lettres : « fre », « fra », « French » → « fr » ; un code déjà court reste tel quel.
pub fn language_code(code: &str) -> String {
    let c = code.trim().to_lowercase();
    let primary = c.split(['-', '_']).next().unwrap_or_default();
    let mapped = match primary {
        "fre" | "fra" | "french" | "français" | "francais" => "fr",
        "eng" | "english" | "anglais" => "en",
        "ger" | "deu" | "german" | "deutsch" | "allemand" => "de",
        "spa" | "spanish" | "español" | "espagnol" => "es",
        "ita" | "italian" | "italien" => "it",
        "dut" | "nld" | "dutch" | "nederlands" | "néerlandais" => "nl",
        "por" | "portuguese" => "pt",
        "rus" => "ru",
        "jpn" => "ja",
        "chi" | "zho" => "zh",
        "lat" => "la",
        "ara" => "ar",
        "pol" => "pl",
        "swe" => "sv",
        _ => return code.trim().to_string(),
    };
    mapped.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_from_file_names() {
        assert_eq!(title_and_author("L'invisible madame Orwell - Anna Funder"), ("L'invisible madame Orwell".into(), Some("Anna Funder".into())));
        assert_eq!(
            title_and_author("2005 - Astérix et Cléopâtre- (René Goscinny) (z-library.sk, 1lib.sk, z-lib.sk)"),
            ("2005 - Astérix et Cléopâtre".into(), Some("René Goscinny".into()))
        );
        assert_eq!(title_and_author("Code civil (Mise à jour du 2026-07-29)"), ("Code civil (Mise à jour du 2026-07-29)".into(), None));
        assert_eq!(title_and_author("Manuel - tome 2"), ("Manuel - tome 2".into(), None));
    }

    #[test]
    fn languages() {
        assert_eq!(language_code("fre"), "fr");
        assert_eq!(language_code("fr-FR"), "fr-FR");
        assert_eq!(language_code("English"), "en");
        assert_eq!(language_code("de"), "de");
    }
}
