//! Écran de paramètres : modifier la section `"settings"` de `config.json`
//! depuis la fenêtre.
//!
//! Même principe que l'éditeur de tuiles (editor.rs) : on ne réécrit pas tout
//! le fichier, seulement la *valeur* de chaque réglage modifié
//! (`jsontext::set_object_field`). Mise en forme et ordre des champs restent
//! les vôtres.
//!
//! Le front n'envoie que les réglages qu'il a changés (un « patch »), pas tous :
//! si vous avez modifié un autre réglage à la main pendant que l'écran était
//! ouvert, il n'est pas écrasé par l'ancienne valeur affichée.
//!
//! Notions Rust abordées : `serde_json::Value` (du JSON « libre », comme un
//! objet JS quelconque) et la conversion struct ⇄ JSON avec `to_value` /
//! `from_value`.

use std::path::Path;

use serde_json::{Map, Value};

use crate::config::{self, Settings};
use crate::error::AppError;
use crate::fsutil;
use crate::jsontext;
use crate::window;

/// Applique `changes` (`{ "hotkey": "Super", "hideOnBlur": false }`) à la
/// section `"settings"` de config.json. Renvoie les réglages enregistrés.
///
/// Refusé, sans rien écrire : un réglage inconnu, une valeur du mauvais type,
/// ou un raccourci illisible. (Un raccourci *valide* mais déjà pris par une
/// autre app est enregistré : l'erreur s'affiche ensuite, comme à la main.)
pub fn update(dir: &Path, changes: &Map<String, Value>) -> Result<Settings, AppError> {
    let _guard = config::write_lock();
    let path = dir.join(config::FILE_NAME);
    let text = config::read_or_create_text(dir)?;
    let current = config::parse(&text, &path)?;

    let wanted = merge(&current.settings, changes)?;
    check(&wanted)?;

    // Réglages qui changent vraiment, sous forme JSON : `"hotkey"` → `"\"Super\""`.
    let before = to_object(&current.settings)?;
    let after = to_object(&wanted)?;
    let mut new_text = text.clone();
    for (key, value) in &after {
        if before.get(key) == Some(value) {
            continue;
        }
        let json =
            serde_json::to_string(value).map_err(|e| AppError::ConfigWrite(e.to_string()))?;
        new_text =
            jsontext::set_object_field(&new_text, "settings", key, &json).ok_or_else(|| {
                AppError::ConfigWrite("la section \"settings\" n'a pas pu être modifiée".into())
            })?;
    }
    if new_text == text {
        return Ok(wanted);
    }

    // Filet de sécurité : relu, le fichier doit donner exactement la config
    // d'avant, réglages mis à part.
    let mut expected = current;
    expected.settings = wanted.clone();
    let check = config::parse(&new_text, &path)
        .map_err(|e| AppError::ConfigWrite(format!("modification refusée ({e})")))?;
    if check != expected {
        return Err(AppError::ConfigWrite(
            "le fichier modifié ne correspond pas à la modification prévue".into(),
        ));
    }
    fsutil::write_atomic(&path, new_text.as_bytes())?;
    Ok(wanted)
}

/// Les réglages actuels, avec `changes` par-dessus.
///
/// On passe par du JSON : `to_value` transforme la struct en objet JSON, on
/// remplace les champs, puis `from_value` refait une struct. C'est serde qui
/// vérifie les noms (`deny_unknown_fields`) et les types : `"hideOnBlur": "oui"`
/// est refusé exactement comme dans le fichier.
fn merge(current: &Settings, changes: &Map<String, Value>) -> Result<Settings, AppError> {
    let mut object = to_object(current)?;
    for (key, value) in changes {
        if !object.contains_key(key) {
            return Err(invalid(format!("réglage inconnu « {key} »")));
        }
        object.insert(key.clone(), value.clone());
    }
    serde_json::from_value(Value::Object(object)).map_err(|e| invalid(e.to_string()))
}

/// Les vérifications que serde ne fait pas.
fn check(settings: &Settings) -> Result<(), AppError> {
    window::parse_trigger(&settings.hotkey).map_err(invalid)?;
    if settings.browser.trim().is_empty() {
        return Err(invalid("choisissez un navigateur pour les apps web".into()));
    }
    if settings.accent.trim().is_empty() {
        return Err(invalid("la couleur d'accent est vide".into()));
    }
    Ok(())
}

fn to_object(settings: &Settings) -> Result<Map<String, Value>, AppError> {
    match serde_json::to_value(settings) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(AppError::ConfigWrite("réglages illisibles".into())),
    }
}

fn invalid(message: String) -> AppError {
    AppError::ConfigWrite(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn changes(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => unreachable!(),
        }
    }

    fn setup(text: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(config::FILE_NAME), text).unwrap();
        dir
    }

    fn read(dir: &tempfile::TempDir) -> String {
        std::fs::read_to_string(dir.path().join(config::FILE_NAME)).unwrap()
    }

    const TEXT: &str = "{\n  \"version\": 1,\n  \"settings\": {\n    \"hotkey\": \"Super+Alt+Space\",\n    \"hideOnBlur\": true\n  },\n  \"tiles\": []\n}\n";

    #[test]
    fn ne_reecrit_que_les_valeurs_modifiees() {
        let dir = setup(TEXT);
        let saved = update(
            dir.path(),
            &changes(json!({ "hotkey": "Super", "hideOnBlur": true, "hideAfterLaunch": false })),
        )
        .unwrap();
        assert_eq!(saved.hotkey, "Super");
        assert!(!saved.hide_after_launch);
        assert_eq!(
            read(&dir),
            "{\n  \"version\": 1,\n  \"settings\": {\n    \"hotkey\": \"Super\",\n    \"hideOnBlur\": true,\n    \"hideAfterLaunch\": false\n  },\n  \"tiles\": []\n}\n"
        );
    }

    #[test]
    fn rien_a_changer_rien_d_ecrit() {
        let dir = setup(TEXT);
        update(dir.path(), &changes(json!({ "hideOnBlur": true }))).unwrap();
        assert_eq!(read(&dir), TEXT);
    }

    #[test]
    fn refuse_un_reglage_inconnu_ou_mal_type() {
        let dir = setup(TEXT);
        assert!(update(dir.path(), &changes(json!({ "theme": "clair" }))).is_err());
        assert!(update(dir.path(), &changes(json!({ "hideOnBlur": "oui" }))).is_err());
        assert!(update(dir.path(), &changes(json!({ "recentsCount": -1 }))).is_err());
        assert!(update(dir.path(), &changes(json!({ "hotkey": "Ctrl+Truc" }))).is_err());
        assert!(update(dir.path(), &changes(json!({ "hotkey": "F9" }))).is_err());
        assert!(update(dir.path(), &changes(json!({ "browser": "  " }))).is_err());
        assert_eq!(read(&dir), TEXT);
    }

    #[test]
    fn un_fichier_invalide_n_est_pas_touche() {
        let broken = "{ \"version\": 1, \"settings\": { \"hotkey\": 3 } }";
        let dir = setup(broken);
        assert!(update(dir.path(), &changes(json!({ "hideOnBlur": false }))).is_err());
        assert_eq!(read(&dir), broken);
    }
}
