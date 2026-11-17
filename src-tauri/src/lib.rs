//! Cœur de l'application côté Rust.
//!
//! Chaque fichier `.rs` est un *module*. `mod config;` déclare le module :
//! sans cette ligne, `src/config.rs` ne serait même pas compilé (en TS, un
//! fichier existe dès qu'il est là ; en Rust, il faut le déclarer).
//! `use config::Config;` est ensuite l'équivalent d'un `import { Config }`.

mod apps;
mod autostart;
mod config;
mod editor;
mod error;
mod favorites;
mod fsutil;
mod icons;
mod jsontext;
mod launch;
mod settings;
mod state;
mod steam;
mod tray;
mod vdf;
mod watch;
mod window;
mod winkey;

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
    // Lecture du disque, réglages et ménage des icônes : du travail qui attend
    // (fichiers, Windows), donc sur un thread d'attente (`spawn_blocking`).
    let path = dir.join(config::FILE_NAME);
    tauri::async_runtime::spawn_blocking(move || reload(&app, &dir))
        .await
        // `await` échoue seulement si le thread a paniqué.
        .unwrap_or_else(|e| {
            Err(AppError::Invalid {
                path,
                problems: vec![format!("erreur interne pendant la lecture : {e}")],
            })
        })
}

/// Un seul rechargement à la fois (voir [`reload`]). `Mutex<()>` ne protège
/// aucune donnée, seulement une portion de code.
static RELOADING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Le travail de `get_config`.
///
/// Deux rechargements peuvent arriver presque ensemble (fenêtre réaffichée
/// pendant que vous enregistrez config.json). Sans verrou, le plus ancien
/// pourrait appliquer ses réglages *après* le plus récent et remettre l'ancien
/// raccourci. Avec le verrou, lecture + application se font d'un bloc, l'une
/// après l'autre : le dernier à lire le fichier est aussi le dernier à appliquer.
fn reload(app: &tauri::AppHandle, dir: &std::path::Path) -> Result<Config, AppError> {
    let _reloading = RELOADING.lock().unwrap_or_else(|p| p.into_inner());
    // Lecture du disque à chaque fois (pas de copie en mémoire) : c'est la
    // commande de rechargement. Elle met aussi à jour la copie mémorisée dont
    // se servent `get_icon` et `get_recents` (voir `config::load_cached`).
    let config = config::load_and_remember(dir)?;
    // Le raccourci et `hideOnBlur` suivent la config qu'on vient de lire.
    // Si le fichier est invalide, on n'arrive pas ici (`?` au-dessus) : les
    // derniers réglages valides restent en place.
    window::sync_settings(app, &config.settings);
    // Lancement au démarrage de Windows : le registre suit `launchAtStartup`.
    autostart::sync(config.settings.launch_at_startup);
    // Ménage du cache d'icônes : celles des tuiles retirées sont supprimées.
    icons::remove_orphans(dir, &config.tiles);
    // Dernière expression sans `;` : c'est la valeur renvoyée par la fonction.
    Ok(config)
}

/// Commande `update_settings` : `invoke("update_settings", { changes: { hotkey: "Super" } })`.
///
/// Écrit les réglages modifiés dans config.json (settings.rs), puis les
/// applique aussitôt (raccourci, `hideOnBlur`, lancement au démarrage) sans
/// attendre la surveillance du fichier. Renvoie l'état du raccourci : s'il
/// est déjà pris par une autre app, l'écran de paramètres l'affiche tout de suite.
#[tauri::command]
async fn update_settings(
    app: tauri::AppHandle,
    changes: serde_json::Map<String, serde_json::Value>,
) -> Result<window::HotkeyStatus, AppError> {
    use tauri::Manager;
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        settings::update(&dir, &changes)?;
        reload(&app, &dir)?;
        Ok(app.state::<window::WindowState>().hotkey_status())
    })
    .await
    .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `get_recents` : ids des derniers lancements, du plus récent au
/// plus ancien (au plus `recentsCount`, et seulement des tuiles qui existent).
#[tauri::command]
async fn get_recents(app: tauri::AppHandle) -> Result<Vec<String>, AppError> {
    let dir = config::config_dir(&app)?;
    let config = config::load_cached(&dir)?;
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

/// Commande `set_favorite` : `invoke("set_favorite", { id: "cs2", favorite: true })`.
///
/// Met la tuile en favori (ou l'en retire) en réécrivant `config.json`, et
/// renvoie la nouvelle liste des favoris. Le fichier est lu, modifié et écrit
/// sur un thread d'attente (`spawn_blocking`), comme pour `launch`.
#[tauri::command]
async fn set_favorite(
    app: tauri::AppHandle,
    id: String,
    favorite: bool,
) -> Result<Vec<String>, AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || favorites::set(&dir, &id, favorite))
        .await
        .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `add_tile` : ajoute une tuile créée dans la fenêtre (éditeur).
///
/// Tauri convertit l'objet JS `draft` en `editor::TileDraft` grâce à
/// `Deserialize` : un champ manquant ou mal typé rejette la promesse côté JS.
/// Renvoie la tuile créée, avec l'id choisi par Rust.
#[tauri::command]
async fn add_tile(
    app: tauri::AppHandle,
    draft: editor::TileDraft,
) -> Result<config::Tile, AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // `std::slice::from_ref` : une liste d'un seul élément, sans copie.
        let mut added = editor::add_tiles(&dir, std::slice::from_ref(&draft))?;
        // `add_tiles` renvoie une tuile par brouillon : ici, exactement une.
        added
            .pop()
            .ok_or_else(|| AppError::ConfigWrite("aucune tuile ajoutée".into()))
    })
    .await
    .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `update_tile` : remplace la tuile `id` (même id) par `draft`.
#[tauri::command]
async fn update_tile(
    app: tauri::AppHandle,
    id: String,
    draft: editor::TileDraft,
) -> Result<config::Tile, AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || editor::update_tile(&dir, &id, &draft))
        .await
        .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `delete_tile` : supprime la tuile `id` (et son favori).
#[tauri::command]
async fn delete_tile(app: tauri::AppHandle, id: String) -> Result<(), AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || editor::delete_tile(&dir, &id))
        .await
        .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `reorder_tiles` : nouvel ordre des tuiles d'une catégorie
/// (glisser-déposer). `ids` : toutes les tuiles de `category`, dans l'ordre voulu.
#[tauri::command]
async fn reorder_tiles(
    app: tauri::AppHandle,
    category: String,
    ids: Vec<String>,
) -> Result<(), AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || editor::reorder_tiles(&dir, &category, &ids))
        .await
        .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `reorder_categories` : nouvel ordre de toutes les catégories.
#[tauri::command]
async fn reorder_categories(app: tauri::AppHandle, ids: Vec<String>) -> Result<(), AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || editor::reorder_categories(&dir, &ids))
        .await
        .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `reorder_favorites` : nouvel ordre de tous les favoris.
#[tauri::command]
async fn reorder_favorites(app: tauri::AppHandle, ids: Vec<String>) -> Result<(), AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || editor::reorder_favorites(&dir, &ids))
        .await
        .unwrap_or_else(|e| Err(AppError::ConfigWrite(e.to_string())))
}

/// Commande `pick_file` : ouvre la boîte « Ouvrir » de Windows et renvoie le
/// chemin choisi (`None` si l'utilisateur annule). `images` : filtre sur les
/// images (icône) au lieu des programmes.
///
/// La boîte est ouverte côté Rust (plugin dialog) : le front n'a pas besoin
/// de permission pour accéder aux fichiers, il ne reçoit qu'un chemin.
#[tauri::command]
async fn pick_file(app: tauri::AppHandle, images: bool) -> Option<String> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    // Pendant que la boîte est ouverte, notre fenêtre n'est plus au premier
    // plan : sans cela, `hideOnBlur` la réduirait.
    window::set_native_dialog_open(&app, true);
    let mut dialog = app.dialog().file().set_title(if images {
        "Choisir une image"
    } else {
        "Choisir un programme ou un raccourci"
    });
    dialog = if images {
        dialog.add_filter(
            "Images",
            &["png", "jpg", "jpeg", "ico", "bmp", "gif", "webp"],
        )
    } else {
        dialog
            .add_filter(
                "Programmes et raccourcis",
                &["exe", "lnk", "bat", "cmd", "url"],
            )
            .add_filter("Tous les fichiers", &["*"])
    };
    // `set_parent` : la boîte est rattachée à notre fenêtre (au-dessus d'elle).
    if let Some(main) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&main);
    }
    // `blocking_pick_file` attend la réponse de l'utilisateur : sur un thread
    // d'attente, jamais sur le thread principal (la fenêtre figerait).
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .ok()
        .flatten();
    window::set_native_dialog_open(&app, false);
    // `into_path` : le chemin Windows (`C:\…`) ; `to_string_lossy` le
    // convertit en texte (les rares caractères non convertibles deviennent `?`).
    picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Commande `detect_steam_games` : jeux Steam installés sur ce PC, triés par
/// nom. Ne modifie rien : le front affiche la liste et l'utilisateur choisit.
///
/// Séparée de l'import pour être réutilisée ailleurs (écran d'accueil qui
/// propose de détecter les applications installées).
#[tauri::command]
async fn detect_steam_games() -> Result<Vec<steam::SteamGame>, AppError> {
    // Lecture de fichiers sur plusieurs disques : sur un thread d'attente.
    tauri::async_runtime::spawn_blocking(steam::detect)
        .await
        .unwrap_or_else(|e| Err(AppError::Steam(e.to_string())))
}

/// Commande `import_steam_games` : `invoke("import_steam_games", { appIds: [730] })`.
///
/// Ajoute à config.json une tuile par jeu choisi (ceux qui en ont déjà une
/// sont ignorés). Tauri convertit `appIds` (camelCase côté JS) en `app_ids`.
///
/// Le front n'envoie que des numéros : le nom des jeux est relu par Rust dans
/// les fichiers de Steam. Seuls des jeux réellement installés peuvent donc
/// être ajoutés, comme pour `get_icon` où seul l'id de la tuile est transmis.
#[tauri::command]
async fn import_steam_games(
    app: tauri::AppHandle,
    app_ids: Vec<u32>,
) -> Result<steam::ImportResult, AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // `into_iter().filter(…).collect()` : comme `games.filter(…)` en JS,
        // mais en *consommant* la liste d'origine (pas de copie des noms).
        let chosen: Vec<steam::SteamGame> = steam::detect()?
            .into_iter()
            .filter(|g| app_ids.contains(&g.app_id))
            .collect();
        steam::import(&dir, &chosen)
    })
    .await
    // Le thread a paniqué (détection ou import) : l'écriture, qui vient en
    // dernier et est atomique, n'a pas eu lieu.
    .unwrap_or_else(|e| Err(AppError::Steam(format!("import interrompu ({e})"))))
}

/// Commande `detect_apps` : applications installées (raccourcis du menu
/// Démarrer), triées par nom, avec celles qui ont déjà une tuile marquées.
/// Ne modifie rien : le front affiche la liste et l'utilisateur coche.
#[tauri::command]
async fn detect_apps(app: tauri::AppHandle) -> Result<Vec<apps::DetectedApp>, AppError> {
    let dir = config::config_dir(&app)?;
    // Parcours de dossiers et lecture de raccourcis : sur un thread d'attente.
    tauri::async_runtime::spawn_blocking(move || apps::detect(apps::current_config(&dir).as_ref()))
        .await
        .map_err(|e| AppError::Detect(format!("recherche interrompue ({e})")))
}

/// Commande `import_apps` : `invoke("import_apps", { paths: ["%APPDATA%\\…\\Discord.lnk"] })`.
///
/// Comme pour Steam, le front n'envoie que des chemins *déjà détectés* : Rust
/// refait la détection et ne garde que ceux qui y figurent. Le front ne peut
/// donc pas faire ajouter n'importe quel programme par ce chemin-là.
#[tauri::command]
async fn import_apps(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<steam::ImportResult, AppError> {
    let dir = config::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let chosen: Vec<apps::DetectedApp> = apps::detect(None)
            .into_iter()
            .filter(|a| paths.contains(&a.path))
            .collect();
        apps::import(&dir, &chosen)
    })
    .await
    .unwrap_or_else(|e| Err(AppError::ConfigWrite(format!("import interrompu ({e})"))))
}

/// Commande `get_icon` : `invoke("get_icon", { id: "discord" })`.
///
/// Renvoie l'icône de la tuile en `data:` URL (`data:image/png;base64,…`), ou
/// `null` si aucune n'a été trouvée : le front garde alors le monogramme.
/// La 1ʳᵉ fois, l'icône est extraite ou téléchargée puis mise en cache dans
/// `icons\` ; ensuite, elle est simplement relue (voir icons.rs).
///
/// Le front n'envoie que l'id : c'est Rust qui relit la config pour savoir quel
/// fichier ou quel site lire. Le front ne peut donc pas lui faire lire autre chose.
#[tauri::command]
async fn get_icon(app: tauri::AppHandle, id: String) -> Option<String> {
    // `.ok()?` : en cas d'erreur (config invalide…), on renvoie `None`. Le
    // message d'erreur de la config est déjà affiché par `get_config`.
    let dir = config::config_dir(&app).ok()?;
    // `load_cached` : le fichier n'est relu que s'il a changé depuis la dernière
    // lecture. À l'ouverture, `get_icon` est appelée une fois par tuile.
    let config = config::load_cached(&dir).ok()?;
    // `into_iter()` consomme la liste : on récupère la tuile elle-même (pas
    // une référence), qu'on peut donner à la tâche async.
    let tile = config.tiles.into_iter().find(|t| t.id == id)?;
    icons::icon_data_url(dir, tile).await
}

/// Commande `open_config_folder` : ouvre le dossier de `config.json` dans
/// l'Explorateur Windows, pour pouvoir éditer le fichier facilement.
#[tauri::command]
async fn open_config_folder(app: tauri::AppHandle) -> Result<(), AppError> {
    open_config_dir(&app)
}

/// Le travail d'`open_config_folder`, aussi utilisé par le menu de l'icône de
/// notification (tray.rs). `pub(crate)` : visible dans tout notre code, mais
/// pas en dehors de la bibliothèque.
pub(crate) fn open_config_dir(app: &tauri::AppHandle) -> Result<(), AppError> {
    let dir = config::config_dir(app)?;
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

/// Commande `hide_window` : réduit la fenêtre (Échap, après un lancement).
///
/// Sans `async`, la commande tourne sur le thread principal : c'est très
/// rapide ici, et c'est le thread qui gère la fenêtre.
#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    window::hide(&app);
}

/// Commande `get_hotkey_status` : le raccourci configuré et, s'il n'a pas pu
/// être enregistré, pourquoi. Le front l'affiche dans un message.
///
/// `tauri::State<'_, WindowState>` : Tauri injecte l'état confié avec
/// `manage` au démarrage. `'_` est une *durée de vie* : l'emprunt de l'état ne
/// dure que le temps de l'appel (le compilateur s'occupe du détail).
#[tauri::command]
fn get_hotkey_status(state: tauri::State<'_, window::WindowState>) -> window::HotkeyStatus {
    state.hotkey_status()
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
        // Les *plugins* ajoutent des fonctions à Tauri, comme des modules NestJS.
        //
        // Instance unique : en premier, pour que la 2ᵉ copie de l'app s'arrête
        // avant d'avoir rien initialisé. Le plugin appelle alors cette closure
        // dans la 1ʳᵉ copie, qui réaffiche sa fenêtre. `|app, _args, _cwd|` sont
        // les paramètres de la closure ; le `_` dit « paramètre inutilisé ».
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Une copie lancée par Windows à l'ouverture de session ne doit
            // pas faire surgir la fenêtre déjà ouverte.
            if !autostart::launched_at_startup(&args) {
                window::show(app);
            }
        }))
        .plugin(window::shortcut_plugin())
        // Boîte « Ouvrir » de Windows, pour choisir un programme (éditeur de tuiles).
        .plugin(tauri_plugin_dialog::init())
        // `manage` confie une valeur à Tauri ; on la retrouve ensuite avec
        // `app.state::<WindowState>()` ou un paramètre `State<…>` de commande.
        .manage(window::WindowState::default())
        .manage(watch::ConfigWatcher::default())
        // `setup` s'exécute une fois, au démarrage, avant l'affichage.
        .setup(|app| {
            let handle = app.handle();
            // Réglages de la fenêtre lus dans config.json. Si le fichier est
            // invalide, on prend les réglages par défaut : le raccourci marche
            // quand même, et le front affiche l'erreur de config.
            // `.ok()` : `Result` -> `Option` (l'erreur ne sert pas ici).
            let dir = config::config_dir(handle).ok();
            let loaded = dir
                .as_deref()
                .and_then(|d| config::load_or_create(d).ok())
                .map(|c| c.settings);
            // Le lancement au démarrage ne suit qu'une config *valide* : une
            // faute de frappe dans config.json ne doit pas le désactiver.
            if let Some(settings) = &loaded {
                autostart::sync(settings.launch_at_startup);
            }
            window::sync_settings(handle, &loaded.unwrap_or_default());
            // Icône de notification. Si elle ne peut pas être créée, l'app
            // marche quand même (la croix quitte alors l'app, comme avant).
            if let Err(e) = tray::create(handle) {
                eprintln!("icône de notification non créée : {e}");
            }
            // Rechargement automatique quand config.json est enregistré (watch.rs).
            // Le dossier existe : `load_or_create` vient de le créer au besoin.
            if let Some(dir) = &dir {
                watch::start(handle, dir);
            }
            // La fenêtre est créée cachée (`"visible": false` dans
            // tauri.conf.json). Lancement normal : on l'affiche, centrée.
            // Lancement par Windows à l'ouverture de session : elle reste
            // cachée, le raccourci ou l'icône de notification l'ouvriront.
            if !autostart::launched_at_startup(std::env::args_os()) {
                window::show(handle);
            }
            Ok(())
        })
        .on_window_event(window::on_window_event)
        // On déclare la liste des commandes appelables depuis le front.
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_recents,
            launch,
            set_favorite,
            add_tile,
            update_tile,
            delete_tile,
            pick_file,
            reorder_tiles,
            reorder_categories,
            reorder_favorites,
            detect_steam_games,
            import_steam_games,
            detect_apps,
            import_apps,
            get_icon,
            open_config_folder,
            hide_window,
            get_hotkey_status,
            update_settings
        ])
        // `generate_context!` lit tauri.conf.json au moment de la compilation.
        .run(tauri::generate_context!())
        // `expect` : si le démarrage échoue, on arrête l'app avec ce message.
        .expect("erreur au démarrage de l'application");
}
