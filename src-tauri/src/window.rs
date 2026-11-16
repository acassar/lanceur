//! La fenêtre : afficher, masquer, centrer, et le raccourci clavier global.
//!
//! « Masquer » veut dire ici **réduire** (décision 1A du plan) : l'icône reste
//! dans la barre des tâches, et un clic dessus restaure la fenêtre.
//!
//! Trois façons de faire réapparaître la fenêtre, qui passent toutes par [`show`] :
//! - le raccourci global (`Ctrl+Alt+Espace` par défaut, réglage `hotkey`) ;
//! - relancer l'exe (plugin *single-instance*, branché dans `lib.rs`) ;
//! - cliquer sur l'icône de la barre des tâches (géré par Windows lui-même).
//!
//! Notions Rust abordées : les plugins, l'état partagé `State<…>` avec un
//! `Mutex` et un `AtomicBool`, et les closures passées aux plugins.

use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::config::Settings;

/// Label de la fenêtre principale (le `label` de `tauri.conf.json`).
const MAIN: &str = "main";

/// Événement envoyé au front quand la fenêtre réapparaît : il y vide la
/// recherche et redonne le focus au champ.
pub const SHOWN_EVENT: &str = "window-shown";

/// État partagé de la fenêtre, confié à Tauri avec `app.manage(…)` dans
/// `lib.rs`. N'importe quelle commande peut ensuite le demander en ajoutant un
/// paramètre `state: tauri::State<'_, WindowState>` (Tauri l'injecte, un peu
/// comme l'injection de dépendances de NestJS).
///
/// Tauri partage cet état entre plusieurs threads (les commandes, la boucle
/// d'événements de la fenêtre…). Rust refuse de compiler un accès concurrent
/// non protégé : chaque champ modifiable est donc enveloppé.
#[derive(Default)]
pub struct WindowState {
    /// Masquer la fenêtre quand elle perd le focus (réglage `hideOnBlur`).
    ///
    /// `AtomicBool` : un booléen qu'on peut lire et écrire depuis plusieurs
    /// threads sans verrou. Il est lu à chaque perte de focus, sur le thread
    /// principal : pour un simple booléen, inutile de passer par un `Mutex`.
    hide_on_blur: AtomicBool,

    /// Le raccourci actuellement enregistré, et la dernière erreur.
    ///
    /// `Mutex<…>` (verrou d'exclusion mutuelle) : pour lire ou modifier la
    /// valeur, il faut d'abord la « prendre » avec `lock()`. Un seul thread à
    /// la fois la tient ; les autres attendent. Le verrou se relâche tout seul
    /// quand la variable qui le tient disparaît (fin du bloc).
    ///
    /// On ne le garde que le temps de lire ou d'écrire quelques champs, jamais
    /// pendant un appel qui attend un autre thread (voir [`sync_settings`]).
    hotkey: Mutex<Hotkey>,

    /// « Tour de parole » de [`sync_settings`] : deux lectures de la config en
    /// même temps ne doivent pas enregistrer deux raccourcis. `Mutex<()>` ne
    /// protège aucune donnée, seulement une portion de code.
    syncing: Mutex<()>,
}

#[derive(Default)]
struct Hotkey {
    /// Texte du raccourci actif, tel qu'écrit dans config.json (`"Ctrl+Alt+Space"`).
    text: String,
    /// Raccourci réellement enregistré auprès de Windows, s'il y en a un.
    registered: Option<Shortcut>,
    /// Message à montrer à l'utilisateur si le raccourci n'a pas pu être pris.
    error: Option<String>,
}

/// Ce que la commande `get_hotkey_status` renvoie au front.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub hotkey: String,
    pub error: Option<String>,
}

impl WindowState {
    pub fn hotkey_status(&self) -> HotkeyStatus {
        let hotkey = lock(&self.hotkey);
        HotkeyStatus {
            hotkey: hotkey.text.clone(),
            error: hotkey.error.clone(),
        }
    }
}

/// Prend le verrou même si un autre thread a paniqué en le tenant (le
/// `Mutex` est alors dit « empoisonné ») : nos données restent cohérentes.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Applique les réglages de config.json qui concernent la fenêtre.
///
/// Appelée au démarrage, puis à chaque lecture de la config (`get_config`).
/// Le front relit la config chaque fois que la fenêtre réapparaît : si vous
/// changez `hotkey` ou `hideOnBlur` à la main, ils s'appliquent au prochain
/// affichage, sans redémarrer l'app.
///
/// Si le nouveau raccourci est invalide ou déjà pris par une autre app, on
/// garde l'ancien (s'il y en avait un) et on retient l'erreur pour l'afficher.
pub fn sync_settings(app: &AppHandle, settings: &Settings) {
    // `app.state::<T>()` récupère l'état confié avec `app.manage(…)`.
    let state = app.state::<WindowState>();
    // `Ordering::Relaxed` : on n'a besoin d'aucune garantie d'ordre avec
    // d'autres variables, juste de la valeur elle-même.
    state
        .hide_on_blur
        .store(settings.hide_on_blur, Ordering::Relaxed);

    // Une seule synchronisation à la fois. Ce verrou n'est jamais pris par du
    // code du thread principal, sauf au démarrage, quand rien d'autre ne tourne.
    let _syncing = lock(&state.syncing);

    // Résultat à enregistrer dans l'état : `Ok(Some(..))` nouveau raccourci,
    // `Ok(None)` rien à changer, `Err(message)` échec.
    let outcome = match parse_hotkey(&settings.hotkey) {
        Err(message) => Err(message),
        Ok(wanted) => {
            // Le bloc `{ … }` limite la durée du verrou à cette seule lecture.
            let current = { lock(&state.hotkey).registered };
            if current == Some(wanted) {
                Ok(None)
            } else {
                // Attention au piège : `register` attend que le thread
                // principal ait fait le travail. Si on tenait `state.hotkey`
                // pendant ce temps et que le thread principal voulait le lire
                // (commande `get_hotkey_status`), chacun attendrait l'autre
                // pour toujours (*deadlock*). On appelle donc `register` sans
                // tenir ce verrou.
                let manager = app.global_shortcut();
                match manager.register(wanted) {
                    Ok(()) => {
                        // Le nouveau est pris : on libère l'ancien.
                        if let Some(old) = current {
                            let _ = manager.unregister(old);
                        }
                        Ok(Some(wanted))
                    }
                    Err(e) => Err(format!(
                        "Le raccourci « {} » n'a pas pu être enregistré ({e}). \
                         Il est peut-être déjà utilisé par une autre application : \
                         choisissez-en un autre dans config.json (réglage hotkey).",
                        settings.hotkey
                    )),
                }
            }
        }
    };

    let mut hotkey = lock(&state.hotkey);
    match outcome {
        Ok(new) => {
            if let Some(shortcut) = new {
                hotkey.registered = Some(shortcut);
            }
            hotkey.text = settings.hotkey.clone();
            hotkey.error = None;
        }
        // L'ancien raccourci (et son texte) reste actif.
        Err(message) => hotkey.error = Some(message),
    }
}

/// Lit un raccourci écrit comme `"Ctrl+Alt+Space"`. Les noms de touches sont
/// ceux de la crate `global-hotkey` : `Ctrl`, `Alt`, `Shift`, `Super` (touche
/// Windows), puis une touche : `Space`, `A`…`Z`, `0`…`9`, `F1`…`F24`…
/// La casse n'a pas d'importance.
///
/// Un raccourci sans modificateur (juste `F9`, ou `Space`) est refusé : il
/// volerait cette touche à toutes les autres applications, jeux compris.
pub fn parse_hotkey(text: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(text.trim())
        .map_err(|e| format!("Raccourci « {text} » invalide dans config.json : {e}."))?;
    if shortcut.mods.is_empty() {
        return Err(format!(
            "Raccourci « {text} » refusé : ajoutez au moins Ctrl, Alt, Shift ou Super, \
             sinon la touche serait volée à toutes les autres applications."
        ));
    }
    Ok(shortcut)
}

/// Le plugin de raccourci global, prêt à être branché dans `lib.rs`.
///
/// `with_handler` reçoit une *closure* : une fonction anonyme, comme
/// `(app, shortcut, event) => { … }` en TypeScript. Le plugin l'appelle à
/// chaque appui et à chaque relâchement d'un raccourci enregistré. Un seul
/// raccourci est enregistré à la fois, donc pas besoin de regarder lequel.
pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_global_shortcut::{Builder, ShortcutState};
    Builder::new()
        .with_handler(|app, _shortcut, event| {
            // On ne réagit qu'à l'appui : sinon le relâchement annulerait aussitôt.
            if event.state == ShortcutState::Pressed {
                toggle(app);
            }
        })
        .build()
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

/// Raccourci global : masque la fenêtre si elle est au premier plan, sinon
/// l'affiche. Si elle est ouverte mais derrière un jeu ou un navigateur, un
/// appui la ramène devant au lieu de la réduire.
pub fn toggle(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    // `unwrap_or(false)` : en cas d'erreur, on considère la fenêtre comme
    // non affichée, et on l'affiche.
    let in_front = window.is_visible().unwrap_or(false)
        && !window.is_minimized().unwrap_or(true)
        && window.is_focused().unwrap_or(false);
    if in_front { hide(app) } else { show(app) }
}

/// Affiche la fenêtre au centre de l'écran où se trouve la souris, lui donne
/// le focus, puis prévient le front (qui vide la recherche et y met le focus).
pub fn show(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    // Chaque appel peut échouer (fenêtre en cours de fermeture…). On ignore ces
    // erreurs (`let _ =`) : afficher la fenêtre « au mieux » suffit ici.
    let _ = window.unminimize();
    let _ = window.show();
    center_on_active_monitor(app, &window);
    let _ = window.set_focus();
    let _ = window.emit(SHOWN_EVENT, ());
}

/// « Masque » la fenêtre : la réduit dans la barre des tâches (décision 1A).
pub fn hide(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        let _ = window.minimize();
    }
}

/// Appelée par Tauri à chaque événement de la fenêtre (branchée dans `lib.rs`).
/// Perte de focus + `hideOnBlur` : on réduit la fenêtre.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::Focused(false) = event {
        let state = window.state::<WindowState>();
        if state.hide_on_blur.load(Ordering::Relaxed) && !window.is_minimized().unwrap_or(false) {
            let _ = window.minimize();
        }
    }
}

/// Centre la fenêtre sur l'écran qui contient la souris (l'« écran actif »).
/// On utilise la *zone de travail* de l'écran, c'est-à-dire sans la barre des
/// tâches.
pub fn center_on_active_monitor(app: &AppHandle, window: &WebviewWindow) {
    // Écran sous la souris, sinon celui de la fenêtre, sinon l'écran principal.
    // `.ok().flatten()` transforme `Result<Option<Monitor>>` en `Option<Monitor>`,
    // et `.or_else(…)` essaie la solution suivante si on n'a rien trouvé.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };

    // Les tailles sont en pixels physiques. Si l'écran cible n'a pas la même
    // mise à l'échelle Windows (100 %, 150 %…), la fenêtre y aura une autre
    // taille en pixels : on la convertit avant de calculer le centre.
    let ratio = monitor.scale_factor() / window.scale_factor().unwrap_or(1.0);
    let size = PhysicalSize::new(
        (size.width as f64 * ratio).round() as u32,
        (size.height as f64 * ratio).round() as u32,
    );

    let area = monitor.work_area();
    let _ = window.set_position(centered(area.position, area.size, size));
}

/// Position du coin haut-gauche pour centrer `window` dans la zone `area`.
/// Si la fenêtre est plus grande que la zone, on la colle en haut à gauche
/// pour que la barre de titre reste visible.
fn centered(
    area_pos: PhysicalPosition<i32>,
    area_size: PhysicalSize<u32>,
    window: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    // `saturating_sub` : une soustraction qui s'arrête à 0 au lieu de passer en
    // négatif (impossible pour un `u32`, qui ne peut pas être négatif).
    let dx = area_size.width.saturating_sub(window.width) / 2;
    let dy = area_size.height.saturating_sub(window.height) / 2;
    PhysicalPosition::new(area_pos.x + dx as i32, area_pos.y + dy as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    #[test]
    fn lit_le_raccourci_par_defaut() {
        let s = parse_hotkey("Ctrl+Alt+Space").unwrap();
        assert_eq!(s.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(s.key, Code::Space);
    }

    #[test]
    fn la_casse_et_les_espaces_autour_sont_ignores() {
        assert_eq!(
            parse_hotkey(" ctrl+shift+f9 ").unwrap(),
            parse_hotkey("Ctrl+Shift+F9").unwrap()
        );
    }

    #[test]
    fn refuse_un_raccourci_invalide() {
        let err = parse_hotkey("Ctrl+Truc").unwrap_err();
        assert!(err.contains("Ctrl+Truc"), "{err}");
        assert!(parse_hotkey("").is_err());
    }

    #[test]
    fn refuse_une_touche_seule() {
        let err = parse_hotkey("F9").unwrap_err();
        assert!(err.contains("Ctrl"), "{err}");
    }

    #[test]
    fn centre_dans_la_zone_de_travail() {
        // Écran 1920×1080 avec la barre des tâches en bas (40 px).
        let pos = centered(
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1040),
            PhysicalSize::new(760, 500),
        );
        assert_eq!(pos, PhysicalPosition::new(580, 270));
    }

    #[test]
    fn centre_sur_un_second_ecran() {
        // Second écran à gauche de l'écran principal : x négatif.
        let pos = centered(
            PhysicalPosition::new(-2560, 0),
            PhysicalSize::new(2560, 1400),
            PhysicalSize::new(1140, 750),
        );
        assert_eq!(pos, PhysicalPosition::new(-1850, 325));
    }

    #[test]
    fn fenetre_plus_grande_que_l_ecran() {
        let pos = centered(
            PhysicalPosition::new(100, 50),
            PhysicalSize::new(600, 400),
            PhysicalSize::new(760, 500),
        );
        assert_eq!(pos, PhysicalPosition::new(100, 50));
    }
}
