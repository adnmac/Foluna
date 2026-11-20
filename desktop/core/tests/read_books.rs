//! Lecture des livres (étape 2) sur des fichiers générés, comme `Tests/AlexandrieCoreTests` côté Mac.

use std::io::Write;
use std::path::{Path, PathBuf};

use foluna_core::epub::EpubReader;
use foluna_core::extract::extract;
use foluna_core::mobi::{palmdoc, MobiBook};
use foluna_core::BookFormat;

/// Image PNG de 1 × 1 pixel.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41,
    0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50, 0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// EPUB 3 avec NCX : 3 chapitres, couverture, série Calibre, ISBN, résumé en HTML, entité HTML dans un titre.
fn sample_epub(dir: &Path) -> PathBuf {
    let path = dir.join("Le Livre d'Essai.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default();
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    let mut manifest = String::new();
    let mut spine = String::new();
    for c in 1..=3 {
        manifest += &format!(r#"<item id="c{c}" href="Text/chapter{c}.xhtml" media-type="application/xhtml+xml"/>"#);
        spine += &format!(r#"<itemref idref="c{c}"/>"#);
    }
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="id">urn:uuid:1234</dc:identifier>
    <dc:identifier>urn:isbn:978-2-07-036024-6</dc:identifier>
    <dc:title>Le Livre d&apos;Essai&nbsp;!</dc:title>
    <dc:creator opf:role="aut">Jeanne Dupont</dc:creator>
    <dc:creator opf:role="ill">Paul Dessin</dc:creator>
    <dc:language>fr</dc:language>
    <dc:publisher>Éditions Test</dc:publisher>
    <dc:description>&lt;p&gt;Un résumé &lt;b&gt;riche&lt;/b&gt;.&lt;/p&gt;</dc:description>
    <dc:date>2003-11-01T05:00:00+00:00</dc:date>
    <dc:subject>Roman</dc:subject>
    <meta name="calibre:series" content="Les Rivières"/>
    <meta name="calibre:series_index" content="2"/>
    <meta name="cover" content="cover"/>
  </metadata>
  <manifest>
    <item id="cover" href="Images/cover.png" media-type="image/png"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    {manifest}
  </manifest>
  <spine toc="ncx">{spine}</spine>
</package>"#
    );
    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    zip.start_file("OEBPS/Images/cover.png", deflated).unwrap();
    zip.write_all(PNG).unwrap();
    let mut points = String::new();
    for c in 1..=3 {
        points += &format!(
            r#"<navPoint id="n{c}"><navLabel><text>Chapitre {c}</text></navLabel><content src="Text/chapter{c}.xhtml"/><navPoint id="n{c}b"><navLabel><text>Section {c}.5</text></navLabel><content src="Text/chapter{c}.xhtml#p5"/></navPoint></navPoint>"#
        );
        zip.start_file(format!("OEBPS/Text/chapter{c}.xhtml"), deflated).unwrap();
        zip.write_all(format!(r#"<?xml version="1.0"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapitre {c}</h1><p id="p5">Texte&nbsp;{c}</p></body></html>"#).as_bytes()).unwrap();
    }
    zip.start_file("OEBPS/toc.ncx", deflated).unwrap();
    zip.write_all(format!(r#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>{points}</navMap></ncx>"#).as_bytes()).unwrap();
    zip.finish().unwrap();
    path
}

#[test]
fn epub_metadata() {
    let dir = temp_dir();
    let path = sample_epub(dir.path());
    let meta = extract(&path).unwrap();
    assert_eq!(meta.title, "Le Livre d'Essai\u{a0}!");
    assert_eq!(meta.authors, ["Jeanne Dupont"]);
    assert_eq!(meta.language.as_deref(), Some("fr"));
    assert_eq!(meta.publisher.as_deref(), Some("Éditions Test"));
    assert_eq!(meta.summary.as_deref(), Some("Un résumé riche."));
    assert_eq!(meta.published_date.as_deref(), Some("2003-11-01"));
    assert_eq!(meta.isbn.as_deref(), Some("9782070360246"));
    assert_eq!(meta.series.as_deref(), Some("Les Rivières"));
    assert_eq!(meta.series_index, Some(2.0));
    assert_eq!(meta.subjects, ["Roman"]);
    assert_eq!(meta.cover_data.as_deref(), Some(PNG));
    assert!(!meta.is_drm_protected);
}

#[test]
fn epub_spine_and_toc() {
    let dir = temp_dir();
    let reader = EpubReader::open(&sample_epub(dir.path())).unwrap();
    assert_eq!(reader.spine.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
               ["OEBPS/Text/chapter1.xhtml", "OEBPS/Text/chapter2.xhtml", "OEBPS/Text/chapter3.xhtml"]);
    let toc = reader.table_of_contents();
    assert_eq!(toc.len(), 3);
    assert_eq!(toc[0].title, "Chapitre 1");
    assert_eq!(toc[0].children[0].fragment.as_deref(), Some("p5"));
    assert!(String::from_utf8(reader.data("OEBPS/Text/chapter2.xhtml").unwrap()).unwrap().contains("Chapitre 2"));
}

/// MOBI KF7 minimal : texte compressé PalmDoc, EXTH (titre, auteurs, ISBN, couverture), une image.
fn sample_mobi(dir: &Path, name: &str, encryption: u16) -> PathBuf {
    let text = "<html><body><p>Il était une fois un livre de test.</p></body></html>".repeat(20).into_bytes();
    let compressed = palmdoc::compress(&text);
    let full_name = "Nom long du livre".as_bytes();
    let mut exth: Vec<u8> = Vec::new();
    let mut count = 0u32;
    let mut add = |kind: u32, value: &[u8]| {
        exth.extend_from_slice(&kind.to_be_bytes());
        exth.extend_from_slice(&((value.len() + 8) as u32).to_be_bytes());
        exth.extend_from_slice(value);
        count += 1;
    };
    add(503, "Le Fleuve".as_bytes());
    add(100, "Jeanne Dupont & Paul Martin".as_bytes());
    add(101, "Éditions Test".as_bytes());
    add(104, b"9782070360246");
    add(106, b"1903");
    add(524, b"fr");
    add(201, &0u32.to_be_bytes());
    let exth_len = 12 + exth.len();
    let header_length = 0xE8usize;
    let mut r0 = vec![0u8; 16 + header_length];
    r0[0..2].copy_from_slice(&2u16.to_be_bytes()); // PalmDoc
    r0[4..8].copy_from_slice(&(text.len() as u32).to_be_bytes());
    r0[8..10].copy_from_slice(&1u16.to_be_bytes());
    r0[10..12].copy_from_slice(&4096u16.to_be_bytes());
    r0[12..14].copy_from_slice(&encryption.to_be_bytes());
    r0[16..20].copy_from_slice(b"MOBI");
    r0[20..24].copy_from_slice(&(header_length as u32).to_be_bytes());
    r0[24..28].copy_from_slice(&2u32.to_be_bytes());
    r0[28..32].copy_from_slice(&65001u32.to_be_bytes());
    r0[36..40].copy_from_slice(&6u32.to_be_bytes());
    let name_offset = 16 + header_length + exth_len;
    r0[0x54..0x58].copy_from_slice(&(name_offset as u32).to_be_bytes());
    r0[0x58..0x5C].copy_from_slice(&(full_name.len() as u32).to_be_bytes());
    r0[0x6C..0x70].copy_from_slice(&2u32.to_be_bytes()); // première ressource
    r0[0x70..0x74].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    r0[0x80..0x84].copy_from_slice(&0x40u32.to_be_bytes());
    r0.extend_from_slice(b"EXTH");
    r0.extend_from_slice(&(exth_len as u32).to_be_bytes());
    r0.extend_from_slice(&count.to_be_bytes());
    r0.extend_from_slice(&exth);
    r0.extend_from_slice(full_name);
    r0.extend_from_slice(&[0, 0]);
    let records: Vec<Vec<u8>> = vec![r0, compressed, PNG.to_vec()];
    let mut out = vec![0u8; 78];
    out[..8].copy_from_slice(b"Le_Fleuv");
    out[60..68].copy_from_slice(b"BOOKMOBI");
    out[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
    let mut offset = 78 + 8 * records.len() + 2;
    for (i, r) in records.iter().enumerate() {
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(2 * i as u32).to_be_bytes());
        offset += r.len();
    }
    out.extend_from_slice(&[0, 0]);
    for r in &records {
        out.extend_from_slice(r);
    }
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

#[test]
fn mobi_metadata_and_text() {
    let dir = temp_dir();
    let path = sample_mobi(dir.path(), "fleuve.mobi", 0);
    let meta = extract(&path).unwrap();
    assert_eq!(meta.title, "Le Fleuve");
    assert_eq!(meta.authors, ["Jeanne Dupont", "Paul Martin"]);
    assert_eq!(meta.publisher.as_deref(), Some("Éditions Test"));
    assert_eq!(meta.isbn.as_deref(), Some("9782070360246"));
    assert_eq!(meta.published_date.as_deref(), Some("1903"));
    assert_eq!(meta.language.as_deref(), Some("fr"));
    assert_eq!(meta.cover_data.as_deref(), Some(PNG));
    assert!(!meta.is_drm_protected);

    let book = MobiBook::open(&path).unwrap();
    assert!(!book.has_kf8());
    assert_eq!(book.kf7.full_name(), "Nom long du livre");
    let text = String::from_utf8(book.text(&book.kf7).unwrap()).unwrap();
    assert!(text.starts_with("<html><body><p>Il était une fois"));
}

#[test]
fn mobi_drm_is_detected() {
    let dir = temp_dir();
    let path = sample_mobi(dir.path(), "protege.azw", 2);
    assert!(extract(&path).unwrap().is_drm_protected);
    let book = MobiBook::open(&path).unwrap();
    assert!(book.text(&book.kf7).is_err());
}

#[test]
fn not_a_mobi() {
    let dir = temp_dir();
    let path = dir.path().join("faux.mobi");
    std::fs::write(&path, vec![7u8; 500]).unwrap();
    assert!(extract(&path).is_err());
}

#[test]
fn pdf_metadata() {
    use lopdf::{dictionary, Document, Object};
    let dir = temp_dir();
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    let info_id = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Manuel du fleuve"),
        "Author" => Object::string_literal("Jeanne Dupont; Paul Martin"),
    });
    doc.trailer.set("Root", catalog_id);
    doc.trailer.set("Info", info_id);
    let path = dir.path().join("manuel.pdf");
    doc.save(&path).unwrap();
    let meta = extract(&path).unwrap();
    assert_eq!(meta.title, "Manuel du fleuve");
    assert_eq!(meta.authors, ["Jeanne Dupont", "Paul Martin"]);
    assert!(!meta.is_drm_protected);
}

#[test]
fn format_from_extension() {
    assert_eq!(BookFormat::from_path(Path::new("a.AZW3")), Some(BookFormat::Azw3));
    assert!(extract(Path::new("x.txt")).is_err());
}
