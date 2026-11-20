//! EPUB 2 et 3 (`Sources/AlexandrieCore/EPUB`).

pub mod reader;
pub mod xml;
pub mod zip_io;

pub use reader::{EpubItem, EpubReader};
