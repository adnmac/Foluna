//! Fichiers Mobipocket : PalmDB, en-têtes MOBI/EXTH, compression PalmDoc et HUFF/CDIC (`Sources/AlexandrieCore/Mobi`).

pub mod book;
pub mod huff;
pub mod palmdoc;

pub use book::MobiBook;
