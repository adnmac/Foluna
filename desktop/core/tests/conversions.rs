//! Conversions (étape 3), sur le modèle de `Tests/AlexandrieCoreTests/ConversionTests.swift`.

use std::path::{Path, PathBuf};

use foluna_core::convert::{epub_to_azw3, mobi_to_epub};
use foluna_core::epub::{metadata_writer, EpubBuilder, EpubReader};
use foluna_core::extract::extract;
use foluna_core::mobi::{metadata_writer as mobi_writer, palmdoc, MobiBook};
use foluna_core::{BookMetadata, MetadataEdits, TocNode};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41,
    0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50, 0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// EPUB de test : chapitres, couverture, lien interne, feuille de style et table hiérarchique (`makeSampleEPUB`).
fn sample_epub(dir: &Path, chapters: usize, paragraphs: usize) -> PathBuf {
    let mut meta = BookMetadata::new("Le Livre d'Essai");
    meta.authors = vec!["Jeanne Dupont".into()];
    meta.language = Some("fr".into());
    meta.publisher = Some("Éditions Test".into());
    meta.summary = Some("Un résumé <b>riche</b>.".into());
    meta.subjects = vec!["Roman".into()];
    let mut epub = EpubBuilder::new(meta);
    epub.add("Images/cover.png", "image/png", PNG.to_vec(), false);
    epub.cover_path = Some("Images/cover.png".into());
    epub.add("Styles/main.css", "text/css", b"body { font-family: serif; } .c { background: url(../Images/cover.png); }".to_vec(), false);
    for c in 1..=chapters {
        let mut body = format!("<h1 id=\"ch{c}\">Chapitre {c} — été</h1>\n");
        for p in 1..=paragraphs {
            body += &format!("<p id=\"p{c}-{p}\">Paragraphe {p} du chapitre {c} : « Ça déménage » à l’œil nu…</p>\n");
        }
        if c == 1 {
            body += "<p><a href=\"chapter3.xhtml#p3-5\">Aller au chapitre 3</a><img src=\"../Images/cover.png\" alt=\"\"/></p>\n";
        }
        let html = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"fr\">\n<head><title>Chapitre {c}</title><link rel=\"stylesheet\" type=\"text/css\" href=\"../Styles/main.css\"/></head>\n<body>\n{body}</body>\n</html>"
        );
        epub.add(format!("Text/chapter{c}.xhtml"), "application/xhtml+xml", html.into_bytes(), true);
        let mut node = TocNode::new(format!("Chapitre {c}"), format!("Text/chapter{c}.xhtml"));
        node.children.push(TocNode { fragment: Some(format!("p{c}-5")), ..TocNode::new(format!("Section {c}.5"), format!("Text/chapter{c}.xhtml")) });
        epub.toc.push(node);
    }
    let path = dir.join("essai.epub");
    epub.write(&path).unwrap();
    path
}

fn text_of(epub: &EpubBuilder, path: &str) -> String {
    String::from_utf8(epub.resources.iter().find(|r| r.path == path).unwrap().data.clone()).unwrap()
}

#[test]
fn builder_writes_readable_epub() {
    let dir = tempfile::tempdir().unwrap();
    let reader = EpubReader::open(&sample_epub(dir.path(), 3, 40)).unwrap();
    let meta = reader.book_metadata();
    assert_eq!(meta.title, "Le Livre d'Essai");
    assert_eq!(meta.authors, ["Jeanne Dupont"]);
    assert_eq!(meta.language.as_deref(), Some("fr"));
    assert!(meta.cover_data.is_some());
    assert_eq!(reader.spine.len(), 3);
    let toc = reader.table_of_contents();
    assert_eq!(toc.len(), 3);
    assert_eq!(toc[0].children[0].fragment.as_deref(), Some("p1-5"));
}

#[test]
fn epub_to_azw3_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let epub = sample_epub(dir.path(), 3, 120);
    let azw3 = dir.path().join("essai.azw3");
    let result = epub_to_azw3::convert(&epub, &azw3, None, None).unwrap();

    let book = MobiBook::open(&azw3).unwrap();
    assert!(book.has_kf8());
    assert!(!book.is_drm_protected());
    let meta = book.metadata();
    assert_eq!(meta.title, "Le Livre d'Essai");
    assert_eq!(meta.authors, ["Jeanne Dupont"]);
    assert_eq!(meta.language.as_deref(), Some("fr"));
    assert!(meta.cover_data.is_some());
    assert_eq!(book.asin().as_deref(), Some(result.asin.as_str()));
    assert_eq!(book.cde_type().as_deref(), Some("EBOK"));

    // Retour en EPUB : mêmes chapitres, texte intact, lien et table résolus.
    let back = mobi_to_epub::build(&book).unwrap();
    let spine: Vec<String> = back.spine_paths().iter().map(|s| s.to_string()).collect();
    assert_eq!(spine.len(), 3);
    let ch1 = text_of(&back, &spine[0]);
    assert!(ch1.contains("Chapitre 1 — été"));
    assert!(ch1.contains("« Ça déménage » à l’œil nu…"));
    assert!(ch1.contains("Paragraphe 120 du chapitre 1"));
    assert!(ch1.contains("href=\"part0002.xhtml#p3-5\""), "lien interne non résolu");
    assert!(ch1.contains("../Images/image00001.png"));
    assert!(ch1.contains("../Styles/style0001.css"));
    let ch3 = text_of(&back, &spine[2]);
    assert!(ch3.contains("Paragraphe 120 du chapitre 3"));
    assert_eq!(back.toc.len(), 3);
    assert_eq!(back.toc[2].children[0].fragment.as_deref(), Some("p3-5"));
    assert!(back.resources.iter().any(|r| r.media_type == "text/css" && String::from_utf8_lossy(&r.data).contains("../Images/image00001.png")));

    let out = dir.path().join("retour.epub");
    back.write(&out).unwrap();
    assert_eq!(EpubReader::open(&out).unwrap().book_metadata().title, "Le Livre d'Essai");
}

#[test]
fn azw3_with_edits() {
    let dir = tempfile::tempdir().unwrap();
    let epub = sample_epub(dir.path(), 1, 3);
    let azw3 = dir.path().join("modifie.azw3");
    let edits = MetadataEdits {
        title: "Titre modifié".into(),
        authors: vec!["A".into(), "B".into()],
        summary: Some("Un résumé riche.".into()),
        published_date: Some("1759".into()),
        ..Default::default()
    };
    epub_to_azw3::convert(&epub, &azw3, Some(&edits), Some("AXTEST0001")).unwrap();
    let meta = extract(&azw3).unwrap();
    assert_eq!(meta.title, "Titre modifié");
    assert_eq!(meta.authors, ["A", "B"]);
    assert_eq!(meta.summary.as_deref(), Some("Un résumé riche."));
    assert_eq!(meta.published_date.as_deref(), Some("1759"));
    assert_eq!(MobiBook::open(&azw3).unwrap().asin().as_deref(), Some("AXTEST0001"));
}

#[test]
fn mobi_metadata_rewrite_keeps_text() {
    let dir = tempfile::tempdir().unwrap();
    let azw3 = dir.path().join("livre.azw3");
    epub_to_azw3::convert(&sample_epub(dir.path(), 2, 10), &azw3, None, Some("AXKEEP0001")).unwrap();
    let before = MobiBook::open(&azw3).unwrap();
    let text_before = before.text(before.main_header()).unwrap();
    let out = dir.path().join("corrige.azw3");
    let edits = MetadataEdits {
        title: "Nouveau titre".into(),
        authors: vec!["Paul Martin".into()],
        language: Some("en".into()),
        isbn: Some("978-2-07-036024-6".into()),
        ..Default::default()
    };
    mobi_writer::rewrite_file(&azw3, &edits, &out).unwrap();
    let after = MobiBook::open(&out).unwrap();
    let meta = after.metadata();
    assert_eq!(meta.title, "Nouveau titre");
    assert_eq!(meta.authors, ["Paul Martin"]);
    assert_eq!(meta.language.as_deref(), Some("en"));
    assert_eq!(meta.isbn.as_deref(), Some("9782070360246"));
    assert_eq!(after.asin().as_deref(), Some("AXKEEP0001"));
    assert_eq!(after.text(after.main_header()).unwrap(), text_before);
    // Le livre corrigé se convertit toujours en EPUB.
    assert_eq!(mobi_to_epub::build(&after).unwrap().spine_paths().len(), 2);
}

#[test]
fn epub_metadata_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample_epub(dir.path(), 2, 5);
    let original = std::fs::read(&source).unwrap();
    let out = dir.path().join("corrige.epub");
    let edits = MetadataEdits {
        title: "Titre corrigé & co".into(),
        authors: vec!["Paul Martin".into(), "Ada Lovelace".into()],
        isbn: Some("2-07-036024-5".into()),
        series: Some("Les Rivières".into()),
        series_index: Some(3.0),
        ..Default::default()
    };
    metadata_writer::rewrite(&source, &edits, &out).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), original, "le fichier source ne doit pas changer");
    let reader = EpubReader::open(&out).unwrap();
    let meta = reader.book_metadata();
    assert_eq!(meta.title, "Titre corrigé & co");
    assert_eq!(meta.authors, ["Paul Martin", "Ada Lovelace"]);
    assert_eq!(meta.isbn.as_deref(), Some("9782070360246"));
    assert_eq!(meta.series.as_deref(), Some("Les Rivières"));
    assert_eq!(meta.series_index, Some(3.0));
    assert_eq!(meta.publisher.as_deref(), Some("Éditions Test"), "un champ vide garde la valeur du livre");
    assert_eq!(reader.spine.len(), 2);
    assert!(meta.cover_data.is_some());
}

/// MOBI KF7 (ancien format) avec saut de page et lien filepos : converti en EPUB de deux chapitres.
#[test]
fn kf7_to_epub() {
    let dir = tempfile::tempdir().unwrap();
    let anchor_target = "<html><body><p><a filepos=0000000000>début</a> Premier chapitre.</p><mbp:pagebreak/><p>Second chapitre avec<br>un saut.</p></body></html>";
    let path = dir.path().join("ancien.mobi");
    std::fs::write(&path, kf7(anchor_target.as_bytes())).unwrap();
    let book = MobiBook::open(&path).unwrap();
    assert!(!book.has_kf8());
    let epub = mobi_to_epub::build(&book).unwrap();
    let spine: Vec<String> = epub.spine_paths().iter().map(|s| s.to_string()).collect();
    assert_eq!(spine.len(), 2);
    let ch1 = text_of(&epub, &spine[0]);
    assert!(ch1.contains("Premier chapitre."));
    assert!(ch1.contains("xmlns=\"http://www.w3.org/1999/xhtml\""));
    let ch2 = text_of(&epub, &spine[1]);
    assert!(ch2.contains("Second chapitre avec<br/>un saut."), "{ch2}");
    let out = dir.path().join("ancien.epub");
    epub.write(&out).unwrap();
    let reread = EpubReader::open(&out).unwrap();
    assert_eq!(reread.book_metadata().title, "Ancien livre");
    assert_eq!(reread.spine.len(), 2);
}

/// Fichier MOBI KF7 minimal autour d'un texte HTML.
fn kf7(text: &[u8]) -> Vec<u8> {
    let compressed = palmdoc::compress(text);
    let title = "Ancien livre".as_bytes();
    let header_length = 0xE8usize;
    let mut r0 = vec![0u8; 16 + header_length];
    r0[0..2].copy_from_slice(&2u16.to_be_bytes());
    r0[4..8].copy_from_slice(&(text.len() as u32).to_be_bytes());
    r0[8..10].copy_from_slice(&1u16.to_be_bytes());
    r0[10..12].copy_from_slice(&4096u16.to_be_bytes());
    r0[16..20].copy_from_slice(b"MOBI");
    r0[20..24].copy_from_slice(&(header_length as u32).to_be_bytes());
    r0[28..32].copy_from_slice(&65001u32.to_be_bytes());
    r0[36..40].copy_from_slice(&6u32.to_be_bytes());
    r0[0x54..0x58].copy_from_slice(&((16 + header_length) as u32).to_be_bytes());
    r0[0x58..0x5C].copy_from_slice(&(title.len() as u32).to_be_bytes());
    r0[0x6C..0x70].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    r0[0xF4..0xF8].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    r0.extend_from_slice(title);
    r0.extend_from_slice(&[0, 0]);
    let records = [r0, compressed];
    let mut out = vec![0u8; 78];
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
    out
}
