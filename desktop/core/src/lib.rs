//! Cœur de Foluna pour Windows : logique sans interface, portée du cœur Swift de l'app Mac
//! (`Sources/AlexandrieCore`). Les modules arrivent au fil des étapes : livres (MOBI, EPUB, PDF),
//! conversions, bibliothèque, liseuses USB.

pub mod epub;
pub mod error;
pub mod convert;
pub mod device;
pub mod extract;
pub mod library;
pub mod metadata;
pub mod mobi;
pub mod pdf;
pub mod update;
pub mod util;

pub use error::{BookError, Result};
pub use metadata::{BookFormat, BookMetadata, MetadataEdits, TocNode};

/// Version de l'app, la même que celle du paquet.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Numéro du passage de la CI qui a construit l'app (`FOLUNA_BUILD`), pour distinguer deux versions de test.
pub const BUILD: Option<&str> = option_env!("FOLUNA_BUILD");

/// Version affichée : « 0.1.0 (9) » pour un build de la CI, sinon « 0.1.0 ».
pub fn display_version() -> String {
    match BUILD.filter(|b| !b.is_empty()) {
        Some(b) => format!("{VERSION} ({b})"),
        None => VERSION.to_string(),
    }
}
