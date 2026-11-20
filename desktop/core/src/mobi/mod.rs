//! Fichiers Mobipocket : PalmDB, en-têtes MOBI/EXTH, compression PalmDoc et HUFF/CDIC, index
//! (`Sources/AlexandrieCore/Mobi`).

pub mod book;
pub mod huff;
pub mod index;
pub mod metadata_writer;
pub mod palmdoc;

pub use book::MobiBook;
