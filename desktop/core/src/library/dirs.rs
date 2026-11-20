//! Dossiers de travail de l'app (jamais dans le dossier de livres de l'utilisateur), comme `AppDirectories`.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppDirectories {
    pub base: PathBuf,
}

impl AppDirectories {
    /// Dossier de données de test (`FOLUNA_STORE`) : tests et captures n'écrivent jamais dans les vraies données.
    pub fn test_store() -> Option<PathBuf> {
        std::env::var_os("FOLUNA_STORE").filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    /// Dossier de données réel : `%APPDATA%\Foluna` sous Windows, `~/.local/share/foluna` ailleurs.
    pub fn real_base() -> PathBuf {
        if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(appdata).join("Foluna");
        }
        if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(data).join("foluna");
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join(".local").join("share").join("foluna")
    }

    /// `base` explicite, sinon `FOLUNA_STORE`, sinon le dossier réel. Les sous-dossiers sont créés.
    pub fn new(base: Option<&Path>) -> Self {
        let base = base.map(Path::to_path_buf).or_else(Self::test_store).unwrap_or_else(Self::real_base);
        let dirs = AppDirectories { base };
        for d in [dirs.covers(), dirs.conversions(), dirs.imports()] {
            let _ = std::fs::create_dir_all(d);
        }
        dirs
    }

    pub fn index(&self) -> PathBuf {
        self.base.join("library.json")
    }
    /// Dossiers de livres (voir `folders`).
    pub fn folders(&self) -> PathBuf {
        self.base.join("folders.json")
    }
    /// Liseuses connues.
    pub fn readers(&self) -> PathBuf {
        self.base.join("readers.json")
    }
    /// Réglages de l'app.
    pub fn settings(&self) -> PathBuf {
        self.base.join("settings.json")
    }
    /// Copies datées de l'index (voir `backups`).
    pub fn backups(&self) -> PathBuf {
        self.base.join("Sauvegardes")
    }
    pub fn covers(&self) -> PathBuf {
        self.base.join("Covers")
    }
    pub fn conversions(&self) -> PathBuf {
        self.base.join("Conversions")
    }
    /// Livres ajoutés par glisser-déposer : copiés ici, pas dans le dossier source.
    pub fn imports(&self) -> PathBuf {
        self.base.join("Imports")
    }
}

/// Couvertures réduites, une par empreinte de livre (`CoverStore`).
#[derive(Debug, Clone)]
pub struct CoverStore {
    pub directory: PathBuf,
}

impl CoverStore {
    pub fn new(directory: PathBuf) -> Self {
        CoverStore { directory }
    }

    pub fn path(&self, key: &str) -> PathBuf {
        self.directory.join(format!("{key}.jpg"))
    }

    /// Stocke une couverture réduite (600 px) ; faux si l'image est illisible.
    pub fn store(&self, data: &[u8], key: &str) -> bool {
        let target = self.path(key);
        if target.exists() {
            return true;
        }
        let Some(jpeg) = crate::util::image::jpeg(data, 600) else { return false };
        let tmp = target.with_extension("tmp");
        std::fs::write(&tmp, jpeg).is_ok() && std::fs::rename(&tmp, &target).is_ok()
    }

    /// Miniature au format attendu par la Kindle (`system/thumbnails`).
    pub fn kindle_thumbnail(&self, key: &str) -> Option<Vec<u8>> {
        crate::util::image::jpeg(&std::fs::read(self.path(key)).ok()?, 500)
    }
}
