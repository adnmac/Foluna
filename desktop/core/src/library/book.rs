//! Livre indexé (`Library/Library.swift`, `Book`). Mêmes clés JSON que l'app Mac (dates en secondes depuis le
//! 1er janvier 2001, comme `JSONEncoder`) ; les champs que cette version ne connaît pas (notice du catalogue,
//! mode PDF…) sont gardés tels quels dans `extra`, pour ne rien perdre en passant d'un système à l'autre.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::metadata::{BookFormat, MetadataEdits};
use crate::util::bytes::safe_filename;

/// Date : secondes depuis le 1er janvier 2001 UTC (référence de `Date` en Swift).
pub type Timestamp = f64;

/// Écart entre 1970 et 2001, en secondes.
const REFERENCE_OFFSET: f64 = 978_307_200.0;

pub fn now() -> Timestamp {
    from_system_time(std::time::SystemTime::now())
}

pub fn from_system_time(t: std::time::SystemTime) -> Timestamp {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64() - REFERENCE_OFFSET,
        Err(e) => -e.duration().as_secs_f64() - REFERENCE_OFFSET,
    }
}

/// Version de l'extraction : un livre indexé par une version antérieure est relu (même valeur que le Mac).
pub const CURRENT_METADATA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub path: String,
    pub format: BookFormat,
    pub fingerprint: String,
    pub file_size: i64,
    pub modified_at: Timestamp,
    pub added_at: Timestamp,
    pub title: String,
    pub authors: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_index: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub tags: Vec<String>,
    pub has_cover: bool,
    #[serde(rename = "isDRMProtected")]
    pub is_drm_protected: bool,
    pub is_imported: bool,
    /// Identifiant stable utilisé comme ASIN lors de la conversion AZW3.
    pub asin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sent_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_filename: Option<String>,
    /// En attente d'envoi vers la prochaine liseuse branchée, quelle qu'elle soit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_send_at: Option<Timestamp>,
    /// En attente d'envoi vers une liseuse précise (identifiant → date de la demande).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_for: Option<BTreeMap<String, Timestamp>>,
    /// Derniers envois, par liseuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_to: Option<BTreeMap<String, Timestamp>>,
    /// Dans la corbeille de Foluna : masqué partout, restaurable ; ses fichiers ne bougent pas avant qu'elle soit vidée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed_at: Option<Timestamp>,
    /// Étagères (identifiants de liseuses) où figure ce livre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shelves: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_version: Option<u32>,
    /// Champs inconnus de cette version, recopiés tels quels.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Book {
    /// Identifiant : le chemin du fichier.
    pub fn id(&self) -> &str {
        &self.path
    }

    pub fn file_path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn is_pending_send(&self) -> bool {
        self.pending_send_at.is_some() || self.pending_for.as_ref().is_some_and(|m| !m.is_empty())
    }

    /// Date de la demande d'envoi valable pour cette liseuse (None : pour n'importe laquelle).
    pub fn pending_date(&self, reader: Option<&str>) -> Option<Timestamp> {
        match reader {
            Some(r) => self.pending_for.as_ref().and_then(|m| m.get(r).copied()).or(self.pending_send_at),
            None => self
                .pending_for
                .iter()
                .flat_map(|m| m.values().copied())
                .chain(self.pending_send_at)
                .min_by(|a, b| a.total_cmp(b)),
        }
    }

    pub fn shelf_ids(&self) -> BTreeSet<String> {
        self.shelves.clone().unwrap_or_default()
    }

    /// Un EPUB sous DRM ne peut pas être converti ; les formats Kindle sous DRM sont copiés tels quels.
    pub fn can_send_to_kindle(&self) -> bool {
        !self.is_drm_protected || self.format.is_kindle_native()
    }

    pub fn author_line(&self) -> String {
        if self.authors.is_empty() { "Auteur inconnu".into() } else { self.authors.join(", ") }
    }

    /// Informations corrigeables, appliquées aux copies envoyées.
    pub fn edits(&self) -> MetadataEdits {
        MetadataEdits {
            title: self.title.clone(),
            authors: self.authors.clone(),
            publisher: self.publisher.clone(),
            language: self.language.clone(),
            summary: self.summary.clone(),
            published_date: self.published_date.clone(),
            isbn: self.isbn.clone(),
            series: self.series.clone(),
            series_index: self.series_index,
            genre: self.genre.clone(),
        }
    }

    pub fn set_edits(&mut self, e: &MetadataEdits) {
        self.title = e.title.clone();
        self.authors = e.authors.clone();
        self.publisher = e.publisher.clone();
        self.language = e.language.clone();
        self.summary = e.summary.clone();
        self.published_date = e.published_date.clone();
        self.isbn = e.isbn.clone();
        self.series = e.series.clone();
        self.series_index = e.series_index;
        self.genre = e.genre.clone();
    }

    /// La Kindle reçoit un AZW3 produit par Foluna (EPUB).
    pub fn becomes_azw3(&self) -> bool {
        self.format == BookFormat::Epub
    }

    /// Nom proposé pour la copie sur la Kindle.
    pub fn kindle_filename(&self) -> String {
        let ext = if self.becomes_azw3() { "azw3" } else { self.format.extension() };
        let author = self.authors.first().map(|a| format!(" - {a}")).unwrap_or_default();
        format!("{}.{ext}", safe_filename(&format!("{}{author}", self.title)))
    }

    /// Nom du fichier, sans dossier.
    pub fn file_name(&self) -> String {
        Path::new(&self.path).file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
    }
}
