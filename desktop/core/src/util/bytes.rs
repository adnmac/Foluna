//! Accès binaires big-endian tolérants (`Util/Bytes.swift`) : une lecture hors limites renvoie 0
//! plutôt que de faire planter l'app sur un fichier corrompu.

pub trait Bytes {
    fn u8_at(&self, o: usize) -> usize;
    fn u16_at(&self, o: usize) -> usize;
    fn u32_at(&self, o: usize) -> usize;
    /// Tranche bornée aux limites du tableau.
    fn slice_at(&self, o: usize, length: usize) -> &[u8];
    fn ascii_at(&self, o: usize, length: usize) -> String;
    fn starts_with_str(&self, s: &str) -> bool;
    /// Première occurrence de `pattern` à partir de `from`.
    fn find_bytes(&self, pattern: &[u8], from: usize) -> Option<usize>;
}

impl Bytes for [u8] {
    #[inline]
    fn u8_at(&self, o: usize) -> usize {
        self.get(o).copied().unwrap_or(0) as usize
    }

    #[inline]
    fn u16_at(&self, o: usize) -> usize {
        (self.u8_at(o) << 8) | self.u8_at(o.wrapping_add(1))
    }

    #[inline]
    fn u32_at(&self, o: usize) -> usize {
        (self.u8_at(o) << 24) | (self.u8_at(o.wrapping_add(1)) << 16) | (self.u8_at(o.wrapping_add(2)) << 8)
            | self.u8_at(o.wrapping_add(3))
    }

    fn slice_at(&self, o: usize, length: usize) -> &[u8] {
        let start = o.min(self.len());
        let end = o.saturating_add(length).min(self.len()).max(start);
        &self[start..end]
    }

    fn ascii_at(&self, o: usize, length: usize) -> String {
        String::from_utf8_lossy(self.slice_at(o, length)).into_owned()
    }

    fn starts_with_str(&self, s: &str) -> bool {
        self.starts_with(s.as_bytes())
    }

    fn find_bytes(&self, pattern: &[u8], from: usize) -> Option<usize> {
        if pattern.is_empty() || pattern.len() > self.len() || from > self.len() {
            return None;
        }
        self[from..].windows(pattern.len()).position(|w| w == pattern).map(|p| p + from)
    }
}

/// Ajouts big-endian, pour écrire les formats Kindle.
pub trait BytesMut {
    fn push_be16(&mut self, v: usize);
    fn push_be32(&mut self, v: usize);
    fn push_str(&mut self, s: &str);
    /// Complète par des zéros jusqu'à un multiple de 4.
    fn align4(&mut self);
}

impl BytesMut for Vec<u8> {
    fn push_be16(&mut self, v: usize) {
        self.extend_from_slice(&[(v >> 8) as u8, v as u8]);
    }

    fn push_be32(&mut self, v: usize) {
        self.extend_from_slice(&[(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8]);
    }

    fn push_str(&mut self, s: &str) {
        self.extend_from_slice(s.as_bytes());
    }

    fn align4(&mut self) {
        while !self.len().is_multiple_of(4) {
            self.push(0);
        }
    }
}

/// Entiers à largeur variable des index MOBI (7 bits par octet).
pub mod varint {
    /// Lecture « avant » : le dernier octet porte le bit 0x80. Renvoie (octets lus, valeur).
    pub fn read_forward(data: &[u8], offset: usize) -> (usize, usize) {
        let (mut value, mut consumed) = (0usize, 0usize);
        while offset + consumed < data.len() {
            let v = data[offset + consumed] as usize;
            consumed += 1;
            value = (value << 7) | (v & 0x7F);
            if v & 0x80 != 0 {
                break;
            }
        }
        (consumed, value)
    }

    /// Encodage « avant » (`encint` de calibre).
    pub fn encode_forward(value: usize) -> Vec<u8> {
        let mut v = value;
        let mut bytes = Vec::new();
        loop {
            bytes.insert(0, (v & 0x7F) as u8);
            v >>= 7;
            if v == 0 {
                break;
            }
        }
        let last = bytes.len() - 1;
        bytes[last] |= 0x80;
        bytes
    }
}

/// Base 32 Amazon (0-9A-V) de `kindle:embed`, `kindle:pos`, `aid`.
pub mod base32 {
    const DIGITS: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";

    pub fn encode(value: usize, width: usize) -> String {
        let mut v = value;
        let mut out = Vec::new();
        loop {
            out.insert(0, DIGITS[v % 32]);
            v /= 32;
            if v == 0 {
                break;
            }
        }
        while out.len() < width {
            out.insert(0, b'0');
        }
        String::from_utf8(out).unwrap_or_default()
    }

    pub fn decode(s: &str) -> usize {
        s.to_ascii_uppercase()
            .bytes()
            .filter_map(|c| DIGITS.iter().position(|&d| d == c))
            .fold(0, |v, d| v * 32 + d)
    }
}

/// Échappement XML minimal pour le texte et les attributs.
pub fn xml_escaped(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Nom de fichier sûr pour la Kindle et les systèmes de fichiers FAT (120 caractères au plus).
pub fn safe_filename(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if "/\\:*?\"<>|\0".contains(c) { '_' } else { c }).collect();
    cleaned.trim().chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerant_reads() {
        let b: &[u8] = &[0x12, 0x34, 0x56, 0x78];
        assert_eq!(b.u32_at(0), 0x1234_5678);
        assert_eq!(b.u16_at(3), 0x7800);
        assert_eq!(b.u32_at(10), 0);
        assert_eq!(b.slice_at(2, 10), &[0x56, 0x78]);
        assert_eq!(b.find_bytes(&[0x56, 0x78], 0), Some(2));
    }

    #[test]
    fn varint_round_trip() {
        for v in [0, 1, 127, 128, 300, 70_000, 1 << 28] {
            let enc = varint::encode_forward(v);
            assert_eq!(varint::read_forward(&enc, 0).1, v);
        }
    }

    #[test]
    fn base32_round_trip() {
        assert_eq!(base32::encode(0, 4), "0000");
        assert_eq!(base32::encode(31, 0), "V");
        assert_eq!(base32::decode(&base32::encode(123_456, 0)), 123_456);
    }

    #[test]
    fn filenames() {
        assert_eq!(safe_filename(" Titre: un/deux? "), "Titre_ un_deux_");
    }
}
