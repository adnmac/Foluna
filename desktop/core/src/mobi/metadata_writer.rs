//! Réécrit les métadonnées (EXTH, titre complet, langue) d'un MOBI/AZW/AZW3 sans toucher au texte
//! (`Mobi/MobiMetadataWriter.swift`).

use std::path::Path;

use crate::error::{BookError, Result};
use crate::metadata::MetadataEdits;
use crate::mobi::book::{MobiBook, MobiHeader};
use crate::util::bytes::{Bytes, BytesMut};

/// Types EXTH remplacés par les corrections ; les autres (ASIN, couverture, type…) sont conservés.
const EDITED_TYPES: [usize; 7] = [100, 101, 103, 104, 106, 503, 524];

pub fn rewrite_file(input: &Path, edits: &MetadataEdits, output: &Path) -> Result<()> {
    let data = rewrite(&std::fs::read(input)?, edits)?;
    let tmp = output.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, output)?;
    Ok(())
}

pub fn rewrite(bytes: &[u8], edits: &MetadataEdits) -> Result<Vec<u8>> {
    let book = MobiBook::from_bytes(bytes)?;
    // Sous DRM, l'enregistrement 0 contient les clés : le modifier rendrait le livre illisible.
    if book.is_drm_protected() {
        return Err(BookError::DrmProtected);
    }
    let e = edits.normalized();
    if e.title.is_empty() {
        return Err(BookError::ConversionFailed("le titre ne peut pas être vide".into()));
    }
    let mut records = book.records.clone();
    let mut headers = vec![&book.kf7];
    if let Some(kf8) = &book.kf8
        && kf8.start != book.kf7.start {
            headers.push(kf8);
        }
    for h in headers {
        records[h.start] = rewrite_header(h, &e);
    }
    Ok(rebuild_database(bytes, &records, &e.title))
}

fn rewrite_header(h: &MobiHeader, e: &MetadataEdits) -> Vec<u8> {
    let mut exth: Vec<(usize, Vec<u8>)> = Vec::new();
    // Ordre d'origine conservé pour les types non modifiés.
    let base = 16 + h.header_length();
    if h.raw.u32_at(0x80) & 0x40 != 0 && h.raw.slice_at(base, 4) == b"EXTH" {
        let mut pos = base + 12;
        for _ in 0..h.raw.u32_at(base + 8) {
            let (kind, len) = (h.raw.u32_at(pos), h.raw.u32_at(pos + 4));
            if len < 8 {
                break;
            }
            if !EDITED_TYPES.contains(&kind) {
                exth.push((kind, h.raw.slice_at(pos + 8, len - 8).to_vec()));
            }
            pos += len;
        }
    }
    let mut add = |kind: usize, value: Option<&str>| {
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            exth.push((kind, v.as_bytes().to_vec()));
        }
    };
    for a in &e.authors {
        add(100, Some(a));
    }
    add(101, e.publisher.as_deref());
    add(103, e.summary.as_deref());
    add(104, e.isbn.as_deref());
    add(106, e.published_date.as_deref());
    add(503, Some(&e.title));
    add(524, e.language.as_deref());

    let mut body = Vec::new();
    for (kind, value) in &exth {
        body.push_be32(*kind);
        body.push_be32(8 + value.len());
        body.extend_from_slice(value);
    }
    let mut block = b"EXTH".to_vec();
    block.push_be32(12 + body.len());
    block.push_be32(exth.len());
    block.extend(body);
    let pad = 4 - block.len() % 4;
    block.extend(std::iter::repeat_n(0, pad));

    // En-tête PalmDoc + MOBI inchangé, puis EXTH, titre complet et remplissage.
    let mut header = h.raw.slice_at(0, base).to_vec();
    header.resize(base, 0);
    let title: Vec<u8> = if h.is_utf8() { e.title.as_bytes().to_vec() } else { encoding_rs::WINDOWS_1252.encode(&e.title).0.into_owned() };
    set32(&mut header, 0x54, base + block.len());
    set32(&mut header, 0x58, title.len());
    set32(&mut header, 0x80, h.raw.u32_at(0x80) | 0x40);
    if let Some(lang) = &e.language
        && h.header_length() + 16 > 0x60 {
            set32(&mut header, 0x5C, language_code(Some(lang)));
        }
    let mut out = header;
    out.extend(block);
    out.extend(title);
    out.extend_from_slice(&[0, 0]);
    out.align4();
    // Marge habituelle (kindlegen, calibre) pour les outils qui enrichissent l'en-tête.
    out.extend(std::iter::repeat_n(0, 1024));
    out
}

fn set32(b: &mut [u8], o: usize, v: usize) {
    if o + 4 <= b.len() {
        b[o..o + 4].copy_from_slice(&(v as u32).to_be_bytes());
    }
}

/// Nom PalmDB : lettres et chiffres ASCII (accents retirés), « _ » ailleurs, 31 caractères au plus.
pub fn palm_name(title: &str, keep_spaces_as_underscore_only: bool) -> Vec<u8> {
    let folded = fold_diacritics(title);
    let ascii: String = folded
        .chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() {
                Some(c)
            } else if keep_spaces_as_underscore_only {
                (c == ' ').then_some('_')
            } else {
                Some('_')
            }
        })
        .collect();
    let mut name: Vec<u8> = ascii.into_bytes().into_iter().take(31).collect();
    name.resize(32, 0);
    name
}

/// « Éléphant » → « Elephant » (lettres latines accentuées courantes).
pub fn fold_diacritics(s: &str) -> String {
    const FROM: &str = "ÀÁÂÃÄÅàáâãäåÈÉÊËèéêëÌÍÎÏìíîïÒÓÔÕÖØòóôõöøÙÚÛÜùúûüÝýÿÇçÑñ";
    const TO: &str = "AAAAAAaaaaaaEEEEeeeeIIIIiiiiOOOOOOooooooUUUUuuuuYyyCcNn";
    s.chars()
        .flat_map(|c| match c {
            'Œ' => "OE".chars().collect::<Vec<_>>(),
            'œ' => "oe".chars().collect(),
            'Æ' => "AE".chars().collect(),
            'æ' => "ae".chars().collect(),
            'ß' => "ss".chars().collect(),
            _ => vec![FROM.chars().position(|f| f == c).and_then(|i| TO.chars().nth(i)).unwrap_or(c)],
        })
        .collect()
}

/// Recopie l'en-tête PalmDB (nom mis à jour) et recalcule les offsets des enregistrements.
fn rebuild_database(original: &[u8], records: &[Vec<u8>], title: &str) -> Vec<u8> {
    let mut out = original.slice_at(0, 78).to_vec();
    out[..32].copy_from_slice(&palm_name(title, false));
    let mut offset = 78 + 8 * records.len() + 2;
    for (i, r) in records.iter().enumerate() {
        out.push_be32(offset);
        out.extend_from_slice(original.slice_at(78 + 8 * i + 4, 4)); // attributs + identifiant d'origine
        offset += r.len();
    }
    out.extend_from_slice(&[0, 0]);
    for r in records {
        out.extend_from_slice(r);
    }
    out
}

/// Code de langue MOBI (LCID simplifié).
pub fn language_code(lang: Option<&str>) -> usize {
    let primary = lang.unwrap_or_default().to_lowercase();
    let primary = primary.split('-').next().unwrap_or_default();
    match primary {
        "en" => 9,
        "fr" => 12,
        "de" => 7,
        "es" => 10,
        "it" => 16,
        "nl" => 19,
        "pt" => 22,
        "ru" => 25,
        "ja" => 17,
        "zh" => 4,
        "ko" => 18,
        "sv" => 29,
        "da" => 6,
        "fi" => 11,
        "no" | "nb" => 20,
        "pl" => 21,
        "cs" => 5,
        "el" => 8,
        "tr" => 31,
        "ca" => 3,
        "ar" => 1,
        "he" => 13,
        "hu" => 14,
        _ => 0,
    }
}
