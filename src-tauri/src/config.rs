//! Lecture de `config.json`.
//!
//! Le fichier vit dans `%APPDATA%\io.github.acassar.lanceur\config.json`.
//! S'il n'existe pas, on le crée à partir de `config.example.json` (embarqué
//! dans l'exe à la compilation). S'il est invalide, on renvoie une erreur
//! précise (ligne, colonne, message) que le front affiche.
//!
//! Les structs ci-dessous sont le miroir exact des types TypeScript de
//! `src/lib/types.ts`. `serde` fait la conversion JSON <-> Rust pour nous.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Nom du fichier de configuration dans le dossier de l'app.
pub const FILE_NAME: &str = "config.json";

/// Config d'exemple écrite au premier démarrage.
///
/// `include_str!` lit le fichier *à la compilation* et colle son contenu dans
/// l'exe sous forme de `&'static str` (une chaîne qui vit aussi longtemps que
/// le programme). Aucun fichier à installer à côté de l'exe.
const EXAMPLE: &str = include_str!("../config.example.json");

/// Seule version du format reconnue pour l'instant.
const SUPPORTED_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Les structs
// ---------------------------------------------------------------------------
//
// `#[derive(...)]` demande au compilateur d'écrire du code à notre place :
// - `Deserialize` : JSON -> struct (lecture du fichier) ;
// - `Serialize`   : struct -> JSON (envoi au front, réécriture à l'étape 5) ;
// - `Debug`       : affichage technique, pratique dans les tests ;
// - `Clone`       : permet de dupliquer la valeur avec `.clone()` ;
// - `PartialEq`   : permet de comparer avec `==` (utile dans les tests).
//
// `#[serde(rename_all = "camelCase")]` : le champ Rust `hide_after_launch`
// (convention snake_case de Rust) correspond à `hideAfterLaunch` dans le JSON.

/// Le fichier `config.json` complet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Lien vers le JSON Schema pour l'autocomplétion dans VS Code (étape 7).
    /// `Option<String>` : la valeur peut être absente (`None`) ou présente (`Some(..)`).
    /// On la garde pour ne pas la perdre quand l'app réécrira le fichier (étape 5).
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Version du format. Obligatoire : sans `default`, serde refuse un fichier sans `version`.
    pub version: u32,

    /// `#[serde(default)]` : si la section manque, on prend `Settings::default()`.
    #[serde(default)]
    pub settings: Settings,

    #[serde(default)]
    pub categories: Vec<Category>,

    /// Ids des tuiles favorites, dans l'ordre d'affichage.
    #[serde(default)]
    pub favorites: Vec<String>,

    #[serde(default)]
    pub tiles: Vec<Tile>,
}

/// Réglages généraux. Chaque champ absent du JSON prend sa valeur par défaut
/// (grâce au `#[serde(default)]` posé sur la struct entière).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkey: String,
    pub hide_after_launch: bool,
    pub hide_on_blur: bool,
    pub accent: String,
    /// `msedge`, `chrome`, `brave` ou chemin complet vers le navigateur.
    pub browser: String,
    pub recents_count: u32,
}

/// `Default` est un *trait* (≈ une interface) : on dit comment construire
/// des `Settings` « par défaut ». `serde(default)` s'en sert.
impl Default for Settings {
    fn default() -> Self {
        Settings {
            // `.to_string()` transforme un `&str` (texte figé dans l'exe)
            // en `String` (texte qui appartient à la struct).
            hotkey: "Ctrl+Alt+Space".to_string(),
            hide_after_launch: true,
            hide_on_blur: true,
            accent: "#f0a35e".to_string(),
            browser: "msedge".to_string(),
            recents_count: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}

/// Une tuile : des champs communs + une *cible* qui dépend du `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub id: String,
    pub name: String,
    pub category: String,
    /// Image qui remplace l'icône trouvée automatiquement (optionnelle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// `flatten` : les champs de la cible (`type`, `appId`, `path`…) sont au
    /// même niveau que `id` et `name` dans le JSON, pas dans un sous-objet.
    #[serde(flatten)]
    pub target: Target,
}

/// Ce que lance une tuile. C'est une `enum` : une valeur est *exactement une*
/// des variantes, chacune avec ses propres champs. C'est l'équivalent de
/// l'union discriminée TypeScript `{ type: "steam"; appId } | { type: "app"; … }`.
///
/// `tag = "type"` : serde lit le champ `"type"` du JSON pour choisir la variante.
/// `rename_all = "lowercase"` : la variante `Steam` correspond à `"steam"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Target {
    /// Jeu Steam, lancé via `steam://rungameid/<appId>` (étape 3).
    Steam {
        #[serde(rename = "appId")]
        app_id: u32,
    },
    /// Exécutable ou raccourci `.lnk`.
    App {
        path: String,
        /// Arguments optionnels : liste vide si absente.
        #[serde(default)]
        args: Vec<String>,
    },
    /// App web ouverte dans une fenêtre dédiée (`msedge --app=<url>`).
    Web { url: String },
}

// ---------------------------------------------------------------------------
// Lecture du fichier
// ---------------------------------------------------------------------------

/// Lit `config.json` dans `dir`, ou le crée à partir de l'exemple s'il n'existe pas.
///
/// `dir: &Path` : on *emprunte* le chemin (lecture seule), l'appelant le garde.
/// Le `Result<Config, AppError>` dit au compilateur (et au lecteur) que la
/// fonction peut échouer, et avec quel type d'erreur.
pub fn load_or_create(dir: &Path) -> Result<Config, AppError> {
    // `join` crée un nouveau `PathBuf` (chemin possédé) : dir + "config.json".
    let path = dir.join(FILE_NAME);

    // `match` sur le résultat de la lecture : trois cas possibles.
    let text = match fs::read_to_string(&path) {
        // Lu avec succès : `text` devient propriétaire de la chaîne lue.
        Ok(text) => text,
        // Le fichier n'existe pas encore : premier démarrage.
        Err(e) if e.kind() == ErrorKind::NotFound => {
            write_example(dir, &path)?;
            EXAMPLE.to_string()
        }
        // Toute autre erreur (droits, fichier verrouillé…) : on la remonte.
        Err(source) => {
            return Err(AppError::Io {
                action: "de lire",
                path,
                source,
            });
        }
    };

    // `parse` ne fait qu'emprunter `text` (`&text`) et `path` (`&path`) :
    // on pourrait encore s'en servir après cet appel.
    parse(&text, &path)
}

/// Transforme le texte JSON en `Config`, puis vérifie sa cohérence.
///
/// Séparée de la lecture du disque pour pouvoir la tester sans fichier.
pub fn parse(text: &str, path: &Path) -> Result<Config, AppError> {
    // `map_err` transforme l'erreur de serde_json en notre `AppError`.
    // Le `?` final : en cas d'erreur, on sort de la fonction avec ce `Err`.
    let config: Config = serde_json::from_str(text).map_err(|e| json_error(e, path))?;

    let problems = validate(&config);
    if problems.is_empty() {
        Ok(config)
    } else {
        // `path.to_path_buf()` : on copie le chemin emprunté pour que l'erreur
        // en possède sa propre version (elle peut vivre plus longtemps que `path`).
        Err(AppError::Invalid {
            path: path.to_path_buf(),
            problems,
        })
    }
}

/// Convertit une erreur serde_json en `AppError::Json`, avec ligne et colonne.
fn json_error(e: serde_json::Error, path: &Path) -> AppError {
    let (line, column) = (e.line(), e.column());
    // Le message de serde_json finit par « at line 3 column 5 » : on retire ce
    // suffixe puisque la ligne et la colonne sont déjà affichées à part.
    let full = e.to_string();
    let suffix = format!(" at line {line} column {column}");
    let message = full.strip_suffix(&suffix).unwrap_or(&full).to_string();
    AppError::Json {
        path: path.to_path_buf(),
        line,
        column,
        message,
    }
}

/// Vérifie ce que serde ne peut pas vérifier seul. Renvoie la liste des
/// problèmes trouvés (vide si tout va bien).
fn validate(config: &Config) -> Vec<String> {
    let mut problems = Vec::new(); // `mut` : la liste va être modifiée

    if config.version != SUPPORTED_VERSION {
        problems.push(format!(
            "\"version\" vaut {}, seule la version {SUPPORTED_VERSION} est reconnue",
            config.version
        ));
    }

    // Un `HashSet` refuse les doublons : `insert` renvoie `false` si la valeur
    // y était déjà. Les éléments sont des `&str` empruntés à `config` : aucune copie.
    let mut category_ids = std::collections::HashSet::new();
    for c in &config.categories {
        if c.id.trim().is_empty() {
            problems.push(format!("la catégorie \"{}\" n'a pas d'id", c.name));
        } else if !category_ids.insert(c.id.as_str()) {
            problems.push(format!(
                "l'id de catégorie \"{}\" est utilisé plusieurs fois",
                c.id
            ));
        }
    }

    let mut tile_ids = std::collections::HashSet::new();
    for t in &config.tiles {
        if t.id.trim().is_empty() {
            problems.push(format!("la tuile \"{}\" n'a pas d'id", t.name));
        } else if !tile_ids.insert(t.id.as_str()) {
            problems.push(format!(
                "l'id de tuile \"{}\" est utilisé plusieurs fois",
                t.id
            ));
        }
        if !category_ids.contains(t.category.as_str()) {
            problems.push(format!(
                "la tuile \"{}\" est dans la catégorie \"{}\", qui n'existe pas dans \"categories\"",
                t.id, t.category
            ));
        }
    }

    for f in &config.favorites {
        if !tile_ids.contains(f.as_str()) {
            problems.push(format!("le favori \"{f}\" ne correspond à aucune tuile"));
        }
    }

    problems
}

/// Crée le dossier de l'app et y écrit la config d'exemple.
fn write_example(dir: &Path, path: &Path) -> Result<(), AppError> {
    // Petite fonction locale (une *closure*) pour construire l'erreur d'écriture.
    // `move` n'est pas nécessaire : elle emprunte `path` le temps de l'appel.
    let io_err = |source| AppError::Io {
        action: "de créer",
        path: path.to_path_buf(),
        source,
    };

    fs::create_dir_all(dir).map_err(io_err)?;
    // `create_new(true)` : échoue si le fichier existe déjà, pour ne jamais
    // écraser une config de l'utilisateur.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_err)?;
    file.write_all(EXAMPLE.as_bytes()).map_err(io_err)?;
    Ok(()) // `()` : « rien », l'équivalent de `void`
}

/// Dossier de configuration de l'app (`%APPDATA%\io.github.acassar.lanceur`).
pub fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    // `Manager` est un trait : l'importer rend la méthode `.path()` disponible.
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map_err(|e| AppError::NoConfigDir(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests (lancés par `cargo test`, en local ou dans la CI)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> PathBuf {
        PathBuf::from("config.json")
    }

    /// Raccourci : parse un JSON et panique si ce n'est pas une erreur.
    fn parse_err(json: &str) -> AppError {
        parse(json, &p()).expect_err("le JSON aurait dû être refusé")
    }

    #[test]
    fn la_config_d_exemple_est_valide() {
        let config = parse(EXAMPLE, &p()).expect("config.example.json doit être valide");
        assert_eq!(config.version, 1);
        assert!(!config.tiles.is_empty());
    }

    #[test]
    fn lit_les_trois_types_de_tuiles() {
        let config = parse(
            r#"{
              "version": 1,
              "categories": [{ "id": "c", "name": "C" }],
              "tiles": [
                { "id": "a", "name": "A", "category": "c", "type": "steam", "appId": 730 },
                { "id": "b", "name": "B", "category": "c", "type": "app", "path": "x.exe" },
                { "id": "w", "name": "W", "category": "c", "type": "web", "url": "https://x", "icon": "i.png" }
              ]
            }"#,
            &p(),
        )
        .unwrap();

        assert_eq!(config.tiles[0].target, Target::Steam { app_id: 730 });
        // `args` absent : liste vide par défaut.
        assert_eq!(
            config.tiles[1].target,
            Target::App {
                path: "x.exe".into(),
                args: vec![]
            }
        );
        assert_eq!(config.tiles[2].icon.as_deref(), Some("i.png"));
        // Réglages absents : valeurs par défaut.
        assert_eq!(config.settings, Settings::default());
    }

    #[test]
    fn renvoie_au_front_le_meme_format_que_le_json() {
        let config = parse(EXAMPLE, &p()).unwrap();
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["settings"]["hideAfterLaunch"], true);
        assert_eq!(json["tiles"][0]["type"], "steam");
        assert_eq!(json["tiles"][0]["appId"], 730);
        assert!(json.get("$schema").is_none());
    }

    #[test]
    fn json_casse_donne_ligne_et_colonne() {
        // Virgule manquante après "version": 1 (ligne 2).
        let err = parse_err("{\n  \"version\": 1\n  \"tiles\": []\n}");
        match err {
            AppError::Json {
                line,
                column,
                message,
                ..
            } => {
                assert_eq!((line, column), (3, 3));
                assert!(
                    !message.contains("at line"),
                    "suffixe non retiré : {message}"
                );
            }
            other => panic!("erreur inattendue : {other:?}"),
        }
    }

    #[test]
    fn champ_manquant_selon_le_type() {
        let err = parse_err(
            r#"{ "version": 1, "categories": [{ "id": "c", "name": "C" }],
                 "tiles": [{ "id": "a", "name": "A", "category": "c", "type": "steam" }] }"#,
        );
        assert!(err.to_string().contains("appId"), "{err}");
    }

    #[test]
    fn type_inconnu() {
        let err = parse_err(
            r#"{ "version": 1, "categories": [{ "id": "c", "name": "C" }],
                 "tiles": [{ "id": "a", "name": "A", "category": "c", "type": "stem", "appId": 1 }] }"#,
        );
        assert!(err.to_string().contains("stem"), "{err}");
    }

    #[test]
    fn incoherences_listees_ensemble() {
        let err = parse_err(
            r#"{ "version": 2,
                 "categories": [{ "id": "c", "name": "C" }],
                 "favorites": ["zz"],
                 "tiles": [
                   { "id": "a", "name": "A", "category": "c", "type": "web", "url": "u" },
                   { "id": "a", "name": "A2", "category": "nope", "type": "web", "url": "u" }
                 ] }"#,
        );
        let AppError::Invalid { problems, .. } = err else {
            panic!("{err:?}")
        };
        assert_eq!(problems.len(), 4, "{problems:#?}");
    }

    #[test]
    fn premier_demarrage_cree_le_fichier_sans_ecraser_ensuite() {
        // Dossier temporaire supprimé automatiquement à la fin du test.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sous-dossier");

        let config = load_or_create(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        assert_eq!(fs::read_to_string(&path).unwrap(), EXAMPLE);
        assert_eq!(config, parse(EXAMPLE, &path).unwrap());

        // L'utilisateur modifie le fichier : on lit sa version, pas l'exemple.
        fs::write(&path, r#"{ "version": 1 }"#).unwrap();
        let config = load_or_create(&dir).unwrap();
        assert!(config.tiles.is_empty());
    }

    #[test]
    fn erreur_envoyee_au_front() {
        let err = parse_err("{ \"version\": }");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["line"], 1);
        assert_eq!(json["path"], "config.json");
        assert!(
            json["message"]
                .as_str()
                .unwrap()
                .starts_with("config.json, ligne 1")
        );
    }
}
