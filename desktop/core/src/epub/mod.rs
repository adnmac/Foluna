//! EPUB 2 et 3 (`Sources/AlexandrieCore/EPUB`).

pub mod dom;
pub mod metadata_writer;
pub mod reader;
pub mod writer;
pub mod xml;
pub mod zip_io;

pub use reader::{EpubItem, EpubReader};
pub use writer::EpubBuilder;
