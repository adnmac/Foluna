//! Surveillance en arrière-plan : dossiers de livres (relus après un changement) et liseuses (branchement,
//! débranchement ; la file d'attente part à la connexion, avec une notification).

use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use foluna_core::device::{self, readers::Reader, VolumeDevice};
use notify::{RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::state::Shared;

/// Observateur des dossiers en cours (remplacé quand la liste des dossiers change).
#[derive(Default)]
pub struct FolderWatch(pub Mutex<Option<notify::RecommendedWatcher>>);

/// (Re)lance l'observation des dossiers de livres et du dossier d'imports.
pub fn restart_folder_watch(app: &AppHandle) {
    let shared = app.state::<Shared>().inner().clone();
    let roots = {
        let state = shared.lock();
        let mut roots: Vec<std::path::PathBuf> = state.library.folders().into_iter().map(Into::into).collect();
        roots.push(state.library.dirs.imports());
        roots
    };
    let (tx, rx) = mpsc::channel::<()>();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(e) = event
            && !matches!(e.kind, notify::EventKind::Access(_)) {
                let _ = tx.send(());
            }
    });
    let Ok(mut watcher) = watcher else { return };
    for root in roots.iter().filter(|r| r.exists()) {
        let _ = watcher.watch(root, RecursiveMode::Recursive);
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        // Plusieurs changements rapprochés (copie de plusieurs livres) : un seul parcours.
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(1500)).is_ok() {}
            let shared = handle.state::<Shared>().inner().clone();
            let _ = shared.lock().library.rescan();
            let _ = handle.emit("library-changed", ());
        }
    });
    if let Ok(mut slot) = app.state::<FolderWatch>().0.lock() {
        *slot = Some(watcher);
    }
}

/// Sondage des disques amovibles toutes les deux secondes.
pub fn start_device_poll(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        poll_once(&handle);
        std::thread::sleep(Duration::from_secs(2));
    });
}

fn poll_once(app: &AppHandle) {
    let shared = app.state::<Shared>().inner().clone();
    let generic = shared.lock().generic_folders();
    let found = VolumeDevice::detect(&generic).into_iter().next();
    let (changed, connected) = {
        let mut state = shared.lock();
        let before = state.device.as_ref().map(VolumeDevice::identifier);
        let after = found.as_ref().map(VolumeDevice::identifier);
        if before == after {
            return;
        }
        state.device = found.clone();
        if let Some(d) = &found {
            let id = d.identifier();
            let now = foluna_core::library::book::now();
            match state.readers.iter_mut().find(|r| r.id == id) {
                Some(r) => r.last_seen_at = now,
                None => {
                    let name = d.name();
                    let folder = (d.brand == device::ReaderBrand::Generic).then(|| d.books_folder.clone());
                    state.readers.push(Reader::new(id, name, d.brand, folder));
                }
            }
            let _ = state.save_readers();
        }
        (true, found.is_some())
    };
    if changed {
        let _ = app.emit("device-changed", ());
        let _ = app.emit("library-changed", ());
    }
    if connected {
        send_pending(app, &shared);
    }
}

/// File d'attente envoyée à la liseuse qui vient d'être branchée ; notification (l'app peut être en arrière-plan).
fn send_pending(app: &AppHandle, shared: &Shared) {
    let note = {
        let mut state = shared.lock();
        let Some(device) = state.device.clone() else { return };
        let name = state.device_name();
        device::send_pending(&mut state.library, &device, name.as_deref())
    };
    if let Some(body) = note {
        let _ = app.notification().builder().title(device::summary::TITLE).body(body).show();
        let _ = app.emit("library-changed", ());
        let _ = app.emit("device-changed", ());
    }
}
