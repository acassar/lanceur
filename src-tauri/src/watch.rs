//! Rechargement automatique : on surveille `config.json` et on prévient le
//! front dès qu'il est enregistré. Il relit alors la config (commande
//! `get_config`), ce qui met aussi à jour le raccourci global et `hideOnBlur`.
//!
//! Résultat : vous modifiez `config.json` dans VS Code, vous enregistrez, et
//! les tuiles suivent aussitôt, sans rouvrir la fenêtre.
//!
//! Notions Rust abordées : un *thread* d'arrière-plan qui appelle notre
//! closure, et une valeur qu'il faut garder en vie (sinon la surveillance
//! s'arrête toute seule).

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use tauri::{AppHandle, Emitter, Manager};

use crate::config;

/// Événement envoyé au front quand `config.json` a changé sur le disque.
pub const CONFIG_CHANGED_EVENT: &str = "config-changed";

/// Délai de regroupement des événements. Un éditeur produit souvent plusieurs
/// événements pour un seul enregistrement (fichier temporaire, renommage,
/// écriture…) : on attend qu'il n'y en ait plus pendant ce délai avant de
/// prévenir le front, une seule fois.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Garde la surveillance en vie.
///
/// Le `Debouncer` arrête la surveillance quand il est *détruit* (en Rust, une
/// valeur est détruite dès que plus rien ne la possède, à la fin du bloc où
/// elle vit). On le confie donc à Tauri avec `manage`, qui le garde jusqu'à la
/// fermeture de l'app. Le `Mutex` est exigé par Tauri : un état partagé doit
/// pouvoir être utilisé depuis plusieurs threads.
#[derive(Default)]
pub struct ConfigWatcher(Mutex<Option<Debouncer<RecommendedWatcher>>>);

/// Démarre la surveillance du dossier de `config.json`.
///
/// On surveille le *dossier* et pas le fichier lui-même : beaucoup d'éditeurs
/// enregistrent en écrivant un fichier temporaire puis en le renommant en
/// `config.json`. Le fichier surveillé serait alors remplacé par un autre, et
/// on ne verrait plus rien. Les autres fichiers du dossier (`state.json`, qui
/// change à chaque lancement) sont ignorés.
///
/// En cas d'échec, on l'écrit dans la console et l'app continue : la config est
/// de toute façon relue chaque fois que la fenêtre réapparaît.
pub fn start(app: &AppHandle, dir: &Path) {
    // `app.clone()` : un `AppHandle` se copie à bas coût (c'est un pointeur
    // partagé). La closure reçoit sa propre copie avec `move`.
    let handle = app.clone();
    let on_events = move |result: DebounceEventResult| match result {
        // `iter().any(…)` : vrai si au moins un événement concerne config.json.
        // Un événement sans nom de fichier (trop de changements d'un coup, la
        // mémoire tampon de Windows a débordé) compte aussi : un rechargement
        // de trop ne coûte rien, une modification manquée si.
        Ok(events)
            if events
                .iter()
                .any(|e| e.path.as_os_str().is_empty() || is_config_file(&e.path)) =>
        {
            // Ce code tourne sur le thread du `Debouncer`, pas sur celui de
            // la fenêtre : `emit` sait envoyer l'événement depuis n'importe où.
            let _ = handle.emit(CONFIG_CHANGED_EVENT, ());
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("surveillance de config.json : {e}");
            let _ = handle.emit(CONFIG_CHANGED_EVENT, ());
        }
    };

    let watcher = new_debouncer(DEBOUNCE, on_events).and_then(|mut debouncer| {
        debouncer
            .watcher()
            .watch(dir, RecursiveMode::NonRecursive)?;
        Ok(debouncer)
    });
    match watcher {
        Ok(debouncer) => {
            let state = app.state::<ConfigWatcher>();
            *state.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(debouncer);
        }
        Err(e) => eprintln!(
            "impossible de surveiller {} : {e} (la config sera relue à chaque affichage)",
            dir.display()
        ),
    }
}

/// `true` si `path` désigne `config.json`. Sous Windows, les noms de fichiers
/// ne tiennent pas compte de la casse : `Config.JSON` est le même fichier.
fn is_config_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case(config::FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ne_reagit_qu_a_config_json() {
        assert!(is_config_file(Path::new("/x/config.json")));
        assert!(is_config_file(Path::new("/x/Config.JSON")));
        assert!(!is_config_file(Path::new("/x/state.json")));
        assert!(!is_config_file(Path::new("/x/config.json.tmp")));
        assert!(!is_config_file(Path::new("/x/icons")));
    }
}
