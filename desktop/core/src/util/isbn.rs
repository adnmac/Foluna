//! Validation et normalisation des ISBN (10 et 13 chiffres), comme `Util/ISBN.swift`.

/// Chiffres seuls (et « X » final d'un ISBN-10), sans préfixe ni tirets.
fn digits(s: &str) -> String {
    let mut t = s.to_uppercase();
    for prefix in ["URN:ISBN:", "ISBN-13", "ISBN-10", "ISBN", "EAN"] {
        t = t.replace(prefix, "");
    }
    t.chars().filter(|c| c.is_ascii_digit() || *c == 'X').collect()
}

fn values(d: &str) -> Vec<u32> {
    d.chars().filter_map(|c| c.to_digit(10)).collect()
}

fn valid13(d: &str) -> bool {
    if !(d.starts_with("978") || d.starts_with("979")) || !d.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = values(d).iter().enumerate().map(|(i, v)| v * if i % 2 == 0 { 1 } else { 3 }).sum();
    sum.is_multiple_of(10)
}

fn valid10(d: &str) -> bool {
    let chars: Vec<char> = d.chars().collect();
    let last = chars[chars.len() - 1];
    if !chars[..chars.len() - 1].iter().all(|c| c.is_ascii_digit()) || !(last.is_ascii_digit() || last == 'X') {
        return false;
    }
    let sum: u32 = chars
        .iter()
        .enumerate()
        .map(|(i, c)| (10 - i as u32) * if *c == 'X' { 10 } else { c.to_digit(10).unwrap_or(0) })
        .sum();
    sum.is_multiple_of(11)
}

fn to13(isbn10: &str) -> String {
    let core = format!("978{}", &isbn10[..9]);
    let sum: u32 = values(&core).iter().enumerate().map(|(i, v)| v * if i % 2 == 0 { 1 } else { 3 }).sum();
    format!("{core}{}", (10 - sum % 10) % 10)
}

pub fn is_valid(s: &str) -> bool {
    let d = digits(s);
    (d.len() == 13 && valid13(&d)) || (d.len() == 10 && valid10(&d))
}

/// Forme canonique ISBN-13 ; None si invalide.
pub fn normalized(s: &str) -> Option<String> {
    let d = digits(s);
    if d.len() == 13 && valid13(&d) {
        return Some(d);
    }
    if d.len() == 10 && valid10(&d) {
        return Some(to13(&d));
    }
    None
}

/// Premier ISBN valide d'une liste d'identifiants (dc:identifier, EXTH 104…), même composites
/// (« urn:isbn:978-2-… », « ISBN 978… (br.) »).
pub fn first<S: AsRef<str>>(candidates: &[S]) -> Option<String> {
    for c in candidates {
        let c = c.as_ref();
        if let Some(n) = normalized(c) {
            return Some(n);
        }
        for token in c.split([' ', ';', ',']) {
            if let Some(n) = normalized(token) {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalized("978-2-07-036024-6").as_deref(), Some("9782070360246"));
        assert_eq!(normalized("2-07-036024-5").as_deref(), Some("9782070360246"));
        assert_eq!(normalized("978-2-07-036024-5"), None);
        assert!(is_valid("0-306-40615-2"));
        assert_eq!(first(&["uuid:123", "urn:isbn:9782070360246"]).as_deref(), Some("9782070360246"));
    }
}
