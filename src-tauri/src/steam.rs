//! Import de la bibliothèque Steam.
//!
//! Deux étapes séparées, pour pouvoir montrer la liste avant d'écrire quoi que ce soit :
//!
//! 1. [`detect`] *lit* les jeux installés, sans rien modifier :
//!    - le dossier de Steam est donné par le registre (voir `icons::steam_dir`) ;
//!    - `steamapps\libraryfolders.vdf` liste les bibliothèques (un dossier
//!      `SteamLibrary` par disque, en plus de celui de Steam) ;
//!    - chaque jeu installé a un fichier `steamapps\appmanifest_<appId>.acf`
//!      dans sa bibliothèque, avec son `appid` et son `name`.
//! 2. [`import`] ajoute les jeux choisis à `config.json` : une tuile `steam`
//!    par jeu, dans la catégorie « Jeux » (créée si besoin). Les jeux qui ont
//!    déjà une tuile sont ignorés, et rien d'autre n'est modifié dans le
//!    fichier : vos tuiles, leur ordre et la mise en forme restent tels quels.
//!
//! Notions Rust abordées : `HashSet` pour éliminer les doublons, `sort_by_key`,
//! et les *itérateurs* (`filter`, `map`, `collect`), proches des méthodes de
//! tableau en JavaScript.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{self, Category, Config, Target, Tile};
use crate::error::AppError;
use crate::fsutil;
use crate::jsontext;
use crate::vdf;

/// Un jeu installé, tel que le front l'affiche dans la liste à cocher.
///
/// `Serialize` : envoyé au front (`{ "appId": 730, "name": "Counter-Strike 2" }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
}

/// Résultat d'un import, pour le message affiché par le front.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// Ids des tuiles ajoutées (vide si tous les jeux étaient déjà là).
    pub added: Vec<String>,
    /// Nom de la catégorie où elles ont été rangées.
    pub category: String,
}

/// Catégorie utilisée quand la config n'en a aucune pour les jeux.
const GAMES_ID: &str = "jeux";
const GAMES_NAME: &str = "Jeux";

/// Applications Steam qui ne sont pas des jeux qu'on lance : composants
/// installés automatiquement par Steam (redistribuables, outils Linux).
const NOT_GAMES: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime 1.0 (scout)
    1391110, // Steam Linux Runtime 2.0 (soldier)
    1628350, // Steam Linux Runtime 3.0 (sniper)
];

/// Bit de `StateFlags` qui signifie « entièrement installé ». Un jeu dont le
/// premier téléchargement n'est pas fini ne l'a pas encore (et ne se lance pas).
const FULLY_INSTALLED: u32 = 4;

// ---------------------------------------------------------------------------
// Détection
// ---------------------------------------------------------------------------

/// Jeux installés sur ce PC, triés par nom. Ne modifie rien.
pub fn detect() -> Result<Vec<SteamGame>, AppError> {
    let dir = crate::icons::steam_dir().ok_or_else(|| {
        AppError::Steam("Steam n'est pas installé sur ce PC (dossier introuvable)".into())
    })?;
    Ok(installed_games(&dir))
}

/// Le travail de [`detect`], à partir du dossier de Steam (testable sans registre).
pub fn installed_games(steam_dir: &Path) -> Vec<SteamGame> {
    let mut seen = HashSet::new();
    let mut games = Vec::new();
    for library in library_folders(steam_dir) {
        // Bibliothèque sur un disque débranché : on passe à la suivante.
        let Ok(entries) = fs::read_dir(library.join("steamapps")) else {
            continue;
        };
        // `flatten()` saute les entrées illisibles (`Err`) et garde les autres.
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !(file_name.starts_with("appmanifest_") && file_name.ends_with(".acf")) {
                continue;
            }
            let Ok(bytes) = fs::read(entry.path()) else {
                continue;
            };
            // `from_utf8_lossy` : un octet invalide devient « � » au lieu de
            // faire échouer la lecture de tout le fichier.
            if let Some(game) = parse_manifest(&String::from_utf8_lossy(&bytes)) {
                // `insert` renvoie `false` si le jeu a déjà été vu (même jeu
                // présent dans deux bibliothèques) : on ne le garde qu'une fois.
                if seen.insert(game.app_id) {
                    games.push(game);
                }
            }
        }
    }
    // Tri sans tenir compte des majuscules ; `to_lowercase` gère les accents.
    // `cached` : la version en minuscules est calculée une fois par jeu.
    games.sort_by_cached_key(|g| g.name.to_lowercase());
    games
}

/// Dossiers des bibliothèques Steam : celui de Steam, puis ceux listés dans
/// `steamapps\libraryfolders.vdf`, sans doublon.
fn library_folders(steam_dir: &Path) -> Vec<PathBuf> {
    let mut folders = vec![steam_dir.to_path_buf()];
    let file = steam_dir.join("steamapps").join("libraryfolders.vdf");
    let Some(root) = fs::read(&file)
        .ok()
        .and_then(|bytes| vdf::parse(&String::from_utf8_lossy(&bytes)))
    else {
        return folders;
    };
    let Some(list) = root.get("libraryfolders") else {
        return folders;
    };
    for (key, value) in list.pairs() {
        // Les bibliothèques sont numérotées "0", "1"… ; les autres clés
        // (`contentstatsid`…) ne sont pas des dossiers.
        if key.parse::<u32>().is_err() {
            continue;
        }
        // Format actuel : `"1" { "path" "D:\\SteamLibrary" … }`.
        // Ancien format (avant 2021) : `"1" "D:\\SteamLibrary"`.
        let path = match value {
            vdf::Value::Block(_) => value.get("path").and_then(|p| p.as_str()),
            vdf::Value::Str(path) => Some(path.as_str()),
        };
        if let Some(path) = path.map(PathBuf::from)
            && !folders.iter().any(|f| same_folder(f, &path))
        {
            folders.push(path);
        }
    }
    folders
}

/// Même dossier ? Sous Windows, `C:/Steam` et `c:\steam` désignent le même :
/// on compare sans tenir compte des majuscules ni du sens des barres.
fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Lit un `appmanifest_<appId>.acf`. `None` si ce n'est pas un jeu à proposer
/// (fichier abîmé, redistribuable, installation pas terminée).
pub fn parse_manifest(text: &str) -> Option<SteamGame> {
    let root = vdf::parse(text)?;
    let state = root.get("AppState")?;
    let field = |key| state.get(key).and_then(|v| v.as_str()).map(str::trim);

    let app_id: u32 = field("appid")?.parse().ok()?;
    if app_id == 0 || NOT_GAMES.contains(&app_id) {
        return None;
    }
    // `StateFlags` absent : on suppose le jeu installé.
    if let Some(flags) = field("StateFlags").and_then(|f| f.parse::<u32>().ok())
        && flags & FULLY_INSTALLED == 0
    {
        return None;
    }
    let name = field("name")
        .filter(|n| !n.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("Jeu Steam {app_id}"));
    Some(SteamGame { app_id, name })
}

// ---------------------------------------------------------------------------
// Import dans config.json
// ---------------------------------------------------------------------------

/// Ce qu'un import va ajouter à la config, calculé sans toucher au fichier.
#[derive(Debug, PartialEq)]
struct Plan {
    category: Category,
    /// `true` si la catégorie n'existe pas encore et doit être ajoutée.
    new_category: bool,
    tiles: Vec<Tile>,
}

/// Ajoute `games` à `config.json` (dans `dir`) et renvoie ce qui a été ajouté.
///
/// Comme pour les favoris : un fichier invalide n'est jamais touché, le texte
/// est modifié sur place (fin des tableaux `"categories"` et `"tiles"`), puis
/// relu et vérifié avant d'être écrit de façon atomique.
pub fn import(dir: &Path, games: &[SteamGame]) -> Result<ImportResult, AppError> {
    let _guard = config::write_lock();
    let path = dir.join(config::FILE_NAME);
    let text = config::read_or_create_text(dir)?;
    let config = config::parse(&text, &path)?;

    let plan = plan(&config, games);
    let result = ImportResult {
        added: plan.tiles.iter().map(|t| t.id.clone()).collect(),
        category: plan.category.name.clone(),
    };
    if plan.tiles.is_empty() {
        return Ok(result);
    }

    let not_found = |key: &str| {
        AppError::ConfigWrite(format!(
            "le tableau \"{key}\" n'a pas pu être complété dans le fichier"
        ))
    };
    let mut new_text = text.clone();
    if plan.new_category {
        new_text =
            jsontext::append_to_array(&new_text, "categories", &[category_json(&plan.category)])
                .ok_or_else(|| not_found("categories"))?;
    }
    let tiles: Vec<String> = plan.tiles.iter().map(tile_json).collect();
    new_text =
        jsontext::append_to_array(&new_text, "tiles", &tiles).ok_or_else(|| not_found("tiles"))?;

    // Filet de sécurité : le nouveau fichier doit donner exactement l'ancienne
    // config, plus la catégorie et les tuiles prévues. Sinon, on n'écrit rien.
    let check = config::parse(&new_text, &path)
        .map_err(|e| AppError::ConfigWrite(format!("modification refusée ({e})")))?;
    let mut expected = config;
    if plan.new_category {
        expected.categories.push(plan.category.clone());
    }
    expected.tiles.extend(plan.tiles);
    if check != expected {
        return Err(AppError::ConfigWrite(
            "le fichier modifié ne correspond pas à l'import prévu".into(),
        ));
    }

    fsutil::write_atomic(&path, new_text.as_bytes())?;
    Ok(result)
}

/// Choisit la catégorie et crée les tuiles, en sautant les jeux déjà présents.
fn plan(config: &Config, games: &[SteamGame]) -> Plan {
    // Catégorie des jeux : id `jeux`, sinon une catégorie nommée « Jeux » ou
    // « Games », sinon une nouvelle catégorie `jeux` (l'id est donc libre).
    let existing = config
        .categories
        .iter()
        .find(|c| c.id == GAMES_ID)
        .or_else(|| {
            config.categories.iter().find(|c| {
                let name = c.name.trim().to_lowercase();
                name == "jeux" || name == "games"
            })
        });
    let (category, new_category) = match existing {
        Some(c) => (c.clone(), false),
        None => (
            Category {
                id: GAMES_ID.into(),
                name: GAMES_NAME.into(),
            },
            true,
        ),
    };

    // Jeux qui ont déjà une tuile (quelle que soit sa catégorie) et ids pris.
    // `filter_map` : garde et transforme en une fois (ici : les appId des tuiles steam).
    let mut app_ids: HashSet<u32> = config
        .tiles
        .iter()
        .filter_map(|t| match t.target {
            Target::Steam { app_id } => Some(app_id),
            _ => None,
        })
        .collect();
    let mut ids: HashSet<String> = config.tiles.iter().map(|t| t.id.clone()).collect();

    let mut tiles = Vec::new();
    for game in games {
        if !app_ids.insert(game.app_id) {
            continue;
        }
        let id = unique_id(&game.name, game.app_id, &mut ids);
        tiles.push(Tile {
            id,
            name: game.name.clone(),
            category: category.id.clone(),
            icon: None,
            target: Target::Steam {
                app_id: game.app_id,
            },
        });
    }
    Plan {
        category,
        new_category,
        tiles,
    }
}

/// Id de tuile tiré du nom (`Baldur's Gate 3` -> `baldurs-gate-3`), rendu
/// unique : en cas de collision, on ajoute l'appId, puis un numéro.
fn unique_id(name: &str, app_id: u32, taken: &mut HashSet<String>) -> String {
    let base = match slug(name) {
        s if s.is_empty() => format!("steam-{app_id}"),
        s => s,
    };
    let mut id = base.clone();
    let mut n = 1;
    while taken.contains(&id) {
        id = if n == 1 {
            format!("{base}-{app_id}")
        } else {
            format!("{base}-{app_id}-{n}")
        };
        n += 1;
    }
    taken.insert(id.clone());
    id
}

/// Longueur maximale d'un id tiré d'un nom de jeu.
const MAX_SLUG: usize = 40;

/// Minuscules sans accents, chiffres et `-` : la règle des ids de config.json.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let folded = fold(c);
        if !folded.is_empty() {
            out.push_str(folded);
        } else if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if matches!(c, '\'' | '’' | '®' | '™' | '©') {
            // Apostrophes et marques : supprimées (« Baldur's » -> « baldurs »).
        } else if !out.ends_with('-') {
            // Tout le reste (espaces, ponctuation, autres alphabets) : un `-`.
            out.push('-');
        }
    }
    let mut out = out.trim_matches('-').to_string();
    if out.len() > MAX_SLUG {
        // Que de l'ASCII : couper à un octet précis est sans risque.
        out.truncate(MAX_SLUG);
        out = out.trim_end_matches('-').to_string();
    }
    out
}

/// Lettre accentuée -> lettres sans accent (vide si `c` n'en est pas une).
fn fold(c: char) -> &'static str {
    match c.to_lowercase().next().unwrap_or(c) {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'æ' => "ae",
        'ç' => "c",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
        'œ' => "oe",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        _ => "",
    }
}

/// Une catégorie sur une ligne, dans le style de la config d'exemple.
fn category_json(c: &Category) -> String {
    format!(
        "{{ \"id\": {}, \"name\": {} }}",
        quote(&c.id),
        quote(&c.name)
    )
}

/// Une tuile Steam sur une ligne, dans le style de la config d'exemple.
fn tile_json(t: &Tile) -> String {
    let app_id = match t.target {
        Target::Steam { app_id } => app_id,
        // `plan` ne crée que des tuiles Steam.
        _ => unreachable!("l'import Steam ne crée que des tuiles steam"),
    };
    format!(
        "{{ \"id\": {}, \"name\": {}, \"category\": {}, \"type\": \"steam\", \"appId\": {app_id} }}",
        quote(&t.id),
        quote(&t.name),
        quote(&t.category)
    )
}

/// Texte JSON entre guillemets (avec les échappements nécessaires).
fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("une chaîne est toujours sérialisable")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = include_str!("../config.default.json");
    const EXAMPLE: &str = include_str!("../config.example.json");

    fn game(app_id: u32, name: &str) -> SteamGame {
        SteamGame {
            app_id,
            name: name.into(),
        }
    }

    fn manifest(app_id: u32, name: &str, flags: u32) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"Universe\"\t\t\"1\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"x\"\n}}\n"
        )
    }

    #[test]
    fn lit_un_manifest_et_ecarte_ce_qui_n_est_pas_un_jeu() {
        assert_eq!(
            parse_manifest(&manifest(730, "Counter-Strike 2", 4)),
            Some(game(730, "Counter-Strike 2"))
        );
        // Mise à jour en attente (4 | 2) : toujours jouable.
        assert!(parse_manifest(&manifest(730, "CS2", 6)).is_some());
        // Premier téléchargement en cours (1024 | 2) : pas encore jouable.
        assert_eq!(parse_manifest(&manifest(730, "CS2", 1026)), None);
        assert_eq!(parse_manifest(&manifest(228980, "Redist", 4)), None);
        assert_eq!(parse_manifest("\"AppState\" { \"name\" \"x\" }"), None);
        assert_eq!(parse_manifest("pas un manifest {"), None);
        // Nom absent : on en invente un plutôt que d'ignorer le jeu.
        assert_eq!(
            parse_manifest("\"AppState\" { \"appid\" \"10\" }"),
            Some(game(10, "Jeu Steam 10"))
        );
    }

    #[test]
    fn trouve_les_jeux_de_toutes_les_bibliotheques() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let other = tmp.path().join("SteamLibrary");
        let old = tmp.path().join("Old");
        for lib in [&steam, &other, &old] {
            fs::create_dir_all(lib.join("steamapps")).unwrap();
        }
        // Chemins écrits comme Steam : barres obliques inverses doublées.
        let esc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            steam.join("steamapps").join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"contentstatsid\"\t\t\"-1\"\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\" {{ \"1086940\" \"1\" }}\n\t}}\n\t\"2\"\t\t\"{}\"\n\t\"3\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                esc(&steam),
                esc(&other),
                esc(&old),
                esc(&tmp.path().join("Débranché"))
            ),
        )
        .unwrap();
        let write = |lib: &Path, id: u32, name: &str, flags: u32| {
            fs::write(
                lib.join("steamapps").join(format!("appmanifest_{id}.acf")),
                manifest(id, name, flags),
            )
            .unwrap()
        };
        write(&steam, 730, "Counter-Strike 2", 4);
        write(&steam, 228980, "Steamworks Common Redistributables", 4);
        write(&other, 1086940, "Baldur's Gate 3", 6);
        write(&other, 730, "Counter-Strike 2", 4); // même jeu, autre disque
        write(&other, 999, "En téléchargement", 1026);
        write(&old, 367520, "hollow Knight", 4);
        fs::write(other.join("steamapps").join("libraryfolders.vdf"), "x").unwrap();

        assert_eq!(
            installed_games(&steam),
            [
                game(1086940, "Baldur's Gate 3"),
                game(730, "Counter-Strike 2"),
                game(367520, "hollow Knight"),
            ]
        );
    }

    #[test]
    fn sans_libraryfolders_lit_le_dossier_de_steam() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("steamapps")).unwrap();
        fs::write(
            tmp.path().join("steamapps").join("appmanifest_70.acf"),
            manifest(70, "Half-Life", 4),
        )
        .unwrap();
        assert_eq!(installed_games(tmp.path()), [game(70, "Half-Life")]);
        assert!(installed_games(&tmp.path().join("absent")).is_empty());
    }

    #[test]
    fn ids_tires_du_nom() {
        assert_eq!(slug("Baldur's Gate 3"), "baldurs-gate-3");
        assert_eq!(
            slug("  Ori and the Will of the Wisps™ "),
            "ori-and-the-will-of-the-wisps"
        );
        assert_eq!(
            slug("Clair Obscur: Expédition 33"),
            "clair-obscur-expedition-33"
        );
        assert_eq!(slug("ÆON — Œuvre"), "aeon-oeuvre");
        assert_eq!(slug("原神"), "");
        assert!(slug(&"a".repeat(80)).len() == MAX_SLUG);

        let mut taken: HashSet<String> = ["portal".to_string()].into();
        assert_eq!(unique_id("Portal", 400, &mut taken), "portal-400");
        assert_eq!(unique_id("Portal", 400, &mut taken), "portal-400-2");
        assert_eq!(unique_id("原神", 1, &mut taken), "steam-1");
    }

    fn parse(text: &str) -> Config {
        config::parse(text, Path::new("config.json")).unwrap()
    }

    #[test]
    fn premier_import_cree_la_categorie_jeux() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(config::FILE_NAME);
        // Pas de config.json : elle est créée (vide) puis complétée.
        let result = import(
            dir.path(),
            &[
                game(1086940, "Baldur's Gate 3"),
                game(730, "Counter-Strike 2"),
            ],
        )
        .unwrap();
        assert_eq!(result.added, ["baldurs-gate-3", "counter-strike-2"]);
        assert_eq!(result.category, "Jeux");

        let text = fs::read_to_string(&path).unwrap();
        let config = parse(&text);
        assert_eq!(config.categories.len(), 1);
        assert_eq!(config.tiles.len(), 2);
        assert_eq!(config.tiles[1].target, Target::Steam { app_id: 730 });
        // Seuls les deux tableaux vides ont changé ; une tuile par ligne.
        assert!(
            text.contains(
                "  \"categories\": [\n    { \"id\": \"jeux\", \"name\": \"Jeux\" }\n  ],\n"
            ),
            "{text}"
        );
        assert!(text.contains("\n    { \"id\": \"counter-strike-2\", \"name\": \"Counter-Strike 2\", \"category\": \"jeux\", \"type\": \"steam\", \"appId\": 730 }\n  ]\n}"), "{text}");
        let back = text
            .replace(
                &text[text.find("\"categories\"").unwrap()..text.find("\"favorites\"").unwrap()],
                "\"categories\": [],\n  ",
            )
            .replace(
                &text[text.find("\"tiles\"").unwrap()..],
                "\"tiles\": []\n}\n",
            );
        assert_eq!(back, DEFAULT);

        // Second import des mêmes jeux : rien n'est ajouté, fichier inchangé.
        let again = import(dir.path(), &[game(730, "Counter-Strike 2")]).unwrap();
        assert!(again.added.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn import_dans_une_config_existante_sans_doublon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(config::FILE_NAME);
        fs::write(&path, EXAMPLE).unwrap();
        let before = parse(EXAMPLE);

        let result = import(
            dir.path(),
            // 730 (cs2) est déjà dans l'exemple ; 1 apparaît deux fois.
            &[
                game(730, "CS2"),
                game(1, "Discord"),
                game(1, "Discord"),
                game(2, "Hades"),
            ],
        )
        .unwrap();
        // « discord » est déjà l'id d'une tuile : l'appId est ajouté.
        assert_eq!(result.added, ["discord-1", "hades"]);

        let after = parse(&fs::read_to_string(&path).unwrap());
        // L'exemple a déjà une catégorie `jeux` : pas de nouvelle catégorie.
        assert_eq!(after.categories, before.categories);
        assert_eq!(after.favorites, before.favorites);
        assert_eq!(after.settings, before.settings);
        assert_eq!(after.tiles[..before.tiles.len()], before.tiles[..]);
        assert_eq!(after.tiles.len(), before.tiles.len() + 2);
        assert!(
            after
                .tiles
                .iter()
                .skip(before.tiles.len())
                .all(|t| t.category == "jeux")
        );
    }

    #[test]
    fn reprend_une_categorie_nommee_jeux() {
        let config =
            parse(r#"{ "version": 1, "categories": [{ "id": "games", "name": " Jeux " }] }"#);
        let plan = plan(&config, &[game(5, "X")]);
        assert!(!plan.new_category);
        assert_eq!(plan.tiles[0].category, "games");
    }

    #[test]
    fn config_invalide_jamais_modifiee() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(config::FILE_NAME);
        fs::write(&path, "{ \"version\": 1,, }").unwrap();
        assert!(import(dir.path(), &[game(5, "X")]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"version\": 1,, }");
    }

    #[test]
    fn noms_avec_guillemets_et_antislash() {
        let dir = tempfile::tempdir().unwrap();
        let name = r#"Le "jeu" \ test"#;
        import(dir.path(), &[game(9, name)]).unwrap();
        let config = config::load_or_create(dir.path()).unwrap();
        assert_eq!(config.tiles[0].name, name);
        assert_eq!(config.tiles[0].id, "le-jeu-test");
    }
}
