//! Lecture XML tolérante (`XMLLoader` et extensions de `XMLElement` du cœur Swift) : entités HTML converties,
//! DOCTYPE accepté, aucune ressource externe.

use roxmltree::{Document, Node, ParsingOptions};

use crate::error::{BookError, Result};
use crate::util::html;

/// Texte XML prêt pour `roxmltree` : UTF-8 (BOM retiré), entités HTML nommées en références numériques.
pub fn prepare(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    html::to_numeric(text.trim_start_matches('\u{FEFF}'))
}

pub fn parse(text: &str) -> Result<Document<'_>> {
    let options = ParsingOptions { allow_dtd: true, ..ParsingOptions::default() };
    Document::parse_with_options(text, options).map_err(|e| BookError::Unreadable(format!("XML invalide ({e})")))
}

/// Enfants directs par nom local (sans préfixe d'espace de noms).
pub fn children<'a, 'i>(node: Node<'a, 'i>, local: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children().filter(move |c| c.is_element() && c.tag_name().name() == local)
}

pub fn first_child<'a, 'i>(node: Node<'a, 'i>, local: &'a str) -> Option<Node<'a, 'i>> {
    children(node, local).next()
}

/// Descendants par nom local, en ordre de document.
pub fn descendants<'a, 'i>(node: Node<'a, 'i>, local: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.descendants().filter(move |c| c.is_element() && c.tag_name().name() == local && *c != node)
}

/// Valeur d'attribut par nom local (ignore le préfixe : `opf:role`, `xlink:href`…).
pub fn attr<'a>(node: Node<'a, '_>, local: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == local).map(|a| a.value())
}

/// Texte de l'élément et de ses descendants, sans les blancs de début et de fin.
pub fn text_content(node: Node) -> String {
    let s: String = node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
    s.trim().to_string()
}
