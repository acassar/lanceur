//! Lecture de `config.json`.
//!
//! Le fichier vit dans `%APPDATA%\io.github.acassar.lanceur\config.json`.
//! S'il n'existe pas, on le crée à partir de `config.default.json` (embarqué
//! dans l'exe à la compilation) : réglages par défaut, mais aucune tuile,
//! l'app affiche alors un écran d'accueil qui explique comment en ajouter. S'il est invalide, on renvoie une erreur
//! en français (ligne, colonne, message, tuile concernée) que le front affiche.
//!
//! Les structs ci-dessous produisent le même JSON que les types TypeScript de
//! `src/lib/types.ts` (seules différences : `$schema`, ignoré par le front, et
//! `args`, toujours envoyé, vide au besoin). `serde` fait la conversion JSON <-> Rust.

use std::collections::HashSet;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Nom du fichier de configuration dans le dossier de l'app.
pub const FILE_NAME: &str = "config.json";

/// Config écrite au premier démarrage : réglages par défaut, aucune tuile.
///
/// `include_str!` lit le fichier *à la compilation* et colle son contenu dans
/// l'exe sous forme de `&'static str` (une chaîne qui vit aussi longtemps que
/// le programme). Aucun fichier à installer à côté de l'exe.
const DEFAULT: &str = include_str!("../config.default.json");

/// Seule version du format reconnue pour l'instant.
const SUPPORTED_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Les structs
// ---------------------------------------------------------------------------
//
// `#[derive(...)]` demande au compilateur d'écrire du code à notre place :
// - `Deserialize` : JSON -> struct (lecture du fichier) ;
// - `Serialize`   : struct -> JSON (envoi au front) ;
// - `Debug`       : affichage technique, pratique dans les tests ;
// - `Clone`       : permet de dupliquer la valeur avec `.clone()` ;
// - `PartialEq`   : permet de comparer avec `==` (utile dans les tests).
//
// `#[serde(rename_all = "camelCase")]` : le champ Rust `hide_after_launch`
// (convention snake_case de Rust) correspond à `hideAfterLaunch` dans le JSON.
//
// `deny_unknown_fields` : un champ inconnu (faute de frappe comme `hideonblur`)
// est refusé au lieu d'être ignoré en silence. Pour les tuiles, ce contrôle
// est fait à la main dans `unknown_tile_fields` (incompatible avec `flatten`).

/// Le fichier `config.json` complet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    /// Lien vers le JSON Schema pour l'autocomplétion dans VS Code (étape 7).
    /// `Option<String>` : la valeur peut être absente (`None`) ou présente (`Some(..)`).
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
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub hotkey: String,
    pub hide_after_launch: bool,
    pub hide_on_blur: bool,
    pub accent: String,
    /// `msedge`, `chrome`, `brave` ou chemin complet vers le navigateur.
    pub browser: String,
    pub recents_count: u32,
    /// Lancer l'app à l'ouverture de session Windows (autostart.rs).
    pub launch_at_startup: bool,
}

/// `Default` est un *trait* (≈ une interface) : on dit comment construire
/// des `Settings` « par défaut ». `serde(default)` s'en sert.
impl Default for Settings {
    fn default() -> Self {
        Settings {
            // `.to_string()` transforme un `&str` (texte figé dans l'exe)
            // en `String` (texte qui appartient à la struct).
            hotkey: "Super+Alt+Space".to_string(),
            hide_after_launch: true,
            hide_on_blur: true,
            accent: "#f0a35e".to_string(),
            browser: "msedge".to_string(),
            recents_count: 4,
            launch_at_startup: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Lit `config.json` dans `dir`, ou le crée (vide de tuiles) s'il n'existe pas.
///
/// `dir: &Path` : on *emprunte* le chemin (lecture seule), l'appelant le garde.
/// Le `Result<Config, AppError>` dit au compilateur (et au lecteur) que la
/// fonction peut échouer, et avec quel type d'erreur.
pub fn load_or_create(dir: &Path) -> Result<Config, AppError> {
    // `join` crée un nouveau `PathBuf` (chemin possédé) : dir + "config.json".
    let path = dir.join(FILE_NAME);

    // `match` sur le résultat de la lecture : un cas par situation.
    let text = match fs::read_to_string(&path) {
        // Lu avec succès : `text` devient propriétaire de la chaîne lue.
        Ok(text) => text,
        // Le fichier n'existe pas encore : premier démarrage. On écrit
        // la config de départ puis on la relit (au cas où une autre instance de l'app
        // l'aurait créé en même temps, c'est sa version qui compte).
        Err(e) if e.kind() == ErrorKind::NotFound => {
            write_default(dir, &path)?;
            read(&path)?
        }
        Err(source) => return Err(read_error(path, source)),
    };

    // `parse` ne fait qu'emprunter `text` (`&text`) et `path` (`&path`) :
    // on pourrait encore s'en servir après cet appel.
    parse(&text, &path)
}

/// Dernière config lue avec succès, et l'état du fichier à ce moment-là.
struct Cached {
    path: PathBuf,
    /// Date de modification et taille de `config.json` lors de la lecture.
    stamp: (SystemTime, u64),
    config: Config,
}

/// Mémoire partagée par tout le programme pour [`load_cached`].
///
/// `static` : une seule valeur pour toute l'app, comme une variable de module
/// en TypeScript. Rust interdit de modifier un `static` sans protection (deux
/// threads pourraient l'écrire en même temps) : d'où le `Mutex`.
/// `Mutex::new(None)` peut être évalué à la compilation, ce qui est exigé pour
/// un `static`.
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// Comme [`load_or_create`], mais sans relire ni revalider le fichier s'il n'a
/// pas changé depuis la dernière lecture (même date de modification, même taille).
///
/// Utile pour `get_icon`, appelée une fois *par tuile* à l'ouverture : avec
/// 40 tuiles, on lisait et validait 40 fois le même fichier. Maintenant, on ne
/// demande à Windows que sa date et sa taille, ce qui est quasi instantané.
///
/// La valeur renvoyée est une *copie* (`clone`) : l'appelant en est
/// propriétaire et peut la garder sans tenir le verrou.
pub fn load_cached(dir: &Path) -> Result<Config, AppError> {
    let path = dir.join(FILE_NAME);
    let stamp = file_stamp(&path);
    {
        // Bloc `{ … }` : le verrou est relâché à sa fin, avant la lecture du
        // fichier (on ne bloque pas les autres threads pendant ce temps).
        let cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        // `if let Some(c) = …` + condition : on ne sert la copie que si elle
        // concerne le même fichier, dans le même état.
        if let Some(c) = cache
            .as_ref()
            .filter(|c| c.path == path && Some(c.stamp) == stamp)
        {
            return Ok(c.config.clone());
        }
    }
    load_and_remember(dir)
}

/// Lit `config.json` sans tenir compte de la mémoire, puis la met à jour.
/// C'est ce que fait `get_config` : un rechargement demandé lit toujours le disque.
pub fn load_and_remember(dir: &Path) -> Result<Config, AppError> {
    let path = dir.join(FILE_NAME);
    // L'état du fichier est relevé *avant* la lecture : s'il est modifié
    // pendant qu'on le lit, la date ne correspondra plus au prochain appel, qui
    // relira donc le fichier. Dans l'autre ordre, on risquerait de garder
    // l'ancienne version en croyant avoir la nouvelle.
    let stamp = file_stamp(&path);
    let config = load_or_create(dir)?;
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    *cache = stamp.map(|stamp| Cached {
        path,
        stamp,
        config: config.clone(),
    });
    Ok(config)
}

/// Date de modification et taille du fichier, ou `None` s'il est illisible
/// (absent au premier démarrage…) : dans ce cas, rien n'est mémorisé.
fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Lit le fichier en texte, en convertissant l'erreur éventuelle.
/// `pub(crate)` : visible dans toute l'app (favorites.rs), mais pas au-delà.
pub(crate) fn read(path: &Path) -> Result<String, AppError> {
    fs::read_to_string(path).map_err(|source| read_error(path.to_path_buf(), source))
}

/// Erreur de lecture : on distingue le cas « pas de l'UTF-8 », fréquent quand
/// le fichier est enregistré en « ANSI » ou « Unicode » dans le Bloc-notes.
fn read_error(path: PathBuf, source: std::io::Error) -> AppError {
    if source.kind() == ErrorKind::InvalidData {
        AppError::Encoding { path }
    } else {
        AppError::Io {
            action: "de lire",
            path,
            source,
        }
    }
}

/// Transforme le texte JSON en `Config`, puis vérifie sa cohérence.
///
/// Séparée de la lecture du disque pour pouvoir la tester sans fichier.
pub fn parse(text: &str, path: &Path) -> Result<Config, AppError> {
    // Certains éditeurs Windows ajoutent un « BOM » (caractère invisible U+FEFF)
    // au début des fichiers UTF-8. serde_json le refuse : on le retire.
    // `strip_prefix` renvoie `Some(reste)` si le texte commence par le BOM,
    // `None` sinon ; `unwrap_or(text)` garde alors le texte tel quel.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let invalid = |problems| AppError::Invalid {
        path: path.to_path_buf(),
        problems,
    };
    if text.trim().is_empty() {
        return Err(invalid(vec!["le fichier est vide".to_string()]));
    }

    // `serde_path_to_error` enveloppe serde_json pour savoir *où* dans la
    // structure l'erreur s'est produite (ex. `tiles[3].appId`).
    let mut de = serde_json::Deserializer::from_str(text);
    // `map_err` transforme l'erreur en notre `AppError`.
    // Le `?` final : en cas d'erreur, on sort de la fonction avec ce `Err`.
    let config: Config =
        serde_path_to_error::deserialize(&mut de).map_err(|e| json_error(e, text, path))?;

    let mut problems = validate(&config);
    problems.extend(unknown_tile_fields(text));
    if problems.is_empty() {
        Ok(config)
    } else {
        Err(invalid(problems))
    }
}

/// Convertit une erreur serde_json en `AppError::Json`, avec ligne, colonne
/// et un message en français.
fn json_error(
    e: serde_path_to_error::Error<serde_json::Error>,
    text: &str,
    path: &Path,
) -> AppError {
    // `e.path()` : où l'erreur s'est produite, `e.inner()` : l'erreur serde_json.
    let location = e.path().to_string();
    let inner = e.inner();
    let line = inner.line();
    let column = char_column(text, line, inner.column());

    // Le message de serde_json finit par « at line 3 column 5 » : on retire ce
    // suffixe puisque la ligne et la colonne sont déjà affichées à part.
    let full = inner.to_string();
    let suffix = format!(" at line {} column {}", line, inner.column());
    let mut message = translate(full.strip_suffix(&suffix).unwrap_or(&full));

    // Erreur dans une tuile : on dit laquelle. serde lit chaque tuile en entier
    // avant de choisir sa variante (`type`), donc la position indiquée est
    // souvent la fin de la tuile, pas le champ fautif : le nom aide à la retrouver.
    if let Some(tile) = tile_label(text, &location) {
        message = format!("{tile} : {message}");
    } else if inner.is_data() && location != "." {
        message = format!("{location} : {message}");
    }

    AppError::Json {
        path: path.to_path_buf(),
        line,
        column,
        message,
    }
}

/// serde_json compte les colonnes en *octets* ; un éditeur compte en
/// *caractères* (« é » fait 2 octets en UTF-8). On convertit.
fn char_column(text: &str, line: usize, byte_column: usize) -> usize {
    let Some(src) = text.lines().nth(line.saturating_sub(1)) else {
        return byte_column;
    };
    let mut end = byte_column.min(src.len());
    // On recule jusqu'à une frontière de caractère pour ne pas couper un « é ».
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    src[..end].chars().count()
}

/// Traduit les messages de serde_json les plus courants.
fn translate(msg: &str) -> String {
    // Paires (début du message anglais, traduction). Le reste du message
    // (noms de champs, types attendus) est recollé tel quel.
    const PREFIXES: &[(&str, &str)] = &[
        ("missing field ", "champ manquant : "),
        ("unknown field ", "champ inconnu : "),
        ("unknown variant ", "valeur inconnue : "),
        ("duplicate field ", "champ en double : "),
        ("invalid type: ", "type invalide : "),
        ("invalid value: ", "valeur invalide : "),
        ("trailing comma", "virgule en trop"),
        (
            "key must be a string",
            "nom de champ attendu entre guillemets (les commentaires // ne sont pas autorisés)",
        ),
        (
            "EOF while parsing",
            "fin du fichier inattendue (accolade ou crochet non fermé ?)",
        ),
        ("expected `,` or `}`", "virgule ou `}` attendue"),
        ("expected `,` or `]`", "virgule ou `]` attendue"),
        ("expected `:`", "`:` attendu"),
        ("expected value", "valeur attendue"),
        ("trailing characters", "texte en trop après la fin du JSON"),
    ];
    for (en, fr) in PREFIXES {
        if let Some(rest) = msg.strip_prefix(en) {
            // Pour les messages entièrement traduits, on ignore la suite anglaise.
            return if en.ends_with(' ') {
                // « expected one of `a`, `b` » -> « attendus : `a`, `b` »
                let rest = rest
                    .replace(", expected one of ", " (attendus : ")
                    .replace(", expected ", " (attendu : ");
                let close = if rest.contains(" (attendu") { ")" } else { "" };
                format!("{fr}{rest}{close}")
            } else {
                fr.to_string()
            };
        }
    }
    msg.to_string()
}

/// Si `location` désigne une tuile (`tiles[3]…`), renvoie « tuile "id" » ou
/// « tuile n°4 » quand elle n'a pas d'id lisible.
fn tile_label(text: &str, location: &str) -> Option<String> {
    // `?` marche aussi avec `Option` : `None` fait sortir de la fonction.
    let rest = location.strip_prefix("tiles[")?;
    let index: usize = rest.split(']').next()?.parse().ok()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    Some(match value["tiles"][index]["id"].as_str() {
        Some(id) => format!("tuile \"{id}\""),
        None => format!("tuile n°{}", index + 1),
    })
}

/// Champs autorisés dans une tuile, selon son `type`.
fn allowed_tile_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "steam" => &["appId"],
        "app" => &["path", "args"],
        "web" => &["url"],
        _ => &[],
    }
}

/// Signale les champs mal orthographiés dans les tuiles (`"ico"` au lieu de
/// `"icon"`…). serde ne peut pas le faire lui-même à cause du `flatten`.
fn unknown_tile_fields(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(tiles) = value["tiles"].as_array() else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    for tile in tiles {
        let Some(obj) = tile.as_object() else {
            continue;
        };
        let kind = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let id = obj.get("id").and_then(|t| t.as_str()).unwrap_or("?");
        for key in obj.keys() {
            let common = ["id", "name", "category", "icon", "type"].contains(&key.as_str());
            if !common && !allowed_tile_fields(kind).contains(&key.as_str()) {
                problems.push(format!(
                    "tuile \"{id}\" : champ inconnu \"{key}\" pour le type \"{kind}\""
                ));
            }
        }
    }
    problems
}

/// Ids de catégorie réservés par l'interface (entrées du rail, sections).
const RESERVED_CATEGORY_IDS: &[&str] = &["all", "favorites", "recents", "results"];

/// Un id valide : minuscules, chiffres, `-` et `_`. Il sert de nom de fichier
/// pour le cache d'icônes (étape 6), d'où cette règle stricte.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
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
    let mut category_ids = HashSet::new();
    for c in &config.categories {
        if !is_valid_id(&c.id) {
            problems.push(format!(
                "la catégorie \"{}\" a un id invalide \"{}\" (minuscules, chiffres, - et _ uniquement)",
                c.name, c.id
            ));
        } else if RESERVED_CATEGORY_IDS.contains(&c.id.as_str()) {
            problems.push(format!(
                "l'id de catégorie \"{}\" est réservé par l'interface, choisissez-en un autre",
                c.id
            ));
        } else if !category_ids.insert(c.id.as_str()) {
            problems.push(format!(
                "l'id de catégorie \"{}\" est utilisé plusieurs fois",
                c.id
            ));
        }
    }

    let mut tile_ids = HashSet::new();
    for t in &config.tiles {
        if !is_valid_id(&t.id) {
            problems.push(format!(
                "la tuile \"{}\" a un id invalide \"{}\" (minuscules, chiffres, - et _ uniquement)",
                t.name, t.id
            ));
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

    let mut favorite_ids = HashSet::new();
    for f in &config.favorites {
        if !tile_ids.contains(f.as_str()) {
            problems.push(format!("le favori \"{f}\" ne correspond à aucune tuile"));
        } else if !favorite_ids.insert(f.as_str()) {
            problems.push(format!("le favori \"{f}\" apparaît plusieurs fois"));
        }
    }

    problems
}

/// Crée le dossier de l'app et y écrit la config de départ.
fn write_default(dir: &Path, path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(dir).map_err(|source| create_error(dir, source))?;
    // `create_new(true)` : échoue si le fichier existe déjà, pour ne jamais
    // écraser une config de l'utilisateur.
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        // Créé entre-temps (par une autre instance) : rien à faire.
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Ok(()),
        Err(source) => return Err(create_error(path, source)),
    };
    // `|source| …` est une *closure* : une fonction anonyme, comme une fonction
    // fléchée JS. Elle *emprunte* `path` (lecture seule) pour construire l'erreur.
    if let Err(source) = file.write_all(DEFAULT.as_bytes()) {
        // Écriture interrompue (disque plein…) : on supprime le fichier à
        // moitié écrit, sinon il ne serait jamais recréé (`create_new`
        // tomberait sur `AlreadyExists`). `drop` ferme le fichier avant, ce
        // que Windows exige pour le supprimer ; `let _ =` ignore un échec ici.
        drop(file);
        let _ = fs::remove_file(path);
        return Err(create_error(path, source));
    }
    Ok(()) // `()` : « rien », l'équivalent de `void`
}

fn create_error(path: &Path, source: std::io::Error) -> AppError {
    AppError::Io {
        action: "de créer",
        path: path.to_path_buf(),
        source,
    }
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

    /// Config d'exemple de la documentation (une tuile de chaque type). Elle
    /// n'est plus écrite au premier démarrage, mais doit rester valide.
    const EXAMPLE: &str = include_str!("../config.example.json");

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
    fn la_config_de_depart_est_valide_et_sans_tuiles() {
        let config = parse(DEFAULT, &p()).expect("config.default.json doit être valide");
        assert!(config.categories.is_empty());
        assert!(config.favorites.is_empty());
        assert!(config.tiles.is_empty());
        // Les réglages écrits dans le fichier sont ceux par défaut : le fichier
        // les montre à l'utilisateur sans rien changer au comportement.
        assert_eq!(config.settings, Settings::default());
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
        // `$schema` est transmis tel quel ; le front l'ignore.
        assert!(
            json["$schema"]
                .as_str()
                .unwrap()
                .ends_with("config.schema.json")
        );
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
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT);
        assert_eq!(config, parse(DEFAULT, &path).unwrap());

        // L'utilisateur modifie le fichier : on lit sa version, pas l'exemple.
        fs::write(&path, r#"{ "version": 1 }"#).unwrap();
        let config = load_or_create(&dir).unwrap();
        assert!(config.tiles.is_empty());
    }

    #[test]
    fn la_memoire_suit_les_modifications_du_fichier() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join(FILE_NAME);
        fs::write(&path, r#"{ "version": 1 }"#).unwrap();
        assert!(load_cached(dir).unwrap().favorites.is_empty());

        // Fichier modifié (taille différente) : il est relu.
        fs::write(
            &path,
            r#"{ "version": 1, "categories": [{ "id": "c", "name": "C" }] }"#,
        )
        .unwrap();
        assert_eq!(load_cached(dir).unwrap().categories.len(), 1);

        // Fichier devenu invalide : l'erreur est renvoyée, pas l'ancienne copie.
        fs::write(&path, "{").unwrap();
        assert!(load_cached(dir).is_err());
    }

    /// `config.schema.json` (à la racine du repo) doit décrire les mêmes champs
    /// que les structs : ce test échoue si on ajoute un réglage sans le schéma.
    #[test]
    fn le_schema_json_suit_les_structs() {
        use std::collections::BTreeSet;
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../config.schema.json")).unwrap();
        // Noms des propriétés d'un objet du schéma, triés.
        let keys = |v: &serde_json::Value| -> BTreeSet<String> {
            v["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect()
        };

        let settings = serde_json::to_value(Settings::default()).unwrap();
        let expected: BTreeSet<String> = settings.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys(&schema["properties"]["settings"]), expected);

        let mut expected: BTreeSet<String> = ["id", "name", "category", "icon", "type"]
            .map(String::from)
            .into();
        for kind in ["steam", "app", "web"] {
            expected.extend(allowed_tile_fields(kind).iter().map(|f| f.to_string()));
        }
        assert_eq!(keys(&schema["definitions"]["tile"]), expected);

        let top: BTreeSet<String> = [
            "$schema",
            "version",
            "settings",
            "categories",
            "favorites",
            "tiles",
        ]
        .map(String::from)
        .into();
        assert_eq!(keys(&schema), top);
        // Les configs de départ et d'exemple pointent vers le schéma.
        for text in [DEFAULT, EXAMPLE] {
            assert_eq!(
                parse(text, &p()).unwrap().schema.as_deref(),
                schema["$id"].as_str()
            );
        }
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

    #[test]
    fn accepte_le_bom_utf8() {
        let text = format!("\u{feff}{EXAMPLE}");
        assert!(parse(&text, &p()).is_ok());
    }

    #[test]
    fn fichier_vide() {
        let err = parse_err("  \n");
        assert!(err.to_string().contains("vide"), "{err}");
    }

    #[test]
    fn erreur_dans_une_tuile_nomme_la_tuile() {
        let err = parse_err(
            r#"{ "version": 1, "categories": [{ "id": "c", "name": "C" }],
                 "tiles": [
                   { "id": "ok", "name": "A", "category": "c", "type": "web", "url": "u" },
                   { "id": "cs2", "name": "B", "category": "c", "type": "steam", "appId": "730" }
                 ] }"#,
        );
        let msg = err.to_string();
        assert!(msg.contains("tuile \"cs2\""), "{msg}");
        assert!(msg.contains("type invalide"), "{msg}");
    }

    #[test]
    fn champs_inconnus_refuses() {
        let err = parse_err(r#"{ "version": 1, "settings": { "hideonblur": false } }"#);
        assert!(err.to_string().contains("hideonblur"), "{err}");

        let err = parse_err(
            r#"{ "version": 1, "categories": [{ "id": "c", "name": "C" }],
                 "tiles": [{ "id": "a", "name": "A", "category": "c", "type": "web", "url": "u", "ico": "x.png" }] }"#,
        );
        assert!(err.to_string().contains("\"ico\""), "{err}");
    }

    #[test]
    fn ids_reserves_invalides_et_favoris_en_double() {
        let err = parse_err(
            r#"{ "version": 1,
                 "categories": [{ "id": "favorites", "name": "F" }, { "id": "Jeux", "name": "J" }, { "id": "c", "name": "C" }],
                 "favorites": ["a", "a"],
                 "tiles": [{ "id": "a", "name": "A", "category": "c", "type": "web", "url": "u" }] }"#,
        );
        let AppError::Invalid { problems, .. } = err else {
            panic!("{err:?}")
        };
        assert_eq!(problems.len(), 3, "{problems:#?}");
    }

    #[test]
    fn colonne_comptee_en_caracteres() {
        // « é » fait 2 octets en UTF-8 : sans conversion, la colonne serait décalée de 2.
        let text = r#"{ "version": 1, "categories": [{ "id": "c", "name": "éé" "x": 1 }] }"#;
        let err = parse_err(text);
        let AppError::Json { column, .. } = err else {
            panic!("{err:?}")
        };
        // Position (en caractères, à partir de 1) du `"x"` fautif.
        let expected = text[..text.find("\"x\"").unwrap()].chars().count() + 1;
        assert_eq!(column, expected);
    }

    #[test]
    fn messages_en_francais() {
        let err = parse_err("{ \"version\": 1, }");
        assert!(err.to_string().contains("virgule en trop"), "{err}");
        let err = parse_err("{ // commentaire\n \"version\": 1 }");
        assert!(err.to_string().contains("commentaires"), "{err}");
    }
}
