//! Éditeur de tuiles : ajouter, modifier ou supprimer une tuile depuis la fenêtre.
//!
//! Comme pour les favoris et l'import Steam, on ne réécrit jamais tout
//! `config.json` : on ne touche qu'au texte de la tuile concernée (et, au
//! besoin, on ajoute une catégorie ou on retire un favori). Votre mise en forme,
//! vos lignes vides et l'ordre des champs restent tels quels ; le fichier
//! reste éditable à la main.
//!
//! Chaque opération suit les mêmes étapes :
//! 1. prendre le verrou d'écriture, lire le texte et le valider (un fichier
//!    invalide n'est jamais touché) ;
//! 2. calculer la config attendue après la modification ;
//! 3. modifier le texte (jsontext.rs) ;
//! 4. relire le nouveau texte : il doit donner *exactement* la config attendue,
//!    sinon rien n'est écrit ;
//! 5. l'écrire de façon atomique (`fsutil::write_atomic`).
//!
//! [`add_tiles`] accepte plusieurs tuiles d'un coup : l'écran d'accueil qui
//! détectera les apps installées pourra s'en servir.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::config::{self, Category, Config, Target, Tile, slug};
use crate::error::AppError;
use crate::favorites;
use crate::fsutil;
use crate::jsontext;
use crate::launch;

/// Une tuile telle que le formulaire l'envoie : tout sauf l'id, que Rust
/// choisit à partir du nom (à l'ajout) ou garde (à la modification).
///
/// `flatten` : les champs de la cible (`type`, `path`, `url`…) sont au même
/// niveau que `name`, exactement comme dans config.json.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TileDraft {
    pub name: String,
    /// Id d'une catégorie existante. Ignoré si `new_category` est rempli.
    #[serde(default)]
    pub category: String,
    /// Nom d'une catégorie à créer (ou à reprendre si une catégorie porte
    /// déjà ce nom). `Option` : le champ peut être absent.
    #[serde(default)]
    pub new_category: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(flatten)]
    pub target: Target,
}

// ---------------------------------------------------------------------------
// Les trois opérations
// ---------------------------------------------------------------------------

/// Ajoute une tuile par brouillon, à la fin de `"tiles"` (donc en dernier
/// dans sa catégorie). Renvoie les tuiles créées, avec leur id.
pub fn add_tiles(dir: &Path, drafts: &[TileDraft]) -> Result<Vec<Tile>, AppError> {
    let _guard = config::write_lock();
    let path = dir.join(config::FILE_NAME);
    let text = config::read_or_create_text(dir)?;
    let config = config::parse(&text, &path)?;

    let mut expected = config.clone();
    let mut new_categories = Vec::new();
    let mut taken: HashSet<String> = config.tiles.iter().map(|t| t.id.clone()).collect();
    let mut added = Vec::new();
    for draft in drafts {
        let (category, created) = resolve_category(&mut expected, draft)?;
        if created {
            new_categories.push(category.clone());
        }
        let id = unique_id(&slug(draft.name.trim()), "tuile", &mut taken);
        let tile = build_tile(id, category.id, draft)?;
        expected.tiles.push(tile.clone());
        added.push(tile);
    }
    if added.is_empty() {
        return Ok(added);
    }

    let mut new_text = text.clone();
    if !new_categories.is_empty() {
        let items: Vec<String> = new_categories.iter().map(category_json).collect();
        new_text = jsontext::append_to_array(&new_text, "categories", &items)
            .ok_or_else(|| not_found("categories"))?;
    }
    let items: Vec<String> = added.iter().map(tile_json).collect();
    new_text =
        jsontext::append_to_array(&new_text, "tiles", &items).ok_or_else(|| not_found("tiles"))?;

    write_checked(&path, &new_text, &expected)?;
    Ok(added)
}

/// Remplace la tuile `id` par le brouillon (l'id ne change pas : favoris,
/// récents et icône en cache restent attachés à la tuile).
///
/// Si la catégorie change, la tuile passe à la fin de `"tiles"` : elle
/// apparaît en dernier dans sa nouvelle catégorie. Sinon, elle garde sa place.
pub fn update_tile(dir: &Path, id: &str, draft: &TileDraft) -> Result<Tile, AppError> {
    let _guard = config::write_lock();
    let path = dir.join(config::FILE_NAME);
    let text = config::read_or_create_text(dir)?;
    let config = config::parse(&text, &path)?;

    let index = position(&config, id)?;
    let mut expected = config.clone();
    let (category, created) = resolve_category(&mut expected, draft)?;
    let tile = build_tile(id.to_string(), category.id.clone(), draft)?;
    if tile == config.tiles[index] {
        return Ok(tile); // rien n'a changé : on n'écrit pas
    }

    let mut new_text = text.clone();
    if created {
        new_text = jsontext::append_to_array(&new_text, "categories", &[category_json(&category)])
            .ok_or_else(|| not_found("categories"))?;
    }
    if tile.category == config.tiles[index].category {
        expected.tiles[index] = tile.clone();
        new_text = jsontext::replace_element(&new_text, "tiles", index, &tile_json(&tile))
            .ok_or_else(|| not_found("tiles"))?;
    } else {
        expected.tiles.remove(index);
        expected.tiles.push(tile.clone());
        new_text = jsontext::remove_element(&new_text, "tiles", index)
            .and_then(|t| jsontext::append_to_array(&t, "tiles", &[tile_json(&tile)]))
            .ok_or_else(|| not_found("tiles"))?;
    }

    write_checked(&path, &new_text, &expected)?;
    Ok(tile)
}

/// Supprime la tuile `id`, et la retire des favoris si besoin. Les catégories
/// ne sont jamais supprimées (même vides). Les récents (state.json) ignorent
/// déjà les tuiles disparues, et son icône en cache est effacée au prochain
/// rechargement (`icons::remove_orphans`).
pub fn delete_tile(dir: &Path, id: &str) -> Result<(), AppError> {
    let _guard = config::write_lock();
    let path = dir.join(config::FILE_NAME);
    let text = config::read_or_create_text(dir)?;
    let config = config::parse(&text, &path)?;

    let index = position(&config, id)?;
    let mut expected = config.clone();
    expected.tiles.remove(index);
    let mut new_text =
        jsontext::remove_element(&text, "tiles", index).ok_or_else(|| not_found("tiles"))?;
    if config.favorites.iter().any(|f| f == id) {
        // `retain` garde les éléments pour lesquels la closure renvoie `true`.
        expected.favorites.retain(|f| f != id);
        new_text = favorites::set_favorites(&new_text, &expected.favorites)
            .ok_or_else(|| not_found("favorites"))?;
    }

    write_checked(&path, &new_text, &expected)
}

// ---------------------------------------------------------------------------
// Outils
// ---------------------------------------------------------------------------

/// Index de la tuile `id` dans `config.tiles`.
fn position(config: &Config, id: &str) -> Result<usize, AppError> {
    config
        .tiles
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| AppError::UnknownTile(id.to_string()))
}

/// Dernier filet de sécurité, puis écriture : le nouveau texte doit donner
/// exactement `expected`. Sinon, c'est un bug, et on n'écrit rien.
fn write_checked(path: &Path, new_text: &str, expected: &Config) -> Result<(), AppError> {
    let check = config::parse(new_text, path)
        .map_err(|e| AppError::ConfigWrite(format!("modification refusée ({e})")))?;
    if &check != expected {
        return Err(AppError::ConfigWrite(
            "le fichier modifié ne correspond pas à la modification prévue".into(),
        ));
    }
    fsutil::write_atomic(path, new_text.as_bytes())
}

fn not_found(key: &str) -> AppError {
    AppError::ConfigWrite(format!(
        "le tableau \"{key}\" n'a pas pu être modifié dans le fichier"
    ))
}

/// Catégorie de la tuile : celle choisie, ou celle à créer. Une « nouvelle »
/// catégorie qui porte le nom d'une catégorie existante (sans tenir compte
/// des majuscules) la réutilise. Renvoie aussi `true` si elle a été créée
/// (elle est alors déjà ajoutée à `expected.categories`).
fn resolve_category(
    expected: &mut Config,
    draft: &TileDraft,
) -> Result<(Category, bool), AppError> {
    let new_name = draft.new_category.as_deref().map(str::trim).unwrap_or("");
    if new_name.is_empty() {
        return expected
            .categories
            .iter()
            .find(|c| c.id == draft.category)
            .map(|c| (c.clone(), false))
            .ok_or_else(|| invalid("choisissez une catégorie"));
    }
    let lower = new_name.to_lowercase();
    if let Some(c) = expected
        .categories
        .iter()
        .find(|c| c.name.trim().to_lowercase() == lower)
    {
        return Ok((c.clone(), false));
    }
    // Ids pris : catégories existantes + ceux réservés par l'interface.
    let mut taken: HashSet<String> = expected.categories.iter().map(|c| c.id.clone()).collect();
    taken.extend(config::RESERVED_CATEGORY_IDS.iter().map(|s| s.to_string()));
    let category = Category {
        id: unique_id(&slug(new_name), "categorie", &mut taken),
        name: new_name.to_string(),
    };
    expected.categories.push(category.clone());
    Ok((category, true))
}

/// Construit la tuile à partir du brouillon, en nettoyant et vérifiant les
/// champs (le formulaire vérifie déjà, mais Rust ne fait confiance à personne).
fn build_tile(id: String, category: String, draft: &TileDraft) -> Result<Tile, AppError> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(invalid("le nom est vide"));
    }
    let target = match &draft.target {
        Target::Steam { app_id } => {
            if *app_id == 0 {
                return Err(invalid(
                    "l'identifiant Steam (appId) doit être un nombre positif",
                ));
            }
            Target::Steam { app_id: *app_id }
        }
        Target::App { path, args } => {
            // Les guillemets autour d'un chemin copié depuis l'Explorateur
            // (« Copier en tant que chemin d'accès ») sont retirés.
            let path = path.trim().trim_matches('"').trim();
            if path.is_empty() {
                return Err(invalid("le chemin du programme est vide"));
            }
            Target::App {
                path: path.to_string(),
                // Gardés tels quels, même vides (`""` peut compter pour le programme).
                args: args.clone(),
            }
        }
        Target::Web { url } => {
            let url = url.trim();
            launch::check_web_url(url)?;
            Target::Web {
                url: url.to_string(),
            }
        }
    };
    // `filter` : une icône vide (champ effacé) revient à « pas d'icône ».
    let icon = draft
        .icon
        .as_deref()
        .map(|s| s.trim().trim_matches('"').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(Tile {
        id,
        name: name.to_string(),
        category,
        icon,
        target,
    })
}

fn invalid(message: &str) -> AppError {
    AppError::ConfigWrite(message.to_string())
}

/// `base` rendu unique parmi `taken` (`-2`, `-3`… en cas de collision), ou
/// `fallback` si le nom ne donne aucun id (nom en caractères non latins…).
fn unique_id(base: &str, fallback: &str, taken: &mut HashSet<String>) -> String {
    let base = if base.is_empty() { fallback } else { base };
    let mut id = base.to_string();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    taken.insert(id.clone());
    id
}

/// Une catégorie sur une ligne, dans le style de la config d'exemple.
pub(crate) fn category_json(c: &Category) -> String {
    format!(
        "{{ \"id\": {}, \"name\": {} }}",
        quote(&c.id),
        quote(&c.name)
    )
}

/// Une tuile sur une ligne, dans le style de la config d'exemple. Les champs
/// sont toujours dans le même ordre ; `args` et `icon` seulement s'ils servent.
pub(crate) fn tile_json(t: &Tile) -> String {
    let mut fields = vec![
        format!("\"id\": {}", quote(&t.id)),
        format!("\"name\": {}", quote(&t.name)),
        format!("\"category\": {}", quote(&t.category)),
    ];
    if let Some(icon) = &t.icon {
        fields.push(format!("\"icon\": {}", quote(icon)));
    }
    match &t.target {
        Target::Steam { app_id } => {
            fields.push("\"type\": \"steam\"".into());
            fields.push(format!("\"appId\": {app_id}"));
        }
        Target::App { path, args } => {
            fields.push("\"type\": \"app\"".into());
            fields.push(format!("\"path\": {}", quote(path)));
            if !args.is_empty() {
                let list: Vec<String> = args.iter().map(|a| quote(a)).collect();
                fields.push(format!("\"args\": [{}]", list.join(", ")));
            }
        }
        Target::Web { url } => {
            fields.push("\"type\": \"web\"".into());
            fields.push(format!("\"url\": {}", quote(url)));
        }
    }
    format!("{{ {} }}", fields.join(", "))
}

/// Texte JSON entre guillemets (avec les échappements nécessaires : `\\`, `\"`…).
fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("une chaîne est toujours sérialisable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const EXAMPLE: &str = include_str!("../config.example.json");
    const DEFAULT: &str = include_str!("../config.default.json");

    /// Un brouillon à partir de JSON, comme l'envoie le front.
    fn draft(json: &str) -> TileDraft {
        serde_json::from_str(json).unwrap()
    }

    fn setup(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(config::FILE_NAME);
        fs::write(&path, text).unwrap();
        (dir, path)
    }

    fn load(dir: &Path) -> Config {
        config::load_or_create(dir).unwrap()
    }

    #[test]
    fn ajoute_une_tuile_de_chaque_type() {
        let (dir, path) = setup(EXAMPLE);
        let added = add_tiles(
            dir.path(),
            &[
                draft(r#"{ "name": " Notepad++ ", "category": "outils", "type": "app", "path": "\"C:\\Tools\\notepad++.exe\"", "args": ["-multiInst"] }"#),
                draft(r#"{ "name": "Discord", "category": "web", "type": "web", "url": "https://discord.com/app" }"#),
                draft(r#"{ "name": "Portal 2", "category": "jeux", "type": "steam", "appId": 620, "icon": "  " }"#),
            ],
        )
        .unwrap();
        let ids: Vec<&str> = added.iter().map(|t| t.id.as_str()).collect();
        // « discord » existe déjà : la nouvelle tuile prend « discord-2 ».
        assert_eq!(ids, ["notepad", "discord-2", "portal-2"]);

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(
            r#"{ "id": "notepad", "name": "Notepad++", "category": "outils", "type": "app", "path": "C:\\Tools\\notepad++.exe", "args": ["-multiInst"] }"#
        ), "{text}");
        assert!(text.contains(r#"{ "id": "portal-2", "name": "Portal 2", "category": "jeux", "type": "steam", "appId": 620 }"#));
        // Le début du fichier n'a pas bougé.
        let end = EXAMPLE.rfind('}').unwrap();
        let last_tile = EXAMPLE[..end].rfind('}').unwrap();
        assert!(text.starts_with(&EXAMPLE[..=last_tile]));
        assert_eq!(
            load(dir.path()).tiles.len(),
            config::parse(EXAMPLE, &path).unwrap().tiles.len() + 3
        );
    }

    #[test]
    fn premiere_tuile_avec_nouvelle_categorie() {
        let (dir, path) = setup(DEFAULT);
        let added = add_tiles(
            dir.path(),
            &[
                draft(r#"{ "name": "YouTube", "newCategory": "Vidéo", "type": "web", "url": "https://www.youtube.com" }"#),
                // Même nouvelle catégorie (majuscules près) : créée une seule fois.
                draft(r#"{ "name": "Twitch", "newCategory": "vidéo ", "type": "web", "url": "https://twitch.tv" }"#),
            ],
        )
        .unwrap();
        assert_eq!(added[0].category, "video");
        assert_eq!(added[1].category, "video");
        let saved = load(dir.path());
        assert_eq!(
            saved.categories,
            [Category {
                id: "video".into(),
                name: "Vidéo".into()
            }]
        );
        assert_eq!(saved.tiles.len(), 2);
        let text = fs::read_to_string(&path).unwrap();
        // Les fins de ligne du fichier sont gardées.
        assert_eq!(text.contains("\r\n"), DEFAULT.contains("\r\n"));
    }

    #[test]
    fn categorie_reservee_ou_sans_nom_latin() {
        let (dir, _) = setup(DEFAULT);
        let added = add_tiles(
            dir.path(),
            &[
                draft(r#"{ "name": "A", "newCategory": "Favorites", "type": "web", "url": "https://a.fr" }"#),
                draft(r#"{ "name": "日本", "newCategory": "日本語", "type": "web", "url": "https://b.fr" }"#),
            ],
        )
        .unwrap();
        assert_eq!(added[0].category, "favorites-2");
        assert_eq!(added[1].category, "categorie");
        assert_eq!(added[1].id, "tuile");
    }

    #[test]
    fn refuse_un_brouillon_invalide_sans_rien_ecrire() {
        let (dir, path) = setup(EXAMPLE);
        for bad in [
            r#"{ "name": " ", "category": "web", "type": "web", "url": "https://a.fr" }"#,
            r#"{ "name": "A", "category": "inconnue", "type": "web", "url": "https://a.fr" }"#,
            r#"{ "name": "A", "category": "web", "type": "web", "url": "javascript:alert(1)" }"#,
            r#"{ "name": "A", "category": "web", "type": "app", "path": "  " }"#,
            r#"{ "name": "A", "category": "jeux", "type": "steam", "appId": 0 }"#,
        ] {
            assert!(add_tiles(dir.path(), &[draft(bad)]).is_err(), "{bad}");
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), EXAMPLE);
    }

    #[test]
    fn modifie_une_tuile_a_sa_place() {
        let (dir, path) = setup(EXAMPLE);
        let tile = update_tile(
            dir.path(),
            "bg3",
            &draft(r#"{ "name": "BG3", "category": "jeux", "type": "steam", "appId": 1086940, "icon": "C:\\img\\bg3.png" }"#),
        )
        .unwrap();
        assert_eq!(tile.id, "bg3");
        let text = fs::read_to_string(&path).unwrap();
        // Seule la ligne de la tuile a changé.
        assert_eq!(
            text.replace(
                r#"{ "id": "bg3", "name": "BG3", "category": "jeux", "icon": "C:\\img\\bg3.png", "type": "steam", "appId": 1086940 }"#,
                r#"{ "id": "bg3", "name": "Baldur's Gate 3", "category": "jeux", "type": "steam", "appId": 1086940 }"#
            ),
            EXAMPLE
        );
        // Même brouillon une seconde fois : rien à écrire.
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        update_tile(
            dir.path(),
            "bg3",
            &draft(r#"{ "name": "BG3", "category": "jeux", "type": "steam", "appId": 1086940, "icon": "C:\\img\\bg3.png" }"#),
        )
        .unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn changer_de_categorie_ou_de_type() {
        let (dir, _) = setup(EXAMPLE);
        // La tuile multi-lignes « steam » devient une app web, dans « web ».
        update_tile(
            dir.path(),
            "steam",
            &draft(r#"{ "name": "Steam Store", "category": "web", "type": "web", "url": "https://store.steampowered.com" }"#),
        )
        .unwrap();
        let saved = load(dir.path());
        let last = saved.tiles.last().unwrap();
        assert_eq!(last.id, "steam");
        assert_eq!(last.category, "web");
        assert!(matches!(last.target, Target::Web { .. }));
        // Toujours en favori si elle l'était, et le reste est intact.
        let original = config::parse(EXAMPLE, Path::new("c")).unwrap();
        assert_eq!(saved.favorites, original.favorites);
        assert_eq!(saved.tiles.len(), original.tiles.len());

        assert!(matches!(
            update_tile(
                dir.path(),
                "inconnue",
                &draft(
                    r#"{ "name": "A", "category": "web", "type": "web", "url": "https://a.fr" }"#
                )
            ),
            Err(AppError::UnknownTile(_))
        ));
    }

    #[test]
    fn supprime_une_tuile_et_son_favori() {
        let (dir, path) = setup(EXAMPLE);
        delete_tile(dir.path(), "discord").unwrap();
        let saved = load(dir.path());
        assert!(saved.tiles.iter().all(|t| t.id != "discord"));
        assert_eq!(saved.favorites, ["cs2", "instagram"]);
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("Discord"));
        assert!(text.contains(r#""favorites": ["cs2", "instagram"]"#));

        // Supprimer toutes les tuiles laisse `"tiles": []` et une config valide.
        for t in saved.tiles {
            delete_tile(dir.path(), &t.id).unwrap();
        }
        let saved = load(dir.path());
        assert!(saved.tiles.is_empty() && saved.favorites.is_empty());
        assert_eq!(saved.categories.len(), 3);
        assert!(matches!(
            delete_tile(dir.path(), "cs2"),
            Err(AppError::UnknownTile(_))
        ));
    }

    #[test]
    fn config_invalide_jamais_modifiee() {
        let broken = EXAMPLE.replacen("\"version\": 1,", "\"version\": 1,,", 1);
        let (dir, path) = setup(&broken);
        assert!(delete_tile(dir.path(), "cs2").is_err());
        assert!(
            add_tiles(
                dir.path(),
                &[draft(
                    r#"{ "name": "A", "category": "web", "type": "web", "url": "https://a.fr" }"#
                )]
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
    }

    #[test]
    fn noms_avec_guillemets_et_antislash() {
        let (dir, _) = setup(EXAMPLE);
        let added = add_tiles(
            dir.path(),
            &[draft(r#"{ "name": "Le \"jeu\" \\ 2", "category": "jeux", "type": "app", "path": "D:\\Jeux\\a b\\jeu.exe", "args": ["--x=\"y\""] }"#)],
        )
        .unwrap();
        let saved = load(dir.path());
        assert_eq!(saved.tiles.last().unwrap(), &added[0]);
        assert_eq!(added[0].name, "Le \"jeu\" \\ 2");
        assert_eq!(added[0].id, "le-jeu-2");
    }
}
