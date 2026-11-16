//! Cœur de l'application côté Rust.
//!
//! Chaque fichier `.rs` est un *module*. `mod config;` dit au compilateur
//! d'inclure `src/config.rs` (≈ un `import` de fichier en TypeScript).

mod config;
mod error;

use config::Config;
use error::AppError;

/// Commande `get_config` : le front l'appelle avec `invoke("get_config")`.
///
/// - `app: tauri::AppHandle` est fourni automatiquement par Tauri (le front
///   n'envoie aucun argument). Il donne accès au dossier de config de l'app.
/// - Le retour `Result<Config, AppError>` devient côté JavaScript une promesse :
///   `Ok(config)` la résout avec la config en JSON, `Err(erreur)` la rejette
///   avec l'erreur sérialisée (`{ message, path, line, column }`).
#[tauri::command]
fn get_config(app: tauri::AppHandle) -> Result<Config, AppError> {
    // `?` : si `config_dir` renvoie une erreur, `get_config` s'arrête et la
    // renvoie telle quelle. Sinon, `dir` reçoit le chemin.
    let dir = config::config_dir(&app)?;
    // Dernière expression sans `;` : c'est la valeur renvoyée par la fonction.
    config::load_or_create(&dir)
}

/// Commande `open_config_folder` : ouvre le dossier de `config.json` dans
/// l'Explorateur Windows, pour pouvoir éditer le fichier facilement.
#[tauri::command]
fn open_config_folder(app: tauri::AppHandle) -> Result<(), AppError> {
    let dir = config::config_dir(&app)?;
    let io_err = |source| AppError::Io {
        action: "d'ouvrir",
        path: dir.clone(),
        source,
    };

    std::fs::create_dir_all(&dir).map_err(io_err)?;
    // On lance `explorer.exe <dossier>` sans attendre qu'il se ferme (`spawn`).
    // Sur Linux/macOS (développement uniquement), on utilise l'équivalent.
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map_err(io_err)?;
    Ok(())
}

/// Construit et démarre l'application.
///
/// `#[cfg_attr(mobile, ...)]` ne concerne que les builds mobiles : on peut l'ignorer.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // On déclare la liste des commandes appelables depuis le front.
        .invoke_handler(tauri::generate_handler![get_config, open_config_folder])
        // `generate_context!` lit tauri.conf.json au moment de la compilation.
        .run(tauri::generate_context!())
        // `expect` : si le démarrage échoue, on arrête l'app avec ce message.
        .expect("erreur au démarrage de l'application");
}
