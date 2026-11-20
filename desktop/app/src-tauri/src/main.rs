// Pas de fenêtre de console en plus de l'app sous Windows (version publiée).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    foluna_lib::run()
}
