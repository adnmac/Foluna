//! Dossiers de livres de la bibliothèque (`LibraryFolders`) : liste enregistrée, dossier d'un livre, ajouts refusés.
//! Les dossiers sont lus, jamais modifiés.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Raison d'un ajout refusé : le dossier est déjà lu, en entier ou en partie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    Same,
    /// À l'intérieur d'un dossier de la liste (chemin de ce dossier).
    Inside(String),
    /// Contient un dossier de la liste (chemin de ce dossier).
    Contains(String),
}

impl Conflict {
    pub fn message(&self) -> String {
        match self {
            Conflict::Same => "Ce dossier fait déjà partie de la bibliothèque.".into(),
            Conflict::Inside(parent) => format!("Ce dossier est déjà lu avec « {} ».", name(parent)),
            Conflict::Contains(child) => {
                format!("Ce dossier contient « {} », déjà dans la bibliothèque. Retirez-le d'abord.", name(child))
            }
        }
    }
}

/// Chemin normalisé terminé par un séparateur, pour qu'un dossier « Livres » ne contienne pas « Livres anciens ».
/// Sous Windows, la casse est ignorée (« C:\Livres » = « c:\livres »).
pub fn prefix(path: &str) -> String {
    let mut p = path.replace('\\', "/");
    while p.len() > 1 && p.ends_with('/') {
        p.pop();
    }
    if cfg!(windows) {
        p = p.to_lowercase();
    }
    // La racine (« / », « c:/ ») finit déjà par un séparateur.
    if p.ends_with('/') { p } else { format!("{p}/") }
}

fn normalized(path: &str) -> String {
    let p = path.replace('\\', "/");
    if cfg!(windows) { p.to_lowercase() } else { p }
}

/// Nom affiché d'un dossier (son dernier élément).
pub fn name(path: &str) -> String {
    Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string()
}

/// Dossier de la liste qui contient ce fichier (le plus précis), ou None (import, dossier retiré).
pub fn owner<'a>(path: &str, folders: &'a [String]) -> Option<&'a String> {
    let p = normalized(path);
    folders.iter().filter(|f| p.starts_with(&prefix(f))).max_by_key(|f| f.len())
}

/// Pourquoi `path` ne peut pas rejoindre `folders`, ou None s'il le peut.
pub fn conflict(path: &str, folders: &[String]) -> Option<Conflict> {
    let new = prefix(path);
    for folder in folders {
        let existing = prefix(folder);
        if new == existing {
            return Some(Conflict::Same);
        }
        if new.starts_with(&existing) {
            return Some(Conflict::Inside(folder.clone()));
        }
        if existing.starts_with(&new) {
            return Some(Conflict::Contains(folder.clone()));
        }
    }
    None
}

/// Un dossier de livres enregistré (`folders.json`) : son chemin, affiché même quand son disque est débranché.
/// Le champ `bookmark` de l'app Mac est lu et gardé s'il existe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<serde_json::Value>,
}

impl Entry {
    pub fn new(path: &Path) -> Self {
        Entry { path: path.to_string_lossy().into_owned(), bookmark: None }
    }

    pub fn path_buf(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

pub fn load(path: &Path) -> Vec<Entry> {
    std::fs::read(path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

pub fn save(entries: &[Entry], path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(entries).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicts() {
        let folders = vec!["/Livres".to_string()];
        assert_eq!(conflict("/Livres/", &folders), Some(Conflict::Same));
        assert_eq!(conflict("/Livres/Policiers", &folders), Some(Conflict::Inside("/Livres".into())));
        assert_eq!(conflict("/", &folders), Some(Conflict::Contains("/Livres".into())));
        assert_eq!(conflict("/Livres anciens", &folders), None);
        assert_eq!(owner("/Livres/a.epub", &folders), Some(&folders[0]));
        assert_eq!(owner("/Livres anciens/a.epub", &folders), None);
    }
}
