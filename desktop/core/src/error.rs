//! Erreurs de lecture et de conversion d'un livre (`BookError` du cœur Swift).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookError {
    /// Fichier illisible, avec la raison.
    Unreadable(String),
    /// Livre protégé par DRM : ni converti ni lu.
    DrmProtected,
    /// Format ou variante non pris en charge.
    Unsupported(String),
    /// La conversion a échoué.
    ConversionFailed(String),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookError::Unreadable(why) => write!(f, "Fichier illisible : {why}."),
            BookError::DrmProtected => write!(f, "Ce livre est protégé par DRM : il ne peut pas être converti."),
            BookError::Unsupported(why) => write!(f, "Non pris en charge : {why}."),
            BookError::ConversionFailed(why) => write!(f, "La conversion a échoué : {why}."),
        }
    }
}

impl std::error::Error for BookError {}

impl From<std::io::Error> for BookError {
    fn from(e: std::io::Error) -> Self {
        BookError::Unreadable(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BookError>;
