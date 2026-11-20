//! État de l'app : bibliothèque, liseuse branchée, liseuses connues, réglages. Partagé entre les commandes et les
//! fils de surveillance (dossiers, branchements).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use foluna_core::device::{catalog::DeviceCatalog, readers, readers::Reader, VolumeDevice};
use foluna_core::library::{AppDirectories, Library};
use serde::{Deserialize, Serialize};

/// Réglages de l'app (`settings.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Langue de l'interface (« fr », « en »…) ; None : celle de Windows.
    #[serde(default)]
    pub language: Option<String>,
    /// Montrer les copies en double d'un même livre.
    #[serde(default)]
    pub show_duplicates: bool,
}

pub struct AppState {
    pub library: Library,
    pub device: Option<VolumeDevice>,
    pub readers: Vec<Reader>,
    pub catalog: DeviceCatalog,
    pub settings: Settings,
    /// Message d'erreur à montrer une fois (index illisible au lancement…).
    pub startup_error: Option<String>,
}

#[derive(Clone)]
pub struct Shared(pub Arc<Mutex<AppState>>);

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, AppState> {
        // Un fil qui a paniqué ne doit pas bloquer l'app : l'état reste utilisable.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl AppState {
    pub fn open() -> AppState {
        let dirs = AppDirectories::new(None);
        let (library, mut error) = Library::open(dirs.clone());
        let readers = readers::load(&dirs.readers()).unwrap_or_else(|e| {
            error.get_or_insert(e);
            Vec::new()
        });
        let settings = std::fs::read(dirs.settings()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        AppState { catalog: DeviceCatalog::new(&dirs), library, device: None, readers, settings, startup_error: error }
    }

    pub fn save_settings(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?;
        std::fs::write(self.library.dirs.settings(), json).map_err(|e| format!("Impossible d'enregistrer les réglages : {e}"))
    }

    pub fn save_readers(&self) -> Result<(), String> {
        readers::save(&self.readers, &self.library.dirs.readers())
    }

    /// Liseuses génériques ajoutées : identifiant → dossier des livres.
    pub fn generic_folders(&self) -> HashMap<String, String> {
        self.readers
            .iter()
            .filter(|r| r.brand() == foluna_core::device::ReaderBrand::Generic)
            .map(|r| (r.id.clone(), r.books_folder.clone().unwrap_or_default()))
            .collect()
    }

    /// Nom donné à la liseuse branchée.
    pub fn device_name(&self) -> Option<String> {
        let device = self.device.as_ref()?;
        let id = device.identifier();
        Some(self.readers.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_else(|| device.name()))
    }

    pub fn covers_dir(&self) -> &Path {
        &self.library.covers.directory
    }
}
