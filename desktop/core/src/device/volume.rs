//! Liseuse en mode disque USB : Kindle jusqu'aux modèles 2023, Kobo, liseuses USB (`VolumeKindle`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::device::brand::ReaderBrand;
use crate::device::platform::{self, Volume};
use crate::device::DeviceError;

/// Livre présent sur la liseuse.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Chemin complet sur le disque.
    pub id: String,
    /// Chemin relatif au dossier parcouru (« Downloads/Items01/livre.kfx »).
    pub path: String,
    pub size: u64,
}

impl Document {
    pub fn name(&self) -> String {
        Path::new(&self.path).file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
    }

    pub fn extension(&self) -> String {
        Path::new(&self.path).extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase()
    }
}

#[derive(Debug, Clone)]
pub struct VolumeDevice {
    pub volume: Volume,
    pub brand: ReaderBrand,
    /// Dossier des livres, relatif à la racine (« documents », « Books », ou celui choisi ; vide : la racine).
    pub books_folder: String,
}

fn thumbnail_name(asin: &str) -> String {
    format!("thumbnail_{asin}_EBOK_portrait.jpg")
}

impl VolumeDevice {
    pub fn new(volume: Volume, brand: ReaderBrand, books_folder: Option<String>) -> Self {
        let books_folder = books_folder.unwrap_or_else(|| brand.default_books_folder().to_string());
        VolumeDevice { volume, brand, books_folder }
    }

    pub fn root(&self) -> &Path {
        &self.volume.root
    }

    pub fn name(&self) -> String {
        if self.volume.label.is_empty() { self.brand.label().to_string() } else { self.volume.label.clone() }
    }

    /// Identifiant stable : numéro de série du volume ; à défaut, nom et capacité.
    pub fn identifier(&self) -> String {
        match &self.volume.serial {
            Some(s) if !s.is_empty() => format!("vol:{s}"),
            _ => format!("vol:{}-{}", self.volume.label, self.volume.total_bytes.unwrap_or(0)),
        }
    }

    /// Dossier où vont les livres envoyés.
    pub fn documents(&self) -> PathBuf {
        if self.books_folder.is_empty() { self.root().to_path_buf() } else { self.root().join(&self.books_folder) }
    }

    fn list_root(&self) -> PathBuf {
        let folder = self.brand.list_folder(&self.books_folder);
        if folder.is_empty() { self.root().to_path_buf() } else { self.root().join(folder) }
    }

    /// Marque reconnue d'après le contenu du disque : Kindle (`documents/` et `system/`), Kobo (`.kobo/`).
    /// Un autre disque n'est jamais pris pour une liseuse : seule une liseuse générique déjà ajoutée l'est.
    pub fn brand_of(root: &Path) -> Option<ReaderBrand> {
        if root.join(".kobo").is_dir() || root.join(".kobo/version").exists() {
            return Some(ReaderBrand::Kobo);
        }
        if root.join("documents").is_dir() && root.join("system").exists() {
            return Some(ReaderBrand::Kindle);
        }
        None
    }

    /// Liseuse d'un volume : Kindle ou Kobo reconnue, ou liseuse générique déjà ajoutée (identifiant → dossier).
    pub fn from_volume(volume: Volume, generic: &HashMap<String, String>) -> Option<VolumeDevice> {
        if let Some(brand) = Self::brand_of(&volume.root) {
            return Some(VolumeDevice::new(volume, brand, None));
        }
        let candidate = VolumeDevice::new(volume, ReaderBrand::Generic, None);
        let folder = generic.get(&candidate.identifier())?.clone();
        Some(VolumeDevice::new(candidate.volume, ReaderBrand::Generic, Some(folder)))
    }

    /// Liseuses branchées.
    pub fn detect(generic: &HashMap<String, String>) -> Vec<VolumeDevice> {
        platform::removable_volumes().into_iter().filter_map(|v| Self::from_volume(v, generic)).collect()
    }

    /// Disques amovibles qui pourraient être une liseuse générique : ni Kindle ni Kobo, pas encore ajoutés, avec
    /// au moins un livre lisible dans les premiers niveaux.
    pub fn candidates(known: &std::collections::HashSet<String>) -> Vec<VolumeDevice> {
        platform::removable_volumes()
            .into_iter()
            .filter(|v| Self::brand_of(&v.root).is_none())
            .map(|v| VolumeDevice::new(v, ReaderBrand::Generic, None))
            .filter(|d| !known.contains(&d.identifier()) && d.has_books())
            .collect()
    }

    fn has_books(&self) -> bool {
        fn walk(dir: &Path, depth: usize) -> bool {
            let Ok(entries) = std::fs::read_dir(dir) else { return false };
            entries.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    return false;
                }
                let path = e.path();
                if path.is_dir() { depth < 3 && walk(&path, depth + 1) } else { ReaderBrand::Generic.is_book(&name) }
            })
        }
        walk(self.root(), 0)
    }

    /// Dossier des livres proposé pour une liseuse générique : `Books`, `Documents`, `eBooks` s'il existe, sinon la racine.
    pub fn suggested_books_folder(&self) -> String {
        ["Books", "books", "Documents", "documents", "eBooks", "Livres"]
            .into_iter()
            .find(|n| self.root().join(n).is_dir())
            .unwrap_or_default()
            .to_string()
    }

    pub fn free_space(&self) -> Option<u64> {
        platform::free_space(self.root())
    }

    /// Démonte et éjecte le disque : la liseuse peut ensuite être débranchée.
    pub fn eject(&self) -> Result<(), DeviceError> {
        platform::eject(self.root()).map_err(DeviceError::EjectFailed)
    }

    /// Livres présents, parcourus récursivement (achats d'une Kindle dans `documents/Downloads/Items01/…`, livres
    /// d'une Kobo n'importe où), dossiers `.sdr` et dossiers système exclus.
    pub fn list_documents(&self) -> Result<Vec<Document>, DeviceError> {
        let base = self.list_root();
        if !base.exists() {
            return Err(DeviceError::NotConnected);
        }
        let mut docs = Vec::new();
        self.walk(&base, &base, &mut docs, 0);
        docs.sort_by(|a, b| crate::library::names::natural_cmp(&a.name(), &b.name()));
        Ok(docs)
    }

    fn walk(&self, base: &Path, dir: &Path, out: &mut Vec<Document>, depth: usize) {
        if depth > 12 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                let is_sdr = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("sdr"));
                if !name.starts_with('.') && !is_sdr && !self.brand.hidden_folders().contains(&name.as_str()) {
                    self.walk(base, &path, out, depth + 1);
                }
            } else if self.brand.is_book(&name) {
                let relative = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                out.push(Document { id: path.to_string_lossy().into_owned(), path: relative, size: meta.len() });
            }
        }
    }

    /// Miniature de couverture générée par la Kindle pour un ASIN, si présente.
    pub fn thumbnail(&self, asin: &str) -> Option<Vec<u8>> {
        let dir = self.root().join("system").join("thumbnails");
        ["EBOK", "PDOC", "EBSP"].iter().find_map(|t| std::fs::read(dir.join(format!("thumbnail_{asin}_{t}_portrait.jpg"))).ok())
    }

    /// Copie `file` dans le dossier des livres et, pour une Kindle, la miniature de couverture.
    pub fn send(&self, file: &Path, filename: &str, thumbnail: Option<(&str, &[u8])>) -> Result<(), DeviceError> {
        if !self.root().exists() {
            return Err(DeviceError::NotConnected);
        }
        let documents = self.documents();
        if !documents.exists() {
            // Dossier des livres d'une Kobo (« Books ») ou d'une liseuse générique : créé s'il manque.
            if self.brand == ReaderBrand::Kindle {
                return Err(DeviceError::NotConnected);
            }
            let _ = std::fs::create_dir_all(&documents);
        }
        let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        if self.free_space().is_some_and(|free| free < size) {
            return Err(DeviceError::InsufficientSpace);
        }
        let target = documents.join(filename);
        let _ = std::fs::remove_file(&target);
        std::fs::copy(file, &target).map_err(|e| DeviceError::TransferFailed(e.to_string()))?;
        if let Some((asin, jpeg)) = thumbnail.filter(|_| self.brand.sends_thumbnails()) {
            let dir = self.root().join("system").join("thumbnails");
            if dir.is_dir() {
                let _ = std::fs::write(dir.join(thumbnail_name(asin)), jpeg);
            }
        }
        Ok(())
    }

    /// Supprime un livre de la liseuse et son dossier `.sdr` (position de lecture, notes de la Kindle).
    pub fn delete(&self, document: &Document) -> Result<(), DeviceError> {
        std::fs::remove_file(&document.id).map_err(|e| DeviceError::TransferFailed(e.to_string()))?;
        let _ = std::fs::remove_dir_all(Path::new(&document.id).with_extension("sdr"));
        Ok(())
    }

    /// Remplace le contenu d'un livre de l'appareil (même emplacement, même nom), sans risque de perte : la copie
    /// complète est faite d'abord, l'original n'est remplacé qu'ensuite.
    pub fn replace(&self, document: &Document, file: &Path) -> Result<(), DeviceError> {
        let target = PathBuf::from(&document.id);
        let temp = target.with_file_name(format!(".{}.foluna-tmp", target.file_name().and_then(|n| n.to_str()).unwrap_or("livre")));
        let _ = std::fs::remove_file(&temp);
        let result = std::fs::copy(file, &temp).and_then(|_| {
            // FAT32 ne remplace pas un fichier existant par un renommage sous Windows : suppression d'abord.
            std::fs::rename(&temp, &target).or_else(|_| std::fs::remove_file(&target).and_then(|_| std::fs::rename(&temp, &target)))
        });
        if let Err(e) = result {
            let _ = std::fs::remove_file(&temp);
            return Err(DeviceError::TransferFailed(e.to_string()));
        }
        Ok(())
    }
}
