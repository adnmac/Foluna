//! Construction d'un EPUB 3 avec toc.ncx pour les lecteurs EPUB 2 (`EPUB/EPUBWriter.swift`).
//! Les chemins des ressources sont relatifs au dossier `OEBPS/`.

use std::io::Write;
use std::path::Path;

use crate::error::{BookError, Result};
use crate::metadata::{BookMetadata, TocNode};
use crate::util::bytes::xml_escaped;

#[derive(Debug, Clone)]
pub struct Resource {
    pub path: String,
    pub media_type: String,
    pub data: Vec<u8>,
    pub in_spine: bool,
    pub properties: Vec<String>,
}

pub struct EpubBuilder {
    pub metadata: BookMetadata,
    pub resources: Vec<Resource>,
    pub toc: Vec<TocNode>,
    pub cover_path: Option<String>,
}

/// Écriture d'une archive ZIP ; le premier fichier est stocké sans compression (exigence EPUB pour `mimetype`).
pub struct ZipOut {
    zip: zip::ZipWriter<std::fs::File>,
    first: bool,
}

impl ZipOut {
    pub fn create(path: &Path) -> Result<Self> {
        let _ = std::fs::remove_file(path);
        Ok(ZipOut { zip: zip::ZipWriter::new(std::fs::File::create(path)?), first: true })
    }

    pub fn add(&mut self, path: &str, data: &[u8], compress: bool) -> Result<()> {
        let method = if compress && !self.first { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
        self.first = false;
        let options = zip::write::SimpleFileOptions::default().compression_method(method);
        self.zip.start_file(path, options).map_err(zip_error)?;
        self.zip.write_all(data)?;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        self.zip.finish().map_err(zip_error)?;
        Ok(())
    }
}

fn zip_error(e: zip::result::ZipError) -> BookError {
    BookError::ConversionFailed(e.to_string())
}

impl EpubBuilder {
    pub fn new(metadata: BookMetadata) -> Self {
        EpubBuilder { metadata, resources: Vec::new(), toc: Vec::new(), cover_path: None }
    }

    pub fn spine_paths(&self) -> Vec<&str> {
        self.resources.iter().filter(|r| r.in_spine).map(|r| r.path.as_str()).collect()
    }

    pub fn add(&mut self, path: impl Into<String>, media_type: impl Into<String>, data: Vec<u8>, in_spine: bool) {
        self.resources.push(Resource { path: path.into(), media_type: media_type.into(), data, in_spine, properties: Vec::new() });
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        // Identifiant figé une fois pour que l'OPF et le NCX concordent.
        let identifier = self.metadata.identifier.clone().unwrap_or_else(|| format!("urn:uuid:{}", uuid_v4()));
        let mut zip = ZipOut::create(path)?;
        zip.add("mimetype", b"application/epub+zip", false)?;
        zip.add(
            "META-INF/container.xml",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
            true,
        )?;
        for r in &self.resources {
            zip.add(&format!("OEBPS/{}", r.path), &r.data, true)?;
        }
        let toc = self.effective_toc();
        zip.add("OEBPS/nav.xhtml", self.nav_document(&toc).as_bytes(), true)?;
        zip.add("OEBPS/toc.ncx", self.ncx_document(&toc, &identifier).as_bytes(), true)?;
        zip.add("OEBPS/content.opf", self.opf_document(&identifier).as_bytes(), true)?;
        zip.finish()
    }

    /// La table ne peut pas être vide : à défaut, une entrée par document de la spine.
    fn effective_toc(&self) -> Vec<TocNode> {
        if !self.toc.is_empty() {
            return self.toc.clone();
        }
        self.spine_paths().iter().enumerate().map(|(i, p)| TocNode::new(format!("Partie {}", i + 1), *p)).collect()
    }

    fn opf_document(&self, identifier: &str) -> String {
        let mut ids = std::collections::HashMap::new();
        let mut manifest = String::new();
        for (i, r) in self.resources.iter().enumerate() {
            let id = format!("item{}", i + 1);
            ids.insert(r.path.as_str(), id.clone());
            let mut props = r.properties.clone();
            if self.cover_path.as_deref() == Some(r.path.as_str()) && !props.iter().any(|p| p == "cover-image") {
                props.push("cover-image".into());
            }
            if r.media_type == "application/xhtml+xml" && String::from_utf8_lossy(&r.data).contains("<svg") {
                props.push("svg".into());
            }
            let p = if props.is_empty() { String::new() } else { format!(" properties=\"{}\"", props.join(" ")) };
            manifest += &format!("    <item id=\"{id}\" href=\"{}\" media-type=\"{}\"{p}/>\n", xml_escaped(&r.path), r.media_type);
        }
        manifest += "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n";
        manifest += "    <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n";
        let spine: Vec<String> =
            self.spine_paths().iter().filter_map(|p| ids.get(p)).map(|id| format!("    <itemref idref=\"{id}\"/>")).collect();
        let m = &self.metadata;
        let mut meta = format!("    <dc:identifier id=\"bookid\">{}</dc:identifier>\n", xml_escaped(identifier));
        meta += &format!("    <dc:title>{}</dc:title>\n", xml_escaped(&m.title));
        for a in &m.authors {
            meta += &format!("    <dc:creator>{}</dc:creator>\n", xml_escaped(a));
        }
        meta += &format!("    <dc:language>{}</dc:language>\n", xml_escaped(m.language.as_deref().unwrap_or("fr")));
        if let Some(p) = &m.publisher {
            meta += &format!("    <dc:publisher>{}</dc:publisher>\n", xml_escaped(p));
        }
        if let Some(d) = &m.published_date {
            meta += &format!("    <dc:date>{}</dc:date>\n", xml_escaped(d));
        }
        if let Some(i) = &m.isbn {
            meta += &format!("    <dc:identifier>urn:isbn:{}</dc:identifier>\n", xml_escaped(i));
        }
        if let Some(s) = &m.series {
            // EPUB 3 (collection) et forme Calibre, lue par la plupart des logiciels.
            meta += &format!("    <meta property=\"belongs-to-collection\" id=\"series\">{}</meta>\n", xml_escaped(s));
            meta += "    <meta refines=\"#series\" property=\"collection-type\">series</meta>\n";
            meta += &format!("    <meta name=\"calibre:series\" content=\"{}\"/>\n", xml_escaped(s));
            if let Some(n) = m.series_index {
                let n = format_index(n);
                meta += &format!("    <meta refines=\"#series\" property=\"group-position\">{n}</meta>\n");
                meta += &format!("    <meta name=\"calibre:series_index\" content=\"{n}\"/>\n");
            }
        }
        if let Some(d) = &m.summary {
            meta += &format!("    <dc:description>{}</dc:description>\n", xml_escaped(d));
        }
        for s in &m.subjects {
            meta += &format!("    <dc:subject>{}</dc:subject>\n", xml_escaped(s));
        }
        meta += &format!("    <meta property=\"dcterms:modified\">{}</meta>\n", now_iso8601());
        if let Some(id) = self.cover_path.as_deref().and_then(|c| ids.get(c)) {
            meta += &format!("    <meta name=\"cover\" content=\"{id}\"/>\n");
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n{meta}  </metadata>\n  <manifest>\n{manifest}  </manifest>\n  <spine toc=\"ncx\">\n{}\n  </spine>\n</package>",
            spine.join("\n")
        )
    }

    fn href(n: &TocNode) -> String {
        xml_escaped(&n.path) + &n.fragment.as_ref().map(|f| format!("#{}", xml_escaped(f))).unwrap_or_default()
    }

    fn nav_document(&self, toc: &[TocNode]) -> String {
        fn list(nodes: &[TocNode], indent: &str) -> String {
            let mut s = format!("{indent}<ol>\n");
            for n in nodes {
                s += &format!("{indent}  <li><a href=\"{}\">{}</a>", EpubBuilder::href(n), xml_escaped(&n.title));
                if !n.children.is_empty() {
                    s += &format!("\n{}{indent}  ", list(&n.children, &format!("{indent}    ")));
                }
                s += "</li>\n";
            }
            s + &format!("{indent}</ol>\n")
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n<head><title>{}</title></head>\n<body>\n  <nav epub:type=\"toc\" id=\"toc\">\n    <h1>Table des matières</h1>\n{}  </nav>\n</body>\n</html>",
            xml_escaped(&self.metadata.title),
            list(toc, "    ")
        )
    }

    fn ncx_document(&self, toc: &[TocNode], identifier: &str) -> String {
        fn points(nodes: &[TocNode], indent: &str, order: &mut usize) -> String {
            let mut s = String::new();
            for n in nodes {
                *order += 1;
                let o = *order;
                s += &format!(
                    "{indent}<navPoint id=\"np{o}\" playOrder=\"{o}\">\n{indent}  <navLabel><text>{}</text></navLabel>\n{indent}  <content src=\"{}\"/>\n{}{indent}</navPoint>\n",
                    xml_escaped(&n.title),
                    EpubBuilder::href(n),
                    points(&n.children, &format!("{indent}  "), order)
                );
            }
            s
        }
        let mut order = 0;
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n  <head><meta name=\"dtb:uid\" content=\"{}\"/></head>\n  <docTitle><text>{}</text></docTitle>\n  <navMap>\n{}  </navMap>\n</ncx>",
            xml_escaped(identifier),
            xml_escaped(&self.metadata.title),
            points(toc, "    ", &mut order)
        )
    }
}

/// Numéro de série : « 2 » plutôt que « 2.0 ».
pub fn format_index(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// Date et heure UTC au format ISO 8601 (« 2026-11-20T21:19:43Z »), sans dépendance.
pub fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Jours depuis 1970 → date civile (algorithme de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Identifiant aléatoire au format UUID v4.
pub fn uuid_v4() -> String {
    let mut b = crate::util::random::bytes::<16>();
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}
