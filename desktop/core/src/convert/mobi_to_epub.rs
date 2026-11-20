//! Conversion MOBI (KF7) / AZW3 (KF8) → EPUB, inspirée de KindleUnpack (`Conversion/MobiToEPUB.swift`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::epub::dom;
use crate::epub::writer::EpubBuilder;
use crate::error::{BookError, Result};
use crate::metadata::{MetadataEdits, TocNode};
use crate::mobi::book::{MobiBook, MobiHeader};
use crate::mobi::index::{IndexEntry, IndexReader};
use crate::util::{archive_path, bytes::base32, bytes::Bytes, html, image};

/// `edits` : informations corrigées dans l'app, écrites dans l'EPUB produit.
pub fn convert(input: &Path, output: &Path, edits: Option<&MetadataEdits>) -> Result<()> {
    let mut epub = build(&MobiBook::open(input)?)?;
    if let Some(e) = edits.map(MetadataEdits::normalized).filter(|e| !e.title.is_empty()) {
        epub.metadata.apply(&e);
    }
    epub.write(output)
}

pub fn build(book: &MobiBook) -> Result<EpubBuilder> {
    if book.is_drm_protected() {
        return Err(BookError::DrmProtected);
    }
    let mut meta = book.metadata();
    meta.cover_data = None;
    let mut epub = EpubBuilder::new(meta);
    let resources = extract_resources(book);
    for r in resources.values() {
        epub.add(r.path.clone(), r.media_type.clone(), r.data.clone(), false);
    }
    if let Some(r) = book.cover_resource_index().and_then(|c| resources.get(&c)) {
        epub.cover_path = Some(r.path.clone());
    }
    let paths: HashMap<usize, String> = resources.iter().map(|(k, r)| (*k, r.path.clone())).collect();
    match &book.kf8 {
        Some(kf8) => Kf8Unpacker { book, header: kf8, resource_paths: &paths }.build(&mut epub)?,
        None => kf7_build(book, &paths, &mut epub)?,
    }
    Ok(epub)
}

struct Resource {
    path: String,
    media_type: String,
    data: Vec<u8>,
}

/// Images et polices, indexées relativement au premier enregistrement de ressource.
fn extract_resources(book: &MobiBook) -> BTreeMap<usize, Resource> {
    let mut out = BTreeMap::new();
    let Some(first) = book.first_resource() else { return out };
    for i in first..book.records.len() {
        let r = &book.records[i];
        if r.starts_with(b"BOUNDARY") || r.as_slice() == [0xE9, 0x8E, 0x0D, 0x0A] {
            break;
        }
        let rel = i - first;
        let name = format!("{:05}", rel + 1);
        if let Some(mime) = image::mime_type(r) {
            out.insert(rel, Resource { path: format!("Images/image{name}.{}", image::file_extension(mime)), media_type: mime.into(), data: r.clone() });
        } else if r.starts_with(b"FONT")
            && let Some(font) = decode_font(r) {
                let otf = font.starts_with(b"OTTO");
                out.insert(rel, Resource {
                    path: format!("Fonts/font{name}.{}", if otf { "otf" } else { "ttf" }),
                    media_type: if otf { "font/otf" } else { "font/ttf" }.into(),
                    data: font,
                });
            }
    }
    out
}

/// Enregistrement FONT : obfuscation XOR éventuelle puis compression zlib éventuelle.
fn decode_font(r: &[u8]) -> Option<Vec<u8>> {
    let (size, flags, data_start) = (r.u32_at(4), r.u32_at(8), r.u32_at(12));
    let (xor_length, xor_start) = (r.u32_at(16), r.u32_at(20));
    if data_start >= r.len() {
        return None;
    }
    let mut font = r[data_start..].to_vec();
    if flags & 2 != 0 && xor_length > 0 {
        let key = r.slice_at(xor_start, xor_length).to_vec();
        if !key.is_empty() {
            for k in 0..font.len().min(1040) {
                font[k] ^= key[k % key.len()];
            }
        }
    }
    if flags & 1 != 0 {
        if font.len() <= 2 || size == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(size.min(1 << 26));
        flate2::read::DeflateDecoder::new(&font[2..]).take(size as u64).read_to_end(&mut out).ok()?;
        return (!out.is_empty()).then_some(out);
    }
    Some(font)
}

/// Arbre de la table des matières à partir des entrées NCX (parent = index, None pour la racine).
fn toc_tree(flat: Vec<(TocNode, Option<usize>)>) -> Vec<TocNode> {
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, (_, parent)) in flat.iter().enumerate() {
        match parent {
            Some(p) if *p < flat.len() && *p != i => children.entry(*p).or_default().push(i),
            _ => roots.push(i),
        }
    }
    fn make(i: usize, depth: usize, flat: &[(TocNode, Option<usize>)], children: &HashMap<usize, Vec<usize>>) -> TocNode {
        let mut n = flat[i].0.clone();
        n.children = if depth > 10 {
            Vec::new()
        } else {
            children.get(&i).map(|c| c.iter().map(|&k| make(k, depth + 1, flat, children)).collect()).unwrap_or_default()
        };
        n
    }
    roots.into_iter().map(|i| make(i, 0, &flat, &children)).collect()
}

fn parent(e: &IndexEntry) -> Option<usize> {
    e.first(21)
}

// KF8

struct Fragment {
    insert_pos: usize,
    selector: String,
    file_number: usize,
    length: usize,
}

struct Part {
    filename: String,
    bytes: Vec<u8>,
    start: usize,
    end: usize,
}

struct Kf8Unpacker<'a> {
    book: &'a MobiBook,
    header: &'a MobiHeader,
    resource_paths: &'a HashMap<usize, String>,
}

static POS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"kindle:pos:fid:([0-9A-Va-v]{4}):off:([0-9A-Va-v]{10})").unwrap());
static AID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\s(?:aid|AID)\s*=\s*['"]([^'"]+)['"]"#).unwrap());
static ID_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)^<[^>]*\sid\s*=\s*['"]([^'"]*)['"]"#).unwrap());
static NAME_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)^<[^>]*\sname\s*=\s*['"]([^'"]*)['"]"#).unwrap());
static AID_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^<[^>]+\s(?:aid|AID)\s*=\s*['"]([^'"]+)['"]"#).unwrap());
static EMBED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"kindle:embed:([0-9A-Va-v]{4})(\?mime=[^'")]*)?"#).unwrap());
static FLOW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"kindle:flow:([0-9A-Va-v]{4})(\?mime=[^'")]*)?"#).unwrap());

impl Kf8Unpacker<'_> {
    fn build(&self, epub: &mut EpubBuilder) -> Result<()> {
        let raw = self.book.text(self.header)?;
        let flows = self.split_flows(&raw);
        let (Some(skel_index), Some(frag_index)) = (self.header.skeleton_index(), self.header.fragment_index()) else {
            return Err(BookError::Unreadable("index KF8 manquants".into()));
        };
        let skel = IndexReader::new(&self.book.records, skel_index)?;
        let frag = IndexReader::new(&self.book.records, frag_index)?;
        let mut fragments: Vec<Fragment> = frag
            .entries
            .iter()
            .map(|e| Fragment {
                insert_pos: String::from_utf8_lossy(&e.key).parse().unwrap_or(0),
                selector: String::from_utf8_lossy(frag.string(e.first(2))).into_owned(),
                file_number: e.first(3).unwrap_or(0),
                length: e.tags.get(&6).and_then(|v| v.get(1).copied()).unwrap_or(0),
            })
            .collect();
        let empty = Vec::new();
        let parts = self.build_parts(flows.first().unwrap_or(&empty), &skel.entries, &mut fragments);

        // Flux secondaires : feuilles de style ou SVG.
        let mut flow_paths: HashMap<usize, String> = HashMap::new();
        for (j, flow) in flows.iter().enumerate().skip(1) {
            let content = self.header.decode(flow);
            if content.contains("<svg") {
                let path = format!("Images/flow{j:05}.svg");
                epub.add(path.clone(), "image/svg+xml", content.into_bytes(), false);
                flow_paths.insert(j, path);
            } else {
                let path = format!("Styles/style{j:04}.css");
                epub.add(path.clone(), "text/css", self.rewrite_embeds(&content, &HashMap::new()).into_bytes(), false);
                flow_paths.insert(j, path);
            }
        }

        // Liens internes kindle:pos → fichier#id.
        let mut linked: HashSet<String> = HashSet::new();
        let mut texts: Vec<String> = parts.iter().map(|p| self.header.decode(&p.bytes)).collect();
        for text in texts.iter_mut() {
            *text = POS
                .replace_all(text, |c: &Captures| {
                    let (file, id) = Self::target(base32::decode(&c[1]), base32::decode(&c[2]), &fragments, &parts, &mut linked);
                    match file {
                        Some(f) => archive_path::relative(&f, "Text") + &if id.is_empty() { String::new() } else { format!("#{id}") },
                        None => "#".into(),
                    }
                })
                .into_owned();
        }
        for (i, text) in texts.iter_mut().enumerate() {
            let with_ids = AID.replace_all(text, |c: &Captures| {
                if linked.contains(&c[1]) { format!(" id=\"aid-{}\"", &c[1]) } else { String::new() }
            });
            *text = self.rewrite_embeds(&with_ids, &flow_paths);
            epub.add(parts[i].filename.clone(), "application/xhtml+xml", text.clone().into_bytes(), true);
        }

        if let Some(index) = self.header.ncx_index().and_then(|n| IndexReader::new(&self.book.records, n).ok()) {
            let mut flat = Vec::new();
            for e in &index.entries {
                let label = self.header.decode(index.string(e.first(3)));
                let mut path = parts.first().map(|p| p.filename.clone()).unwrap_or_default();
                let mut fragment = None;
                if let Some(pf) = e.tags.get(&6).filter(|v| v.len() >= 2) {
                    let mut ignored = HashSet::new();
                    let (file, id) = Self::target(pf[0], pf[1], &fragments, &parts, &mut ignored);
                    if let Some(f) = file {
                        path = f;
                    }
                    if !id.is_empty() {
                        // Les ancres « aid » n'existent que si elles ont été liées dans le texte.
                        let unlinked = id.strip_prefix("aid-").is_some_and(|a| !linked.contains(a));
                        fragment = (!unlinked).then_some(id);
                    }
                }
                flat.push((TocNode { title: label, path, fragment, children: Vec::new() }, parent(e)));
            }
            epub.toc = toc_tree(flat);
        }
        Ok(())
    }

    fn split_flows(&self, raw: &[u8]) -> Vec<Vec<u8>> {
        let Some(r) = self.header.fdst_index().and_then(|f| self.book.records.get(f)).filter(|r| r.starts_with(b"FDST")) else {
            return vec![raw.to_vec()];
        };
        (0..r.u32_at(8))
            .map(|j| {
                let start = r.u32_at(12 + 8 * j).min(raw.len());
                let end = r.u32_at(16 + 8 * j).min(raw.len()).max(start);
                raw[start..end].to_vec()
            })
            .collect()
    }

    fn build_parts(&self, text: &[u8], skeletons: &[IndexEntry], fragments: &mut [Fragment]) -> Vec<Part> {
        let mut parts = Vec::new();
        let mut frag_ptr = 0;
        for (file_number, s) in skeletons.iter().enumerate() {
            let count = s.first(1).unwrap_or(0);
            let skel_pos = s.first(6).unwrap_or(0);
            let skel_len = s.tags.get(&6).and_then(|v| v.get(1).copied()).unwrap_or(0);
            let mut base = skel_pos + skel_len;
            let mut skeleton = text.slice_at(skel_pos, skel_len).to_vec();
            for _ in 0..count {
                if frag_ptr >= fragments.len() {
                    break;
                }
                let f = &mut fragments[frag_ptr];
                let slice = text.slice_at(base, f.length).to_vec();
                let mut insert = f.insert_pos.saturating_sub(skel_pos).min(skeleton.len());
                if inside_tag(&skeleton, insert)
                    && let Some(fixed) = locate_after_tag(&skeleton, &f.selector) {
                        insert = fixed;
                        f.insert_pos = fixed + skel_pos;
                    }
                skeleton.splice(insert..insert, slice);
                base += f.length;
                frag_ptr += 1;
            }
            parts.push(Part { filename: format!("Text/part{file_number:04}.xhtml"), bytes: skeleton, start: skel_pos, end: base });
        }
        parts
    }

    /// Convertit (fid, off) en (fichier, id) en cherchant l'ancre la plus proche avant la position.
    fn target(fid: usize, off: usize, fragments: &[Fragment], parts: &[Part], linked: &mut HashSet<String>) -> (Option<String>, String) {
        let Some(f) = fragments.get(fid) else { return (parts.first().map(|p| p.filename.clone()), String::new()) };
        let mut pos = f.insert_pos + off;
        let index = match parts.iter().position(|p| pos >= p.start && pos < p.end) {
            Some(i) => i,
            None => {
                if f.file_number >= parts.len() {
                    return (parts.first().map(|p| p.filename.clone()), String::new());
                }
                pos = parts[f.file_number].start;
                f.file_number
            }
        };
        let part = &parts[index];
        (Some(part.filename.clone()), id_tag(&part.bytes, pos - part.start, linked))
    }

    /// kindle:embed / kindle:flow → chemins relatifs (depuis Text/ ou Styles/, même profondeur).
    fn rewrite_embeds(&self, s: &str, flow_paths: &HashMap<usize, String>) -> String {
        let out = EMBED.replace_all(s, |c: &Captures| {
            base32::decode(&c[1]).checked_sub(1).and_then(|i| self.resource_paths.get(&i)).map(|p| format!("../{p}")).unwrap_or_default()
        });
        FLOW.replace_all(&out, |c: &Captures| flow_paths.get(&base32::decode(&c[1])).map(|p| format!("../{p}")).unwrap_or_default())
            .into_owned()
    }
}

fn rfind(b: &[u8], byte: u8, before: usize) -> Option<usize> {
    b[..before.min(b.len())].iter().rposition(|&x| x == byte)
}

fn find(b: &[u8], byte: u8, from: usize) -> Option<usize> {
    b.get(from..)?.iter().position(|&x| x == byte).map(|p| p + from)
}

fn inside_tag(b: &[u8], pos: usize) -> bool {
    let lt = rfind(b, b'<', pos).map(|p| p as isize).unwrap_or(-1);
    let gt = rfind(b, b'>', pos).map(|p| p as isize).unwrap_or(-1);
    lt > gt
}

/// Position juste après la balise portant `aid` (sélecteur `P-//*[@aid='X']`).
fn locate_after_tag(b: &[u8], selector: &str) -> Option<usize> {
    let mut quotes = selector.splitn(3, '\'');
    quotes.next()?;
    let aid = quotes.next()?;
    for quote in ['"', '\''] {
        let pattern = format!("aid={quote}{aid}{quote}");
        if let Some(p) = b.find_bytes(pattern.as_bytes(), 0)
            && let Some(gt) = find(b, b'>', p) {
                return Some(gt + 1);
            }
    }
    None
}

fn id_tag(block: &[u8], position: usize, linked: &mut HashSet<String>) -> String {
    let mut npos = position.min(block.len());
    let gt = find(block, b'>', npos).unwrap_or(block.len());
    let lt = find(block, b'<', npos).unwrap_or(block.len());
    if lt == npos || gt < lt {
        npos = (gt + 1).min(block.len());
    }
    let mut end = npos;
    while let Some(close) = rfind(block, b'>', end) {
        let Some(open) = rfind(block, b'<', close) else { break };
        let tag = String::from_utf8_lossy(&block[open..=close]).into_owned();
        end = open;
        if tag.starts_with("<body ") || tag == "<body>" {
            return String::new();
        }
        if tag.starts_with("<meta ") || tag.starts_with("</") {
            continue;
        }
        if let Some(c) = ID_TAG.captures(&tag).or_else(|| NAME_TAG.captures(&tag)) {
            return c[1].to_string();
        }
        if let Some(c) = AID_TAG.captures(&tag) {
            linked.insert(c[1].to_string());
            return format!("aid-{}", &c[1]);
        }
    }
    String::new()
}

// KF7

static FILEPOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)filepos=['"]?0*(\d+)"#).unwrap());
static RECINDEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)\srecindex=['"]?0*(\d+)['"]?"#).unwrap());
static BODY_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<body[^>]*>").unwrap());
static BODY_CLOSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)</body>").unwrap());
static ANCHOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"id="(filepos\d+)""#).unwrap());
static HREF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r##"href="#(filepos\d+)""##).unwrap());
static CLEANUP: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"(?is)<guide[^>]*>.*?</guide>", ""),
        (r#"(?is)(<a\b[^>]*?)\s+filepos=['"]?0*(\d+)['"]?"#, "$1 href=\"#filepos$2\""),
        (r#"(?is)\s(?:hi|lo)recindex=['"]?\d+['"]?"#, ""),
        (r"(?is)<mbp:pagebreak[^>]*>", "\u{1}PAGEBREAK\u{1}"),
        (r"(?is)</?mbp:[^>]*>", ""),
    ]
    .into_iter()
    .map(|(p, t)| (Regex::new(p).unwrap(), t))
    .collect()
});

/// Octets vus comme du Latin-1 : un caractère par octet, pour travailler sans changer les positions.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn from_latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

fn kf7_build(book: &MobiBook, resource_paths: &HashMap<usize, String>, epub: &mut EpubBuilder) -> Result<()> {
    let header = &book.kf7;
    let mut html = book.text(header)?;

    // 1. Positions ciblées par les liens filepos et par le NCX.
    let mut targets: HashSet<usize> = FILEPOS.captures_iter(&latin1(&html)).filter_map(|c| c[1].parse().ok()).collect();
    let mut ncx: Option<IndexReader> = None;
    if let Some(index) = header.ncx_index().and_then(|n| IndexReader::new(&book.records, n).ok()) {
        targets.extend(index.entries.iter().filter_map(|e| e.first(1)));
        ncx = Some(index);
    }

    // 2. Ancres insérées de la fin vers le début pour préserver les positions.
    let mut sorted: Vec<usize> = targets.into_iter().filter(|&p| p < html.len()).collect();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    for pos in sorted {
        let mut p = pos;
        let lt = rfind(&html, b'<', p).map(|x| x as isize).unwrap_or(-1);
        let gt = rfind(&html, b'>', p).map(|x| x as isize).unwrap_or(-1);
        if lt > gt {
            p = lt as usize;
        }
        let anchor = format!("<a id=\"filepos{pos}\"></a>").into_bytes();
        html.splice(p..p, anchor);
    }

    // 3. Nettoyage du balisage propre au MOBI, en Latin-1 pour rester octet par octet.
    let mut s = latin1(&html);
    for (re, template) in CLEANUP.iter() {
        s = re.replace_all(&s, *template).into_owned();
    }
    s = RECINDEX
        .replace_all(&s, |c: &Captures| {
            c[1].parse::<usize>()
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| resource_paths.get(&i))
                .map(|p| format!(" src=\"../{p}\""))
                .unwrap_or_default()
        })
        .into_owned();
    if let Some(m) = BODY_OPEN.find(&s) {
        s = s[m.end()..].to_string();
    }
    if let Some(m) = BODY_CLOSE.find_iter(&s).last() {
        s = s[..m.start()].to_string();
    }

    // 4. Découpage en chapitres et retour à l'encodage réel.
    let chunks: Vec<String> =
        s.split("\u{1}PAGEBREAK\u{1}").map(|chunk| header.decode(&from_latin1(chunk))).filter(|c| !c.trim().is_empty()).collect();
    let names: Vec<String> = (0..chunks.len()).map(|i| format!("Text/chapter{i:04}.xhtml")).collect();
    let mut anchor_file: HashMap<String, String> = HashMap::new();
    for (i, chunk) in chunks.iter().enumerate() {
        for c in ANCHOR.captures_iter(chunk) {
            anchor_file.insert(c[1].to_string(), names[i].clone());
        }
    }
    let title = epub.metadata.title.clone();
    for (i, chunk) in chunks.iter().enumerate() {
        let linked = HREF.replace_all(chunk, |c: &Captures| match anchor_file.get(&c[1]) {
            Some(file) => format!("href=\"{}#{}\"", archive_path::relative(file, "Text"), &c[1]),
            None => "href=\"#\"".into(),
        });
        epub.add(names[i].clone(), "application/xhtml+xml", xhtml(&linked, &title).into_bytes(), true);
    }

    // 5. Table des matières.
    if let Some(index) = ncx {
        let mut flat = Vec::new();
        for e in &index.entries {
            let Some(pos) = e.first(1) else { continue };
            let key = format!("filepos{pos}");
            let Some(file) = anchor_file.get(&key) else { continue };
            let label = header.decode(index.string(e.first(3)));
            flat.push((TocNode { title: label, path: file.clone(), fragment: Some(key), children: Vec::new() }, parent(e)));
        }
        epub.toc = toc_tree(flat);
    }
    Ok(())
}

/// Fragment HTML 3.2 → XHTML bien formé, lu comme le ferait un navigateur.
fn xhtml(body: &str, title: &str) -> String {
    let source = format!("<html><head><title>{}</title></head><body>{body}</body></html>", crate::util::bytes::xml_escaped(title));
    let mut root = dom::parse_html(&source);
    if root.children.is_empty() {
        let text = crate::util::bytes::xml_escaped(&html::strip_html(body));
        return format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{}</title></head><body><p>{text}</p></body></html>",
            crate::util::bytes::xml_escaped(title)
        );
    }
    root.namespaces = vec![(String::new(), "http://www.w3.org/1999/xhtml".into())];
    format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{}", root.to_xml())
}
