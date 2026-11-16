//! Les récents, stockés dans `state.json` (à côté de `config.json`).
//!
//! Ce fichier n'appartient qu'à l'app : il change à chaque lancement, et le
//! garder à part évite de réécrire votre `config.json` à tout bout de champ.
//!
//! ```json
//! { "recents": [ { "id": "bg3", "at": "2026-11-16T13:40:00Z" } ] }
//! ```
//!
//! Le plus récent est en premier. On en garde jusqu'à [`MAX_STORED`] : si vous
//! augmentez `recentsCount` dans la config, l'historique est déjà là.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::AppError;

pub const FILE_NAME: &str = "state.json";

/// Nombre maximum de récents gardés dans le fichier.
const MAX_STORED: usize = 50;

/// Contenu de `state.json`. `default` : un champ absent prend sa valeur par
/// défaut (liste vide), ce qui permettra d'ajouter des champs plus tard.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub recents: Vec<Recent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    /// Id de la tuile lancée.
    pub id: String,
    /// Date du lancement, au format ISO 8601 en UTC (`2026-11-16T13:40:00Z`).
    pub at: String,
}

/// Verrou qui empêche deux lancements simultanés de lire puis réécrire
/// `state.json` en même temps (le second effacerait l'ajout du premier).
///
/// `static` : une seule valeur pour tout le programme. `Mutex<()>` ne protège
/// aucune donnée ici, il sert juste de « tour de parole » : tant qu'un appel
/// tient le verrou (`lock()`), les autres attendent.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Lit `state.json`. S'il n'existe pas ou s'il est abîmé, on repart d'un état
/// vide plutôt que de bloquer l'app : ce ne sont que des récents.
pub fn load(dir: &Path) -> State {
    let path = dir.join(FILE_NAME);
    match fs::read_to_string(&path) {
        // `unwrap_or_else` : si la lecture du JSON échoue, on exécute la closure
        // (message dans la console) et on prend un `State` vide à la place.
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{} ignoré (JSON invalide) : {e}", path.display());
            State::default()
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => State::default(),
        Err(e) => {
            eprintln!("{} illisible : {e}", path.display());
            State::default()
        }
    }
}

/// Ids des récents à afficher : seulement ceux qui existent encore dans la
/// config, au plus `recentsCount`.
pub fn visible_recents(state: &State, config: &Config) -> Vec<String> {
    // Un *itérateur* enchaîne des étapes comme `.filter().map()` en JS, mais
    // rien n'est calculé avant le `collect()` final, qui construit le `Vec`.
    state
        .recents
        .iter()
        .filter(|r| config.tiles.iter().any(|t| t.id == r.id))
        .take(config.settings.recents_count as usize)
        .map(|r| r.id.clone())
        .collect()
}

/// Place `id` en tête des récents (en retirant son ancienne position).
///
/// `&mut State` : on emprunte l'état *en écriture* ; l'appelant voit le
/// changement, sans copie.
pub fn push_recent(state: &mut State, id: &str, at: String) {
    state.recents.retain(|r| r.id != id);
    state.recents.insert(
        0,
        Recent {
            id: id.to_string(),
            at,
        },
    );
    state.recents.truncate(MAX_STORED);
}

/// Enregistre un lancement dans `state.json` et renvoie le nouvel état.
pub fn record_launch(dir: &Path, id: &str) -> Result<State, AppError> {
    // `lock()` renvoie une erreur si un autre thread a paniqué en tenant le
    // verrou. Ce n'est pas grave ici : on récupère le verrou quand même.
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // `_guard` relâche le verrou tout seul à la fin de la fonction, quand la
    // variable disparaît (c'est le principe « RAII » de Rust : pas de `finally`).

    let mut state = load(dir);
    push_recent(&mut state, id, now_iso());
    save(dir, &state)?;
    Ok(state)
}

/// Écrit `state.json` de façon *atomique* : on écrit d'abord un fichier
/// temporaire, puis on le renomme. Si l'app est coupée en plein milieu,
/// l'ancien fichier reste intact au lieu d'être à moitié écrit.
fn save(dir: &Path, state: &State) -> Result<(), AppError> {
    let path = dir.join(FILE_NAME);
    let tmp = dir.join("state.json.tmp");
    let io_err = |path: &Path| {
        let path = path.to_path_buf();
        move |source| AppError::Io {
            action: "d'écrire",
            path,
            source,
        }
    };

    fs::create_dir_all(dir).map_err(io_err(dir))?;
    // `to_string_pretty` ne peut pas échouer sur ces structs simples ;
    // `expect` documente cette certitude (et arrêterait l'app sinon).
    let json = serde_json::to_string_pretty(state).expect("State est toujours sérialisable");
    fs::write(&tmp, json).map_err(io_err(&tmp))?;
    // Sous Windows, `rename` remplace le fichier de destination s'il existe.
    fs::rename(&tmp, &path).map_err(io_err(&path))?;
    Ok(())
}

/// Date et heure actuelles en UTC, au format `2026-11-16T13:40:00Z`.
fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_utc(secs)
}

/// Convertit un nombre de secondes depuis le 1ᵉʳ janvier 1970 (UTC) en date
/// ISO 8601. Algorithme « days to civil » de Howard Hinnant, pour éviter
/// d'ajouter une bibliothèque de dates juste pour cette ligne.
fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tile_ids: &[&str], recents_count: u32) -> Config {
        let tiles: Vec<String> = tile_ids
            .iter()
            .map(|id| {
                format!(r#"{{ "id": "{id}", "name": "{id}", "category": "c", "type": "web", "url": "https://x" }}"#)
            })
            .collect();
        let json = format!(
            r#"{{ "version": 1, "settings": {{ "recentsCount": {recents_count} }},
                 "categories": [{{ "id": "c", "name": "C" }}], "tiles": [{}] }}"#,
            tiles.join(",")
        );
        crate::config::parse(&json, Path::new("config.json")).unwrap()
    }

    fn ids(state: &State) -> Vec<&str> {
        state.recents.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn un_lancement_passe_en_tete_sans_doublon() {
        let mut state = State::default();
        push_recent(&mut state, "a", "t1".into());
        push_recent(&mut state, "b", "t2".into());
        push_recent(&mut state, "a", "t3".into());
        assert_eq!(ids(&state), ["a", "b"]);
        assert_eq!(state.recents[0].at, "t3");
    }

    #[test]
    fn l_historique_est_borne() {
        let mut state = State::default();
        for i in 0..MAX_STORED + 10 {
            push_recent(&mut state, &format!("t{i}"), String::new());
        }
        assert_eq!(state.recents.len(), MAX_STORED);
    }

    #[test]
    fn visible_ignore_les_tuiles_supprimees_et_respecte_recents_count() {
        let mut state = State::default();
        for id in ["d", "supprimee", "c", "b", "a"] {
            push_recent(&mut state, id, String::new());
        }
        // Ordre actuel : a, b, c, supprimee, d
        assert_eq!(
            visible_recents(&state, &config(&["a", "b", "c", "d"], 3)),
            ["a", "b", "c"]
        );
        assert_eq!(
            visible_recents(&state, &config(&["a", "b", "c", "d"], 10)),
            ["a", "b", "c", "d"]
        );
        assert!(visible_recents(&state, &config(&["a"], 0)).is_empty());
    }

    #[test]
    fn enregistre_et_relit_state_json() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), State::default());
        record_launch(dir.path(), "a").unwrap();
        let state = record_launch(dir.path(), "b").unwrap();
        assert_eq!(ids(&state), ["b", "a"]);
        assert_eq!(load(dir.path()), state);
        assert!(!dir.path().join("state.json.tmp").exists());
    }

    #[test]
    fn un_state_json_abime_repart_de_zero() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{ pas du json").unwrap();
        assert_eq!(load(dir.path()), State::default());
        let state = record_launch(dir.path(), "a").unwrap();
        assert_eq!(ids(&state), ["a"]);
    }

    #[test]
    fn format_de_date() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_794_836_400), "2026-11-16T13:40:00Z");
    }
}
