//! Lecture d'une archive ZIP par chemin interne (`Container/ZipIO.swift`).

use std::cell::RefCell;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::error::{BookError, Result};

pub struct ZipReader {
    archive: RefCell<ZipArchive<File>>,
    names: Vec<String>,
}

impl ZipReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let archive = ZipArchive::new(file).map_err(|_| BookError::Unreadable("archive ZIP invalide".into()))?;
        let names = archive.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).collect();
        Ok(ZipReader { archive: RefCell::new(archive), names })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn contains(&self, path: &str) -> bool {
        self.names.iter().any(|n| n == path)
    }

    /// Contenu d'une entrée ; à défaut du nom exact, sans tenir compte de la casse.
    pub fn data(&self, path: &str) -> Result<Vec<u8>> {
        let name = if self.contains(path) {
            path.to_string()
        } else {
            let lower = path.to_lowercase();
            self.names
                .iter()
                .find(|n| n.to_lowercase() == lower)
                .cloned()
                .ok_or_else(|| BookError::Unreadable(format!("fichier absent de l'archive : {path}")))?
        };
        let mut archive = self.archive.borrow_mut();
        let mut entry =
            archive.by_name(&name).map_err(|_| BookError::Unreadable(format!("fichier absent de l'archive : {path}")))?;
        let mut out = Vec::with_capacity(entry.size().min(1 << 26) as usize);
        entry.read_to_end(&mut out).map_err(|e| BookError::Unreadable(e.to_string()))?;
        Ok(out)
    }
}
