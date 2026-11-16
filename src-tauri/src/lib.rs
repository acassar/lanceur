//! Cœur de l'application côté Rust.
//!
//! Chaque fichier `.rs` est un *module*. `mod config;` déclare le module :
//! sans cette ligne, `src/config.rs` ne serait même pas compilé (en TS, un
//! fichier existe dès qu'il est là ; en Rust, il faut le déclarer).
//! `use config::Config;` est ensuite l'équivalent d'un `import { Config }`.

mod config;
mod error;

use config::Config;
use error::AppError;

/// Commande `get_config` : le front l'appelle avec `invoke("get_config")`.
///
/// - `app: tauri::AppHandle` est fourni automatiquement par Tauri (le front
///   n'envoie aucun argument). Il donne accès au dossier de config de l'app.
/// - `async` : la commande tourne en arrière-plan. Sans lui, Tauri l'exécute
///   sur le thread principal, et une lecture de fichier lente figerait la fenêtre.
/// - Le retour `Result<Config, AppError>` devient côté JavaScript une promesse :
///   `Ok(config)` la résout avec la config en JSON, `Err(erreur)` la rejette
///   avec l'erreur sérialisée (`{ message, path, line, column }`).
#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<Config, AppError> {
    // `?` : si `config_dir` renvoie une erreur, `get_config` s'arrête et la
    // renvoie telle quelle. Sinon, `dir` reçoit le chemin.
    let dir = config::config_dir(&app)?;
    // Dernière expression sans `;` : c'est la valeur renvoyée par la fonction.
    config::load_or_create(&dir)
}

/// Commande `open_config_folder` : ouvre le dossier de `config.json` dans
/// l'Explorateur Windows, pour pouvoir éditer le fichier facilement.
#[tauri::command]
async fn open_config_folder(app: tauri::AppHandle) -> Result<(), AppError> {
    let dir = config::config_dir(&app)?;
    // Closure qui fabrique l'erreur ; elle emprunte `dir` le temps qu'elle existe.
    let io_err = |source| AppError::Io {
        action: "d'ouvrir",
        path: dir.clone(),
        source,
    };

    std::fs::create_dir_all(&dir).map_err(io_err)?;
    // On lance l'Explorateur sans attendre qu'il se ferme (`spawn`).
    std::process::Command::new(file_manager())
        .arg(&dir)
        .spawn()
        .map_err(io_err)?;
    Ok(())
}

/// Programme qui ouvre un dossier. Sous Windows, on donne le chemin complet
/// d'`explorer.exe` (dans `%SystemRoot%`) plutôt que de le chercher dans le PATH.
/// Sur Linux/macOS (développement uniquement), on utilise l'équivalent.
fn file_manager() -> std::path::PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        std::path::Path::new(&root).join("explorer.exe")
    } else if cfg!(target_os = "macos") {
        "open".into()
    } else {
        "xdg-open".into()
    }
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
