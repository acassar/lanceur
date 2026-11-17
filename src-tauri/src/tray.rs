//! Icône dans la zone de notification (à droite de la barre des tâches).
//!
//! - Clic gauche : affiche la fenêtre, ou la masque si elle était devant.
//! - Clic droit : un menu « Afficher le lanceur », « Importer les jeux
//!   Steam… », « Ouvrir le dossier de config » et « Quitter ».
//!
//! Avec cette icône, la croix de la fenêtre ne quitte plus l'app : elle la
//! masque, comme Échap (voir `window::on_window_event`). On quitte par le menu.
//!
//! Notions Rust abordées : le *builder pattern* (on enchaîne `.tooltip(…)`,
//! `.menu(…)`… puis `.build()`), et `match` sur des textes.

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

use crate::window;

/// Identifiant de notre icône : `app.tray_by_id(TRAY_ID)` la retrouve.
pub const TRAY_ID: &str = "main";

// Identifiants des entrées du menu, reçus dans `on_menu`.
const SHOW: &str = "show";
const OPEN_CONFIG: &str = "open-config";
const IMPORT_STEAM: &str = "import-steam";

/// Événement envoyé au front pour qu'il ouvre la fenêtre d'import Steam
/// (`STEAM_IMPORT_EVENT` dans src/lib/ipc.ts).
pub const STEAM_IMPORT_EVENT: &str = "steam-import-requested";
const QUIT: &str = "quit";

/// Crée l'icône. Appelée une fois, dans le `setup` de `lib.rs`.
///
/// `tauri::Result<()>` : en cas d'échec (zone de notification indisponible…),
/// l'erreur est renvoyée à `setup`, qui l'écrit dans la console et continue.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    // `MenuItem::with_id(app, id, texte, actif, raccourci)`.
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, SHOW, "Afficher le lanceur", true, None::<&str>)?,
            &MenuItem::with_id(
                app,
                IMPORT_STEAM,
                "Importer les jeux Steam…",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app,
                OPEN_CONFIG,
                "Ouvrir le dossier de config",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, "Quitter", true, None::<&str>)?,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Lanceur")
        .menu(&menu)
        // Le menu ne s'ouvre qu'au clic droit ; le clic gauche affiche la fenêtre.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_click);
    // Même image que la fenêtre (icons/ du bundle). `if let Some(x) = …` :
    // n'exécute le bloc que s'il y a bien une icône.
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    // L'icône est gardée par Tauri jusqu'à la fin de l'app : on n'a pas besoin
    // de conserver la valeur renvoyée (`_`).
    let _ = builder.build(app)?;
    Ok(())
}

/// L'icône existe-t-elle ? Sinon, la croix de la fenêtre quitte l'app comme
/// avant (il faut bien un moyen de la fermer).
pub fn exists(app: &AppHandle) -> bool {
    app.tray_by_id(TRAY_ID).is_some()
}

/// Clic sur une entrée du menu (sur le thread principal).
fn on_menu(app: &AppHandle, event: MenuEvent) {
    // `event.id.as_ref()` : l'identifiant en `&str`, pour le comparer à nos constantes.
    match event.id.as_ref() {
        SHOW => window::show(app),
        // La liste des jeux à cocher est affichée par le front : on montre la
        // fenêtre, puis on lui demande d'ouvrir l'import.
        IMPORT_STEAM => {
            window::show(app);
            let _ = app.emit(STEAM_IMPORT_EVENT, ());
        }
        OPEN_CONFIG => {
            if let Err(e) = crate::open_config_dir(app) {
                eprintln!("dossier de config non ouvert : {e}");
            }
        }
        // Quitte vraiment : la fenêtre est fermée sans passer par la croix.
        QUIT => app.exit(0),
        _ => {}
    }
}

/// Clic gauche sur l'icône elle-même. À l'appui, on note si la fenêtre est
/// devant ; on agit au *relâchement* (comme un bouton normal). Un double-clic
/// envoie aussi deux clics simples : le second est ignoré, pour ne pas masquer
/// aussitôt la fenêtre que le premier vient d'afficher (voir window.rs).
fn on_click(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state,
        ..
    } = event
    {
        match button_state {
            MouseButtonState::Down => window::tray_pressed(tray.app_handle()),
            MouseButtonState::Up => window::tray_released(tray.app_handle()),
        }
    }
}
