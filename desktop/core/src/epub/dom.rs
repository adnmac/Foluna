//! Arbre XML modifiable : ce que `XMLDocument`/`XMLElement` font pour le cœur Swift. Lu depuis du XML
//! (`roxmltree`) ou du HTML quelconque (`html5ever`, pour le HTML 3.2 des MOBI), écrit en XHTML.

use std::collections::HashSet;

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use crate::epub::xml;
use crate::error::Result;
use crate::util::bytes::xml_escaped;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    /// Nom qualifié tel qu'écrit (`dc:title`, `html`).
    pub name: String,
    /// Déclarations d'espaces de noms portées par cet élément : (préfixe, vide pour le défaut ; URI).
    pub namespaces: Vec<(String, String)>,
    /// Attributs : (nom qualifié, valeur).
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

/// Éléments vides par nature : écrits `<br/>` ; les autres toujours avec une balise fermante.
const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

impl Element {
    pub fn new(name: impl Into<String>) -> Self {
        Element { name: name.into(), ..Default::default() }
    }

    /// Nom sans préfixe.
    pub fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    /// Valeur d'attribut par nom local (ignore le préfixe).
    pub fn attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == local)
            .or_else(|| self.attrs.iter().find(|(n, _)| n.rsplit(':').next() == Some(local)))
            .map(|(_, v)| v.as_str())
    }

    pub fn set_attr(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|(n, _)| n == name) {
            Some(a) => a.1 = value,
            None => self.attrs.push((name.to_string(), value)),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(n, _)| n != name);
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| if let Node::Element(e) = c { Some(e) } else { None })
    }

    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.children.iter_mut().filter_map(|c| if let Node::Element(e) = c { Some(e) } else { None })
    }

    /// Premier enfant direct de ce nom local.
    pub fn child(&self, local: &str) -> Option<&Element> {
        self.elements().find(|e| e.local_name() == local)
    }

    pub fn child_mut(&mut self, local: &str) -> Option<&mut Element> {
        self.elements_mut().find(|e| e.local_name() == local)
    }

    /// Texte de l'élément et de ses descendants.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for c in &self.children {
            match c {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => out.push_str(&e.text()),
                Node::Comment(_) => {}
            }
        }
        out
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.children = vec![Node::Text(text.into())];
    }

    /// XML de l'élément : éléments vides par nature auto-fermants, les autres avec balise fermante.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    pub fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (prefix, uri) in &self.namespaces {
            if prefix.is_empty() {
                out.push_str(&format!(" xmlns=\"{}\"", xml_escaped(uri)));
            } else {
                out.push_str(&format!(" xmlns:{prefix}=\"{}\"", xml_escaped(uri)));
            }
        }
        for (n, v) in &self.attrs {
            out.push_str(&format!(" {n}=\"{}\"", xml_escaped(v)));
        }
        if self.children.is_empty() && VOID.contains(&self.local_name().to_ascii_lowercase().as_str()) {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            c.write(out);
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }
}

impl Node {
    pub fn write(&self, out: &mut String) {
        match self {
            Node::Element(e) => e.write(out),
            Node::Text(t) => out.push_str(&t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")),
            // « -- » est interdit dans un commentaire XML.
            Node::Comment(c) => out.push_str(&format!("<!--{}-->", c.replace("--", "- -"))),
        }
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}

// Lecture XML

/// Élément racine d'un document XML (entités HTML acceptées, DOCTYPE ignoré).
pub fn parse_xml(data: &[u8]) -> Result<Element> {
    let text = xml::prepare(data);
    let doc = xml::parse(&text)?;
    Ok(convert_xml(doc.root_element(), None))
}

fn qualified(node: roxmltree::Node, uri: Option<&str>, local: &str) -> String {
    match uri.and_then(|u| node.lookup_prefix(u)) {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}:{local}"),
        _ => local.to_string(),
    }
}

fn convert_xml(node: roxmltree::Node, parent: Option<roxmltree::Node>) -> Element {
    let tag = node.tag_name();
    let inherited: HashSet<(Option<&str>, &str)> =
        parent.map(|p| p.namespaces().map(|n| (n.name(), n.uri())).collect()).unwrap_or_default();
    let namespaces = node
        .namespaces()
        .filter(|n| !inherited.contains(&(n.name(), n.uri())) && n.name() != Some("xml"))
        .map(|n| (n.name().unwrap_or_default().to_string(), n.uri().to_string()))
        .collect();
    let attrs = node
        .attributes()
        .map(|a| {
            let name = match a.namespace() {
                Some("http://www.w3.org/XML/1998/namespace") => format!("xml:{}", a.name()),
                ns => qualified(node, ns, a.name()),
            };
            (name, a.value().to_string())
        })
        .collect();
    let children = node
        .children()
        .filter_map(|c| {
            if c.is_element() {
                Some(Node::Element(convert_xml(c, Some(node))))
            } else if c.is_text() {
                c.text().map(|t| Node::Text(t.to_string()))
            } else if c.is_comment() {
                c.text().map(|t| Node::Comment(t.to_string()))
            } else {
                None
            }
        })
        .collect();
    Element { name: qualified(node, tag.namespace(), tag.name()), namespaces, attrs, children }
}

// Lecture HTML

/// Document HTML quelconque (HTML 3.2 des MOBI KF7, balises mal fermées…) lu comme un navigateur.
pub fn parse_html(html: &str) -> Element {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let root = dom.document.children.borrow().iter().find_map(convert_html).and_then(|n| match n {
        Node::Element(e) => Some(e),
        _ => None,
    });
    root.unwrap_or_else(|| Element::new("html"))
}

fn convert_html(handle: &Handle) -> Option<Node> {
    match &handle.data {
        NodeData::Element { name, attrs, .. } => {
            let local = name.local.to_string();
            // Noms d'attributs valides en XML seulement (le HTML en accepte de bien plus libres).
            let attrs = attrs
                .borrow()
                .iter()
                .map(|a| (a.name.local.to_string(), a.value.to_string()))
                .filter(|(n, _)| is_xml_name(n) && !n.starts_with("xmlns"))
                .fold(Vec::<(String, String)>::new(), |mut acc, a| {
                    if !acc.iter().any(|(n, _)| *n == a.0) {
                        acc.push(a);
                    }
                    acc
                });
            let children = handle.children.borrow().iter().filter_map(convert_html).collect();
            if !is_xml_name(&local) {
                return None;
            }
            Some(Node::Element(Element { name: local, namespaces: Vec::new(), attrs, children }))
        }
        NodeData::Text { contents } => Some(Node::Text(contents.borrow().to_string())),
        NodeData::Comment { contents } => Some(Node::Comment(contents.to_string())),
        _ => None,
    }
}

fn is_xml_name(n: &str) -> bool {
    let mut chars = n.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || "-_.".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_round_trip() {
        let root = parse_xml(br#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>A &amp; B&nbsp;</dc:title><meta name="x"/></metadata></package>"#).unwrap();
        let xml = root.to_xml();
        assert!(xml.starts_with(r#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="#));
        assert!(xml.contains("<dc:title>A &amp; B\u{a0}</dc:title>"));
        assert!(xml.contains(r#"<meta name="x"></meta>"#) || xml.contains(r#"<meta name="x"/>"#));
        assert_eq!(root.child("metadata").unwrap().child("title").unwrap().text(), "A & B\u{a0}");
    }

    #[test]
    fn html_to_xhtml() {
        let root = parse_html("<html><body><p>Un<br>deux<p>trois &eacute;<img src=a.jpg></body>");
        let xml = root.to_xml();
        assert!(xml.contains("<p>Un<br/>deux</p><p>trois é<img src=\"a.jpg\"/></p>"), "{xml}");
    }
}
