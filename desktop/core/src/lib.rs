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
