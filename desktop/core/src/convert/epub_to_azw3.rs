//! Conversion EPUB → AZW3 (KF8 pur), sur le modèle du writer KF8 de calibre (`Conversion/EPUBToAZW3.swift`).
//!
//! Chaque document de la spine devient un squelette (`<html><head>…<body>` + `</body></html>`) dans lequel
//! sont insérés des fragments d'environ 8 Ko, découpés entre les enfants directs de `<body>`. Les liens internes
//! deviennent des `kindle:pos:fid:…:off:…`, les images et polices des `kindle:embed:…`, les feuilles de style des
//! flux `kindle:flow:…`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::epub::dom::{self, Element, Node};
use crate::epub::xml;
use crate::epub::{EpubItem, EpubReader};
use crate::error::{BookError, Result};
use crate::metadata::{BookMetadata, MetadataEdits, TocNode};
use crate::mobi::book::MOBI_NULL;
use crate::mobi::index::{CncxBuilder, IndexWriter};
use crate::mobi::metadata_writer::{language_code, palm_name};
use crate::mobi::palmdoc;
use crate::util::archive_path;
use crate::util::bytes::{base32, xml_escaped, BytesMut};
use crate::util::random;

pub struct ConversionResult {
    pub asin: String,
    pub cover_data: Option<Vec<u8>>,
}

/// `edits` remplace les métadonnées de l'EPUB (corrections faites dans l'app).
pub fn convert(input: &Path, output: &Path, edits: Option<&MetadataEdits>, asin: Option<&str>) -> Result<ConversionResult> {
    let reader = EpubReader::open(input)?;
    let mut meta = reader.book_metadata();
    if meta.is_drm_protected {
        return Err(BookError::DrmProtected);
    }
    if let Some(e) = edits.map(MetadataEdits::normalized).filter(|e| !e.title.is_empty()) {
        meta.apply(&e);
    }
    let asin = asin.map(String::from).unwrap_or_else(make_asin);
    let data = Kf8Builder::new(&reader, &meta, &asin).build()?;
    let tmp = output.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, output)?;
    Ok(ConversionResult { asin, cover_data: meta.cover_data })
}

/// Identifiant de 10 caractères, stable par livre si l'appelant le mémorise.
pub fn make_asin() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let tail: String = (0..8).map(|_| CHARS[(random::next_u64() % CHARS.len() as u64) as usize] as char).collect();
    format!("AX{tail}")
}

const RECORD_SIZE: usize = 4096;
const CHUNK_SIZE: usize = 8192;

struct Chunk {
    raw: Vec<u8>,
    selector: String,
    insert_pos: usize,
    file_number: usize,
    start_in_file: usize,
}

struct Skeleton {
    file_number: usize,
    start: usize,
    length: usize,
    chunk_count: usize,
}

#[derive(Clone, Copy)]
struct Position {
    chunk: usize,
    offset: usize,
    absolute: usize,
}

struct Kf8Builder<'a> {
    reader: &'a EpubReader,
    meta: &'a BookMetadata,
    asin: &'a str,
    /// Ressources (images, polices) : index 0-based → kindle:embed:(index+1).
    resource_records: Vec<Vec<u8>>,
    resource_index: HashMap<String, (usize, String)>,
    /// Flux : 0 = texte, puis CSS et SVG.
    flows: Vec<Vec<u8>>,
    flow_index: HashMap<String, (usize, String)>,
    text: Vec<u8>,
    chunks: Vec<Chunk>,
    skeletons: Vec<Skeleton>,
    links: Vec<(String, Option<String>)>,
    id_positions: HashMap<String, Position>,
    file_positions: HashMap<String, Position>,
    aid_counter: usize,
    /// Chemin du document en cours (pour les liens `#ancre` intra-fichier).
    document_path: String,
}

const FONT_TYPES: &[&str] = &[
    "font/ttf", "font/otf", "font/sfnt", "application/x-font-ttf", "application/x-font-truetype",
    "application/x-font-opentype", "application/vnd.ms-opentype", "application/font-sfnt", "application/x-font-otf",
];

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"url\(\s*(['"]?)([^'")]+)['"]?\s*\)"#).unwrap());
static IMPORT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@import[^;]*;").unwrap());
static ID_ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\s(?:id|name)\s*=\s*["']([^"']*)["']"#).unwrap());
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"kindle:pos:fid:([0-9A-V]{4}):off:0000000000").unwrap());

impl<'a> Kf8Builder<'a> {
    fn new(reader: &'a EpubReader, meta: &'a BookMetadata, asin: &'a str) -> Self {
        Kf8Builder {
            reader,
            meta,
            asin,
            resource_records: Vec::new(),
            resource_index: HashMap::new(),
            flows: vec![Vec::new()],
            flow_index: HashMap::new(),
            text: Vec::new(),
            chunks: Vec::new(),
            skeletons: Vec::new(),
            links: Vec::new(),
            id_positions: HashMap::new(),
            file_positions: HashMap::new(),
            aid_counter: 0,
            document_path: String::new(),
        }
    }

    fn build(mut self) -> Result<Vec<u8>> {
        self.collect_resources();
        self.collect_stylesheets();
        let documents: Vec<EpubItem> = self.reader.spine.iter().filter(|i| i.media_type.contains("html")).cloned().collect();
        if documents.is_empty() {
            return Err(BookError::ConversionFailed("aucun document dans la spine".into()));
        }
        for (n, item) in documents.iter().enumerate() {
            self.add_document(item, n)?;
        }
        self.fill_link_placeholders();
        self.flows[0] = self.text.clone();
        let mut all_text = Vec::new();
        let mut fdst = Vec::new();
        for f in &self.flows {
            fdst.push((all_text.len(), all_text.len() + f.len()));
            all_text.extend_from_slice(f);
        }
        Ok(self.assemble(&all_text, &fdst))
    }

    // Ressources

    fn collect_resources(&mut self) {
        let obfuscated = self.obfuscated_paths();
        for item in &self.reader.items {
            let Ok(data) = self.reader.data(&item.path) else { continue };
            let mime = item.media_type.to_lowercase();
            let ext = Path::new(&item.path).extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
            if ["image/jpeg", "image/png", "image/gif"].contains(&mime.as_str()) {
                self.add_resource(&item.path, data, &mime);
            } else if mime.starts_with("image/") && mime != "image/svg+xml" {
                if let Some(jpeg) = crate::util::image::jpeg(&data, 2400) {
                    self.add_resource(&item.path, jpeg, "image/jpeg");
                }
            } else if mime == "image/svg+xml" {
                self.flows.push(data);
                self.flow_index.insert(item.path.clone(), (self.flows.len() - 1, mime));
            } else if (FONT_TYPES.contains(&mime.as_str()) || ext == "ttf" || ext == "otf") && !obfuscated.contains(&item.path) {
                let mut record = b"FONT".to_vec();
                record.push_be32(data.len());
                record.push_be32(0);
                record.push_be32(24);
                record.push_be32(0);
                record.push_be32(0);
                record.extend(data);
                self.add_resource(&item.path, record, if ext == "otf" { "font/otf" } else { "font/ttf" });
            }
        }
    }

    fn add_resource(&mut self, path: &str, record: Vec<u8>, mime: &str) {
        self.resource_index.insert(path.to_string(), (self.resource_records.len(), mime.to_string()));
        self.resource_records.push(record);
    }

    /// Polices obfusquées (encryption.xml) : ignorées plutôt que d'embarquer des données illisibles.
    fn obfuscated_paths(&self) -> std::collections::HashSet<String> {
        let Ok(data) = self.reader.data("META-INF/encryption.xml") else { return Default::default() };
        let text = xml::prepare(&data);
        let Ok(doc) = xml::parse(&text) else { return Default::default() };
        xml::descendants(doc.root(), "CipherReference")
            .filter_map(|c| xml::attr(c, "URI"))
            .map(|u| archive_path::resolve(u, "").0)
            .collect()
    }

    fn collect_stylesheets(&mut self) {
        let items: Vec<EpubItem> = self.reader.items.iter().filter(|i| i.media_type == "text/css").cloned().collect();
        for item in items {
            let Ok(data) = self.reader.data(&item.path) else { continue };
            let css = self.rewrite_css(&String::from_utf8_lossy(&data), archive_path::directory(&item.path));
            self.flows.push(css.into_bytes());
            self.flow_index.insert(item.path.clone(), (self.flows.len() - 1, "text/css".into()));
        }
    }

    fn rewrite_css(&self, css: &str, base_dir: &str) -> String {
        let no_imports = IMPORT.replace_all(css, "");
        URL.replace_all(&no_imports, |c: &Captures| {
            let reference = &c[2];
            if archive_path::is_external(reference) || reference.starts_with("data:") {
                return c[0].to_string();
            }
            let path = archive_path::resolve(reference, base_dir).0;
            match self.embed_url(&path) {
                Some(embed) => format!("url({embed})"),
                None => c[0].to_string(),
            }
        })
        .into_owned()
    }

    fn embed_url(&self, path: &str) -> Option<String> {
        if let Some((index, mime)) = self.resource_index.get(path) {
            return Some(format!("kindle:embed:{}?mime={mime}", base32::encode(index + 1, 4)));
        }
        self.flow_index.get(path).map(|(index, mime)| format!("kindle:flow:{}?mime={mime}", base32::encode(*index, 4)))
    }

    // Documents

    fn next_aid(&mut self) -> String {
        let aid = base32::encode(self.aid_counter, 0);
        self.aid_counter += 1;
        aid
    }

    fn add_document(&mut self, item: &EpubItem, file_number: usize) -> Result<()> {
        self.document_path = item.path.clone();
        let data = self.reader.data(&item.path)?;
        // XHTML invalide : relu comme du HTML, à la manière d'un navigateur.
        let mut root = dom::parse_xml(&data).unwrap_or_else(|_| dom::parse_html(&String::from_utf8_lossy(&data)));
        let base_dir = archive_path::directory(&item.path).to_string();
        self.rewrite(&mut root, &base_dir);
        if root.child("body").is_none() {
            root.children.push(Node::Element(Element::new("body")));
        }
        let body_aid = self.next_aid();

        // Balise ouvrante du squelette.
        let mut root_tag = String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"");
        for (prefix, uri) in &root.namespaces {
            if !prefix.is_empty() {
                root_tag += &format!(" xmlns:{prefix}=\"{}\"", xml_escaped(uri));
            }
        }
        for (n, v) in &root.attrs {
            root_tag += &format!(" {n}=\"{}\"", xml_escaped(v));
        }
        root_tag.push('>');
        let head_xml = root
            .child("head")
            .map(Element::to_xml)
            .unwrap_or_else(|| format!("<head><title>{}</title></head>", xml_escaped(&self.meta.title)));
        let body_id = root.child("body").and_then(|b| b.attr("id")).map(String::from);
        let mut body_tag = String::from("<body");
        for (n, v) in &root.child("body").map(|b| b.attrs.clone()).unwrap_or_default() {
            if n != "aid" {
                body_tag += &format!(" {n}=\"{}\"", xml_escaped(v));
            }
        }
        body_tag += &format!(" aid=\"{body_aid}\">");
        let skeleton_head = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{root_tag}{head_xml}{body_tag}").into_bytes();
        let skeleton_tail = b"</body></html>".to_vec();

        // Fragments : enfants directs de <body>, regroupés par ~8 Ko, coupés après un élément.
        let mut body = root.child("body").cloned().unwrap_or_default();
        let mut groups: Vec<(Vec<u8>, Option<String>)> = Vec::new();
        let mut current: Vec<u8> = Vec::new();
        let mut current_aid: Option<String> = None;
        for node in body.children.iter_mut() {
            let raw = match node {
                Node::Element(el) => {
                    let aid = self.next_aid();
                    el.set_attr("aid", aid.clone());
                    current_aid = Some(aid);
                    el.to_xml().into_bytes()
                }
                other => other.to_xml().into_bytes(),
            };
            current.extend(raw);
            if current.len() >= CHUNK_SIZE && current_aid.is_some() {
                groups.push((std::mem::take(&mut current), current_aid.take()));
            }
        }
        if !current.is_empty() || groups.is_empty() {
            if current.is_empty() {
                current = b" ".to_vec();
            }
            if groups.is_empty() || current_aid.is_some() {
                groups.push((current, current_aid));
            } else {
                // Texte final sans élément : rattaché au fragment précédent.
                let last = groups.len() - 1;
                groups[last].0.extend(current);
            }
        }

        let skel_start = self.text.len();
        self.text.extend_from_slice(&skeleton_head);
        self.text.extend_from_slice(&skeleton_tail);
        let mut insert = skel_start + skeleton_head.len();
        let mut in_file = 0;
        let first_chunk = self.chunks.len();
        let mut previous_aid: Option<String> = None;
        let group_count = groups.len();
        for (raw, last_aid) in groups {
            let selector = match &previous_aid {
                Some(a) => format!("S-//*[@aid='{a}']"),
                None => format!("P-//*[@aid='{body_aid}']"),
            };
            let chunk_number = self.chunks.len();
            self.index_ids(&raw, &item.path, chunk_number, insert);
            self.text.extend_from_slice(&raw);
            let length = raw.len();
            self.chunks.push(Chunk { raw, selector, insert_pos: insert, file_number, start_in_file: in_file });
            insert += length;
            in_file += length;
            previous_aid = last_aid;
        }
        self.skeletons.push(Skeleton {
            file_number,
            start: skel_start,
            length: skeleton_head.len() + skeleton_tail.len(),
            chunk_count: group_count,
        });
        let top = Position { chunk: first_chunk, offset: 0, absolute: skel_start + skeleton_head.len() };
        self.file_positions.insert(item.path.clone(), top);
        // Les ancres posées sur <body> renvoient au début du fichier.
        if let Some(id) = body_id {
            self.id_positions.insert(format!("{}#{id}", item.path), top);
        }
        Ok(())
    }

    /// Réécrit récursivement les références (styles, images, liens) d'un document.
    fn rewrite(&mut self, el: &mut Element, base_dir: &str) {
        let mut i = 0;
        while i < el.children.len() {
            let Node::Element(child) = &mut el.children[i] else {
                i += 1;
                continue;
            };
            let mut remove = false;
            let name = child.local_name().to_string();
            match name.as_str() {
                "script" => remove = true,
                "link" => {
                    if let Some(href) = child.attr("href").filter(|h| !archive_path::is_external(h)).map(String::from) {
                        let path = archive_path::resolve(&href, base_dir).0;
                        match self.flow_index.get(&path) {
                            Some((index, mime)) if mime == "text/css" => {
                                child.set_attr("href", format!("kindle:flow:{}?mime=text/css", base32::encode(*index, 4)));
                            }
                            _ => remove = true,
                        }
                    }
                }
                "style" => {
                    let css = child.text();
                    if !css.is_empty() {
                        child.set_text(self.rewrite_css(&css, base_dir));
                    }
                }
                "img" | "image" | "source" | "object" => {
                    for (name, value) in child.attrs.iter_mut() {
                        let local = name.rsplit(':').next().unwrap_or(name);
                        if !["src", "href", "data"].contains(&local) || archive_path::is_external(value) {
                            continue;
                        }
                        let path = archive_path::resolve(value, base_dir).0;
                        if let Some(embed) = self.embed_url(&path) {
                            *value = embed;
                        }
                    }
                }
                "a" | "area" => {
                    if let Some(href) = child.attrs.iter().find(|(n, _)| n == "href").map(|(_, v)| v.clone())
                        && !archive_path::is_external(&href) {
                            let (path, fragment) = match href.strip_prefix('#') {
                                Some(f) => (String::new(), Some(f.to_string())),
                                None => archive_path::resolve(&href, base_dir),
                            };
                            self.links.push((if path.is_empty() { self.document_path.clone() } else { path }, fragment));
                            child.set_attr("href", format!("kindle:pos:fid:{}:off:0000000000", base32::encode(self.links.len() - 1, 4)));
                        }
                }
                _ => {}
            }
            if remove {
                el.children.remove(i);
                continue;
            }
            if let Node::Element(child) = &mut el.children[i] {
                if let Some(css) = child.attrs.iter().find(|(n, _)| n == "style").map(|(_, v)| v.clone())
                    && css.contains("url(") {
                        let rewritten = self.rewrite_css(&css, base_dir);
                        child.set_attr("style", rewritten);
                    }
                self.rewrite(child, base_dir);
            }
            i += 1;
        }
    }

    /// Mémorise la position de chaque `id` (début de la balise) dans un fragment.
    fn index_ids(&mut self, raw: &[u8], path: &str, chunk: usize, insert_pos: usize) {
        let mut i = 0;
        while let Some(open) = raw[i..].iter().position(|&b| b == b'<').map(|p| p + i) {
            let close = raw[open..].iter().position(|&b| b == b'>').map(|p| p + open).unwrap_or(raw.len() - 1);
            let tag = String::from_utf8_lossy(&raw[open..=close]);
            if tag.contains("id")
                && let Some(c) = ID_ATTR.captures(&tag) {
                    let key = format!("{path}#{}", &c[1]);
                    self.id_positions.entry(key).or_insert(Position { chunk, offset: open, absolute: insert_pos + open });
                }
            i = close + 1;
            if i >= raw.len() {
                break;
            }
        }
    }

    /// Remplace les marqueurs de liens par les positions réelles (même longueur → offsets inchangés).
    fn fill_link_placeholders(&mut self) {
        let s = String::from_utf8_lossy(&self.text).into_owned();
        let filled = PLACEHOLDER.replace_all(&s, |c: &Captures| {
            let (path, fragment) = &self.links[base32::decode(&c[1])];
            let pos = self.position(path, fragment.as_deref());
            format!("kindle:pos:fid:{}:off:{}", base32::encode(pos.chunk, 4), base32::encode(pos.offset, 10))
        });
        self.text = filled.into_owned().into_bytes();
    }

    fn position(&self, path: &str, fragment: Option<&str>) -> Position {
        if let Some(f) = fragment.filter(|f| !f.is_empty())
            && let Some(p) = self.id_positions.get(&format!("{path}#{f}")) {
                return *p;
            }
        self.file_positions.get(path).copied().unwrap_or(Position {
            chunk: 0,
            offset: 0,
            absolute: self.chunks.first().map_or(0, |c| c.insert_pos),
        })
    }

    // Index et assemblage

    fn chunk_index(&self) -> Vec<Vec<u8>> {
        let mut cncx = CncxBuilder::default();
        let mut w = IndexWriter::new(vec![(2, 1, 1), (3, 1, 2), (4, 1, 4), (6, 2, 8)]);
        for (seq, c) in self.chunks.iter().enumerate() {
            let values = HashMap::from([
                (2, vec![cncx.offset(&c.selector)]),
                (3, vec![c.file_number]),
                (4, vec![seq]),
                (6, vec![c.start_in_file, c.raw.len()]),
            ]);
            w.entries.push((format!("{:010}", c.insert_pos), values));
        }
        w.cncx_records = cncx.finish();
        w.records()
    }

    fn skeleton_index(&self) -> Vec<Vec<u8>> {
        let mut w = IndexWriter::new(vec![(1, 1, 3), (6, 2, 12)]);
        for s in &self.skeletons {
            let values = HashMap::from([(1, vec![s.chunk_count, s.chunk_count]), (6, vec![s.start, s.length, s.start, s.length])]);
            w.entries.push((format!("SKEL{:010}", s.file_number), values));
        }
        w.records()
    }

    fn ncx_index(&self, text_length: usize) -> Vec<Vec<u8>> {
        struct Flat {
            label: String,
            depth: usize,
            parent: Option<usize>,
            first_child: Option<usize>,
            last_child: Option<usize>,
            pos: Position,
        }
        let mut toc = self.reader.table_of_contents();
        if toc.is_empty() {
            toc = self
                .reader
                .spine
                .iter()
                .filter(|i| i.media_type.contains("html"))
                .enumerate()
                .map(|(n, i)| TocNode::new(format!("Partie {}", n + 1), i.path.clone()))
                .collect();
        }
        // Parcours en largeur : les enfants d'un même parent sont contigus.
        let mut flat: Vec<Flat> = Vec::new();
        let mut queue: Vec<(TocNode, usize, Option<usize>)> = toc.into_iter().map(|n| (n, 0, None)).collect();
        let mut q = 0;
        while q < queue.len() {
            let (node, depth, parent) = queue[q].clone();
            q += 1;
            let idx = flat.len();
            flat.push(Flat {
                label: if node.title.is_empty() { "—".into() } else { node.title.clone() },
                depth,
                parent,
                first_child: None,
                last_child: None,
                pos: self.position(&node.path, node.fragment.as_deref()),
            });
            if let Some(p) = parent {
                if flat[p].first_child.is_none() {
                    flat[p].first_child = Some(idx);
                }
                flat[p].last_child = Some(idx);
            }
            queue.extend(node.children.into_iter().map(|c| (c, depth + 1, Some(idx))));
        }
        let mut sorted: Vec<usize> = flat.iter().map(|f| f.pos.absolute).chain([text_length]).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let mut cncx = CncxBuilder::default();
        let mut w = IndexWriter::new(vec![(1, 1, 1), (2, 1, 2), (3, 1, 4), (4, 1, 8), (21, 1, 16), (22, 1, 32), (23, 1, 64), (6, 2, 128)]);
        let width = format!("{:X}", flat.len().saturating_sub(1)).len().max(2);
        for (i, e) in flat.iter().enumerate() {
            let next = sorted.iter().copied().find(|&o| o > e.pos.absolute).unwrap_or(text_length);
            let mut values = HashMap::from([
                (1, vec![e.pos.absolute]),
                (2, vec![next.saturating_sub(e.pos.absolute).max(1)]),
                (3, vec![cncx.offset(&e.label)]),
                (4, vec![e.depth]),
                (6, vec![e.pos.chunk, e.pos.offset]),
            ]);
            if let Some(p) = e.parent {
                values.insert(21, vec![p]);
            }
            if let Some(c) = e.first_child {
                values.insert(22, vec![c]);
            }
            if let Some(c) = e.last_child {
                values.insert(23, vec![c]);
            }
            w.entries.push((format!("{i:0width$X}"), values));
        }
        w.cncx_records = cncx.finish();
        w.records()
    }

    /// Découpe le texte en enregistrements de 4 Ko compressés ; un caractère UTF-8 coupé est recopié en fin
    /// d'enregistrement (« multibyte overlap »).
    fn text_records(bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut records = Vec::new();
        let mut pos = 0;
        while pos < bytes.len() {
            let end = (pos + RECORD_SIZE).min(bytes.len());
            let mut overlap: &[u8] = &[];
            if end < bytes.len() {
                // Octets de continuation (10xxxxxx) au début de l'enregistrement suivant.
                let mut k = end;
                while k < bytes.len() && k - end < 3 && bytes[k] & 0xC0 == 0x80 {
                    k += 1;
                }
                overlap = &bytes[end..k];
            }
            let mut rec = palmdoc::compress(&bytes[pos..end]);
            rec.extend_from_slice(overlap);
            rec.push(overlap.len() as u8);
            records.push(rec);
            pos = end;
        }
        records
    }

    fn assemble(&self, text_bytes: &[u8], fdst: &[(usize, usize)]) -> Vec<u8> {
        let mut records: Vec<Vec<u8>> = vec![Vec::new()];
        let text = Self::text_records(text_bytes);
        let text_count = text.len();
        let text_size: usize = text.iter().map(Vec::len).sum();
        records.extend(text);
        if !text_size.is_multiple_of(4) {
            records.push(vec![0; text_size % 4]);
        }
        let first_non_text = records.len();
        let chunk_start = records.len();
        records.extend(self.chunk_index());
        let skel_start = records.len();
        records.extend(self.skeleton_index());
        let ncx_start = records.len();
        records.extend(self.ncx_index(self.text.len()));
        let first_resource = if self.resource_records.is_empty() { MOBI_NULL } else { records.len() };
        records.extend(self.resource_records.iter().cloned());

        let fdst_record = records.len();
        let mut fdst_bytes = b"FDST".to_vec();
        fdst_bytes.push_be32(12);
        fdst_bytes.push_be32(fdst.len());
        for &(s, e) in fdst {
            fdst_bytes.push_be32(s);
            fdst_bytes.push_be32(e);
        }
        records.push(fdst_bytes);
        let flis_record = records.len();
        records.push(vec![
            0x46, 0x4C, 0x49, 0x53, 0, 0, 0, 8, 0, 0x41, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 1, 0, 3, 0, 0, 0, 3, 0, 0, 0, 1,
            0xFF, 0xFF, 0xFF, 0xFF,
        ]);
        let fcis_record = records.len();
        let mut fcis = vec![0x46, 0x43, 0x49, 0x53, 0, 0, 0, 0x14, 0, 0, 0, 0x10, 0, 0, 0, 2, 0, 0, 0, 0];
        fcis.push_be32(text_bytes.len());
        fcis.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0x28, 0, 0, 0, 0, 0, 0, 0, 0x28, 0, 0, 0, 8, 0, 1, 0, 1, 0, 0, 0, 0]);
        records.push(fcis);
        records.push(vec![0xE9, 0x8E, 0x0D, 0x0A]);

        records[0] = self.record_zero(RecordZero {
            text_length: text_bytes.len(),
            text_record_count: text_count,
            first_non_text,
            first_resource,
            fdst_record,
            fdst_count: fdst.len(),
            fcis: fcis_record,
            flis: flis_record,
            ncx: ncx_start,
            chunk: chunk_start,
            skel: skel_start,
        });
        self.palm_database(&records)
    }

    fn record_zero(&self, z: RecordZero) -> Vec<u8> {
        let exth = self.exth_block();
        let title = self.meta.title.as_bytes();
        let mut r: Vec<u8> = Vec::new();
        r.push_be16(2); // compression PalmDoc
        r.push_be16(0);
        r.push_be32(z.text_length);
        r.push_be16(z.text_record_count);
        r.push_be16(RECORD_SIZE);
        r.push_be16(0); // pas de chiffrement
        r.push_be16(0);
        r.push_str("MOBI");
        r.push_be32(264); // longueur d'en-tête MOBI
        r.push_be32(2); // type : livre
        r.push_be32(65001); // UTF-8
        r.push_be32((random::next_u64() & 0xFFFF_FFFF) as usize);
        r.push_be32(8); // version de fichier
        for _ in 0..10 {
            r.push_be32(MOBI_NULL); // 40–80 : index orth/infl + index annexes
        }
        r.push_be32(z.first_non_text); // 80
        r.push_be32(280 + exth.len()); // 84 : offset du titre
        r.push_be32(title.len()); // 88
        r.push_be32(language_code(self.meta.language.as_deref()));
        r.push_be32(0);
        r.push_be32(0); // 96 : langues de dictionnaire
        r.push_be32(8); // 104 : version minimale
        r.push_be32(z.first_resource); // 108
        for _ in 0..4 {
            r.push_be32(0); // 112 : HUFF
        }
        r.push_be32(0x50); // 128 : drapeaux EXTH
        r.extend_from_slice(&[0; 32]); // 132
        r.push_be32(MOBI_NULL); // 164
        r.push_be32(MOBI_NULL); // 168 : DRM
        r.push_be32(0);
        r.push_be32(0);
        r.push_be32(0);
        r.extend_from_slice(&[0; 8]); // 184
        r.push_be32(z.fdst_record); // 192
        r.push_be32(z.fdst_count);
        r.push_be32(z.fcis); // 200
        r.push_be32(1);
        r.push_be32(z.flis); // 208
        r.push_be32(1);
        r.extend_from_slice(&[0; 8]); // 216
        r.push_be32(MOBI_NULL); // 224 : SRCS
        r.push_be32(0);
        r.extend_from_slice(&[0xFF; 8]); // 232
        r.push_be32(1); // 240 : octets multi-octets en fin d'enregistrement
        r.push_be32(z.ncx); // 244
        r.push_be32(z.chunk); // 248
        r.push_be32(z.skel); // 252
        r.push_be32(MOBI_NULL); // 256 : DATP
        r.push_be32(MOBI_NULL); // 260 : guide
        r.push_be32(MOBI_NULL);
        r.push_be32(0);
        r.push_be32(MOBI_NULL);
        r.push_be32(0);
        debug_assert_eq!(r.len(), 280);
        r.extend(exth);
        r.extend_from_slice(title);
        r.extend(std::iter::repeat_n(0, 8192));
        r
    }

    fn exth_block(&self) -> Vec<u8> {
        let mut entries: Vec<(usize, Vec<u8>)> = Vec::new();
        let mut s = |kind: usize, value: Option<&str>| {
            if let Some(v) = value.filter(|v| !v.is_empty()) {
                entries.push((kind, v.as_bytes().to_vec()));
            }
        };
        let m = self.meta;
        for a in &m.authors {
            s(100, Some(a));
        }
        s(101, m.publisher.as_deref());
        s(103, m.summary.as_deref());
        s(104, m.isbn.as_deref());
        s(106, m.published_date.as_deref());
        for subject in &m.subjects {
            s(105, Some(subject));
        }
        s(113, Some(self.asin));
        s(501, Some("EBOK"));
        s(503, Some(&m.title));
        s(504, Some(self.asin));
        s(524, Some(m.language.as_deref().unwrap_or("fr")));
        s(525, Some("horizontal-lr"));
        let mut n = |kind: usize, value: usize| entries.push((kind, (value as u32).to_be_bytes().to_vec()));
        n(125, 0);
        if let Some(cover) = self.cover_resource() {
            n(201, cover);
            n(202, cover);
            n(203, 0);
        }
        let mut body = Vec::new();
        for (kind, value) in &entries {
            body.push_be32(*kind);
            body.push_be32(8 + value.len());
            body.extend_from_slice(value);
        }
        let mut out = b"EXTH".to_vec();
        out.push_be32(12 + body.len());
        out.push_be32(entries.len());
        out.extend(body);
        let pad = 4 - out.len() % 4;
        out.extend(std::iter::repeat_n(0, pad));
        out
    }

    fn cover_resource(&self) -> Option<usize> {
        self.reader.cover_item().and_then(|c| self.resource_index.get(&c.path)).map(|(i, _)| *i)
    }

    fn palm_database(&self, records: &[Vec<u8>]) -> Vec<u8> {
        let mut name = palm_name(&self.meta.title, true);
        if name[0] == 0 {
            name = palm_name("book", true);
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as usize
            & 0xFFFF_FFFF;
        let mut out = name;
        out.push_be16(0);
        out.push_be16(0);
        out.push_be32(now);
        out.push_be32(now);
        for _ in 0..4 {
            out.push_be32(0);
        }
        out.push_str("BOOKMOBI");
        out.push_be32(2 * records.len() - 1);
        out.push_be32(0);
        out.push_be16(records.len());
        let mut offset = out.len() + 8 * records.len() + 2;
        for (i, r) in records.iter().enumerate() {
            out.push_be32(offset);
            out.push(0);
            let uid = 2 * i;
            out.extend_from_slice(&[(uid >> 16) as u8, (uid >> 8) as u8, uid as u8]);
            offset += r.len();
        }
        out.extend_from_slice(&[0, 0]);
        for r in records {
            out.extend_from_slice(r);
        }
        out
    }
}

struct RecordZero {
    text_length: usize,
    text_record_count: usize,
    first_non_text: usize,
    first_resource: usize,
    fdst_record: usize,
    fdst_count: usize,
    fcis: usize,
    flis: usize,
    ncx: usize,
    chunk: usize,
    skel: usize,
}
