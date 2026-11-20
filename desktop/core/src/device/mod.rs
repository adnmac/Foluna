//! Liseuses branchées par USB en mode disque : Kindle (jusqu'aux modèles 2023), Kobo, liseuses EPUB
//! (`Sources/AlexandrieCore/Kindle` et la partie sans interface de `KindleMonitor`). MTP viendra plus tard.

pub mod brand;
pub mod catalog;
pub mod platform;
pub mod readers;
pub mod shelf;
pub mod summary;
pub mod transfer;
pub mod volume;

use std::fmt;

pub use brand::ReaderBrand;
pub use volume::{Document, VolumeDevice};

use crate::library::{CoverStore, Library};

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceError {
    NotConnected,
    TransferFailed(String),
    InsufficientSpace,
    EjectFailed(String),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::NotConnected => write!(f, "La liseuse n'est plus connectée."),
            DeviceError::TransferFailed(why) => write!(f, "Le transfert a échoué : {why}."),
            DeviceError::InsufficientSpace => write!(f, "Espace insuffisant sur la liseuse."),
            DeviceError::EjectFailed(why) => write!(f, "Impossible d'éjecter la liseuse : {why}."),
        }
    }
}

impl std::error::Error for DeviceError {}

/// Résultat de l'envoi d'un livre.
#[derive(Debug, Clone, PartialEq)]
pub enum SendOutcome {
    Sent { filename: String },
    /// Refusé avant tout essai (DRM incompatible, livre dans la corbeille, dossier débranché).
    Refused(String),
    Failed(String),
}

/// Envoie des livres à la liseuse branchée et met la bibliothèque à jour (dernier envoi, nom du fichier, file
/// d'attente, étagère). `reader` : identifiant de la liseuse (`VolumeDevice::identifier`). `progress` est appelé
/// avant chaque livre (identifiant, conversion nécessaire).
pub fn send(
    books: &[crate::library::Book],
    library: &mut Library,
    device: &VolumeDevice,
    progress: &mut dyn FnMut(&str, bool),
) -> Vec<(String, SendOutcome)> {
    let brand = device.brand;
    let reader = device.identifier();
    let covers: CoverStore = library.covers.clone();
    let mut results = Vec::new();
    for book in books {
        if !book.can_send(brand) {
            results.push((book.path.clone(), SendOutcome::Refused(format!("« {} » est protégé par DRM : il ne peut pas être lu sur cette {}.", book.title, brand.label()))));
            continue;
        }
        if book.trashed_at.is_some() || library.is_unavailable(book) {
            results.push((book.path.clone(), SendOutcome::Refused(format!("« {} » est indisponible : son dossier de livres est introuvable (disque débranché ?).", book.title))));
            continue;
        }
        progress(&book.path, book.needs_conversion(brand));
        let outcome = transfer::prepare(book, &library.dirs, brand)
            .map_err(|e| e.to_string())
            .and_then(|file| {
                let filename = book.filename(brand);
                // Miniature de couverture : seulement pour un AZW3 produit pour une Kindle.
                let thumbnail = (brand.sends_thumbnails() && book.becomes_azw3())
                    .then(|| covers.kindle_thumbnail(&book.fingerprint))
                    .flatten();
                device
                    .send(&file, &filename, thumbnail.as_deref().map(|t| (book.asin.as_str(), t)))
                    .map(|_| filename)
                    .map_err(|e| e.to_string())
            });
        match outcome {
            Ok(filename) => {
                let _ = library.mark_sent(&book.path, &filename, &reader);
                results.push((book.path.clone(), SendOutcome::Sent { filename }));
            }
            Err(e) => results.push((book.path.clone(), SendOutcome::Failed(e))),
        }
    }
    results
}

/// Envoie la file d'attente de cette liseuse ; appelé quand elle vient d'être branchée. Un livre dont l'envoi
/// échoue reste en attente pour la prochaine connexion. Rend le texte de la notification, s'il y a lieu.
pub fn send_pending(library: &mut Library, device: &VolumeDevice, reader_name: Option<&str>) -> Option<String> {
    let pending = library.pending_sends(Some(&device.identifier()));
    if pending.is_empty() {
        return None;
    }
    let results = send(&pending, library, device, &mut |_, _| {});
    let sent: Vec<String> = results
        .iter()
        .filter(|(_, o)| matches!(o, SendOutcome::Sent { .. }))
        .filter_map(|(id, _)| library.book(id).map(|b| b.title.clone()))
        .collect();
    let failed = results.iter().filter(|(_, o)| !matches!(o, SendOutcome::Sent { .. })).count();
    summary::body(&sent, failed, reader_name)
}
