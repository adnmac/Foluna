//! Chemins relatifs à l'intérieur d'une archive EPUB (`ArchivePath` du cœur Swift).

pub fn directory(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// Résout `href` relativement à `base_dir` ; renvoie (chemin, fragment).
pub fn resolve(href: &str, base_dir: &str) -> (String, Option<String>) {
    let (target, fragment) = match href.find('#') {
        Some(i) => (&href[..i], Some(href[i + 1..].to_string())),
        None => (href, None),
    };
    let target = percent_decode(target);
    if target.is_empty() {
        return (String::new(), fragment);
    }
    let mut parts: Vec<&str> = if target.starts_with('/') { Vec::new() } else { base_dir.split('/').filter(|p| !p.is_empty()).collect() };
    for comp in target.split('/').filter(|p| !p.is_empty()) {
        match comp {
            "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    (parts.join("/"), fragment)
}

/// Chemin relatif de `target` vu depuis le dossier `from_dir`.
pub fn relative(target: &str, from_dir: &str) -> String {
    let a: Vec<&str> = from_dir.split('/').filter(|p| !p.is_empty()).collect();
    let b: Vec<&str> = target.split('/').filter(|p| !p.is_empty()).collect();
    let mut i = 0;
    while i < a.len() && i + 1 < b.len() && a[i] == b[i] {
        i += 1;
    }
    let mut out: Vec<&str> = vec![".."; a.len() - i];
    out.extend_from_slice(&b[i..]);
    out.join("/")
}

pub fn is_external(href: &str) -> bool {
    let lower = href.to_lowercase();
    lower.contains(':') && !lower.starts_with("kindle:")
}

/// `%20` → espace ; une séquence invalide reste telle quelle.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(resolve("../Images/a%20b.png#x", "OEBPS/Text"), ("OEBPS/Images/a b.png".to_string(), Some("x".to_string())));
        assert_eq!(relative("OEBPS/Images/a.png", "OEBPS/Text"), "../Images/a.png");
        assert_eq!(directory("OEBPS/content.opf"), "OEBPS");
        assert!(is_external("http://x") && !is_external("kindle:embed:0001"));
    }
}
