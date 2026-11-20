//! Liseuses connues (Kindle, Kobo, liseuse USB) : reconnues par leur identifiant, nommées par l'utilisateur
//! (`Readers.swift`, `readers.json`, mêmes clés que l'app Mac).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::device::brand::ReaderBrand;
use crate::library::book::{now, Timestamp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reader {
    /// Identifiant de l'appareil (`Device::identifier`).
    pub id: String,
    pub name: String,
    pub first_seen_at: Timestamp,
    pub last_seen_at: Timestamp,
    /// Marque ; absente dans les `readers.json` d'avant les Kobo : une Kindle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brand: Option<ReaderBrand>,
    /// Dossier des livres d'une liseuse générique (relatif à la racine du disque ; vide : la racine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub books_folder: Option<String>,
}

impl Reader {
    pub fn new(id: impl Into<String>, name: impl Into<String>, brand: ReaderBrand, books_folder: Option<String>) -> Self {
        let t = now();
        Reader { id: id.into(), name: name.into(), first_seen_at: t, last_seen_at: t, brand: Some(brand), books_folder }
    }

    pub fn brand(&self) -> ReaderBrand {
        self.brand.unwrap_or(ReaderBrand::Kindle)
    }

    pub fn set_brand(&mut self, brand: ReaderBrand) {
        self.brand = Some(brand);
    }
}

/// Liste vide si le fichier n'existe pas ; erreur s'il est illisible (jamais écrasé par une liste vide).
pub fn load(path: &Path) -> Result<Vec<Reader>, String> {
    match std::fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).map_err(|e| format!("Liste des liseuses illisible : {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("Liste des liseuses illisible : {e}")),
    }
}

pub fn save(readers: &[Reader], path: &Path) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(readers).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| format!("Impossible d'enregistrer les liseuses : {e}"))
}
