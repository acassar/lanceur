//! Cœur de l'application côté Rust.
//!
//! Chaque fichier `.rs` est un *module*. `mod config;` déclare le module :
//! sans cette ligne, `src/config.rs` ne serait même pas compilé (en TS, un
//! fichier existe dès qu'il est là ; en Rust, il faut le déclarer).
//! `use config::Config;` est ensuite l'équivalent d'un `import { Config }`.

mod config;
mod error;
mod launch;
mod state;

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

/// Commande `get_recents` : ids des derniers lancements, du plus récent au
/// plus ancien (au plus `recentsCount`, et seulement des tuiles qui existent).
#[tauri::command]
async fn get_recents(app: tauri::AppHandle) -> Result<Vec<String>, AppError> {
    let dir = config::config_dir(&app)?;
    let config = config::load_or_create(&dir)?;
    Ok(state::visible_recents(&state::load(&dir), &config))
}

/// Commande `launch` : `invoke("launch", { id: "cs2" })`.
///
/// Tauri lit l'argument `id` dans l'objet JSON envoyé par le front et le
/// convertit en `String`. On relit la config (elle a pu être modifiée à la
/// main depuis l'ouverture), on lance la tuile, puis on met à jour les récents.
/// Renvoie la nouvelle liste des récents, que le front affiche aussitôt.
#[tauri::command]
async fn launch(app: tauri::AppHandle, id: String) -> Result<Vec<String>, AppError> {
    let dir = config::config_dir(&app)?;
    // Lire les fichiers et attendre Windows peut prendre un moment (Steam qui
    // démarre…). `spawn_blocking` fait ce travail sur un thread prévu pour
    // attendre, sans occuper ceux qui traitent les autres commandes.
    // `move` donne à la closure la propriété de `dir` et `id`.
    tauri::async_runtime::spawn_blocking(move || launch_tile(&dir, &id))
        .await
        // `await` échoue seulement si le thread a paniqué.
        .unwrap_or_else(|e| {
            Err(AppError::Launch {
                target: "la tuile".into(),
                reason: e.to_string(),
            })
        })
}

/// Le travail de la commande `launch`, en code « normal » (non async).
fn launch_tile(dir: &std::path::Path, id: &str) -> Result<Vec<String>, AppError> {
    let config = config::load_or_create(dir)?;

    // `iter().find(…)` renvoie `Option<&Tile>` ; `ok_or_else` le transforme
    // en `Result` : `None` devient l'erreur `UnknownTile`, et `?` la renvoie.
    let tile = config
        .tiles
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::UnknownTile(id.to_string()))?;

    launch::launch(&tile.target, &config.settings.browser)?;

    // La tuile est lancée : si l'écriture des récents échoue, on ne veut pas
    // afficher « Impossible de lancer ». On le signale dans la console et on
    // renvoie les récents sans ce lancement.
    let state = state::record_launch(dir, &tile.id).unwrap_or_else(|e| {
        eprintln!("récents non enregistrés : {e}");
        state::load(dir)
    });
    Ok(state::visible_recents(&state, &config))
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
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_recents,
            launch,
            open_config_folder
        ])
        // `generate_context!` lit tauri.conf.json au moment de la compilation.
        .run(tauri::generate_context!())
        // `expect` : si le démarrage échoue, on arrête l'app avec ce message.
        .expect("erreur au démarrage de l'application");
}
