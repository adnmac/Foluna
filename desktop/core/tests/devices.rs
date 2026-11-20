//! Liseuses USB (étape 5), simulées par des dossiers : Kindle (`documents/`, `system/`) et Kobo (`.kobo/`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use foluna_core::convert::epub_to_azw3;
use foluna_core::device::platform::Volume;
use foluna_core::device::{self, catalog::DeviceCatalog, readers, shelf, summary, transfer, ReaderBrand, SendOutcome, VolumeDevice};
use foluna_core::epub::{EpubBuilder, EpubReader};
use foluna_core::library::{groups, AppDirectories, Library};
use foluna_core::mobi::MobiBook;
use foluna_core::BookMetadata;

/// Image PNG de 1 × 1 pixel (couverture).
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41,
    0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50, 0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn epub(dir: &Path, name: &str, title: &str) -> PathBuf {
    let mut meta = BookMetadata::new(title);
    meta.authors = vec!["Jeanne Dupont".into()];
    meta.language = Some("fr".into());
    let mut b = EpubBuilder::new(meta);
    b.add("Images/cover.png", "image/png", PNG.to_vec(), false);
    b.cover_path = Some("Images/cover.png".into());
    b.add(
        "Text/c1.xhtml",
        "application/xhtml+xml",
        format!("<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body><p>Texte de {title}.</p></body></html>").into_bytes(),
        true,
    );
    let path = dir.join(name);
    b.write(&path).unwrap();
    path
}

struct Setup {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    library: Library,
}

impl Setup {
    fn new() -> Setup {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let books = root.join("Livres");
        std::fs::create_dir_all(&books).unwrap();
        epub(&books, "Candide.epub", "Candide");
        epub(&books, "Zadig.epub", "Zadig");
        let azw3 = books.join("Fleuve.azw3");
        epub_to_azw3::convert(&epub(&root, "fleuve.epub", "Le Fleuve"), &azw3, None, Some("AXFLEUVE01")).unwrap();
        lopdf_pdf(&books.join("Manuel.pdf"));
        let (mut library, error) = Library::open(AppDirectories::new(Some(&root.join("store"))));
        assert!(error.is_none());
        library.add_folder(&books).unwrap();
        library.rescan().unwrap();
        Setup { _tmp: tmp, root, library }
    }

    fn book(&self, title: &str) -> foluna_core::library::Book {
        self.library.books().iter().find(|b| b.title == title).unwrap_or_else(|| panic!("{title} absent")).clone()
    }

    fn kindle(&self) -> VolumeDevice {
        let root = self.root.join("KINDLE");
        std::fs::create_dir_all(root.join("documents")).unwrap();
        std::fs::create_dir_all(root.join("system/thumbnails")).unwrap();
        VolumeDevice::from_volume(Volume::at(&root), &HashMap::new()).expect("Kindle reconnue")
    }

    fn kobo(&self) -> VolumeDevice {
        let root = self.root.join("KOBOeReader");
        std::fs::create_dir_all(root.join(".kobo")).unwrap();
        std::fs::write(root.join(".kobo/version"), "N418,4.38").unwrap();
        VolumeDevice::from_volume(Volume::at(&root), &HashMap::new()).expect("Kobo reconnue")
    }
}

fn lopdf_pdf(path: &Path) {
    use lopdf::{dictionary, Document, Object};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    let info_id = doc.add_object(dictionary! { "Title" => Object::string_literal("Manuel") });
    doc.trailer.set("Root", catalog_id);
    doc.trailer.set("Info", info_id);
    doc.save(path).unwrap();
}

#[test]
fn brands_are_recognised() {
    let s = Setup::new();
    assert_eq!(s.kindle().brand, ReaderBrand::Kindle);
    assert_eq!(s.kindle().books_folder, "documents");
    assert_eq!(s.kobo().brand, ReaderBrand::Kobo);
    let other = s.root.join("CLE_USB");
    std::fs::create_dir_all(&other).unwrap();
    assert!(VolumeDevice::from_volume(Volume::at(&other), &HashMap::new()).is_none(), "un disque quelconque n'est pas une liseuse");
    // Liseuse générique déjà ajoutée : reconnue par son identifiant, avec son dossier.
    let generic = VolumeDevice::new(Volume::at(&other), ReaderBrand::Generic, None);
    let known = HashMap::from([(generic.identifier(), "Livres".to_string())]);
    let found = VolumeDevice::from_volume(Volume::at(&other), &known).unwrap();
    assert_eq!((found.brand, found.books_folder.as_str()), (ReaderBrand::Generic, "Livres"));
}

#[test]
fn send_to_kindle() {
    let mut s = Setup::new();
    let kindle = s.kindle();
    let mut candide = s.book("Candide");
    candide.title = "Candide corrigé".into();
    s.library.update(&candide, &[]).unwrap();
    let candide = s.book("Candide corrigé");
    let fleuve = s.book("Le Fleuve");
    let manuel = s.book("Manuel");
    let mut seen = Vec::new();
    let results = device::send(&[candide.clone(), fleuve.clone(), manuel.clone()], &mut s.library, &kindle, &mut |id, convert| {
        seen.push((id.to_string(), convert))
    });
    assert!(results.iter().all(|(_, o)| matches!(o, SendOutcome::Sent { .. })), "{results:?}");
    assert_eq!(seen[0], (candide.path.clone(), true), "l'EPUB est converti");
    assert!(!seen[1].1 && !seen[2].1);

    let docs = kindle.root().join("documents");
    let azw3 = docs.join("Candide corrigé - Jeanne Dupont.azw3");
    let book = MobiBook::open(&azw3).unwrap();
    assert_eq!(book.metadata().title, "Candide corrigé");
    assert_eq!(book.asin().as_deref(), Some(candide.asin.as_str()));
    assert!(kindle.root().join(format!("system/thumbnails/thumbnail_{}_EBOK_portrait.jpg", candide.asin)).exists());
    assert_eq!(MobiBook::open(&docs.join("Le Fleuve - Jeanne Dupont.azw3")).unwrap().metadata().title, "Le Fleuve");
    assert!(docs.join("Manuel.pdf").exists());

    let sent = s.book("Candide corrigé");
    assert_eq!(sent.sent_filename.as_deref(), Some("Candide corrigé - Jeanne Dupont.azw3"));
    assert!(sent.last_sent_at.is_some());
    assert!(sent.sent_to.unwrap().contains_key(&kindle.identifier()));
    assert!(sent.shelves.unwrap().contains(&kindle.identifier()));

    // Liste de l'appareil : sans « My Clippings.txt » ni dossier .sdr ; suppression avec le .sdr.
    std::fs::write(docs.join("My Clippings.txt"), "notes").unwrap();
    std::fs::create_dir_all(docs.join("Candide corrigé - Jeanne Dupont.sdr")).unwrap();
    std::fs::write(docs.join("Candide corrigé - Jeanne Dupont.sdr/notes.azw3"), "x").unwrap();
    let list = kindle.list_documents().unwrap();
    assert_eq!(list.len(), 3, "{list:?}");
    let doc = list.iter().find(|d| d.name().starts_with("Candide")).unwrap();
    kindle.delete(doc).unwrap();
    assert!(!azw3.exists() && !docs.join("Candide corrigé - Jeanne Dupont.sdr").exists());
}

#[test]
fn send_to_kobo() {
    let mut s = Setup::new();
    let kobo = s.kobo();
    let mut zadig = s.book("Zadig");
    zadig.authors = vec!["Voltaire".into()];
    s.library.update(&zadig, &[]).unwrap();
    let zadig = s.book("Zadig");
    let fleuve = s.book("Le Fleuve");
    let results = device::send(&[zadig, fleuve], &mut s.library, &kobo, &mut |_, _| {});
    assert!(results.iter().all(|(_, o)| matches!(o, SendOutcome::Sent { .. })), "{results:?}");
    let books = kobo.root().join("Books");
    assert!(books.is_dir(), "dossier Books créé");
    let meta = EpubReader::open(&books.join("Zadig - Voltaire.epub")).unwrap().book_metadata();
    assert_eq!(meta.authors, ["Voltaire"]);
    let converted = EpubReader::open(&books.join("Le Fleuve - Jeanne Dupont.epub")).unwrap();
    assert_eq!(converted.book_metadata().title, "Le Fleuve");
    assert_eq!(kobo.list_documents().unwrap().len(), 2);
}

#[test]
fn drm_and_unavailable_books_are_refused() {
    let mut s = Setup::new();
    let kobo = s.kobo();
    let mut fleuve = s.book("Le Fleuve");
    fleuve.is_drm_protected = true;
    let results = device::send(&[fleuve.clone()], &mut s.library, &kobo, &mut |_, _| {});
    assert!(matches!(&results[0].1, SendOutcome::Refused(m) if m.contains("DRM")));
    // Un AZW3 sous DRM part tel quel sur une Kindle.
    assert!(fleuve.can_send(ReaderBrand::Kindle));
    assert_eq!(transfer::prepare(&fleuve, &s.library.dirs, ReaderBrand::Kindle).unwrap(), fleuve.file_path());
}

#[test]
fn queue_and_send_on_connection() {
    let mut s = Setup::new();
    let kindle = s.kindle();
    let reader = kindle.identifier();
    let candide = s.book("Candide");
    let zadig = s.book("Zadig");
    s.library.queue_for(std::slice::from_ref(&zadig.path), None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    s.library.queue_for(std::slice::from_ref(&candide.path), Some(&reader)).unwrap();
    assert_eq!(s.library.pending_sends(Some(&reader)).iter().map(|b| b.title.clone()).collect::<Vec<_>>(), ["Zadig", "Candide"]);
    assert!(s.library.pending_sends(Some("autre-liseuse")).iter().all(|b| b.title == "Zadig"));
    assert!(s.book("Candide").shelf_ids().contains(&reader), "la liseuse choisie reçoit le livre sur son étagère");

    let note = device::send_pending(&mut s.library, &kindle, Some("Kindle de Jeanne")).unwrap();
    assert_eq!(note, "2 livres envoyés sur « Kindle de Jeanne »");
    assert!(s.library.pending_sends(None).is_empty());
    assert!(!s.book("Candide").is_pending_send());

    // Un échec laisse le livre en attente.
    s.library.queue_for(std::slice::from_ref(&zadig.path), Some(&reader)).unwrap();
    std::fs::remove_dir_all(kindle.root().join("documents")).unwrap();
    let note = device::send_pending(&mut s.library, &kindle, None).unwrap();
    assert_eq!(note, "Échec de l'envoi de 1 livre");
    assert!(s.book("Zadig").is_pending_send());
    s.library.remove_from_queue(std::slice::from_ref(&zadig.path), Some(&reader)).unwrap();
    assert!(!s.book("Zadig").is_pending_send());
}

#[test]
fn shelf_and_catalog() {
    let mut s = Setup::new();
    let kindle = s.kindle();
    let reader = kindle.identifier();
    let ids: Vec<String> = ["Candide", "Zadig"].iter().map(|t| s.book(t).path).collect();
    s.library.add_to_shelf(&ids, &reader).unwrap();
    let candide = s.book("Candide");
    device::send(std::slice::from_ref(&candide), &mut s.library, &kindle, &mut |_, _| {});
    let docs = kindle.list_documents().unwrap();
    let catalog = DeviceCatalog::new(&s.library.dirs);
    let infos: Vec<_> = docs.iter().map(|d| catalog.info(d, &kindle)).collect();
    assert_eq!(infos[0].title, "Candide");
    assert_eq!(infos[0].format, "AZW3");
    assert!(infos[0].cover_key.is_none() || catalog.cover_path(&infos[0]).is_some());
    assert!(catalog.cached(&docs[0]).is_some());
    catalog.save();
    assert!(DeviceCatalog::new(&s.library.dirs).cached(&docs[0]).is_some(), "fiches gardées entre deux sessions");

    let names: HashSet<String> = docs.iter().map(|d| d.name()).collect();
    let titles: HashSet<String> = infos.iter().map(|i| shelf::normalize(&i.title)).collect();
    let shelf_groups = groups::make(&s.library.shelf_books(&reader), None);
    let missing = shelf::missing(&shelf_groups, &names, &titles);
    assert_eq!(missing.iter().map(|b| b.title.clone()).collect::<Vec<_>>(), ["Zadig"]);
    s.library.clear_shelf(&reader).unwrap();
    assert!(s.library.shelf_books(&reader).is_empty());
}

#[test]
fn kfx_title_is_guessed() {
    let doc = device::Document { id: "x".into(), path: "Downloads/Items01/Le_Grand_Livre-B0ABCDEFGH.kfx".into(), size: 1 };
    let info = device::catalog::guess(&doc);
    assert_eq!(info.title, "Le Grand Livre");
    assert_eq!(info.asin.as_deref(), Some("B0ABCDEFGH"));
    assert!(info.is_guessed);
}

#[test]
fn mac_readers_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("readers.json");
    std::fs::write(&path, r#"[{"firstSeenAt":784111777,"id":"vol:ABCD","lastSeenAt":784111800,"name":"Kindle de Jeanne"},{"booksFolder":"Livres","brand":"generic","firstSeenAt":1,"id":"vol:X","lastSeenAt":2,"name":"PocketBook"}]"#).unwrap();
    let list = readers::load(&path).unwrap();
    assert_eq!(list[0].brand(), ReaderBrand::Kindle, "sans marque : une Kindle");
    assert_eq!(list[1].brand(), ReaderBrand::Generic);
    assert_eq!(list[1].books_folder.as_deref(), Some("Livres"));
    readers::save(&list, &path).unwrap();
    assert_eq!(readers::load(&path).unwrap(), list);
    std::fs::write(&path, "[{").unwrap();
    assert!(readers::load(&path).is_err());
}

#[test]
fn conversions_cache_is_cleaned() {
    let mut s = Setup::new();
    let kobo = s.kobo();
    let zadig = s.book("Zadig");
    device::send(std::slice::from_ref(&zadig), &mut s.library, &kobo, &mut |_, _| {});
    let copy = transfer::cached_copy(&zadig, &s.library.dirs, ReaderBrand::Kobo);
    assert!(copy.exists());
    let stale = s.library.dirs.conversions().join("disparu-0000.azw3");
    std::fs::write(&stale, "x").unwrap();
    s.library.rescan().unwrap();
    assert!(copy.exists(), "la copie d'un livre présent est gardée");
    assert!(!stale.exists(), "celle d'un livre disparu est supprimée");
}

#[test]
fn summaries() {
    assert_eq!(summary::body(&[], 0, None), None);
    assert_eq!(summary::body(&["A".into()], 0, None).unwrap(), "« A » envoyé sur la liseuse");
    assert_eq!(summary::body(&["A".into(), "B".into()], 1, None).unwrap(), "2 livres envoyés, 1 échec");
}

#[test]
fn removable_volumes_do_not_fail() {
    // Un vrai disque amovible ne se simule pas : la liste doit seulement s'obtenir sans erreur.
    let _ = device::platform::removable_volumes();
    let _ = VolumeDevice::detect(&HashMap::new());
}
