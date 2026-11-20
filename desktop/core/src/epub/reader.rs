//! Lecteur EPUB 2/3 : container.xml → OPF → manifeste, spine, métadonnées, table des matières (`EPUBReader.swift`).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::epub::xml::{self, attr, children, descendants, first_child, text_content};
use crate::epub::zip_io::ZipReader;
use crate::error::{BookError, Result};
use crate::metadata::{BookMetadata, TocNode};
use crate::util::{archive_path, isbn};

#[derive(Debug, Clone, PartialEq)]
pub struct EpubItem {
    pub id: String,
    /// Chemin complet dans l'archive.
    pub path: String,
    pub media_type: String,
    pub properties: HashSet<String>,
}

/// Élément de `<metadata>` gardé après lecture de l'OPF.
#[derive(Debug, Clone)]
struct MetaElement {
    name: String,
    attrs: HashMap<String, String>,
    text: String,
}

impl MetaElement {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

pub struct EpubReader {
    zip: ZipReader,
    pub opf_path: String,
    pub items: Vec<EpubItem>,
    pub spine: Vec<EpubItem>,
    metadata: Vec<MetaElement>,
    ncx_id: Option<String>,
}

impl EpubReader {
    pub fn open(path: &Path) -> Result<Self> {
        let zip = ZipReader::open(path)?;
        let container_text = xml::prepare(&zip.data("META-INF/container.xml")?);
        let container = xml::parse(&container_text)?;
        let opf_path = descendants(container.root(), "rootfile")
            .find_map(|r| attr(r, "full-path"))
            .ok_or_else(|| BookError::Unreadable("container.xml sans rootfile".into()))?
            .to_string();
        let mut reader = EpubReader { zip, opf_path, items: Vec::new(), spine: Vec::new(), metadata: Vec::new(), ncx_id: None };
        reader.parse_opf()?;
        Ok(reader)
    }

    fn parse_opf(&mut self) -> Result<()> {
        let text = xml::prepare(&self.zip.data(&self.opf_path)?);
        let doc = xml::parse(&text)?;
        let package = doc.root_element();
        let base_dir = archive_path::directory(&self.opf_path).to_string();
        if let Some(metadata) = first_child(package, "metadata") {
            self.metadata = metadata
                .children()
                .filter(|c| c.is_element())
                .map(|c| MetaElement {
                    name: c.tag_name().name().to_string(),
                    attrs: c.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect(),
                    text: text_content(c),
                })
                .collect();
        }
        let mut by_id = HashMap::new();
        if let Some(manifest) = first_child(package, "manifest") {
            for item in children(manifest, "item") {
                let (Some(id), Some(href)) = (attr(item, "id"), attr(item, "href")) else { continue };
                let entry = EpubItem {
                    id: id.to_string(),
                    path: archive_path::resolve(href, &base_dir).0,
                    media_type: attr(item, "media-type").unwrap_or_default().to_string(),
                    properties: attr(item, "properties").unwrap_or_default().split_whitespace().map(String::from).collect(),
                };
                by_id.insert(entry.id.clone(), entry.clone());
                self.items.push(entry);
            }
        }
        if let Some(spine) = first_child(package, "spine") {
            self.ncx_id = attr(spine, "toc").map(String::from);
            self.spine = children(spine, "itemref").filter_map(|r| attr(r, "idref").and_then(|id| by_id.get(id).cloned())).collect();
        }
        Ok(())
    }

    pub fn data(&self, path: &str) -> Result<Vec<u8>> {
        self.zip.data(path)
    }

    pub fn item_at_path(&self, path: &str) -> Option<&EpubItem> {
        self.items.iter().find(|i| i.path == path)
    }

    fn item_by_id(&self, id: &str) -> Option<&EpubItem> {
        self.items.iter().find(|i| i.id == id)
    }

    // Métadonnées

    fn dc(&self, name: &str) -> impl Iterator<Item = &MetaElement> {
        let name = name.to_string();
        self.metadata.iter().filter(move |m| m.name == name)
    }

    fn metas(&self) -> impl Iterator<Item = &MetaElement> {
        self.dc("meta")
    }

    pub fn cover_item(&self) -> Option<&EpubItem> {
        if let Some(item) = self.items.iter().find(|i| i.properties.contains("cover-image")) {
            return Some(item);
        }
        let cover_id = self.metas().find(|m| m.attr("name") == Some("cover")).and_then(|m| m.attr("content")).map(String::from);
        if let Some(id) = &cover_id {
            if let Some(item) = self.item_by_id(id).filter(|i| i.media_type.starts_with("image/")) {
                return Some(item);
            }
            // Certains EPUB désignent la couverture par son chemin plutôt que par son identifiant.
            if let Some(item) = self.items.iter().find(|i| i.path.ends_with(id.as_str())) {
                return Some(item);
            }
        }
        self.items.iter().find(|i| i.media_type.starts_with("image/") && i.path.to_lowercase().contains("cover"))
    }

    pub fn book_metadata(&self) -> BookMetadata {
        let mut meta = BookMetadata::new(self.dc("title").next().map(|m| m.text.clone()).unwrap_or_default());
        meta.authors = self
            .dc("creator")
            .filter(|m| matches!(m.attr("role"), None | Some("aut")))
            .map(|m| m.text.clone())
            .filter(|t| !t.is_empty())
            .collect();
        meta.language = self.dc("language").next().map(|m| m.text.clone());
        meta.publisher = self.dc("publisher").next().map(|m| m.text.clone());
        meta.summary = self.dc("description").next().map(|m| m.text.clone());
        meta.identifier = self.dc("identifier").next().map(|m| m.text.clone());
        let identifiers: Vec<String> = self.dc("identifier").map(|m| m.text.clone()).collect();
        meta.isbn = isbn::first(&identifiers);
        // EPUB 2 peut porter plusieurs dates : l'événement « publication » d'abord.
        let dates: Vec<&MetaElement> = self.dc("date").collect();
        meta.published_date =
            dates.iter().find(|d| d.attr("event") == Some("publication")).or(dates.first()).map(|d| d.text.clone());
        meta.subjects = self.dc("subject").map(|m| m.text.clone()).filter(|t| !t.is_empty()).collect();
        let metas: Vec<&MetaElement> = self.metas().collect();
        if let Some(series) = metas.iter().find(|m| m.attr("name") == Some("calibre:series")).and_then(|m| m.attr("content")) {
            meta.series = Some(series.to_string());
            meta.series_index = metas
                .iter()
                .find(|m| m.attr("name") == Some("calibre:series_index"))
                .and_then(|m| m.attr("content"))
                .and_then(|v| v.parse().ok());
        } else if let Some(coll) = metas.iter().find(|m| m.attr("property") == Some("belongs-to-collection")) {
            meta.series = Some(coll.text.clone());
            // EPUB 3 : la position est un <meta refines="#id" property="group-position">.
            if let Some(id) = coll.attr("id") {
                let refines = format!("#{id}");
                meta.series_index = metas
                    .iter()
                    .find(|m| m.attr("refines") == Some(refines.as_str()) && m.attr("property") == Some("group-position"))
                    .and_then(|m| m.text.replace(',', ".").parse().ok());
            }
        }
        if let Some(cover) = self.cover_item() {
            meta.cover_data = self.zip.data(&cover.path).ok();
        }
        meta.is_drm_protected = self.zip.contains("META-INF/encryption.xml") && self.has_encrypted_content();
        meta
    }

    /// `encryption.xml` sert aussi à l'obfuscation des polices, qui n'est pas un DRM.
    fn has_encrypted_content(&self) -> bool {
        let Ok(data) = self.zip.data("META-INF/encryption.xml") else { return false };
        let text = String::from_utf8_lossy(&data);
        let fonts = ["http://www.idpf.org/2008/embedding", "http://ns.adobe.com/pdf/enc#RC"];
        text.split("Algorithm=\"").skip(1).filter_map(|s| s.split('"').next()).any(|a| !fonts.contains(&a))
    }

    // Table des matières

    /// Table du document de navigation EPUB 3, sinon du NCX.
    pub fn table_of_contents(&self) -> Vec<TocNode> {
        if let Some(nav) = self.items.iter().find(|i| i.properties.contains("nav")) {
            let toc = self.parse_nav(&nav.path).unwrap_or_default();
            if !toc.is_empty() {
                return toc;
            }
        }
        let ncx = self
            .ncx_id
            .as_deref()
            .and_then(|id| self.item_by_id(id))
            .or_else(|| self.items.iter().find(|i| i.media_type == "application/x-dtbncx+xml"));
        ncx.and_then(|n| self.parse_ncx(&n.path).ok()).unwrap_or_default()
    }

    fn parse_nav(&self, path: &str) -> Result<Vec<TocNode>> {
        let text = xml::prepare(&self.zip.data(path)?);
        let doc = xml::parse(&text)?;
        let navs: Vec<_> = descendants(doc.root(), "nav").collect();
        let toc_nav = navs.iter().find(|n| attr(**n, "type").unwrap_or_default().contains("toc")).or(navs.first());
        let Some(ol) = toc_nav.and_then(|n| descendants(*n, "ol").next()) else { return Ok(Vec::new()) };
        let dir = archive_path::directory(path).to_string();
        fn walk(list: roxmltree::Node, dir: &str) -> Vec<TocNode> {
            children(list, "li")
                .filter_map(|li| {
                    let link = first_child(li, "a").or_else(|| first_child(li, "span"));
                    let title = link.map(text_content).unwrap_or_default();
                    let kids = first_child(li, "ol").map(|ol| walk(ol, dir)).unwrap_or_default();
                    match link.and_then(|l| attr(l, "href")) {
                        Some(href) => {
                            let (p, fragment) = archive_path::resolve(href, dir);
                            Some(TocNode { title, path: p, fragment, children: kids })
                        }
                        None if !kids.is_empty() => Some(TocNode {
                            title,
                            path: kids[0].path.clone(),
                            fragment: kids[0].fragment.clone(),
                            children: kids,
                        }),
                        None => None,
                    }
                })
                .collect()
        }
        Ok(walk(ol, &dir))
    }

    fn parse_ncx(&self, path: &str) -> Result<Vec<TocNode>> {
        let text = xml::prepare(&self.zip.data(path)?);
        let doc = xml::parse(&text)?;
        let Some(nav_map) = first_child(doc.root_element(), "navMap") else { return Ok(Vec::new()) };
        let dir = archive_path::directory(path).to_string();
        fn walk(el: roxmltree::Node, dir: &str) -> Vec<TocNode> {
            children(el, "navPoint")
                .filter_map(|point| {
                    let title = first_child(point, "navLabel").and_then(|l| first_child(l, "text")).map(text_content).unwrap_or_default();
                    let src = first_child(point, "content").and_then(|c| attr(c, "src"))?;
                    let (p, fragment) = archive_path::resolve(src, dir);
                    Some(TocNode { title, path: p, fragment, children: walk(point, dir) })
                })
                .collect()
        }
        Ok(walk(nav_map, &dir))
    }
}
