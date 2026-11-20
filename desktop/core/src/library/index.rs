//! Index de la bibliothèque en JSON (`LibraryIndex`), lisible par l'app Mac 0.6.0 et l'inverse.
//! Un livre d'un format que cette version ne connaît pas est gardé tel quel et réécrit à chaque enregistrement.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::library::book::Book;

/// Contenu de l'index : livres connus, et entrées d'un format inconnu recopiées sans les comprendre.
#[derive(Debug, Default, Clone)]
pub struct IndexContent {
    pub books: Vec<Book>,
    pub others: Vec<Value>,
}

#[derive(Debug)]
pub enum IndexError {
    /// Fichier présent mais illisible : il ne doit jamais être écrasé par une liste vide.
    Unreadable(String),
    Io(std::io::Error),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::Unreadable(why) => write!(f, "index illisible ({why})"),
            IndexError::Io(e) => write!(f, "{e}"),
        }
    }
}

const KNOWN_FORMATS: [&str; 5] = ["epub", "mobi", "azw3", "azw", "pdf"];

/// Index absent : bibliothèque vide. Index illisible : erreur.
pub fn load(path: &Path) -> Result<IndexContent, IndexError> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(IndexContent::default()),
        Err(e) => return Err(IndexError::Io(e)),
    };
    let entries: Vec<Value> = serde_json::from_slice(&data).map_err(|e| IndexError::Unreadable(e.to_string()))?;
    let mut content = IndexContent::default();
    for entry in entries {
        let format = entry.get("format").and_then(Value::as_str).unwrap_or_default().to_string();
        if !format.is_empty() && !KNOWN_FORMATS.contains(&format.as_str()) {
            content.others.push(entry);
            continue;
        }
        let book: Book = serde_json::from_value(entry).map_err(|e| IndexError::Unreadable(e.to_string()))?;
        content.books.push(book);
    }
    Ok(content)
}

/// Écrit l'index (clés triées, comme le Mac), de façon atomique.
pub fn save(content: &IndexContent, path: &Path) -> std::io::Result<()> {
    let paths: std::collections::HashSet<&str> = content.books.iter().map(|b| b.path.as_str()).collect();
    let mut entries: Vec<Value> = content.books.iter().map(|b| serde_json::to_value(b).unwrap_or(Value::Null)).collect();
    entries.extend(content.others.iter().filter(|o| !o.get("path").and_then(Value::as_str).is_some_and(|p| paths.contains(p))).cloned());
    let json = serde_json::to_vec(&entries).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Met de côté un index illisible (`library.corrupt-AAAAMMJJ-HHMMSS.json`) et renvoie son nouvel emplacement.
pub fn quarantine(path: &Path) -> std::io::Result<PathBuf> {
    let stamp = crate::epub::writer::now_iso8601().replace(['-', ':'], "").replace('T', "-").trim_end_matches('Z').to_string();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("library");
    let target = path.with_file_name(format!("{stem}.corrupt-{stamp}.json"));
    std::fs::rename(path, &target)?;
    Ok(target)
}

/// Copie du jour dans `dir` (« library-AAAA-MM-JJ.json ») si elle n'existe pas ; les `keep` plus récentes gardées.
pub fn back_up_today(index: &Path, dir: &Path, keep: usize) -> std::io::Result<Option<PathBuf>> {
    if !index.exists() {
        return Ok(None);
    }
    std::fs::create_dir_all(dir)?;
    let day = &crate::epub::writer::now_iso8601()[..10];
    let target = dir.join(format!("library-{day}.json"));
    if target.exists() {
        return Ok(None);
    }
    std::fs::copy(index, &target)?;
    for old in list_backups(dir).into_iter().skip(keep) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(target))
}

/// Copies datées, de la plus récente à la plus ancienne.
pub fn list_backups(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let day = name.strip_prefix("library-")?.strip_suffix(".json")?.to_string();
            (day.len() == 10 && day.as_bytes()[4] == b'-' && day.as_bytes()[7] == b'-' && day.chars().filter(char::is_ascii_digit).count() == 8)
                .then(|| (day, e.path()))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, p)| p).collect()
}
