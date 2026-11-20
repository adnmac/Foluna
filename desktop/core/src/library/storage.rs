//! Ménage du dossier de l'app (`StorageMaintenance`) et vidage de la corbeille (`LibraryTrash`).

use std::collections::HashSet;
use std::path::Path;

use crate::library::book::Book;
use crate::library::dirs::{AppDirectories, CoverStore};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    pub removed_files: usize,
    pub freed_bytes: u64,
}

/// Supprime les couvertures des livres disparus, et les conversions qui ne sont pas dans `keep_conversions`
/// (noms de fichiers ; None : les conversions ne sont pas touchées).
pub fn purge(keeping: &[Book], dirs: &AppDirectories, keep_conversions: Option<&HashSet<String>>) -> Report {
    let covers = CoverStore::new(dirs.covers());
    let keep_covers: HashSet<String> =
        keeping.iter().filter_map(|b| covers.path(&b.fingerprint).file_name().map(|n| n.to_string_lossy().into_owned())).collect();
    let mut report = Report::default();
    let mut sweep = |dir: &Path, keep: &HashSet<String>| {
        for (path, size) in files(dir) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !keep.contains(&name) && std::fs::remove_file(&path).is_ok() {
                report.removed_files += 1;
                report.freed_bytes += size;
            }
        }
    };
    sweep(&dirs.covers(), &keep_covers);
    if let Some(keep) = keep_conversions {
        sweep(&dirs.conversions(), keep);
    }
    report
}

/// Taille totale des fichiers d'un dossier (non récursif : les dossiers de l'app sont plats).
pub fn size(dir: &Path) -> u64 {
    files(dir).iter().map(|(_, s)| s).sum()
}

/// Vide un dossier de cache ; son contenu est recréé à la demande.
pub fn clear(dir: &Path) {
    for (path, _) in files(dir) {
        let _ = std::fs::remove_file(path);
    }
}

fn files(dir: &Path) -> Vec<(std::path::PathBuf, u64)> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| (e.path(), meta.len()))
        })
        .collect()
}

/// Résultat du vidage de la corbeille de Foluna.
#[derive(Debug, Default)]
pub struct TrashResult {
    /// Fichiers placés dans la Corbeille du système (ou déjà absents du disque).
    pub removed: Vec<String>,
    /// Fichiers restés en place, avec la raison.
    pub failures: Vec<(String, String)>,
}

/// Place les fichiers dans la Corbeille de Windows (ou du système), où ils restent récupérables.
pub fn empty_trash(books: &[Book], move_to_trash: &dyn Fn(&Path) -> Result<(), String>) -> TrashResult {
    let mut result = TrashResult::default();
    for book in books {
        let path = book.file_path();
        if !path.exists() {
            result.removed.push(book.path.clone());
            continue;
        }
        match move_to_trash(&path) {
            Ok(()) => result.removed.push(book.path.clone()),
            Err(e) => result.failures.push((book.path.clone(), e)),
        }
    }
    result
}

/// Corbeille du système.
pub fn system_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}
