//! Index INDX/TAGX/CNCX : squelettes, fragments, NCX (`Mobi/MobiIndex.swift`).

use std::collections::HashMap;

use crate::error::{BookError, Result};
use crate::mobi::book::MOBI_NULL;
use crate::util::bytes::{varint, Bytes, BytesMut};

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub key: Vec<u8>,
    pub tags: HashMap<usize, Vec<usize>>,
}

impl IndexEntry {
    /// Première valeur d'un tag.
    pub fn first(&self, tag: usize) -> Option<usize> {
        self.tags.get(&tag).and_then(|v| v.first().copied())
    }
}

pub struct IndexReader {
    pub entries: Vec<IndexEntry>,
    /// Chaînes CNCX par offset.
    pub strings: HashMap<usize, Vec<u8>>,
}

struct TagDef {
    tag: usize,
    per_entry: usize,
    mask: usize,
    end: usize,
}

impl IndexReader {
    pub fn new(records: &[Vec<u8>], index: usize) -> Result<Self> {
        let header = records
            .get(index)
            .filter(|r| r.starts_with(b"INDX"))
            .ok_or_else(|| BookError::Unreadable("index INDX introuvable".into()))?;
        let header_length = header.u32_at(4);
        let data_records = header.u32_at(24);
        let cncx_count = header.u32_at(52);
        let mut strings = HashMap::new();
        for j in 0..cncx_count {
            let Some(rec) = records.get(index + data_records + 1 + j) else { break };
            for (off, s) in read_cncx(rec) {
                strings.insert(off + j * 0x10000, s);
            }
        }
        let (control_bytes, table) = read_tagx(header, header_length);
        let mut entries = Vec::new();
        for data in records.iter().take((index + 1 + data_records).min(records.len())).skip(index + 1) {
            if !data.starts_with(b"INDX") {
                continue;
            }
            let (idxt, count) = (data.u32_at(20), data.u32_at(24));
            let mut positions: Vec<usize> = (0..count).map(|j| data.u16_at(idxt + 4 + 2 * j)).collect();
            positions.push(idxt);
            for j in 0..count {
                let (start, end) = (positions[j], positions[j + 1]);
                let key_length = data.u8_at(start);
                let key = data.slice_at(start + 1, key_length).to_vec();
                let tags = tag_map(control_bytes, &table, data, start + 1 + key_length, end);
                entries.push(IndexEntry { key, tags });
            }
        }
        Ok(IndexReader { entries, strings })
    }

    pub fn string(&self, offset: Option<usize>) -> &[u8] {
        offset.and_then(|o| self.strings.get(&o)).map(|v| v.as_slice()).unwrap_or_default()
    }
}

fn read_cncx(data: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < data.len() && data[offset] != 0 {
        let start = offset;
        let (consumed, length) = varint::read_forward(data, offset);
        offset += consumed;
        out.push((start, data.slice_at(offset, length).to_vec()));
        offset += length;
    }
    out
}

fn read_tagx(data: &[u8], start: usize) -> (usize, Vec<TagDef>) {
    if data.slice_at(start, 4) != b"TAGX" {
        return (0, Vec::new());
    }
    let first_entry = data.u32_at(start + 4);
    let control_bytes = data.u32_at(start + 8);
    let mut table = Vec::new();
    let mut i = 12;
    while i < first_entry {
        let p = start + i;
        table.push(TagDef { tag: data.u8_at(p), per_entry: data.u8_at(p + 1), mask: data.u8_at(p + 2), end: data.u8_at(p + 3) });
        i += 4;
    }
    (control_bytes, table)
}

fn tag_map(control_byte_count: usize, table: &[TagDef], data: &[u8], start: usize, end: usize) -> HashMap<usize, Vec<usize>> {
    struct Pending {
        tag: usize,
        count: Option<usize>,
        bytes: Option<usize>,
        per_entry: usize,
    }
    let mut pending = Vec::new();
    let mut control_index = 0;
    let mut data_start = start + control_byte_count;
    for t in table {
        if t.end == 1 {
            control_index += 1;
            continue;
        }
        let mut mask = t.mask;
        let mut value = data.u8_at(start + control_index) & mask;
        if value == 0 {
            continue;
        }
        if value == mask {
            if mask.count_ones() > 1 {
                let (consumed, v) = varint::read_forward(data, data_start);
                data_start += consumed;
                pending.push(Pending { tag: t.tag, count: None, bytes: Some(v), per_entry: t.per_entry });
            } else {
                pending.push(Pending { tag: t.tag, count: Some(1), bytes: None, per_entry: t.per_entry });
            }
        } else {
            while mask & 1 == 0 {
                mask >>= 1;
                value >>= 1;
            }
            pending.push(Pending { tag: t.tag, count: Some(value), bytes: None, per_entry: t.per_entry });
        }
    }
    let mut result = HashMap::new();
    for p in pending {
        let mut values = Vec::new();
        if let Some(count) = p.count {
            for _ in 0..count * p.per_entry {
                let (consumed, v) = varint::read_forward(data, data_start);
                data_start += consumed;
                values.push(v);
            }
        } else if let Some(bytes) = p.bytes {
            let mut total = 0;
            while total < bytes && data_start < end {
                let (consumed, v) = varint::read_forward(data, data_start);
                if consumed == 0 {
                    break;
                }
                data_start += consumed;
                total += consumed;
                values.push(v);
            }
        }
        result.insert(p.tag, values);
    }
    result
}

/// Écriture d'un index INDX (en-tête + enregistrements de données + CNCX), sur le modèle du writer KF8 de calibre.
pub struct IndexWriter {
    /// (numéro, valeurs par entrée, masque).
    pub tags: Vec<(usize, usize, usize)>,
    /// Entrées triées : clé + valeurs par numéro de tag.
    pub entries: Vec<(String, HashMap<usize, Vec<usize>>)>,
    pub cncx_records: Vec<Vec<u8>>,
}

const HEADER_LENGTH: usize = 192;

impl IndexWriter {
    pub fn new(tags: Vec<(usize, usize, usize)>) -> Self {
        IndexWriter { tags, entries: Vec::new(), cncx_records: Vec::new() }
    }

    fn tagx(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for &(number, per_entry, mask) in &self.tags {
            body.extend_from_slice(&[number as u8, per_entry as u8, mask as u8, 0]);
        }
        body.extend_from_slice(&[0, 0, 0, 1]);
        let mut out = b"TAGX".to_vec();
        out.push_be32(12 + body.len());
        out.push_be32(1);
        out.extend(body);
        out
    }

    fn control_byte(&self, values: &HashMap<usize, Vec<usize>>) -> u8 {
        let mut cb = 0;
        for &(number, per_entry, mask) in &self.tags {
            let n = values.get(&number).map_or(0, |v| v.len()) / per_entry;
            cb |= mask & (n << mask.trailing_zeros());
        }
        cb as u8
    }

    pub fn records(&self) -> Vec<Vec<u8>> {
        let record_limit = 0x10000 - HEADER_LENGTH - 1048;
        let mut blocks: Vec<Vec<u8>> = vec![Vec::new()];
        let mut idxts: Vec<Vec<u8>> = vec![Vec::new()];
        let mut counts = vec![0usize];
        let mut last_keys: Vec<Vec<u8>> = vec![Vec::new()];
        for (key, values) in &self.entries {
            let key = key.as_bytes().to_vec();
            let mut raw = vec![key.len() as u8];
            raw.extend_from_slice(&key);
            raw.push(self.control_byte(values));
            for &(number, _, _) in &self.tags {
                for &v in values.get(&number).map(|v| v.as_slice()).unwrap_or_default() {
                    raw.extend(varint::encode_forward(v));
                }
            }
            let last = blocks.len() - 1;
            if blocks[last].len() + idxts[last].len() + raw.len() + 2 > record_limit {
                blocks.push(Vec::new());
                idxts.push(Vec::new());
                counts.push(0);
                last_keys.push(Vec::new());
            }
            let last = blocks.len() - 1;
            let pos = HEADER_LENGTH + blocks[last].len();
            idxts[last].push_be16(pos);
            blocks[last].extend(raw);
            counts[last] += 1;
            last_keys[last] = key;
        }
        let mut data_records = Vec::new();
        for k in 0..blocks.len() {
            let mut block = blocks[k].clone();
            block.align4();
            let mut idxt = b"IDXT".to_vec();
            idxt.extend_from_slice(&idxts[k]);
            idxt.align4();
            let mut rec = b"INDX".to_vec();
            rec.push_be32(HEADER_LENGTH);
            rec.push_be32(0);
            rec.push_be32(1);
            rec.push_be32(0);
            rec.push_be32(HEADER_LENGTH + block.len());
            rec.push_be32(counts[k]);
            rec.extend_from_slice(&[0xFF; 8]);
            rec.extend_from_slice(&[0; 156]);
            rec.extend(block);
            rec.extend(idxt);
            data_records.push(rec);
        }
        // En-tête : TAGX + géométrie (dernière clé de chaque enregistrement) + IDXT.
        let tagx = self.tagx();
        let mut geometry = Vec::new();
        let mut idxt = b"IDXT".to_vec();
        let mut pos = HEADER_LENGTH + tagx.len();
        for (key, count) in last_keys.iter().zip(&counts) {
            idxt.push_be16(pos);
            let mut g = vec![key.len() as u8];
            g.extend_from_slice(key);
            g.push_be16(*count);
            pos += g.len();
            geometry.extend(g);
        }
        let (mut tagx_a, mut geometry_a, mut idxt_a) = (tagx, geometry, idxt);
        tagx_a.align4();
        geometry_a.align4();
        idxt_a.align4();
        let mut header = b"INDX".to_vec();
        header.push_be32(HEADER_LENGTH);
        header.extend_from_slice(&[0; 8]);
        header.push_be32(2);
        header.push_be32(HEADER_LENGTH + tagx_a.len() + geometry_a.len());
        header.push_be32(data_records.len());
        header.push_be32(65001);
        header.push_be32(MOBI_NULL);
        header.push_be32(self.entries.len());
        header.push_be32(0);
        header.push_be32(0);
        header.push_be32(0);
        header.push_be32(self.cncx_records.len());
        header.extend_from_slice(&[0; 124]);
        header.push_be32(HEADER_LENGTH);
        header.extend_from_slice(&[0; 8]);
        header.extend(tagx_a);
        header.extend(geometry_a);
        header.extend(idxt_a);
        let mut out = vec![header];
        out.extend(data_records);
        out.extend(self.cncx_records.iter().cloned());
        out
    }
}

/// Table de chaînes CNCX ; renvoie l'offset de chaque chaîne.
#[derive(Default)]
pub struct CncxBuilder {
    records: Vec<Vec<u8>>,
    current: Vec<u8>,
    offsets: HashMap<String, usize>,
}

impl CncxBuilder {
    pub fn offset(&mut self, string: &str) -> usize {
        if let Some(&o) = self.offsets.get(string) {
            return o;
        }
        let truncated: String = string.chars().take(500).collect();
        let mut raw = varint::encode_forward(truncated.len());
        raw.extend_from_slice(truncated.as_bytes());
        if self.current.len() + raw.len() > 0x10000 - 1024 {
            let mut rec = std::mem::take(&mut self.current);
            rec.align4();
            self.records.push(rec);
        }
        let o = self.records.len() * 0x10000 + self.current.len();
        self.current.extend(raw);
        self.offsets.insert(string.to_string(), o);
        o
    }

    pub fn finish(mut self) -> Vec<Vec<u8>> {
        if !self.current.is_empty() {
            let mut rec = std::mem::take(&mut self.current);
            rec.align4();
            self.records.push(rec);
        }
        self.records
    }
}
