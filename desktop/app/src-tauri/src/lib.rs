//! App Tauri : fenêtre, commandes appelées par l'interface (`app/ui`), état de la bibliothèque.

/// Version de l'app, affichée par l'interface.
#[tauri::command]
fn app_version() -> &'static str {
    foluna_core::VERSION
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_version])
        .run(tauri::generate_context!())
        .expect("impossible de lancer Foluna");
}
