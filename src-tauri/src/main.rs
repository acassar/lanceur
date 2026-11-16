// Point d'entrée de l'exécutable.
//
// La ligne `#![cfg_attr(...)]` est un *attribut* : une instruction pour le compilateur.
// En release (build de la CI), elle dit à Windows de lancer l'app sans fenêtre console noire.
// En debug (`npm run tauri dev`), la console reste visible pour voir les logs. NE PAS SUPPRIMER.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Tout le code vit dans la bibliothèque `lanceur_lib` (src/lib.rs).
    // main.rs se contente de l'appeler : c'est la convention des projets Tauri 2.
    lanceur_lib::run()
}
