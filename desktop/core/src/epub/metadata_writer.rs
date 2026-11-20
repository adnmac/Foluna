//! Copie d'un EPUB dont l'OPF porte les corrections de l'app (`EPUB/EPUBMetadataWriter.swift`) : ce qu'une Kobo
//! ou une liseuse EPUB affiche. Le fichier source n'est jamais modifié ; un champ vide garde la valeur du livre.

use std::collections::HashSet;
use std::path::Path;

use crate::epub::dom::{self, Element, Node};
use crate::epub::writer::{format_index, ZipOut};
use crate::epub::xml::{self, attr, descendants};
use crate::epub::zip_io::ZipReader;
use crate::error::{BookError, Result};
use crate::metadata::MetadataEdits;
use crate::util::isbn;

const DC: &str = "http://purl.org/dc/elements/1.1/";
const OPF: &str = "http://www.idpf.org/2007/opf";

pub fn rewrite(source: &Path, edits: &MetadataEdits, output: &Path) -> Result<()> {
    let zip = ZipReader::open(source)?;
    let container_text = xml::prepare(&zip.data("META-INF/container.xml")?);
    let container = xml::parse(&container_text)?;
    let opf_path = descendants(container.root(), "rootfile")
        .find_map(|r| attr(r, "full-path"))
        .ok_or_else(|| BookError::Unreadable("container.xml sans rootfile".into()))?
        .to_string();
    let mut package = dom::parse_xml(&zip.data(&opf_path)?)?;
    if package.child("metadata").is_none() {
        return Err(BookError::Unreadable("OPF sans métadonnées".into()));
    }
    apply(edits, &mut package);
    let opf = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", package.to_xml());

    let tmp = output.with_file_name(format!(".{}.tmp", output.file_name().and_then(|n| n.to_str()).unwrap_or("copie")));
    let mut writer = ZipOut::create(&tmp)?;
    // `mimetype` d'abord, sans compression (exigence EPUB).
    let mimetype = zip.data("mimetype").unwrap_or_else(|_| b"application/epub+zip".to_vec());
    writer.add("mimetype", &mimetype, false)?;
    for path in zip.names().iter().filter(|p| *p != "mimetype" && !p.ends_with('/')) {
        if *path == opf_path {
            writer.add(path, opf.as_bytes(), true)?;
        } else {
            writer.add(path, &zip.data(path)?, true)?;
        }
    }
    writer.finish()?;
    let _ = std::fs::remove_file(output);
    std::fs::rename(&tmp, output)?;
    Ok(())
}

fn text(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(String::from)
}

/// `dc:<name>` avec sa déclaration d'espace de noms, valable quel que soit le préfixe choisi par l'OPF.
fn dc_element(name: &str, value: &str) -> Element {
    let mut el = Element::new(format!("dc:{name}"));
    el.namespaces.push(("dc".into(), DC.into()));
    el.set_text(value);
    el
}

fn meta(property: &str, refines: Option<&str>, scheme: Option<&str>, value: &str) -> Element {
    let mut m = Element::new("meta");
    m.set_attr("property", property);
    if let Some(r) = refines {
        m.set_attr("refines", r);
    }
    if let Some(s) = scheme {
        m.set_attr("scheme", s);
    }
    m.set_text(value);
    m
}

/// Remplace tous les `dc:<name>` par les valeurs données.
fn replace(metadata: &mut Element, name: &str, values: &[String]) -> Vec<usize> {
    metadata.children.retain(|c| !matches!(c, Node::Element(e) if e.local_name() == name));
    values
        .iter()
        .map(|v| {
            metadata.children.push(Node::Element(dc_element(name, v)));
            metadata.children.len() - 1
        })
        .collect()
}

pub fn apply(e: &MetadataEdits, package: &mut Element) {
    let is_epub3 = package.attr("version").unwrap_or_default().starts_with('3');
    let unique = package.attr("unique-identifier").map(String::from);
    let Some(metadata) = package.child_mut("metadata") else { return };
    if let Some(title) = text(&Some(e.title.clone())) {
        replace(metadata, "title", &[title]);
    }
    let authors: Vec<String> = e.authors.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
    if !authors.is_empty() {
        // Les précisions EPUB 3 (rôle, tri) des anciens auteurs partent avec eux.
        let old: HashSet<String> =
            metadata.elements().filter(|c| c.local_name() == "creator").filter_map(|c| c.attr("id")).map(|id| format!("#{id}")).collect();
        metadata.children.retain(|c| !matches!(c, Node::Element(m) if m.local_name() == "meta" && m.attr("refines").is_some_and(|r| old.contains(r))));
        let added = replace(metadata, "creator", &authors);
        for (i, idx) in added.into_iter().enumerate() {
            if is_epub3 {
                let id = format!("alx-creator-{}", i + 1);
                if let Node::Element(el) = &mut metadata.children[idx] {
                    el.set_attr("id", id.clone());
                }
                metadata.children.push(Node::Element(meta("role", Some(&format!("#{id}")), Some("marc:relators"), "aut")));
            } else if let Node::Element(el) = &mut metadata.children[idx] {
                el.namespaces.push(("opf".into(), OPF.into()));
                el.set_attr("opf:role", "aut");
            }
        }
    }
    for (name, value) in [("publisher", &e.publisher), ("date", &e.published_date), ("language", &e.language), ("description", &e.summary)] {
        if let Some(v) = text(value) {
            replace(metadata, name, &[v]);
        }
    }
    if let Some(isbn) = text(&e.isbn).and_then(|i| isbn::normalized(&i)) {
        // L'identifiant unique du paquet reste : seuls les autres ISBN sont remplacés.
        metadata.children.retain(|c| match c {
            Node::Element(el) if el.local_name() == "identifier" => {
                let t = el.text();
                el.attr("id") == unique.as_deref() || !(t.to_lowercase().contains("isbn") || isbn::normalized(&t).is_some())
            }
            _ => true,
        });
        metadata.children.push(Node::Element(dc_element("identifier", &format!("urn:isbn:{isbn}"))));
    }
    if let Some(series) = text(&e.series) {
        let removed_ids: HashSet<String> = metadata
            .elements()
            .filter(|m| is_series_meta(m))
            .filter_map(|m| m.attr("id"))
            .map(|id| format!("#{id}"))
            .collect();
        metadata.children.retain(|c| match c {
            Node::Element(m) if m.local_name() == "meta" => !is_series_meta(m) && !m.attr("refines").is_some_and(|r| removed_ids.contains(r)),
            _ => true,
        });
        let index = e.series_index.map(format_index);
        let mut calibre = Element::new("meta");
        calibre.set_attr("name", "calibre:series");
        calibre.set_attr("content", series.clone());
        metadata.children.push(Node::Element(calibre));
        if let Some(index) = &index {
            let mut i = Element::new("meta");
            i.set_attr("name", "calibre:series_index");
            i.set_attr("content", index.clone());
            metadata.children.push(Node::Element(i));
        }
        if is_epub3 {
            let mut collection = meta("belongs-to-collection", None, None, &series);
            collection.set_attr("id", "alx-series");
            metadata.children.push(Node::Element(collection));
            metadata.children.push(Node::Element(meta("collection-type", Some("#alx-series"), None, "series")));
            if let Some(index) = &index {
                metadata.children.push(Node::Element(meta("group-position", Some("#alx-series"), None, index)));
            }
        }
    }
}

fn is_series_meta(m: &Element) -> bool {
    m.local_name() == "meta"
        && (matches!(m.attr("name"), Some("calibre:series" | "calibre:series_index")) || m.attr("property") == Some("belongs-to-collection"))
}
