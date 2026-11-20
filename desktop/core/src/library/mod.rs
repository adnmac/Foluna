//! Bibliothèque : index, dossiers de livres, parcours, imports, corbeille (`Sources/AlexandrieCore/Library` et
//! la partie sans interface de `LibraryStore` de l'app Mac).

pub mod book;
pub mod dirs;
pub mod folders;
pub mod groups;
pub mod index;
pub mod merge;
pub mod names;
pub mod scanner;
pub mod storage;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use book::Book;
pub use dirs::{AppDirectories, CoverStore};

use crate::metadata::BookFormat;

/// État de la bibliothèque, sans interface. Toute modification est enregistrée aussitôt.
pub struct Library {
    pub dirs: AppDirectories,
    pub covers: CoverStore,
    books: Vec<Book>,
    /// Entrées d'un format inconnu de cette version, gardées dans l'index.
    others: Vec<serde_json::Value>,
    folders: Vec<folders::Entry>,
    /// Dossiers introuvables au dernier parcours (disque débranché) : leurs livres restent, indisponibles.
    unavailable: HashSet<String>,
}

/// Résultat d'un parcours.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Livres nouveaux (premier passage dans la bibliothèque).
    pub new_books: Vec<String>,
    /// Fichiers relus (nouveaux ou modifiés).
    pub read: usize,
}

impl Library {
    /// Ouvre la bibliothèque. Un index illisible est mis de côté (jamais écrasé) et l'erreur est rendue avec la
    /// bibliothèque vide ; la copie du jour est faite avant tout changement.
    pub fn open(dirs: AppDirectories) -> (Library, Option<String>) {
        let covers = CoverStore::new(dirs.covers());
        let mut error = None;
        let content = match index::load(&dirs.index()) {
            Ok(c) => {
                let _ = index::back_up_today(&dirs.index(), &dirs.backups(), 10);
                c
            }
            Err(e) => {
                let moved = index::quarantine(&dirs.index()).ok();
                error = Some(format!(
                    "Index de la bibliothèque illisible ({e}).{}",
                    moved
                        .and_then(|m| m.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .map(|n| format!(" Il a été mis de côté sous « {n} »."))
                        .unwrap_or_default()
                ));
                Default::default()
            }
        };
        let mut library = Library { covers, books: content.books, others: content.others, folders: Vec::new(), unavailable: HashSet::new(), dirs };
        library.load_folders();
        // Séries enregistrées avec leur numéro (« Brunetti 8 ») : nom et numéro séparés une fois pour toutes.
        let mut changed = false;
        for b in library.books.iter_mut() {
            let (s, i) = names::normalized_series(b.series.as_deref(), b.series_index);
            if s != b.series || i != b.series_index {
                (b.series, b.series_index) = (s, i);
                changed = true;
            }
        }
        if changed {
            library.save_or_report(&mut error);
        }
        (library, error)
    }

    fn save_or_report(&self, error: &mut Option<String>) {
        if let Err(e) = self.save() {
            *error = Some(e);
        }
    }

    /// Enregistre l'index.
    pub fn save(&self) -> Result<(), String> {
        let content = index::IndexContent { books: self.books.clone(), others: self.others.clone() };
        index::save(&content, &self.dirs.index()).map_err(|e| format!("Impossible d'enregistrer l'index : {e}"))
    }

    pub fn books(&self) -> &[Book] {
        &self.books
    }

    pub fn book(&self, id: &str) -> Option<&Book> {
        self.books.iter().find(|b| b.path == id)
    }

    // Dossiers de livres

    /// Lit `folders.json`. Données de test : sans liste enregistrée, les dossiers viennent de `FOLUNA_BOOKS`.
    fn load_folders(&mut self) {
        let path = self.dirs.folders();
        if AppDirectories::test_store().is_some() && !path.exists() {
            if let Some(books) = std::env::var_os("FOLUNA_BOOKS") {
                self.folders = std::env::split_paths(&books).map(|p| folders::Entry::new(&p)).collect();
                let _ = folders::save(&self.folders, &path);
            }
        } else {
            self.folders = folders::load(&path);
        }
        self.refresh_folders();
    }

    fn refresh_folders(&mut self) {
        self.unavailable = self.folders.iter().filter(|f| !Path::new(&f.path).exists()).map(|f| f.path.clone()).collect();
    }

    pub fn folders(&self) -> Vec<String> {
        self.folders.iter().map(|f| f.path.clone()).collect()
    }

    pub fn unavailable_folders(&self) -> &HashSet<String> {
        &self.unavailable
    }

    /// Ajoute un dossier de livres ; ses livres arrivent au parcours suivant. Rend la raison d'un refus.
    pub fn add_folder(&mut self, path: &Path) -> Result<(), String> {
        if !path.is_dir() {
            return Err("Ce dossier est introuvable.".into());
        }
        if let Some(c) = folders::conflict(&path.to_string_lossy(), &self.folders()) {
            return Err(c.message());
        }
        self.folders.push(folders::Entry::new(path));
        folders::save(&self.folders, &self.dirs.folders()).map_err(|e| format!("Impossible d'enregistrer les dossiers de livres : {e}"))?;
        self.refresh_folders();
        Ok(())
    }

    /// Retire un dossier : ses livres quittent la bibliothèque, ses fichiers restent sur le disque.
    /// Le dernier dossier ne se retire pas.
    pub fn remove_folder(&mut self, path: &str) -> Result<(), String> {
        if self.folders.len() <= 1 || !self.folders.iter().any(|f| f.path == path) {
            return Ok(());
        }
        let prefix = folders::prefix(path);
        self.folders.retain(|f| f.path != path);
        folders::save(&self.folders, &self.dirs.folders()).map_err(|e| e.to_string())?;
        self.refresh_folders();
        self.books.retain(|b| b.is_imported || !folders::prefix(&b.path).starts_with(&prefix));
        self.save()
    }

    /// Dossier de livres d'un fichier (None : livre importé).
    pub fn folder_of(&self, book: &Book) -> Option<String> {
        if book.is_imported { None } else { folders::owner(&book.path, &self.folders()).cloned() }
    }

    /// Livre d'un dossier débranché : affiché, mais ni envoyé ni lu tant que le disque manque.
    pub fn is_unavailable(&self, book: &Book) -> bool {
        !self.unavailable.is_empty() && self.folder_of(book).is_some_and(|f| self.unavailable.contains(&f))
    }

    fn roots(&self) -> Vec<PathBuf> {
        self.folders.iter().map(folders::Entry::path_buf).chain([self.dirs.imports()]).collect()
    }

    // Parcours

    /// Parcours incrémental : seuls les fichiers nouveaux ou modifiés sont relus. Les livres d'un dossier
    /// injoignable sont gardés tels quels.
    pub fn rescan(&mut self) -> Result<ScanReport, String> {
        self.refresh_folders();
        let roots: Vec<PathBuf> = self.roots();
        let unreachable: Vec<String> = roots.iter().filter(|r| !r.exists()).map(|r| folders::prefix(&r.to_string_lossy())).collect();
        let reachable: Vec<PathBuf> = roots.into_iter().filter(|r| r.exists()).collect();
        let is_offline = |b: &Book| unreachable.iter().any(|p| folders::prefix(&b.path).starts_with(p));
        let (offline, known): (Vec<Book>, Vec<Book>) = self.books.iter().cloned().partition(|b| is_offline(b));
        let imports_path = self.dirs.imports().to_string_lossy().into_owned();
        let plan = merge::plan(&scanner::scan(&reachable), &known, &imports_path);
        let mut books = offline;
        books.extend(plan.kept);
        let mut report = ScanReport { read: plan.changed.len(), ..Default::default() };
        for (file, previous) in plan.changed {
            let imported = file.path_string().starts_with(&imports_path);
            let mut book = scanner::make_book(&file, imported, &self.covers);
            match previous {
                Some(p) => merge::carry_over(&p, &mut book),
                None => report.new_books.push(book.path.clone()),
            }
            books.push(book);
        }
        self.books = books;
        self.save()?;
        if unreachable.is_empty() {
            let keep = crate::device::transfer::cache_names(&self.books, &self.dirs);
            storage::purge(&self.books, &self.dirs, Some(&keep));
        }
        Ok(report)
    }

    // Imports

    /// Copie les fichiers dans le dossier d'imports de l'app (la source n'est jamais modifiée). Un nom déjà pris
    /// reçoit « (2) », « (3) »… Rend les fichiers créés ; à suivre d'un parcours.
    pub fn import_files(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
        let mut imported = Vec::new();
        let mut errors = Vec::new();
        for path in paths {
            if BookFormat::from_path(path).is_none() {
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = path.extension().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut target = self.dirs.imports().join(&name);
            let mut n = 2;
            while target.exists() {
                target = self.dirs.imports().join(format!("{stem} ({n}).{ext}"));
                n += 1;
            }
            match std::fs::copy(path, &target) {
                Ok(_) => imported.push(target),
                Err(e) => errors.push(format!("Import impossible de « {name} » : {e}")),
            }
        }
        if let Some(e) = errors.into_iter().next() {
            return Err(e);
        }
        if imported.is_empty() {
            return Err("Aucun fichier pris en charge (EPUB, MOBI, AZW3, AZW, PDF).".into());
        }
        Ok(imported)
    }

    // Fiches

    /// Enregistre un livre et recopie ses informations corrigées sur les autres fichiers du même livre.
    /// L'attente d'envoi, les envois, la corbeille et les étagères ne changent que par leurs propres actions.
    pub fn update(&mut self, book: &Book, siblings: &[String]) -> Result<(), String> {
        let Some(i) = self.books.iter().position(|b| b.path == book.path) else { return Ok(()) };
        let mut book = book.clone();
        let current = &self.books[i];
        book.pending_send_at = current.pending_send_at;
        book.pending_for = current.pending_for.clone();
        book.sent_to = current.sent_to.clone();
        book.trashed_at = current.trashed_at;
        book.shelves = current.shelves.clone();
        let edits = book.edits();
        let tags = book.tags.clone();
        let own = book.path.clone();
        self.books[i] = book;
        for b in self.books.iter_mut().filter(|b| siblings.contains(&b.path) && b.path != own) {
            b.set_edits(&edits);
            b.tags = tags.clone();
        }
        self.save()
    }

    /// Identité de chaque fichier (voir `groups::identities`).
    pub fn identities(&self) -> HashMap<String, String> {
        groups::identities(&self.books)
    }

    /// Copies masquées (doublons d'un même livre dans un même format).
    pub fn hidden_duplicates(&self) -> HashSet<String> {
        groups::duplicates(&self.books, Some(&self.identities()))
    }

    /// Livres affichés : hors corbeille, et hors doublons sauf si `show_duplicates`.
    pub fn visible_books(&self, show_duplicates: bool) -> Vec<Book> {
        let hidden = if show_duplicates { HashSet::new() } else { self.hidden_duplicates() };
        self.books.iter().filter(|b| b.trashed_at.is_none() && !hidden.contains(&b.path)).cloned().collect()
    }

    // Corbeille de Foluna

    pub fn trashed_books(&self) -> Vec<Book> {
        self.books.iter().filter(|b| b.trashed_at.is_some()).cloned().collect()
    }

    /// Met le livre entier à la corbeille (tous ses fichiers) ; rien ne bouge sur le disque.
    pub fn move_to_trash(&mut self, id: &str) -> Result<(), String> {
        let ids = self.identities();
        let Some(key) = ids.get(id).cloned() else { return Ok(()) };
        let now = book::now();
        for b in self.books.iter_mut().filter(|b| b.trashed_at.is_none() && ids.get(&b.path) == Some(&key)) {
            b.trashed_at = Some(now);
            b.pending_send_at = None;
            b.pending_for = None;
        }
        self.save()
    }

    pub fn restore_from_trash(&mut self, id: &str) -> Result<(), String> {
        let ids = self.identities();
        let Some(key) = ids.get(id).cloned() else { return Ok(()) };
        for b in self.books.iter_mut().filter(|b| b.trashed_at.is_some() && ids.get(&b.path) == Some(&key)) {
            b.trashed_at = None;
        }
        self.save()
    }

    /// Place dans la Corbeille du système les fichiers donnés (seulement ceux de la corbeille de Foluna) et les
    /// retire de la bibliothèque. Un fichier qui ne peut pas être déplacé reste dans la corbeille de Foluna.
    pub fn delete_permanently(&mut self, ids: &[String], move_to_trash: &dyn Fn(&Path) -> Result<(), String>) -> Result<(), String> {
        let files: Vec<Book> = self.books.iter().filter(|b| b.trashed_at.is_some() && ids.contains(&b.path)).cloned().collect();
        if files.is_empty() {
            return Ok(());
        }
        let result = storage::empty_trash(&files, move_to_trash);
        let removed: HashSet<String> = result.removed.into_iter().collect();
        self.books.retain(|b| !removed.contains(&b.path));
        self.save()?;
        match result.failures.first() {
            Some((path, reason)) => Err(format!(
                "{} fichier(s) n'ont pas pu être placés dans la Corbeille et restent dans celle de Foluna. Premier : « {} » — {reason}",
                result.failures.len(),
                Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            )),
            None => Ok(()),
        }
    }

    pub fn empty_trash(&mut self, move_to_trash: &dyn Fn(&Path) -> Result<(), String>) -> Result<(), String> {
        let ids: Vec<String> = self.trashed_books().into_iter().map(|b| b.path).collect();
        self.delete_permanently(&ids, move_to_trash)
    }

    // File d'envoi et étagères des liseuses

    /// Livres à envoyer à cette liseuse dès qu'elle sera branchée (ou à n'importe laquelle si None), dans l'ordre
    /// des demandes ; ceux de la corbeille sont exclus.
    pub fn pending_sends(&self, reader: Option<&str>) -> Vec<Book> {
        let mut list: Vec<(Book, f64)> =
            self.books.iter().filter(|b| b.trashed_at.is_none()).filter_map(|b| b.pending_date(reader).map(|d| (b.clone(), d))).collect();
        list.sort_by(|a, b| a.1.total_cmp(&b.1));
        list.into_iter().map(|(b, _)| b).collect()
    }

    /// Met en attente pour une liseuse (qui reçoit aussi le livre sur son étagère), ou pour la prochaine liseuse
    /// branchée si aucune n'est choisie.
    pub fn queue_for(&mut self, ids: &[String], reader: Option<&str>) -> Result<(), String> {
        let now = book::now();
        let selected: Vec<String> = self
            .books
            .iter()
            .filter(|b| ids.contains(&b.path) && b.trashed_at.is_none() && b.can_send_to_any_reader())
            .map(|b| b.path.clone())
            .collect();
        self.mutate(&selected, |b| match reader {
            Some(r) => {
                b.pending_for.get_or_insert_with(Default::default).entry(r.to_string()).or_insert(now);
            }
            None => {
                b.pending_send_at.get_or_insert(now);
            }
        });
        if let Some(r) = reader {
            self.set_shelf(&selected, r, true);
        }
        self.save()
    }

    /// Retire de la file : pour une liseuse (et l'attente « n'importe laquelle »), ou entièrement si None.
    pub fn remove_from_queue(&mut self, ids: &[String], reader: Option<&str>) -> Result<(), String> {
        self.mutate(ids, |b| {
            b.pending_send_at = None;
            match reader {
                Some(r) => {
                    if let Some(map) = b.pending_for.as_mut() {
                        map.remove(r);
                    }
                    if b.pending_for.as_ref().is_some_and(|m| m.is_empty()) {
                        b.pending_for = None;
                    }
                }
                None => b.pending_for = None,
            }
        });
        self.save()
    }

    /// Livre arrivé sur une liseuse : date et nom d'envoi, retiré de sa file, noté envoyé et mis sur l'étagère.
    pub fn mark_sent(&mut self, id: &str, filename: &str, reader: &str) -> Result<(), String> {
        let now = book::now();
        let ids = [id.to_string()];
        self.mutate(&ids, |b| {
            b.last_sent_at = Some(now);
            b.sent_filename = Some(filename.to_string());
            b.pending_send_at = None;
            if let Some(map) = b.pending_for.as_mut() {
                map.remove(reader);
            }
            if b.pending_for.as_ref().is_some_and(|m| m.is_empty()) {
                b.pending_for = None;
            }
            b.sent_to.get_or_insert_with(Default::default).insert(reader.to_string(), now);
        });
        self.set_shelf(&ids, reader, true);
        self.save()
    }

    /// Retire un livre de la liste « Envoyés » (tous ses fichiers) ; le nom du fichier envoyé est gardé : il sert
    /// à reconnaître le livre s'il est encore sur la liseuse.
    pub fn forget_sent(&mut self, id: &str) -> Result<(), String> {
        let ids = self.identities();
        let Some(key) = ids.get(id).cloned() else { return Ok(()) };
        for b in self.books.iter_mut().filter(|b| ids.get(&b.path) == Some(&key)) {
            b.last_sent_at = None;
        }
        self.save()
    }

    /// Livres d'une étagère (copies en double et corbeille exclues).
    pub fn shelf_books(&self, reader: &str) -> Vec<Book> {
        self.visible_books(false).into_iter().filter(|b| b.shelf_ids().contains(reader)).collect()
    }

    pub fn add_to_shelf(&mut self, ids: &[String], reader: &str) -> Result<(), String> {
        self.set_shelf(ids, reader, true);
        self.save()
    }

    /// Retire de l'étagère sans rien supprimer, ni de la bibliothèque ni de la liseuse.
    pub fn remove_from_shelf(&mut self, ids: &[String], reader: &str) -> Result<(), String> {
        self.set_shelf(ids, reader, false);
        self.save()
    }

    /// Liseuse oubliée : son étagère disparaît.
    pub fn clear_shelf(&mut self, reader: &str) -> Result<(), String> {
        for b in self.books.iter_mut() {
            if let Some(s) = b.shelves.as_mut() {
                s.remove(reader);
                if s.is_empty() {
                    b.shelves = None;
                }
            }
        }
        self.save()
    }

    /// Étagère du livre entier (tous les fichiers du même livre).
    fn set_shelf(&mut self, ids: &[String], reader: &str, on: bool) {
        let identities = self.identities();
        let keys: HashSet<String> = ids.iter().map(|id| identities.get(id).cloned().unwrap_or_else(|| id.clone())).collect();
        for b in self.books.iter_mut() {
            let k = identities.get(&b.path).cloned().unwrap_or_else(|| b.path.clone());
            if !keys.contains(&k) || b.shelf_ids().contains(reader) == on {
                continue;
            }
            let mut shelves = b.shelf_ids();
            if on {
                shelves.insert(reader.to_string());
            } else {
                shelves.remove(reader);
            }
            b.shelves = if shelves.is_empty() { None } else { Some(shelves) };
        }
    }

    /// Seuls les états d'envoi changent : les livres reçus peuvent être des fichiers principaux de groupe complétés.
    fn mutate(&mut self, ids: &[String], mut change: impl FnMut(&mut Book)) {
        for b in self.books.iter_mut().filter(|b| ids.contains(&b.path)) {
            change(b);
        }
    }

    // Caches

    /// Taille des couvertures et des conversions.
    pub fn cache_size(&self) -> (u64, u64) {
        (storage::size(&self.dirs.covers()), storage::size(&self.dirs.conversions()))
    }

    pub fn clear_conversions(&self) {
        storage::clear(&self.dirs.conversions());
    }
}
