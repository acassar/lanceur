//! La touche Windows **seule** comme raccourci (réglage `"hotkey": "Super"`),
//! à la place du menu Démarrer.
//!
//! Windows ne sait pas réserver une touche de modification seule
//! (`RegisterHotKey` exige une « vraie » touche : `Win+Espace`, pas `Win`).
//! On installe donc un *hook clavier bas niveau* (`WH_KEYBOARD_LL`) : Windows
//! nous montre chaque touche, de n'importe quelle application, avant de la
//! transmettre. On ne bloque rien, sauf dans un cas précis :
//!
//! 1. Win est enfoncée : on note laquelle (gauche ou droite).
//! 2. Une autre touche est enfoncée pendant ce temps (`Win+E`, `Win+Maj+S`…) :
//!    c'est un raccourci Windows, on le laisse entièrement passer.
//! 3. Win est relâchée sans qu'aucune autre touche n'ait été enfoncée : c'est
//!    là que Windows ouvrirait le menu Démarrer. On *retient* ce relâchement,
//!    et on envoie à la place trois touches simulées : une touche « neutre »
//!    (code 0xE8, qui n'existe sur aucun clavier), puis le relâchement de Win.
//!    Windows voit alors « Win + une autre touche » et n'ouvre pas le menu
//!    (c'est l'astuce qu'utilise AutoHotkey). Puis on affiche ou masque le
//!    lanceur.
//!
//! Le menu Démarrer reste accessible : `Ctrl+Échap`, un clic sur le bouton
//! Démarrer, ou en remettant un raccourci classique dans les paramètres.
//!
//! Limite connue : quand la fenêtre au premier plan tourne en administrateur
//! (Gestionnaire des tâches…), Windows ne nous montre pas ses touches : la
//! touche Windows y ouvre le menu Démarrer comme d'habitude.
//!
//! Notions Rust abordées : un thread dédié avec sa boucle de messages, des
//! variables globales (`static`) partagées entre threads, et une fonction
//! `extern "system"` appelée directement par Windows.

use tauri::AppHandle;

/// Installe le hook (sans effet s'il est déjà en place).
pub fn start(app: &AppHandle) -> Result<(), String> {
    imp::start(app)
}

/// Retire le hook (sans effet s'il n'est pas en place).
pub fn stop() {
    imp::stop()
}

/// Ce que le hook doit faire d'une touche : la logique pure, sans Windows,
/// testable sous Linux. Les codes de touches sont ceux de Windows (`VK_…`).
// Hors Windows, seuls les tests s'en servent : pas d'avertissement « inutilisé ».
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Default)]
struct Tracker {
    /// Touche Windows enfoncée en premier (`VK_LWIN` ou `VK_RWIN`), 0 sinon.
    win: u32,
    /// Une autre touche a été enfoncée depuis : ce n'est pas « Win seule ».
    combined: bool,
    /// Dernière touche vue, et si c'était un appui : sert à reconnaître les
    /// répétitions automatiques d'une touche maintenue.
    last: (u32, bool),
}

/// Décision du hook pour une touche.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Laisser passer la touche normalement.
    Pass,
    /// Win relâchée seule : retenir ce relâchement, masquer le menu Démarrer
    /// et afficher/masquer le lanceur.
    Toggle,
}

#[cfg_attr(not(windows), allow(dead_code))]
const VK_LWIN: u32 = 0x5B;
#[cfg_attr(not(windows), allow(dead_code))]
const VK_RWIN: u32 = 0x5C;

#[cfg_attr(not(windows), allow(dead_code))]
impl Tracker {
    /// `vk` : code de la touche ; `down` : enfoncée (sinon relâchée) ;
    /// `modifiers_held` : dit si Ctrl, Maj ou Alt étaient déjà enfoncées.
    /// C'est une fonction, appelée seulement au premier appui sur Win : le
    /// hook voit passer toutes les touches du PC, on ne demande l'état des
    /// autres touches à Windows que quand c'est utile.
    fn on_key(&mut self, vk: u32, down: bool, modifiers_held: impl FnOnce() -> bool) -> Action {
        let is_win = vk == VK_LWIN || vk == VK_RWIN;
        // Une touche maintenue envoie des appuis à répétition, sans rien entre
        // eux. Un appui précédé d'autre chose est donc un *nouvel* appui.
        let repeat = down && self.last == (vk, true);
        self.last = (vk, down);
        match (is_win, down) {
            (true, true) => {
                if repeat {
                    // Win maintenue : rien de nouveau.
                } else if self.win == 0 || vk == self.win {
                    // Nouvel appui. (`vk == self.win` : le relâchement
                    // précédent ne nous est jamais parvenu, par exemple après
                    // Win+L, l'écran de verrouillage l'a gardé : on repart de zéro.)
                    self.win = vk;
                    // `Ctrl` déjà enfoncée puis Win : c'est une combinaison.
                    self.combined = modifiers_held();
                } else {
                    // Les deux touches Windows en même temps.
                    self.combined = true;
                }
                Action::Pass
            }
            (true, false) => {
                if vk != self.win {
                    return Action::Pass;
                }
                self.win = 0;
                if self.combined {
                    Action::Pass
                } else {
                    Action::Toggle
                }
            }
            (false, true) => {
                if self.win != 0 {
                    self.combined = true;
                }
                Action::Pass
            }
            (false, false) => Action::Pass,
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};
    use std::thread::JoinHandle;

    use tauri::AppHandle;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_MENU,
        VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW,
        PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
        WM_QUIT, WM_SYSKEYDOWN,
    };

    use super::{Action, Tracker};

    /// L'app, pour afficher la fenêtre depuis le hook. `OnceLock` : une
    /// variable globale qu'on remplit une seule fois, ensuite en lecture seule.
    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// Identifiant du thread du hook, s'il tourne (pour lui dire de s'arrêter),
    /// et de quoi attendre qu'il soit fini (`JoinHandle`).
    static THREAD: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);
    /// État des touches. Le hook n'est appelé que depuis son propre thread,
    /// le verrou n'est donc jamais disputé ; il rassure seulement le compilateur.
    static TRACKER: Mutex<Tracker> = Mutex::new(Tracker {
        win: 0,
        combined: false,
        last: (0, false),
    });

    /// Code de la touche « neutre » : non attribué, aucun clavier ne l'envoie.
    const MASK_KEY: u16 = 0xE8;
    /// Marque nos touches simulées (`dwExtraInfo`), pour que le hook les ignore.
    const OURS: usize = 0x4C41_4E43; // « LANC »

    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn start(app: &AppHandle) -> Result<(), String> {
        let _ = APP.set(app.clone());
        let mut thread = lock(&THREAD);
        if thread.is_some() {
            return Ok(());
        }
        // Le hook appartient au thread qui l'installe, et ce thread doit
        // ensuite attendre des messages en boucle : Windows lui fait passer les
        // touches par là. On crée donc un thread dédié, qui nous répond par
        // un *canal* (`mpsc::channel`) si l'installation a réussi.
        let (tx, rx) = mpsc::channel::<Result<u32, String>>();
        let handle = std::thread::Builder::new()
            .name("touche-windows".into())
            .spawn(move || run(tx))
            .map_err(|e| e.to_string())?;
        let id = rx
            .recv()
            .map_err(|_| "le thread du hook s'est arrêté".to_string())??;
        *thread = Some((id, handle));
        // Repartir de zéro (une touche restée « enfoncée » d'un essai précédent).
        *lock(&TRACKER) = Tracker::default();
        Ok(())
    }

    pub fn stop() {
        // On garde le verrou jusqu'au bout : un `start()` qui suivrait de près
        // attend ainsi que l'ancien hook soit retiré (jamais deux à la fois).
        let mut thread = lock(&THREAD);
        if let Some((id, handle)) = thread.take() {
            // `WM_QUIT` fait sortir la boucle de messages : le thread retire
            // alors le hook et s'arrête. `join` attend cette fin (immédiate).
            if unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
                let _ = handle.join();
            }
        }
    }

    /// Le thread du hook : installation, puis boucle de messages.
    fn run(tx: mpsc::Sender<Result<u32, String>>) {
        unsafe {
            let mut msg = MSG::default();
            // Crée la file de messages du thread *avant* de donner son
            // identifiant : sinon un `stop()` immédiat serait perdu.
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
            let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0) {
                Ok(hook) => hook,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };
            let _ = tx.send(Ok(GetCurrentThreadId()));
            // `GetMessageW` renvoie 0 pour `WM_QUIT`, -1 en cas d'erreur.
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
            let _ = UnhookWindowsHookEx(hook);
        }
    }

    /// Appelée par Windows à chaque touche, dans le thread du hook.
    ///
    /// Elle doit répondre très vite : au-delà d'un délai (quelques centaines
    /// de ms), Windows passe outre et finit par retirer le hook. On n'y fait
    /// donc qu'un calcul, et l'affichage de la fenêtre est confié au thread
    /// principal (`run_on_main_thread`), sans l'attendre.
    ///
    /// `extern "system"` : la convention d'appel de l'API Windows.
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // `lparam` pointe vers la description de la touche (fournie par Windows).
            let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if key.dwExtraInfo != OURS {
                let message = wparam.0 as u32;
                let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
                let modifiers = || {
                    [VK_CONTROL, VK_SHIFT, VK_MENU]
                        .iter()
                        // Bit de poids fort : touche actuellement enfoncée.
                        .any(|&vk| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0)
                };
                let action = lock(&TRACKER).on_key(key.vkCode, down, modifiers);
                if action == Action::Toggle {
                    hide_start_menu(key.vkCode as u16);
                    toggle_launcher();
                    // Valeur non nulle : Windows oublie ce relâchement de Win
                    // (on vient d'en envoyer un, précédé de la touche neutre).
                    return LRESULT(1);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// Touche neutre (appui + relâchement) puis relâchement de Win : pour
    /// Windows, Win a servi à une combinaison, pas d'ouverture du menu Démarrer.
    fn hide_start_menu(win: u16) {
        let key = |vk: u16, flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: OURS,
                },
            },
        };
        let inputs = [
            key(MASK_KEY, KEYBD_EVENT_FLAGS(0)),
            key(MASK_KEY, KEYEVENTF_KEYUP),
            // Les touches Windows sont des touches « étendues ».
            key(win, KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY),
        ];
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    }

    fn toggle_launcher() {
        if let Some(app) = APP.get() {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || crate::window::toggle(&handle));
        }
    }
}

/// Hors Windows (développement sous Linux) : pas de touche Windows.
#[cfg(not(windows))]
mod imp {
    use tauri::AppHandle;

    pub fn start(_app: &AppHandle) -> Result<(), String> {
        Err("uniquement sous Windows".into())
    }

    pub fn stop() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Suite de touches : `(code, enfoncée)`. Renvoie les décisions.
    fn run(keys: &[(u32, bool)]) -> Vec<Action> {
        let mut t = Tracker::default();
        keys.iter()
            .map(|&(vk, down)| t.on_key(vk, down, || false))
            .collect()
    }

    const E: u32 = 0x45;

    #[test]
    fn win_seule_declenche_au_relachement() {
        assert_eq!(
            run(&[(VK_LWIN, true), (VK_LWIN, true), (VK_LWIN, false)]),
            [Action::Pass, Action::Pass, Action::Toggle]
        );
        assert_eq!(run(&[(VK_RWIN, true), (VK_RWIN, false)])[1], Action::Toggle);
    }

    #[test]
    fn les_raccourcis_windows_passent() {
        // Win+E : rien de spécial, même au relâchement de Win.
        assert_eq!(
            run(&[(VK_LWIN, true), (E, true), (E, false), (VK_LWIN, false)]),
            [Action::Pass; 4]
        );
        // Les deux touches Windows ensemble.
        assert_eq!(
            run(&[
                (VK_LWIN, true),
                (VK_RWIN, true),
                (VK_RWIN, false),
                (VK_LWIN, false)
            ]),
            [Action::Pass; 4]
        );
        // Ctrl déjà enfoncée avant Win.
        let mut t = Tracker::default();
        assert_eq!(t.on_key(VK_LWIN, true, || true), Action::Pass);
        assert_eq!(t.on_key(VK_LWIN, false, || true), Action::Pass);
    }

    #[test]
    fn repart_de_zero_apres_chaque_relachement() {
        assert_eq!(
            run(&[
                (VK_LWIN, true),
                (E, true),
                (VK_LWIN, false),
                (E, false),
                (VK_LWIN, true),
                (VK_LWIN, false)
            ]),
            [
                Action::Pass,
                Action::Pass,
                Action::Pass,
                Action::Pass,
                Action::Pass,
                Action::Toggle
            ]
        );
        // Relâchement de Win jamais reçu (Win+L : l'écran de verrouillage
        // l'a gardé) : le prochain appui repart de zéro.
        assert_eq!(
            run(&[
                (VK_LWIN, true),
                (0x4C, true),
                (0x4C, false),
                (VK_LWIN, true),
                (VK_LWIN, false)
            ])[4],
            Action::Toggle
        );
        // Une touche relâchée seule (enfoncée avant Win) ne compte pas.
        assert_eq!(
            run(&[(E, true), (VK_LWIN, true), (E, false), (VK_LWIN, false)])[3],
            Action::Toggle
        );
    }
}
