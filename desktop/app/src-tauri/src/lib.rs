//! App Tauri de Foluna pour Windows : fenêtre, commandes appelées par l'interface (`app/ui`), surveillance des
//! dossiers de livres et des liseuses.

mod commands;
mod state;
mod watch;

use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::state::{AppState, Shared};

/// Version de l'app, affichée par l'interface.
#[tauri::command]
fn app_version() -> &'static str {
    foluna_core::VERSION
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = AppState::open();
            // Couvertures affichées par l'interface : seuls ces dossiers lui sont ouverts.
            let scope = app.asset_protocol_scope();
            let _ = scope.allow_directory(state.covers_dir(), false);
            let _ = scope.allow_directory(state.catalog.covers.directory.clone(), false);
            app.manage(Shared(Arc::new(Mutex::new(state))));
            app.manage(watch::FolderWatch::default());
            let handle = app.handle().clone();
            // Parcours au lancement (livres ajoutés pendant que l'app était fermée), puis surveillance.
            std::thread::spawn(move || {
                let shared = handle.state::<Shared>().inner().clone();
                let _ = shared.lock().library.rescan();
                let _ = tauri::Emitter::emit(&handle, "library-changed", ());
                watch::restart_folder_watch(&handle);
                watch::start_device_poll(&handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::list,
            commands::sidebar,
            commands::detail,
            commands::save_book,
            commands::rescan,
            commands::add_folder,
            commands::remove_folder,
            commands::import,
            commands::trash,
            commands::restore,
            commands::delete_permanently,
            commands::empty_trash,
            commands::queue,
            commands::unqueue,
            commands::forget_sent,
            commands::device,
            commands::send,
            commands::eject,
            commands::delete_document,
            commands::rename_reader,
            commands::forget_reader,
            commands::settings,
            commands::set_language,
            commands::set_show_duplicates,
            commands::clear_conversions,
            commands::export_epub,
        ])
        .run(tauri::generate_context!())
        .expect("impossible de lancer Foluna");
}
