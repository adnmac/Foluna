//! Fiches des livres présents sur la liseuse, en cache (clé : chemin + taille) (`KindleCatalog.swift`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::device::volume::{Document, VolumeDevice};
use crate::epub::EpubReader;
use crate::library::{AppDirectories, CoverStore};
use crate::mobi::MobiBook;
use crate::util::html::strip_html;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceBookInfo {
    pub title: String,
    pub authors: Vec<String>,
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isbn: Option<String>,
    #[serde(rename = "isDRMProtected")]
    pub is_drm_protected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asin: Option<String>,
    /// Type de document Kindle (EBOK : livre acheté ou converti, PDOC : document personnel…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cde_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_key: Option<String>,
    /// Vrai si le fichier n'a pas pu être analysé (KFX, format inconnu) : titre déduit du nom.
    pub is_guessed: bool,
}

pub struct DeviceCatalog {
    pub covers: CoverStore,
    cache_path: PathBuf,
    cache: Mutex<HashMap<String, DeviceBookInfo>>,
}

const PARSABLE: &[&str] = &["mobi", "azw", "azw3", "prc", "pdf", "epub"];

static ASIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(B0[0-9A-Z]{8})").unwrap());

impl DeviceCatalog {
    pub fn new(dirs: &AppDirectories) -> Self {
        let cover_dir = dirs.base.join("KindleCovers");
        let _ = std::fs::create_dir_all(&cover_dir);
        let cache_path = dirs.base.join("kindle-catalog-v3.json");
        let cache = std::fs::read(&cache_path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        DeviceCatalog { covers: CoverStore::new(cover_dir), cache_path, cache: Mutex::new(cache) }
    }

    fn key(doc: &Document) -> String {
        format!("{}|{}", doc.path, doc.size)
    }

    pub fn cached(&self, doc: &Document) -> Option<DeviceBookInfo> {
        self.cache.lock().ok()?.get(&Self::key(doc)).cloned()
    }

    /// Analyse un document (ou rend la fiche en cache).
    pub fn info(&self, doc: &Document, device: &VolumeDevice) -> DeviceBookInfo {
        if let Some(hit) = self.cached(doc) {
            return hit;
        }
        let (mut info, mut cover) = (guess(doc), None);
        if PARSABLE.contains(&doc.extension().as_str())
            && let Some((i, c)) = read(Path::new(&doc.id), &doc.extension()) {
                (info, cover) = (i, c);
            }
        if cover.is_none() {
            cover = info.asin.as_deref().and_then(|a| device.thumbnail(a));
        }
        let digest = Sha256::digest(Self::key(doc).as_bytes());
        let cover_key = format!("k-{}", digest[..12].iter().map(|b| format!("{b:02x}")).collect::<String>());
        if cover.as_deref().is_some_and(|c| self.covers.store(c, &cover_key)) {
            info.cover_key = Some(cover_key);
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(Self::key(doc), info.clone());
        }
        info
    }

    pub fn save(&self) {
        let Ok(cache) = self.cache.lock() else { return };
        if let Ok(json) = serde_json::to_vec(&*cache) {
            let _ = std::fs::write(&self.cache_path, json);
        }
    }

    pub fn cover_path(&self, info: &DeviceBookInfo) -> Option<PathBuf> {
        info.cover_key.as_deref().map(|k| self.covers.path(k))
    }
}

/// Fiche et image de couverture d'un fichier de l'appareil. Les informations EXTH et les images restent lisibles
/// sous DRM (seul le texte est chiffré).
fn read(path: &Path, format: &str) -> Option<(DeviceBookInfo, Option<Vec<u8>>)> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let base = |title: String, format: &str| DeviceBookInfo {
        title: if title.is_empty() { stem.clone() } else { title },
        authors: Vec::new(),
        format: format.to_string(),
        language: None,
        publisher: None,
        summary: None,
        published_date: None,
        isbn: None,
        is_drm_protected: false,
        asin: None,
        cde_type: None,
        cover_key: None,
        is_guessed: false,
    };
    let (mut info, meta) = match format {
        "epub" => {
            let m = EpubReader::open(path).ok()?.book_metadata();
            (base(m.title.clone(), "EPUB"), m)
        }
        "pdf" => {
            let m = crate::pdf::metadata(path).ok()?;
            (base(m.title.clone(), "PDF"), m)
        }
        _ => {
            let book = MobiBook::open(path).ok()?;
            let m = book.metadata();
            let mut info = base(m.title.clone(), &if book.has_kf8() { "AZW3".to_string() } else { format.to_uppercase() });
            info.asin = book.asin();
            info.cde_type = book.cde_type();
            (info, m)
        }
    };
    info.authors = meta.authors;
    info.language = meta.language;
    info.publisher = meta.publisher;
    info.summary = meta.summary.as_deref().map(strip_html);
    info.published_date = meta.published_date;
    info.isbn = meta.isbn;
    info.is_drm_protected = meta.is_drm_protected;
    Some((info, meta.cover_data))
}

/// Titre déduit du nom de fichier (« Le_Titre-B0ABCDEFGH.kfx »), ASIN s'il y figure.
pub fn guess(doc: &Document) -> DeviceBookInfo {
    let name = doc.name();
    let base = Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let asin = ASIN.captures(&doc.path).map(|c| c[1].to_string());
    let mut title = base.clone();
    if let Some(a) = &asin {
        title = title.replace(a.as_str(), "");
    }
    let title = title.replace('_', " ").trim_matches(|c: char| " -_.".contains(c)).to_string();
    DeviceBookInfo {
        title: if title.is_empty() { base } else { title },
        authors: Vec::new(),
        format: doc.extension().to_uppercase(),
        language: None,
        publisher: None,
        summary: None,
        published_date: None,
        isbn: None,
        is_drm_protected: false,
        asin,
        cde_type: None,
        cover_key: None,
        is_guessed: true,
    }
}
