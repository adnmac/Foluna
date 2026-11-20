//! Commandes appelées par l'interface (`app/ui`). Les données partent en JSON (clés en camelCase) ; les erreurs
//! sont des messages prêts à afficher.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use foluna_core::device::{self, shelf, transfer, ReaderBrand, SendOutcome};
use foluna_core::library::{book, folders, groups, names, storage, Book};
use foluna_core::metadata::MetadataEdits;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, Shared};

type Res<T> = Result<T, String>;

/// Ligne de la liste : un livre (fichier principal de son groupe, avec les formats de tous ses fichiers).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    /// Tous les fichiers du livre (actions sur la ligne).
    pub ids: Vec<String>,
    pub title: String,
    pub author_line: String,
    pub formats: Vec<String>,
    pub cover: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub added_at: f64,
    pub last_sent_at: Option<f64>,
    pub pending: bool,
    pub trashed: bool,
    pub unavailable: bool,
    pub drm: bool,
}

fn cover_path(state: &AppState, b: &Book) -> Option<String> {
    b.has_cover.then(|| state.library.covers.path(&b.fingerprint)).filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned())
}

fn row(state: &AppState, group: &groups::BookGroup) -> Row {
    let p = group.primary();
    Row {
        id: group.id.clone(),
        ids: group.books.iter().map(|b| b.path.clone()).collect(),
        title: p.title.clone(),
        author_line: p.author_line(),
        formats: group.formats().iter().map(|f| f.label()).collect(),
        cover: group.books.iter().find_map(|b| cover_path(state, b)),
        series: p.series.clone(),
        series_index: p.series_index,
        added_at: group.books.iter().map(|b| b.added_at).fold(f64::MIN, f64::max),
        last_sent_at: group.books.iter().filter_map(|b| b.last_sent_at).reduce(f64::max),
        pending: group.books.iter().any(Book::is_pending_send),
        trashed: group.books.iter().all(|b| b.trashed_at.is_some()),
        unavailable: group.books.iter().all(|b| state.library.is_unavailable(b)),
        drm: p.is_drm_protected,
    }
}

/// Livres d'une page de la barre latérale (« all », « recent », « sent », « pending », « trash »,
/// « shelf:<liseuse> », « format:epub », « author:<clé> », « series:<clé> », « folder:<chemin> »).
fn page_books(state: &AppState, page: &str) -> Vec<Book> {
    let lib = &state.library;
    if page == "trash" {
        return lib.trashed_books();
    }
    let visible = lib.visible_books(state.settings.show_duplicates);
    let (kind, arg) = page.split_once(':').unwrap_or((page, ""));
    match kind {
        "recent" => {
            let limit = book::now() - 30.0 * 86_400.0;
            visible.into_iter().filter(|b| b.added_at >= limit).collect()
        }
        "sent" => visible.into_iter().filter(|b| b.last_sent_at.is_some()).collect(),
        "pending" => visible.into_iter().filter(Book::is_pending_send).collect(),
        "shelf" => visible.into_iter().filter(|b| b.shelf_ids().contains(arg)).collect(),
        "format" => visible.into_iter().filter(|b| b.format.extension() == arg).collect(),
        "author" => visible.into_iter().filter(|b| names::author_keys(b).contains(arg)).collect(),
        "series" => visible.into_iter().filter(|b| b.series.as_deref().is_some_and(|s| names::series_key(s) == arg)).collect(),
        "folder" => {
            let prefix = folders::prefix(arg);
            visible.into_iter().filter(|b| !b.is_imported && folders::prefix(&b.path).starts_with(&prefix)).collect()
        }
        _ => visible,
    }
}

#[tauri::command]
pub fn list(state: State<'_, Shared>, page: String, query: String, sort: String) -> Vec<Row> {
    let state = state.lock();
    let books = page_books(&state, &page);
    let ids = state.library.identities();
    let mut rows: Vec<Row> = groups::make(&books, Some(&ids)).iter().map(|g| row(&state, g)).collect();
    let q = names::key(&query);
    if !q.is_empty() {
        rows.retain(|r| names::key(&r.title).contains(&q) || names::key(&r.author_line).contains(&q));
    }
    match (page.as_str(), sort.as_str()) {
        ("recent", _) | (_, "added") => rows.sort_by(|a, b| b.added_at.total_cmp(&a.added_at)),
        (p, _) if p.starts_with("series:") => {
            rows.sort_by(|a, b| a.series_index.unwrap_or(f64::MAX).total_cmp(&b.series_index.unwrap_or(f64::MAX)))
        }
        (_, "author") => rows.sort_by(|a, b| names::natural_cmp(&a.author_line, &b.author_line)),
        _ => rows.sort_by(|a, b| names::natural_cmp(&a.title, &b.title)),
    }
    rows
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub key: String,
    pub name: String,
    pub count: usize,
    pub unavailable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidebar {
    pub counts: HashMap<String, usize>,
    pub folders: Vec<Entry>,
    pub formats: Vec<Entry>,
    pub authors: Vec<Entry>,
    pub series: Vec<Entry>,
    pub shelves: Vec<Entry>,
    pub device: Option<Entry>,
}

#[tauri::command]
pub fn sidebar(state: State<'_, Shared>) -> Sidebar {
    let state = state.lock();
    let lib = &state.library;
    let visible = lib.visible_books(state.settings.show_duplicates);
    let ids = lib.identities();
    let count = |books: &[Book]| groups::make(books, Some(&ids)).len();
    let mut counts = HashMap::new();
    for page in ["all", "recent", "sent", "pending", "trash"] {
        counts.insert(page.to_string(), count(&page_books(&state, page)));
    }
    let entry = |key: String, name: String, count: usize| Entry { key, name, count, unavailable: false };
    let folders = lib
        .folders()
        .into_iter()
        .map(|f| Entry {
            count: count(&page_books(&state, &format!("folder:{f}"))),
            name: folders::name(&f),
            unavailable: lib.unavailable_folders().contains(&f),
            key: f,
        })
        .collect();
    let formats = foluna_core::BookFormat::ALL
        .iter()
        .map(|f| entry(f.extension().into(), f.label(), visible.iter().filter(|b| b.format == *f).count()))
        .filter(|e| e.count > 0)
        .collect();
    let authors = names::author_groups(&visible).into_iter().map(|g| entry(g.key, g.name, g.book_count)).collect();
    let series = names::series_groups(&visible).into_iter().map(|g| entry(g.key, g.name, g.book_count)).collect();
    let shelves = state
        .readers
        .iter()
        .map(|r| entry(r.id.clone(), r.name.clone(), count(&page_books(&state, &format!("shelf:{}", r.id)))))
        .collect();
    let device = state.device.as_ref().map(|d| entry(d.identifier(), state.device_name().unwrap_or_default(), 0));
    Sidebar { counts, folders, formats, authors, series, shelves, device }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub id: String,
    pub format: String,
    pub path: String,
    pub size: i64,
    pub hidden_copy: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub id: String,
    pub ids: Vec<String>,
    pub edits: MetadataEdits,
    pub tags: Vec<String>,
    pub cover: Option<String>,
    pub files: Vec<FileInfo>,
    pub folder: Option<String>,
    pub drm: bool,
    pub trashed: bool,
    pub unavailable: bool,
    pub pending: bool,
    pub last_sent_at: Option<f64>,
    pub added_at: f64,
    /// Ce qui partira sur la liseuse branchée (« Sera converti en AZW3… »), clé de traduction.
    pub send_note: Option<String>,
    pub can_send: bool,
}

/// Fiche d'un livre (`id` : identifiant d'un de ses fichiers ou de sa ligne).
#[tauri::command]
pub fn detail(state: State<'_, Shared>, ids: Vec<String>) -> Option<Detail> {
    let state = state.lock();
    let books: Vec<Book> = ids.iter().filter_map(|id| state.library.book(id).cloned()).collect();
    let group = groups::BookGroup { id: ids.first()?.clone(), books };
    if group.books.is_empty() {
        return None;
    }
    let p = group.primary();
    let hidden = state.library.hidden_duplicates();
    let brand = state.device.as_ref().map(|d| d.brand);
    let send_note = brand.and_then(|b| {
        if !p.can_send(b) {
            Some("drm".to_string())
        } else if b == ReaderBrand::Kindle && p.becomes_azw3() {
            Some("azw3".to_string())
        } else if b.reads_epub() && p.sent_extension(b) != p.format.extension() {
            Some("epub".to_string())
        } else {
            None
        }
    });
    Some(Detail {
        id: p.path.clone(),
        ids: group.books.iter().map(|b| b.path.clone()).collect(),
        edits: p.edits(),
        tags: p.tags.clone(),
        cover: group.books.iter().find_map(|b| cover_path(&state, b)),
        files: group
            .books
            .iter()
            .map(|b| FileInfo { id: b.path.clone(), format: b.format.label(), path: b.path.clone(), size: b.file_size, hidden_copy: hidden.contains(&b.path) })
            .collect(),
        folder: state.library.folder_of(&p),
        drm: p.is_drm_protected,
        trashed: p.trashed_at.is_some(),
        unavailable: state.library.is_unavailable(&p),
        pending: group.books.iter().any(Book::is_pending_send),
        last_sent_at: group.books.iter().filter_map(|b| b.last_sent_at).reduce(f64::max),
        added_at: p.added_at,
        can_send: p.can_send_to_any_reader() && p.trashed_at.is_none(),
        send_note,
    })
}

/// Enregistre les informations corrigées sur tous les fichiers du livre (le fichier source n'est pas modifié).
#[tauri::command]
pub fn save_book(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>, edits: MetadataEdits, tags: Vec<String>) -> Res<()> {
    let mut state = state.lock();
    let Some(mut book) = ids.iter().find_map(|id| state.library.book(id).cloned()) else { return Ok(()) };
    let e = edits.normalized();
    if e.title.is_empty() {
        return Err("Le titre ne peut pas être vide.".into());
    }
    book.set_edits(&e);
    book.tags = tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    state.library.update(&book, &ids)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

// Dossiers, parcours, imports

fn rescan_and_notify(app: &AppHandle, shared: &Shared) -> Res<()> {
    let result = shared.lock().library.rescan();
    let _ = app.emit("library-changed", ());
    result.map(|_| ())
}

#[tauri::command]
pub async fn rescan(app: AppHandle, state: State<'_, Shared>) -> Res<()> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || rescan_and_notify(&app, &shared)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn add_folder(app: AppHandle, state: State<'_, Shared>, path: String) -> Res<()> {
    state.lock().library.add_folder(&PathBuf::from(&path))?;
    crate::watch::restart_folder_watch(&app);
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || rescan_and_notify(&app, &shared)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn remove_folder(app: AppHandle, state: State<'_, Shared>, path: String) -> Res<()> {
    state.lock().library.remove_folder(&path)?;
    crate::watch::restart_folder_watch(&app);
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn import(app: AppHandle, state: State<'_, Shared>, paths: Vec<String>) -> Res<usize> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let imported = shared.lock().library.import_files(&paths)?;
        rescan_and_notify(&app, &shared)?;
        Ok(imported.len())
    })
    .await
    .map_err(|e| e.to_string())?
}

// Corbeille

#[tauri::command]
pub fn trash(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<()> {
    let mut state = state.lock();
    for id in &ids {
        state.library.move_to_trash(id)?;
    }
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub fn restore(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<()> {
    let mut state = state.lock();
    for id in &ids {
        state.library.restore_from_trash(id)?;
    }
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub fn delete_permanently(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<()> {
    let result = state.lock().library.delete_permanently(&ids, &storage::system_trash);
    let _ = app.emit("library-changed", ());
    result
}

#[tauri::command]
pub fn empty_trash(app: AppHandle, state: State<'_, Shared>) -> Res<()> {
    let result = state.lock().library.empty_trash(&storage::system_trash);
    let _ = app.emit("library-changed", ());
    result
}

// File d'envoi

#[tauri::command]
pub fn queue(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<()> {
    let mut state = state.lock();
    // La seule liseuse connue, à défaut la prochaine branchée.
    let reader = (state.readers.len() == 1).then(|| state.readers[0].id.clone());
    state.library.queue_for(&ids, reader.as_deref())?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub fn unqueue(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<()> {
    state.lock().library.remove_from_queue(&ids, None)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub fn forget_sent(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<()> {
    let mut state = state.lock();
    for id in &ids {
        state.library.forget_sent(id)?;
    }
    let _ = app.emit("library-changed", ());
    Ok(())
}

// Liseuse

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDocument {
    pub id: String,
    pub name: String,
    pub title: String,
    pub author_line: String,
    pub format: String,
    pub size: u64,
    pub cover: Option<String>,
    pub drm: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub free_space: Option<u64>,
    pub documents: Vec<DeviceDocument>,
    /// Livres de l'étagère absents de l'appareil.
    pub missing: Vec<Row>,
}

#[tauri::command]
pub async fn device(state: State<'_, Shared>) -> Res<Option<DeviceInfo>> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = shared.lock();
        let Some(device) = state.device.clone() else { return Ok(None) };
        let docs = device.list_documents().map_err(|e| e.to_string())?;
        let documents: Vec<DeviceDocument> = docs
            .iter()
            .map(|d| {
                let info = state.catalog.info(d, &device);
                DeviceDocument {
                    id: d.id.clone(),
                    name: d.name(),
                    author_line: if info.authors.is_empty() { String::new() } else { info.authors.join(", ") },
                    cover: state.catalog.cover_path(&info).filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned()),
                    title: info.title,
                    format: info.format,
                    size: d.size,
                    drm: info.is_drm_protected,
                }
            })
            .collect();
        state.catalog.save();
        let names: HashSet<String> = docs.iter().map(|d| d.name()).collect();
        let titles: HashSet<String> = documents.iter().map(|d| shelf::normalize(&d.title)).collect();
        let reader = device.identifier();
        let ids = state.library.identities();
        let shelf_groups = groups::make(&state.library.shelf_books(&reader), Some(&ids));
        let missing_paths: HashSet<String> = shelf::missing(&shelf_groups, &names, &titles).into_iter().map(|b| b.path).collect();
        let missing = shelf_groups.iter().filter(|g| missing_paths.contains(&g.primary().path)).map(|g| row(&state, g)).collect();
        Ok(Some(DeviceInfo {
            id: reader,
            name: state.device_name().unwrap_or_default(),
            brand: device.brand.label().into(),
            free_space: device.free_space(),
            documents,
            missing,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub id: String,
    pub title: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// Envoie tout de suite si une liseuse est branchée ; sinon met en attente (rend une liste vide).
#[tauri::command]
pub async fn send(app: AppHandle, state: State<'_, Shared>, ids: Vec<String>) -> Res<Vec<SendResult>> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = shared.lock();
        let Some(device) = state.device.clone() else {
            let reader = (state.readers.len() == 1).then(|| state.readers[0].id.clone());
            state.library.queue_for(&ids, reader.as_deref())?;
            let _ = app.emit("library-changed", ());
            return Ok(Vec::new());
        };
        // Un livre par groupe : son fichier principal, celui qui part sur la liseuse.
        let all: Vec<Book> = ids.iter().filter_map(|id| state.library.book(id).cloned()).collect();
        let identities = state.library.identities();
        let books: Vec<Book> = groups::make(&all, Some(&identities)).iter().map(|g| g.primary()).collect();
        let progress_app = app.clone();
        let results = device::send(&books, &mut state.library, &device, &mut |id, converting| {
            let _ = progress_app.emit("send-progress", (id.to_string(), converting));
        });
        let _ = app.emit("library-changed", ());
        let _ = app.emit("device-changed", ());
        Ok(results
            .into_iter()
            .map(|(id, outcome)| {
                let title = state.library.book(&id).map(|b| b.title.clone()).unwrap_or_default();
                match outcome {
                    SendOutcome::Sent { .. } => SendResult { id, title, ok: true, message: None },
                    SendOutcome::Refused(m) | SendOutcome::Failed(m) => SendResult { id, title, ok: false, message: Some(m) },
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn eject(app: AppHandle, state: State<'_, Shared>) -> Res<String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (device, name) = {
            let state = shared.lock();
            (state.device.clone().ok_or("Aucune liseuse à éjecter.")?, state.device_name().unwrap_or_default())
        };
        device.eject().map_err(|e| e.to_string())?;
        shared.lock().device = None;
        let _ = app.emit("device-changed", ());
        Ok(name)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn delete_document(app: AppHandle, state: State<'_, Shared>, id: String) -> Res<()> {
    let state = state.lock();
    let device = state.device.as_ref().ok_or("La liseuse n'est plus connectée.")?;
    let doc = device.list_documents().map_err(|e| e.to_string())?.into_iter().find(|d| d.id == id).ok_or("Livre introuvable sur la liseuse.")?;
    device.delete(&doc).map_err(|e| e.to_string())?;
    let _ = app.emit("device-changed", ());
    Ok(())
}

#[tauri::command]
pub fn rename_reader(app: AppHandle, state: State<'_, Shared>, id: String, name: String) -> Res<()> {
    let mut state = state.lock();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Ok(());
    }
    if let Some(r) = state.readers.iter_mut().find(|r| r.id == id) {
        r.name = name;
    }
    state.save_readers()?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

/// Oublie une liseuse : son étagère disparaît (aucun livre n'est supprimé).
#[tauri::command]
pub fn forget_reader(app: AppHandle, state: State<'_, Shared>, id: String) -> Res<()> {
    let mut state = state.lock();
    state.library.clear_shelf(&id)?;
    state.readers.retain(|r| r.id != id);
    state.save_readers()?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

// Réglages

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInfo {
    pub language: Option<String>,
    pub show_duplicates: bool,
    pub hidden_duplicates: usize,
    pub covers_bytes: u64,
    pub conversions_bytes: u64,
    pub data_folder: String,
    pub version: String,
    pub folders: Vec<Entry>,
    pub readers: Vec<Entry>,
    pub startup_error: Option<String>,
}

#[tauri::command]
pub fn settings(state: State<'_, Shared>) -> SettingsInfo {
    let mut state = state.lock();
    let (covers, conversions) = state.library.cache_size();
    let startup_error = state.startup_error.take();
    SettingsInfo {
        language: state.settings.language.clone(),
        show_duplicates: state.settings.show_duplicates,
        hidden_duplicates: state.library.hidden_duplicates().len(),
        covers_bytes: covers,
        conversions_bytes: conversions,
        data_folder: state.library.dirs.base.to_string_lossy().into_owned(),
        version: foluna_core::VERSION.to_string(),
        folders: state
            .library
            .folders()
            .into_iter()
            .map(|f| Entry { name: folders::name(&f), unavailable: state.library.unavailable_folders().contains(&f), key: f, count: 0 })
            .collect(),
        readers: state.readers.iter().map(|r| Entry { key: r.id.clone(), name: r.name.clone(), count: 0, unavailable: false }).collect(),
        startup_error,
    }
}

#[tauri::command]
pub fn set_language(state: State<'_, Shared>, language: Option<String>) -> Res<()> {
    let mut state = state.lock();
    state.settings.language = language.filter(|l| !l.is_empty());
    state.save_settings()
}

#[tauri::command]
pub fn set_show_duplicates(app: AppHandle, state: State<'_, Shared>, show: bool) -> Res<()> {
    let mut state = state.lock();
    state.settings.show_duplicates = show;
    state.save_settings()?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub fn clear_conversions(state: State<'_, Shared>) {
    state.lock().library.clear_conversions();
}

/// Exporte en EPUB un livre MOBI/AZW3 (copie dans le dossier choisi).
#[tauri::command]
pub async fn export_epub(state: State<'_, Shared>, id: String, target: String) -> Res<()> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = shared.lock();
        let book = state.library.book(&id).cloned().ok_or("Livre introuvable.")?;
        let file = transfer::export_epub(&book, &state.library.dirs).map_err(|e| e.to_string())?;
        std::fs::copy(file, target).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
