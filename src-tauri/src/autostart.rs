//! Lancement au démarrage de Windows (réglage `launchAtStartup`).
//!
//! Windows lance à l'ouverture de session les programmes listés dans la clé de
//! registre `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` :
//! une *valeur* par programme, son nom (`Lanceur`) et la ligne de commande à
//! exécuter. C'est la liste « Applications de démarrage » du Gestionnaire des
//! tâches. `HKEY_CURRENT_USER` : seulement pour votre compte, sans droits admin.
//!
//! La source de vérité est `config.json` : à chaque lecture de la config,
//! [`sync`] crée, met à jour ou supprime la valeur pour qu'elle corresponde au
//! réglage. Si vous désactivez l'app dans le Gestionnaire des tâches, Windows
//! garde la valeur mais la marque « désactivée » ailleurs : on n'y touche pas,
//! votre choix est respecté.
//!
//! La commande enregistrée ajoute l'argument [`ARG`] : au démarrage de Windows,
//! l'app se lance sans afficher sa fenêtre (seule l'icône de notification
//! apparaît), et le raccourci clavier l'ouvre comme d'habitude.

use std::sync::Mutex;

/// Argument ajouté à la commande lancée par Windows à l'ouverture de session.
pub const ARG: &str = "--autostart";

/// L'app a-t-elle été lancée par Windows à l'ouverture de session ?
///
/// `impl IntoIterator<Item = …>` : « n'importe quoi qu'on peut parcourir et
/// qui donne des textes » (les arguments du programme, ou une liste en test).
/// `AsRef<OsStr>` accepte les `String` comme les `OsString` de
/// `std::env::args_os()` ; `args()` (en `String`) planterait si un argument
/// n'était pas de l'Unicode valide.
pub fn launched_at_startup<S: AsRef<std::ffi::OsStr>>(args: impl IntoIterator<Item = S>) -> bool {
    args.into_iter().any(|a| a.as_ref() == ARG)
}

/// Ligne de commande enregistrée dans le registre. Le chemin est entre
/// guillemets : sans eux, un chemin avec des espaces (`C:\Program Files\…`)
/// serait coupé au premier espace.
#[cfg_attr(not(windows), allow(dead_code))]
fn command_line(exe: &std::path::Path) -> String {
    format!("\"{}\" {ARG}", exe.display())
}

/// Aligne le registre sur le réglage `launchAtStartup`. Les erreurs sont
/// seulement écrites dans la console : l'app marche aussi sans.
///
/// La config est relue à chaque affichage de la fenêtre : pour ne pas lire le
/// registre à chaque fois, on retient le dernier réglage appliqué et on ne
/// fait rien s'il n'a pas changé.
pub fn sync(wanted: bool) {
    // En debug (`npm run tauri dev`), on n'écrit rien : on ne veut pas que
    // Windows lance l'exe de développement de `target\debug` à chaque démarrage.
    if cfg!(debug_assertions) {
        return;
    }
    // `Option<bool>` : `None` tant que rien n'a été appliqué depuis le lancement.
    static APPLIED: Mutex<Option<bool>> = Mutex::new(None);
    let mut applied = APPLIED.lock().unwrap_or_else(|p| p.into_inner());
    if *applied == Some(wanted) {
        return;
    }
    match imp::sync(wanted) {
        Ok(()) => *applied = Some(wanted),
        // `applied` reste inchangé : on réessaiera à la prochaine lecture.
        Err(e) => eprintln!("lancement au démarrage non mis à jour : {e}"),
    }
}

/// Partie propre à Windows. `mod imp` existe en deux versions : le compilateur
/// ne garde que celle qui correspond au système visé (`#[cfg(...)]`).
#[cfg(windows)]
mod imp {
    use windows_registry::CURRENT_USER;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Où le Gestionnaire des tâches note les programmes « désactivés ».
    const APPROVED_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    /// Nom de la valeur dans la clé `Run`.
    const VALUE_NAME: &str = "Lanceur";

    pub fn sync(wanted: bool) -> Result<(), String> {
        // `map_err(|e| e.to_string())` : on convertit chaque type d'erreur en
        // simple texte, pour pouvoir utiliser `?` partout.
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // `create` ouvre la clé en lecture/écriture (et la crée si besoin).
        let key = CURRENT_USER.create(RUN_KEY).map_err(|e| e.to_string())?;
        // `.ok()` : valeur absente (ou illisible) → `None`.
        let current = key.get_string(VALUE_NAME).ok();
        let expected = super::command_line(&exe);

        if wanted {
            // On réécrit aussi si l'exe a changé de place (réinstallation).
            if current.as_deref() != Some(expected.as_str()) {
                key.set_string(VALUE_NAME, &expected)
                    .map_err(|e| e.to_string())?;
                // On (ré)active : un ancien « désactivé » du Gestionnaire des
                // tâches empêcherait sinon le lancement. S'il n'existe pas,
                // l'erreur est sans importance (`let _ =`).
                if let Ok(approved) = CURRENT_USER.options().read().write().open(APPROVED_KEY) {
                    let _ = approved.remove_value(VALUE_NAME);
                }
            }
        } else {
            // Même si la lecture a échoué (valeur d'un autre type…) : on
            // supprime. Valeur déjà absente : erreur sans importance.
            let _ = key.remove_value(VALUE_NAME);
        }
        Ok(())
    }
}

/// Hors Windows (développement sous Linux/macOS) : rien à faire.
#[cfg(not(windows))]
mod imp {
    pub fn sync(_wanted: bool) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_l_argument_de_demarrage() {
        assert!(launched_at_startup(["lanceur.exe", "--autostart"]));
        assert!(!launched_at_startup(["lanceur.exe"]));
        assert!(!launched_at_startup(Vec::<String>::new()));
    }

    #[test]
    fn met_le_chemin_entre_guillemets() {
        let exe = std::path::Path::new(r"C:\Program Files\Lanceur\lanceur.exe");
        assert_eq!(
            command_line(exe),
            r#""C:\Program Files\Lanceur\lanceur.exe" --autostart"#
        );
    }
}
