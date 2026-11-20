//! Bibliothèque (étape 4), sur le modèle de `LibraryMergeTests` et du self-test de l'app Mac.

use std::path::{Path, PathBuf};

use foluna_core::epub::EpubBuilder;
use foluna_core::library::{folders, groups, index, merge, names, scanner, AppDirectories, Book, Library};
use foluna_core::BookMetadata;

fn epub(dir: &Path, name: &str, title: &str, author: &str) -> PathBuf {
    let mut meta = BookMetadata::new(title);
    meta.authors = vec![author.into()];
    meta.language = Some("fr".into());
    let mut b = EpubBuilder::new(meta);
    b.add(
        "Text/c1.xhtml",
        "application/xhtml+xml",
        format!("<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body><p>{name}</p></body></html>").into_bytes(),
        true,
    );
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    b.write(&path).unwrap();
    path
}

struct Fixture {
    _tmp: tempfile::TempDir,
    store: PathBuf,
    books: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let books = tmp.path().join("Livres");
    std::fs::create_dir_all(&books).unwrap();
    Fixture { store, books, _tmp: tmp }
}

fn open(f: &Fixture) -> Library {
    let (lib, error) = Library::open(AppDirectories::new(Some(&f.store)));
    assert!(error.is_none(), "{error:?}");
    lib
}

#[test]
fn folder_scan_and_reopen() {
    let f = fixture();
    epub(&f.books, "Candide.epub", "Candide", "Voltaire");
    epub(&f.books, "Sous-dossier/Zadig.epub", "Zadig", "Voltaire");
    std::fs::write(f.books.join(".cache.epub"), b"x").unwrap();
    std::fs::write(f.books.join("notes.txt"), b"x").unwrap();
    let mut lib = open(&f);
    lib.add_folder(&f.books).unwrap();
    let report = lib.rescan().unwrap();
    assert_eq!(report.new_books.len(), 2);
    let mut titles: Vec<String> = lib.books().iter().map(|b| b.title.clone()).collect();
    titles.sort();
    assert_eq!(titles, ["Candide", "Zadig"]);
    assert!(lib.books().iter().all(|b| !b.is_imported && b.asin.starts_with("AX") && b.asin.len() == 10));

    // Réouverture : index relu, rien n'est relu au parcours suivant.
    drop(lib);
    let mut lib = open(&f);
    assert_eq!(lib.books().len(), 2);
    assert_eq!(lib.rescan().unwrap().read, 0);
    assert_eq!(lib.folders().len(), 1);
}

#[test]
fn folder_conflicts() {
    let f = fixture();
    std::fs::create_dir_all(f.books.join("Policiers")).unwrap();
    let mut lib = open(&f);
    lib.add_folder(&f.books).unwrap();
    assert!(lib.add_folder(&f.books).unwrap_err().contains("déjà"));
    assert!(lib.add_folder(&f.books.join("Policiers")).unwrap_err().contains("déjà lu"));
    assert!(lib.add_folder(&f.books.join("absent")).is_err());
}

#[test]
fn edits_survive_moves_and_changes() {
    let f = fixture();
    let path = epub(&f.books, "Candide.epub", "Candide", "Voltaire");
    let mut lib = open(&f);
    lib.add_folder(&f.books).unwrap();
    lib.rescan().unwrap();
    let mut book = lib.books()[0].clone();
    let asin = book.asin.clone();
    book.title = "Candide corrigé".into();
    lib.update(&book, &[]).unwrap();

    // Déplacé et renommé : retrouvé par son empreinte, avec ses corrections et son ASIN.
    let moved = f.books.join("Classiques/Candide ou l'optimisme.epub");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&path, &moved).unwrap();
    let report = lib.rescan().unwrap();
    assert!(report.new_books.is_empty());
    assert_eq!(lib.books().len(), 1);
    let b = &lib.books()[0];
    assert_eq!(b.title, "Candide corrigé");
    assert_eq!(b.asin, asin);
    assert_eq!(Path::new(&b.path), moved);

    // Fichier modifié : relu, les corrections restent prioritaires.
    epub(&f.books, "Classiques/Candide ou l'optimisme.epub", "Candide (nouvelle édition)", "Voltaire");
    let report = lib.rescan().unwrap();
    assert_eq!(report.read, 1);
    assert_eq!(lib.books()[0].title, "Candide corrigé");
    assert_eq!(lib.books()[0].asin, asin);
}

#[test]
fn imports_and_trash() {
    let f = fixture();
    let outside = f.books.parent().unwrap().join("Téléchargements");
    std::fs::create_dir_all(&outside).unwrap();
    let source = epub(&outside, "Zadig.epub", "Zadig", "Voltaire");
    let original = std::fs::read(&source).unwrap();
    let mut lib = open(&f);
    let first = lib.import_files(std::slice::from_ref(&source)).unwrap();
    let second = lib.import_files(std::slice::from_ref(&source)).unwrap();
    assert_eq!(second[0].file_name().unwrap(), "Zadig (2).epub");
    assert!(first[0].starts_with(lib.dirs.imports()));
    assert!(lib.import_files(&[outside.join("notes.txt")]).is_err());
    lib.rescan().unwrap();
    assert_eq!(lib.books().len(), 2);
    assert!(lib.books().iter().all(|b| b.is_imported));
    // Deux copies identiques : une seule est affichée.
    assert_eq!(lib.visible_books(false).len(), 1);
    assert_eq!(lib.visible_books(true).len(), 2);

    // Corbeille de Foluna : le livre entier (ses deux fichiers), rien ne bouge sur le disque.
    let id = lib.books()[0].path.clone();
    lib.move_to_trash(&id).unwrap();
    assert!(lib.visible_books(true).is_empty());
    assert_eq!(lib.trashed_books().len(), 2);
    lib.restore_from_trash(&id).unwrap();
    assert!(lib.trashed_books().is_empty());
    lib.move_to_trash(&id).unwrap();
    // Vidage : un fichier qui ne part pas reste dans la corbeille de Foluna.
    let refuse = id.clone();
    let result = lib.empty_trash(&|p: &Path| {
        if p.to_string_lossy() == refuse { Err("verrouillé".into()) } else { std::fs::remove_file(p).map_err(|e| e.to_string()) }
    });
    assert!(result.unwrap_err().contains("verrouillé"));
    assert_eq!(lib.books().len(), 1);
    assert_eq!(lib.books()[0].path, id);
    assert_eq!(std::fs::read(&source).unwrap(), original, "la source d'un import n'est jamais touchée");
}

#[test]
fn unplugged_folder_keeps_its_books() {
    let f = fixture();
    let disk = f.books.parent().unwrap().join("Disque");
    epub(&disk, "Candide.epub", "Candide", "Voltaire");
    epub(&f.books, "Zadig.epub", "Zadig", "Voltaire");
    let mut lib = open(&f);
    lib.add_folder(&f.books).unwrap();
    lib.add_folder(&disk).unwrap();
    lib.rescan().unwrap();
    assert_eq!(lib.books().len(), 2);
    let renamed = disk.with_file_name("Disque débranché");
    std::fs::rename(&disk, &renamed).unwrap();
    lib.rescan().unwrap();
    assert_eq!(lib.books().len(), 2);
    let candide = lib.books().iter().find(|b| b.title == "Candide").unwrap().clone();
    assert!(lib.is_unavailable(&candide));
    std::fs::rename(&renamed, &disk).unwrap();
    lib.rescan().unwrap();
    assert!(!lib.is_unavailable(lib.books().iter().find(|b| b.title == "Candide").unwrap()));
    // Retirer un dossier : ses livres partent, ses fichiers restent.
    lib.remove_folder(&disk.to_string_lossy()).unwrap();
    assert_eq!(lib.books().len(), 1);
    assert!(disk.join("Candide.epub").exists());
}

#[test]
fn corrupt_index_is_set_aside() {
    let f = fixture();
    std::fs::create_dir_all(&f.store).unwrap();
    std::fs::write(f.store.join("library.json"), "[{\"path\": 1").unwrap();
    let (lib, error) = Library::open(AppDirectories::new(Some(&f.store)));
    assert!(error.unwrap().contains("library.corrupt-"));
    assert!(lib.books().is_empty());
    assert!(std::fs::read_dir(&f.store).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("library.corrupt-")));
}

/// Index écrit par l'app Mac 0.6.0 : relu, et les champs que Windows ne gère pas (notice, mode PDF) sont gardés ;
/// un livre d'un format inconnu aussi.
#[test]
fn mac_index_round_trip() {
    let f = fixture();
    std::fs::create_dir_all(&f.store).unwrap();
    let mac = r#"[{"addedAt":784111777.5,"asin":"AXABCDEFGH","authors":["Voltaire"],"catalogCheckedAt":784111800,"catalogRecord":{"source":"bnf","title":"Candide"},"fileSize":12345,"fingerprint":"0123456789abcdef01234567-12345","format":"epub","hasCover":false,"isDRMProtected":false,"isImported":false,"metadataVersion":4,"modifiedAt":784111777,"path":"/Livres/Candide.epub","pdfMode":"cropped","seriesIndex":2,"shelves":["kindle-1"],"tags":[],"title":"Candide"},{"path":"/Livres/x.fb2","format":"fb2","title":"Futur"}]"#;
    std::fs::write(f.store.join("library.json"), mac).unwrap();
    let content = index::load(&f.store.join("library.json")).unwrap();
    assert_eq!(content.books.len(), 1);
    assert_eq!(content.others.len(), 1);
    let b = &content.books[0];
    assert_eq!(b.title, "Candide");
    assert_eq!(b.series_index, Some(2.0));
    assert_eq!(b.added_at, 784111777.5);
    assert!(b.shelf_ids().contains("kindle-1"));
    index::save(&content, &f.store.join("library.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(f.store.join("library.json")).unwrap()).unwrap();
    let entries = saved.as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["catalogRecord"]["source"], "bnf");
    assert_eq!(entries[0]["pdfMode"], "cropped");
    assert_eq!(entries[0]["isDRMProtected"], false);
    assert_eq!(entries[1]["format"], "fb2");
}

#[test]
fn groups_and_identities() {
    let f = fixture();
    epub(&f.books, "Candide.epub", "Candide : roman", "Voltaire");
    epub(&f.books, "Candide (12)/Candide.epub", "Candide", "VOLTAIRE");
    epub(&f.books, "Les Enchanteurs - Gary.epub", "Les Enchanteurs", "Romain Gary");
    epub(&f.books, "Les Enchanteurs - Ellroy.epub", "Les Enchanteurs", "James Ellroy");
    let mut lib = open(&f);
    lib.add_folder(&f.books).unwrap();
    lib.rescan().unwrap();
    let ids = lib.identities();
    let group_list = groups::make(lib.books(), Some(&ids));
    assert_eq!(group_list.len(), 3, "Candide réuni, les deux « Enchanteurs » séparés");
    let candide = group_list.iter().find(|g| g.is_group()).unwrap();
    assert_eq!(candide.books.len(), 2);
    assert!(groups::is_group_id(&candide.id));
    let authors = names::author_groups(lib.books());
    assert_eq!(authors.iter().find(|a| a.key == names::author_key("Voltaire")).unwrap().book_count, 2);
}

#[test]
fn merge_plan_keeps_unchanged_files() {
    let f = fixture();
    epub(&f.books, "Candide.epub", "Candide", "Voltaire");
    let files = scanner::scan(std::slice::from_ref(&f.books));
    let covers = foluna_core::library::CoverStore::new(f.store.clone());
    let book: Book = scanner::make_book(&files[0], false, &covers);
    let plan = merge::plan(&files, std::slice::from_ref(&book), "/Imports");
    assert_eq!(plan.kept.len(), 1);
    assert!(plan.changed.is_empty());
    assert!(folders::owner(&book.path, &[f.books.to_string_lossy().into_owned()]).is_some());
}
