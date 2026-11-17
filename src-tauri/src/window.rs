//! La fenêtre : afficher, masquer, centrer, et le raccourci clavier global.
//!
//! « Masquer » veut dire ici **réduire** (décision 1A du plan) : l'icône reste
//! dans la barre des tâches, et un clic dessus restaure la fenêtre.
//!
//! Trois façons de faire réapparaître la fenêtre, qui passent toutes par [`show`] :
//! - le raccourci global (`Win+Alt+Espace` par défaut, réglage `hotkey`) ;
//! - relancer l'exe (plugin *single-instance*, branché dans `lib.rs`) ;
//! - cliquer sur l'icône de la barre des tâches (géré par Windows lui-même) ;
//! - cliquer sur l'icône de la zone de notification (tray.rs, [`tray_released`]).
//!
//! Au démarrage de Windows (`--autostart`, voir autostart.rs), la fenêtre n'est
//! pas affichée du tout : `"visible": false` dans `tauri.conf.json`, et `setup`
//! ne l'affiche que pour un lancement normal.
//!
//! Notions Rust abordées : les plugins, l'état partagé `State<…>` avec un
//! `Mutex` et un `AtomicBool`, et les closures passées aux plugins.

use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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

    /// Dernière perte de focus de la fenêtre (voir [`tray_released`]).
    last_blur: Mutex<Option<Instant>>,

    /// Clics sur l'icône de notification (voir [`tray_pressed`]).
    tray: Mutex<TrayClick>,

    /// Nombre de boîtes de dialogue Windows (« Ouvrir ») ouvertes au-dessus de
    /// la fenêtre : la perte de focus est alors normale, on ne réduit pas.
    /// Un compteur plutôt qu'un booléen : si deux boîtes se chevauchent, la
    /// fermeture de la première ne réactive pas `hideOnBlur` trop tôt.
    native_dialogs: AtomicUsize,

    /// La fenêtre a été réduite (taille 0×0) depuis le dernier
    /// redimensionnement : le prochain sera une restauration.
    was_minimized: AtomicBool,
}

/// Signale l'ouverture (`true`) ou la fermeture (`false`) d'une boîte de
/// dialogue Windows, pendant laquelle `hideOnBlur` est suspendu.
pub fn set_native_dialog_open(app: &AppHandle, open: bool) {
    let count = &app.state::<WindowState>().native_dialogs;
    if open {
        count.fetch_add(1, Ordering::Relaxed);
    } else {
        // `fetch_update` : décrémente sans jamais passer sous zéro.
        let _ = count.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1));
    }
}

#[derive(Default)]
struct TrayClick {
    /// Moment où le bouton a été enfoncé, et si la fenêtre était alors devant.
    pressed: Option<(Instant, bool)>,
    /// Dernier clic pris en compte (pour ignorer le 2ᵉ clic d'un double-clic).
    last_toggle: Option<Instant>,
}

#[derive(Default)]
struct Hotkey {
    /// Texte du raccourci actif, tel qu'écrit dans config.json (`"Super+Alt+Space"`).
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
/// Le front relit la config dès que `config.json` est enregistré (watch.rs) et
/// chaque fois que la fenêtre réapparaît : si vous changez `hotkey` ou
/// `hideOnBlur` à la main, ils s'appliquent aussitôt, sans redémarrer l'app.
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

/// Lit un raccourci écrit comme `"Super+Alt+Space"`. Les noms de touches sont
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
    if is_in_front(&window) {
        hide(app)
    } else {
        show(app)
    }
}

/// La fenêtre est-elle affichée, non réduite, et au premier plan ?
///
/// Sous Windows, on ne se fie pas à `is_focused()` : le clavier n'est pas tenu
/// par notre fenêtre elle-même mais par la WebView2 qu'elle contient (une
/// fenêtre « enfant »), et `is_focused()` répond alors `false` alors que la
/// fenêtre est bien devant. On demande donc directement à Windows quelle est
/// la fenêtre au premier plan (`GetForegroundWindow`) et on compare.
fn is_in_front(window: &WebviewWindow) -> bool {
    if !window.is_visible().unwrap_or(false) || window.is_minimized().unwrap_or(true) {
        return false;
    }
    // `#[cfg(windows)]` : ce bloc n'est compilé que pour Windows. Sur les autres
    // systèmes (dev sous Linux), c'est le bloc `#[cfg(not(windows))]` qui compte.
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        // `unsafe` : appel direct à l'API Win32, que le compilateur Rust ne peut
        // pas vérifier. Cette fonction-là ne fait que lire, sans risque.
        let foreground = unsafe { GetForegroundWindow() };
        // `is_ok_and(…)` : vrai seulement si `hwnd()` réussit *et* que la
        // condition est remplie ; si `hwnd()` échoue, on répond « pas devant ».
        use windows::Win32::UI::WindowsAndMessaging::{GA_ROOTOWNER, GetAncestor};
        if foreground.0.is_null() {
            return false;
        }
        // Une fenêtre *possédée* par la nôtre (liste déroulante d'un `<select>`,
        // boîte « Ouvrir »…) peut passer au premier plan : on remonte jusqu'à
        // sa propriétaire, qui doit être notre fenêtre.
        let root = unsafe { GetAncestor(foreground, GA_ROOTOWNER) };
        window
            .hwnd()
            .is_ok_and(|hwnd| hwnd.0 == foreground.0 || hwnd.0 == root.0)
    }
    #[cfg(not(windows))]
    {
        window.is_focused().unwrap_or(false)
    }
}

/// Une perte de focus survenue jusqu'à ce délai *avant* l'appui sur l'icône
/// de notification compte encore comme « la fenêtre était devant ».
const TRAY_BLUR_GRACE: Duration = Duration::from_millis(600);

/// Un 2ᵉ clic sur l'icône de notification aussi rapproché est ignoré
/// (double-clic : Windows envoie deux clics simples).
const TRAY_DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// Bouton gauche enfoncé sur l'icône de notification : on note si la fenêtre
/// est devant *à cet instant*, avant que le clic ne change le premier plan.
pub fn tray_pressed(app: &AppHandle) {
    let in_front = main_window(app).is_some_and(|w| is_in_front(&w));
    lock(&app.state::<WindowState>().tray).pressed = Some((Instant::now(), in_front));
}

/// Bouton gauche relâché sur l'icône : masque la fenêtre si elle était devant
/// au moment du clic, sinon l'affiche.
///
/// Pourquoi ne pas réutiliser [`toggle`] ? Appuyer sur l'icône donne le
/// premier plan à la barre des tâches : au relâchement, notre fenêtre n'est
/// *déjà plus* devant (avec `hideOnBlur`, elle a même pu être réduite si le
/// bouton est resté enfoncé plus de 200 ms), et `toggle` la réafficherait.
/// On considère donc qu'elle « était devant » si :
/// - elle l'était à l'appui ([`tray_pressed`]) ;
/// - ou elle a perdu le focus peu avant l'appui, ou pendant : la perte de focus
///   signalée par la WebView2 arrive en différé, parfois après l'appui.
pub fn tray_released(app: &AppHandle) {
    let state = app.state::<WindowState>();
    let was_in_front = {
        let mut tray = lock(&state.tray);
        // `take()` vide `pressed` et rend son ancienne valeur.
        let pressed = tray.pressed.take();
        // `is_some_and` : vrai seulement s'il y a une date *et* qu'elle est récente.
        if tray
            .last_toggle
            .is_some_and(|t| t.elapsed() < TRAY_DOUBLE_CLICK)
        {
            return;
        }
        tray.last_toggle = Some(Instant::now());
        // Pas d'appui noté (ne devrait pas arriver) : on prend l'instant présent.
        let (pressed_at, in_front) = pressed.unwrap_or((Instant::now(), false));
        let since = pressed_at
            .checked_sub(TRAY_BLUR_GRACE)
            .unwrap_or(pressed_at);
        in_front || lock(&state.last_blur).is_some_and(|t| t >= since)
    };
    if was_in_front { hide(app) } else { show(app) }
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
    focus_webview(&window);
    let _ = window.emit(SHOWN_EVENT, ());
}

/// Donne le focus clavier à la page (la WebView) en plus de la fenêtre.
///
/// `set_focus()` ne s'occupe que de la fenêtre. Quand elle sort d'une
/// réduction, Windows peut lui rendre le focus *avant* qu'elle soit restaurée :
/// la WebView refuse alors de le prendre, et la frappe n'arrive plus dans la
/// recherche. `as_ref()` donne accès à la partie « webview » de la fenêtre.
fn focus_webview(window: &WebviewWindow) {
    let webview: &tauri::Webview = window.as_ref();
    let _ = webview.set_focus();
}

/// « Masque » la fenêtre : la réduit dans la barre des tâches (décision 1A).
pub fn hide(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        let _ = window.minimize();
    }
}

/// Délai avant de réduire la fenêtre après une perte de focus (`hideOnBlur`).
const BLUR_DELAY: Duration = Duration::from_millis(200);

/// Appelée par Tauri à chaque événement de la fenêtre (branchée dans `lib.rs`).
///
/// - Perte de focus + `hideOnBlur` : on réduit la fenêtre, après vérification.
/// - Retour de la fenêtre (clic sur l'icône de la barre des tâches) : on
///   redonne le focus à la recherche.
/// - Croix de la fenêtre : si l'icône de notification existe, on masque la
///   fenêtre au lieu de quitter (on quitte par le menu de l'icône).
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::CloseRequested { api, .. }
            if crate::tray::exists(window.app_handle()) =>
        {
            // `prevent_close` annule la fermeture demandée par la croix.
            api.prevent_close();
            hide(window.app_handle());
        }
        tauri::WindowEvent::Focused(false) => {
            *lock(&window.state::<WindowState>().last_blur) = Some(Instant::now());
            if window
                .state::<WindowState>()
                .hide_on_blur
                .load(Ordering::Relaxed)
            {
                hide_after_blur(window.app_handle().clone());
            }
        }
        // Une réduction donne une taille de 0×0 : on la note.
        tauri::WindowEvent::Resized(size) if size.width == 0 || size.height == 0 => {
            window
                .state::<WindowState>()
                .was_minimized
                .store(true, Ordering::Relaxed);
        }
        // Une restauration change la taille (de 0×0 à la vraie taille). On ne
        // rend le focus à la page que dans ce cas-là, pas à chaque
        // redimensionnement, pour ne jamais le voler sans raison.
        tauri::WindowEvent::Resized(_) => {
            // `swap(false)` remet le drapeau à faux et rend l'ancienne valeur.
            let restored = window
                .state::<WindowState>()
                .was_minimized
                .swap(false, Ordering::Relaxed);
            if restored
                && let Some(main) = main_window(window.app_handle())
                && is_in_front(&main)
            {
                focus_webview(&main);
            }
        }
        _ => {}
    }
}

/// Réduit la fenêtre si, un instant après une perte de focus, elle n'est
/// toujours pas au premier plan.
///
/// Pourquoi attendre ? Sous Windows, l'événement « focus perdu » vient de la
/// WebView2 et arrive *en différé*. Pendant qu'une fenêtre sort d'une
/// réduction, le focus passe de main en main (barre des tâches, fenêtre, page)
/// et la WebView peut signaler une perte de focus qui n'en est pas une. Avant
/// ce correctif, on réduisait aussitôt : la fenêtre se restaurait puis se
/// re-réduisait dans la foulée, et semblait ne plus jamais réapparaître.
///
/// On attend donc `BLUR_DELAY`, puis on demande à Windows qui est réellement au
/// premier plan : si c'est encore nous, c'était une fausse alerte.
fn hide_after_blur(app: AppHandle) {
    // `std::thread::spawn` lance un nouveau thread, pour attendre sans bloquer
    // le thread principal (qui fait tourner toute l'interface). `move` donne à
    // la closure la propriété de `app` : elle l'emporte avec elle dans le thread.
    std::thread::spawn(move || {
        std::thread::sleep(BLUR_DELAY);
        // Les fenêtres se manipulent depuis le thread principal :
        // `run_on_main_thread` y renvoie la suite du travail.
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(window) = main_window(&handle) else {
                return;
            };
            let enabled = handle
                .state::<WindowState>()
                .hide_on_blur
                .load(Ordering::Relaxed);
            let dialog = handle
                .state::<WindowState>()
                .native_dialogs
                .load(Ordering::Relaxed)
                > 0;
            let minimized = window.is_minimized().unwrap_or(true);
            if !enabled || dialog || minimized {
                return;
            }
            // Fausse alerte si la fenêtre est toujours devant : on ne fait rien.
            //
            // Surtout, on ne rend *pas* le focus à la page : la perte de focus
            // vient souvent d'elle-même (liste déroulante d'un `<select>`, qui
            // est une petite fenêtre à part). Reprendre le focus refermait la
            // liste aussitôt, et le front remettait le clavier dans la
            // recherche en sélectionnant son texte : la fenêtre « scintillait »,
            // les listes étaient inutilisables et la frappe se perdait.
            if !is_in_front(&window) {
                let _ = window.minimize();
            }
        });
    });
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
        let s = parse_hotkey(&crate::config::Settings::default().hotkey).unwrap();
        assert_eq!(s.mods, Modifiers::SUPER | Modifiers::ALT);
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
