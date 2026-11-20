//! Fichier MOBI / AZW / AZW3 : KF7, KF8 ou hybride (`Mobi/MobiBook.swift`).

use std::collections::HashMap;
use std::path::Path;

use crate::error::{BookError, Result};
use crate::metadata::BookMetadata;
use crate::mobi::{huff::HuffCdicReader, palmdoc};
use crate::util::{bytes::Bytes, image, isbn};

/// Valeur « absente » des champs d'index MOBI.
pub const MOBI_NULL: usize = 0xFFFF_FFFF;

/// En-tête PalmDoc + MOBI + EXTH (enregistrement 0 d'une section KF7 ou KF8).
#[derive(Debug, Clone)]
pub struct MobiHeader {
    pub raw: Vec<u8>,
    /// Index de l'enregistrement 0 de cette section dans la base PalmDB.
    pub start: usize,
    /// Enregistrements EXTH : type → valeurs (un type peut se répéter, ex. plusieurs auteurs).
    pub exth: HashMap<usize, Vec<Vec<u8>>>,
}

impl MobiHeader {
    pub fn new(raw: Vec<u8>, start: usize) -> Self {
        let mut exth: HashMap<usize, Vec<Vec<u8>>> = HashMap::new();
        let base = 16 + raw.u32_at(20);
        if raw.u32_at(0x80) & 0x40 != 0 && raw.slice_at(base, 4) == b"EXTH" {
            let count = raw.u32_at(base + 8);
            let mut pos = base + 12;
            for _ in 0..count {
                let (kind, len) = (raw.u32_at(pos), raw.u32_at(pos + 4));
                if len < 8 {
                    break;
                }
                exth.entry(kind).or_default().push(raw.slice_at(pos + 8, len - 8).to_vec());
                pos += len;
            }
        }
        MobiHeader { raw, start, exth }
    }

    pub fn compression(&self) -> usize {
        self.raw.u16_at(0)
    }
    pub fn text_length(&self) -> usize {
        self.raw.u32_at(4)
    }
    pub fn text_record_count(&self) -> usize {
        self.raw.u16_at(8)
    }
    pub fn encryption(&self) -> usize {
        self.raw.u16_at(12)
    }
    pub fn header_length(&self) -> usize {
        self.raw.u32_at(20)
    }
    pub fn encoding(&self) -> usize {
        self.raw.u32_at(28)
    }
    pub fn version(&self) -> usize {
        self.raw.u32_at(36)
    }
    pub fn is_kf8(&self) -> bool {
        self.version() >= 8
    }
    pub fn is_utf8(&self) -> bool {
        self.encoding() == 65001
    }

    /// Champ d'index converti en index absolu (None si absent).
    fn index(&self, offset: usize) -> Option<usize> {
        if self.header_length() + 16 <= offset {
            return None;
        }
        let v = self.raw.u32_at(offset);
        (v != MOBI_NULL).then_some(v + self.start)
    }

    pub fn first_resource(&self) -> Option<usize> {
        self.index(0x6C)
    }
    pub fn huff_record(&self) -> Option<usize> {
        self.index(0x70)
    }
    pub fn huff_count(&self) -> usize {
        self.raw.u32_at(0x74)
    }
    pub fn fdst_index(&self) -> Option<usize> {
        if self.is_kf8() { self.index(0xC0) } else { None }
    }
    pub fn ncx_index(&self) -> Option<usize> {
        self.index(0xF4)
    }
    pub fn fragment_index(&self) -> Option<usize> {
        if self.is_kf8() { self.index(0xF8) } else { None }
    }
    pub fn skeleton_index(&self) -> Option<usize> {
        if self.is_kf8() { self.index(0xFC) } else { None }
    }

    pub fn extra_data_flags(&self) -> usize {
        if self.header_length() >= 0xE4 && self.version() >= 5 { self.raw.u16_at(0xF2) } else { 0 }
    }

    pub fn full_name(&self) -> String {
        self.decode(self.raw.slice_at(self.raw.u32_at(0x54), self.raw.u32_at(0x58)))
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        if self.is_utf8() {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
        }
    }

    pub fn exth_string(&self, kind: usize) -> Option<String> {
        self.exth.get(&kind)?.first().map(|v| self.decode(v).trim().to_string())
    }

    pub fn exth_strings(&self, kind: usize) -> Vec<String> {
        self.exth.get(&kind).map(|vs| vs.iter().map(|v| self.decode(v).trim().to_string()).collect()).unwrap_or_default()
    }

    pub fn exth_int(&self, kind: usize) -> Option<usize> {
        let v = self.exth.get(&kind)?.first()?;
        (v.len() >= 4).then(|| v.u32_at(0))
    }
}

/// Livre Mobipocket lu en mémoire.
pub struct MobiBook {
    pub records: Vec<Vec<u8>>,
    pub kf7: MobiHeader,
    /// Section KF8 (AZW3 pur ou partie KF8 d'un MOBI hybride).
    pub kf8: Option<MobiHeader>,
}

impl MobiBook {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_bytes(&std::fs::read(path)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() <= 78 {
            return Err(BookError::Unreadable("fichier trop court".into()));
        }
        let kind = bytes.ascii_at(60, 8);
        if kind != "BOOKMOBI" && kind != "TEXtREAd" {
            return Err(BookError::Unreadable(format!("signature PalmDB inconnue ({kind})")));
        }
        let count = bytes.u16_at(76);
        let mut offsets: Vec<usize> = (0..count).map(|i| bytes.u32_at(78 + 8 * i)).collect();
        offsets.push(bytes.len());
        let records: Vec<Vec<u8>> = (0..count)
            .map(|i| {
                let a = offsets[i].min(bytes.len());
                let b = offsets[i + 1].min(bytes.len()).max(a);
                bytes[a..b].to_vec()
            })
            .collect();
        let first = records.first().ok_or_else(|| BookError::Unreadable("aucun enregistrement".into()))?;
        let kf7 = MobiHeader::new(first.clone(), 0);
        let kf8 = if kf7.is_kf8() {
            Some(kf7.clone())
        } else {
            match kf7.exth_int(121) {
                Some(boundary) if boundary != MOBI_NULL => Self::locate_kf8(&records, boundary),
                _ => None,
            }
        };
        Ok(MobiBook { records, kf7, kf8 })
    }

    /// EXTH 121 désigne l'enregistrement BOUNDARY ou l'en-tête KF8 qui le suit, selon les outils.
    fn locate_kf8(records: &[Vec<u8>], boundary: usize) -> Option<MobiHeader> {
        for candidate in [boundary, boundary + 1] {
            let Some(r) = records.get(candidate) else { continue };
            if r.slice_at(16, 4) == b"MOBI" {
                let h = MobiHeader::new(r.clone(), candidate);
                if h.is_kf8() {
                    return Some(h);
                }
            }
        }
        None
    }

    pub fn is_drm_protected(&self) -> bool {
        self.kf7.encryption() != 0 || self.kf8.as_ref().is_some_and(|h| h.encryption() != 0)
    }

    pub fn has_kf8(&self) -> bool {
        self.kf8.is_some()
    }

    /// En-tête de référence pour les métadonnées et les ressources.
    pub fn main_header(&self) -> &MobiHeader {
        self.kf8.as_ref().unwrap_or(&self.kf7)
    }

    /// Premier enregistrement de ressource. Dans un hybride, les ressources sont partagées et
    /// commencent là où l'indique l'en-tête KF7.
    pub fn first_resource(&self) -> Option<usize> {
        if self.kf8.is_some() && !self.kf7.is_kf8() {
            return self.kf7.first_resource();
        }
        self.main_header().first_resource()
    }

    pub fn resource(&self, relative: usize) -> Option<&[u8]> {
        let i = self.first_resource()? + relative;
        self.records.get(i).map(|r| r.as_slice())
    }

    // Texte

    /// Texte brut décompressé d'une section (HTML KF7 ou « rawML » KF8).
    pub fn text(&self, header: &MobiHeader) -> Result<Vec<u8>> {
        if header.encryption() != 0 {
            return Err(BookError::DrmProtected);
        }
        let mut huff = None;
        match header.compression() {
            17480 => {
                let h = header
                    .huff_record()
                    .filter(|&h| h < self.records.len())
                    .ok_or_else(|| BookError::Unreadable("table HUFF absente".into()))?;
                let cdics: Vec<&[u8]> =
                    (1..header.huff_count().max(1)).filter_map(|k| self.records.get(h + k).map(|r| r.as_slice())).collect();
                huff = Some(HuffCdicReader::new(&self.records[h], &cdics)?);
            }
            1 | 2 => {}
            other => return Err(BookError::Unsupported(format!("compression {other}"))),
        }
        let mut out = Vec::with_capacity(header.text_length());
        for i in 1..=header.text_record_count().max(1) {
            let Some(rec) = self.records.get(header.start + i) else { break };
            let trailing = Self::trailing_size(rec, header.extra_data_flags());
            let rec = &rec[..rec.len().saturating_sub(trailing)];
            match (header.compression(), huff.as_mut()) {
                (2, _) => out.extend(palmdoc::decompress(rec)),
                (17480, Some(h)) => out.extend(h.unpack(rec)),
                _ => out.extend_from_slice(rec),
            }
        }
        out.truncate(header.text_length());
        Ok(out)
    }

    /// Taille des données de fin d'enregistrement (TBS, octets multi-octets…).
    pub fn trailing_size(rec: &[u8], flags: usize) -> usize {
        let entry_size = |size: usize| -> usize {
            let (mut bitpos, mut result, mut size) = (0, 0usize, size);
            while size > 0 {
                let v = rec[size - 1] as usize;
                result |= (v & 0x7F) << bitpos;
                bitpos += 7;
                size -= 1;
                if v & 0x80 != 0 || bitpos >= 28 {
                    break;
                }
            }
            result
        };
        let mut num = 0usize;
        let mut test = flags >> 1;
        while test != 0 {
            if test & 1 != 0 {
                num += entry_size(rec.len().saturating_sub(num));
            }
            test >>= 1;
        }
        if flags & 1 != 0 && rec.len() > num {
            num += (rec[rec.len() - num - 1] & 0x3) as usize + 1;
        }
        num
    }

    // Métadonnées

    pub fn metadata(&self) -> BookMetadata {
        let h = self.main_header();
        let mut meta = BookMetadata::new(h.exth_string(503).unwrap_or_else(|| h.full_name()));
        meta.authors = h
            .exth_strings(100)
            .iter()
            .flat_map(|s| s.split('&').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect::<Vec<_>>())
            .collect();
        meta.publisher = h.exth_string(101);
        meta.summary = h.exth_string(103);
        meta.identifier = h.exth_string(104).or_else(|| h.exth_string(113));
        meta.published_date = h.exth_string(106);
        meta.isbn = isbn::first(&h.exth_strings(104));
        meta.subjects = h.exth_strings(105);
        meta.language = h.exth_string(524);
        meta.is_drm_protected = self.is_drm_protected();
        meta.cover_data = self.cover_image().map(|c| c.to_vec());
        meta
    }

    /// Index relatif (dans les ressources) de l'image de couverture.
    pub fn cover_resource_index(&self) -> Option<usize> {
        let h = self.main_header();
        [201, 202].into_iter().find_map(|kind| {
            let off = h.exth_int(kind).filter(|&o| o != MOBI_NULL)?;
            self.resource(off).filter(|r| image::mime_type(r).is_some()).map(|_| off)
        })
    }

    pub fn cover_image(&self) -> Option<&[u8]> {
        if let Some(i) = self.cover_resource_index() {
            return self.resource(i);
        }
        // À défaut, la première image des ressources.
        let first = self.first_resource()?;
        (first..self.records.len().min(first + 50))
            .map(|i| self.records[i].as_slice())
            .find(|r| image::mime_type(r).is_some())
    }

    /// EXTH 113 (ASIN) : utile pour les miniatures sur la Kindle.
    pub fn asin(&self) -> Option<String> {
        self.main_header().exth_string(113)
    }

    pub fn cde_type(&self) -> Option<String> {
        self.main_header().exth_string(501)
    }
}
