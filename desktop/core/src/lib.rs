//! Cœur de Foluna pour Windows : logique sans interface, portée du cœur Swift de l'app Mac
//! (`Sources/AlexandrieCore`). Les modules arrivent au fil des étapes : livres (MOBI, EPUB, PDF),
//! conversions, bibliothèque, liseuses USB.

/// Version de l'app, la même que celle du paquet.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
