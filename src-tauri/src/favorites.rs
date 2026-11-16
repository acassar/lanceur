//! Favoris : ajout ou retrait d'une tuile dans `"favorites"` de `config.json`.
//!
//! `config.json` est aussi *votre* fichier, édité à la main : on ne veut ni
//! perdre sa mise en forme (tuiles sur une ligne, lignes vides, ordre des
//! champs), ni réécrire tout le fichier pour changer une liste. On ne remplace
//! donc *que le texte du tableau* `"favorites": [ … ]`, octet pour octet ;
//! tout le reste du fichier est recopié tel quel.
//!
//! Le tableau garde son style : sur une seule ligne, ou un id par ligne
//! (même retrait). Un tableau vidé devient `[]`.
//!
//! Étapes de [`set`] :
//! 1. lire le texte et le valider (un fichier invalide n'est jamais touché) ;
//! 2. calculer la nouvelle liste ;
//! 3. remplacer le tableau dans le texte (ou l'ajouter s'il n'existe pas) ;
//! 4. revalider le nouveau texte, par sécurité ;
//! 5. l'écrire de façon atomique (`fsutil::write_atomic`).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::config;
use crate::error::AppError;
use crate::fsutil;

/// Verrou : deux clics très rapprochés ne doivent pas lire le même fichier
/// puis l'écrire chacun de leur côté (le second effacerait le premier).
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Met `id` en favori (`favorite = true`, ajouté à la fin) ou l'en retire.
/// Renvoie la nouvelle liste, que le front affiche aussitôt.
///
/// Le front envoie l'état *voulu* (celui que montre l'étoile) plutôt que
/// « inverse ce qui est sur le disque » : un double-clic, ou un fichier modifié
/// à la main pendant que la fenêtre est ouverte, ne peuvent donc pas faire
/// l'inverse de ce que l'utilisateur voit. Répéter le même appel ne change rien.
pub fn set(dir: &Path, id: &str, favorite: bool) -> Result<Vec<String>, AppError> {
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let path = dir.join(config::FILE_NAME);
    // On a besoin du *texte* du fichier : c'est lui qu'on va modifier.
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        // Premier démarrage : on crée la config d'exemple, puis on la lit.
        Err(e) if e.kind() == ErrorKind::NotFound => {
            config::load_or_create(dir)?;
            config::read(&path)?
        }
        // Les autres erreurs (encodage, droits…) : même message que `get_config`.
        Err(_) => config::read(&path)?,
    };
    let config = config::parse(&text, &path)?;

    if !config.tiles.iter().any(|t| t.id == id) {
        return Err(AppError::UnknownTile(id.to_string()));
    }

    let mut favorites = config.favorites.clone();
    // `contains` : déjà favori ? Si l'état voulu est déjà là, rien à écrire.
    if favorites.iter().any(|f| f == id) == favorite {
        return Ok(favorites);
    }
    if favorite {
        favorites.push(id.to_string());
    } else {
        // `retain` garde les éléments pour lesquels la closure renvoie `true`.
        favorites.retain(|f| f != id);
    }

    let new_text = set_favorites(&text, &favorites).ok_or_else(|| {
        AppError::ConfigWrite("le tableau \"favorites\" est introuvable dans le fichier".into())
    })?;

    // Filet de sécurité : le nouveau texte doit donner exactement la même
    // config, aux favoris près. Sinon, c'est un bug : on n'écrit rien.
    let mut check = config::parse(&new_text, &path)
        .map_err(|e| AppError::ConfigWrite(format!("modification refusée ({e})")))?;
    if check.favorites != favorites {
        return Err(AppError::ConfigWrite("favoris mal réécrits".into()));
    }
    // `clone()` : `config` reste entier pour la comparaison juste après
    // (déplacer `config.favorites` le rendrait partiellement inutilisable).
    check.favorites = config.favorites.clone();
    if check != config {
        return Err(AppError::ConfigWrite(
            "le reste de la config aurait changé".into(),
        ));
    }

    fsutil::write_atomic(&path, new_text.as_bytes())?;
    Ok(favorites)
}

// ---------------------------------------------------------------------------
// Remplacement du tableau dans le texte
// ---------------------------------------------------------------------------

/// Les deux champs de premier niveau dont on veut la *position* dans le texte.
///
/// `&'a RawValue` : au lieu de convertir la valeur, serde_json nous donne le
/// morceau de texte JSON brut, *emprunté* au texte d'origine (aucune copie).
/// `'a` est une *durée de vie* : elle dit au compilateur que ces morceaux ne
/// peuvent pas vivre plus longtemps que le texte qu'ils empruntent.
/// `borrow` demande à serde de faire cet emprunt. Les autres champs du JSON
/// sont ignorés (pas de `deny_unknown_fields` ici).
#[derive(Deserialize)]
struct Spans<'a> {
    #[serde(borrow, default)]
    favorites: Option<&'a RawValue>,
    #[serde(borrow, default)]
    tiles: Option<&'a RawValue>,
}

/// Renvoie `text` avec `"favorites"` remplacé par `ids`, ou `None` si la
/// structure du fichier n'est pas celle attendue.
fn set_favorites(text: &str, ids: &[String]) -> Option<String> {
    // Même traitement du BOM que `config::parse`. `body` est un morceau de
    // `text` : les positions calculées plus bas restent valables dans `text`.
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let spans: Spans = serde_json::from_str(body).ok()?;
    // Fins de ligne du fichier : `\r\n` s'il a été écrit sous Windows.
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };

    let (start, end, replacement) = match spans.favorites {
        // Cas courant : on remplace le tableau existant, en gardant son style
        // (sur une ligne, ou un id par ligne).
        Some(raw) => {
            let start = offset(text, raw.get());
            let end = start + raw.get().len();
            (start, end, format_array(text, start, raw.get(), ids, nl))
        }
        // Pas de `"favorites"` : on l'ajoute juste avant `"tiles"`, avec la
        // même indentation. (`tiles` existe forcément : la tuile y est.)
        None => {
            let key = key_start(text, offset(text, spans.tiles?.get()))?;
            let line = line_start(text, key);
            let array = format_array(text, key, "[]", ids, nl);
            let insert = if text[line..key].trim().is_empty() {
                // `"tiles"` est seul sur sa ligne : nouvelle ligne, même retrait.
                format!("\"favorites\": {array},{nl}{}", &text[line..key])
            } else {
                // Fichier compact, tout sur une ligne.
                format!("\"favorites\": {array}, ")
            };
            (key, key, insert)
        }
    };

    // `[..start]` et `[end..]` : les morceaux avant et après, recopiés tels quels.
    Some(format!("{}{replacement}{}", &text[..start], &text[end..]))
}

/// Position (en octets) de `part` dans `text`. `part` doit être un morceau de
/// `text` (ce que garantit `RawValue` emprunté) : on soustrait leurs adresses.
fn offset(text: &str, part: &str) -> usize {
    part.as_ptr() as usize - text.as_ptr() as usize
}

/// Écrit le tableau JSON des ids, dans le style de l'ancien tableau `old`.
fn format_array(text: &str, start: usize, old: &str, ids: &[String], nl: &str) -> String {
    // `serde_json::to_string` met les guillemets (et échapperait au besoin).
    let quoted: Vec<String> = ids
        .iter()
        .map(|id| serde_json::to_string(id).expect("une chaîne est toujours sérialisable"))
        .collect();

    if quoted.is_empty() {
        return "[]".to_string();
    }
    if !old.contains('\n') {
        // Sur une seule ligne, comme dans la config d'exemple.
        return format!("[{}]", quoted.join(", "));
    }

    // Un id par ligne. Le `]` reprend le retrait de la ligne du `[`, et les
    // ids celui du premier élément de l'ancien tableau (sinon 2 espaces de plus).
    let outer = indent_at(text, start);
    let inside = &old[1..]; // après le `[`
    let first = inside.trim_start(); // à partir du premier élément (ou du `]`)
    let skipped = &inside[..inside.len() - first.len()];
    let inner = if skipped.contains('\n') && !first.starts_with(']') {
        // Position du premier élément dans `text` : `start` + sa place dans `old`.
        indent_at(text, start + old.len() - first.len())
    } else {
        format!("{outer}  ")
    };
    let lines: Vec<String> = quoted.iter().map(|q| format!("{inner}{q}")).collect();
    format!("[{nl}{}{nl}{outer}]", lines.join(&format!(",{nl}")))
}

/// Début de la ligne qui contient la position `pos`.
fn line_start(text: &str, pos: usize) -> usize {
    // `rfind` cherche depuis la fin ; `map(|i| i + 1)` saute le `\n` trouvé.
    text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Retrait (espaces et tabulations) au début de la ligne qui contient `pos`.
fn indent_at(text: &str, pos: usize) -> String {
    let line = &text[line_start(text, pos)..];
    let len = line.len() - line.trim_start_matches([' ', '\t']).len();
    line[..len].to_string()
}

/// Position du `"` ouvrant de la clé dont la valeur commence en `value`.
/// Entre la clé et la valeur, le JSON n'autorise que des espaces et un `:`.
fn key_start(text: &str, value: usize) -> Option<usize> {
    let before = text[..value].trim_end().strip_suffix(':')?.trim_end();
    // `before` se termine par le `"` fermant de la clé : on cherche le `"`
    // ouvrant, en sautant les `\"` (guillemets échappés dans la clé).
    let mut end = before.len().checked_sub(1)?;
    if !before[end..].starts_with('"') {
        return None;
    }
    loop {
        let quote = before[..end].rfind('"')?;
        let backslashes = before[..quote]
            .bytes()
            .rev()
            .take_while(|&b| b == b'\\')
            .count();
        if backslashes % 2 == 0 {
            return Some(quote);
        }
        end = quote;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const EXAMPLE: &str = include_str!("../config.example.json");

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn favorites_of(text: &str) -> Vec<String> {
        config::parse(text, Path::new("config.json"))
            .unwrap()
            .favorites
    }

    #[test]
    fn ne_change_que_la_ligne_des_favoris() {
        let new = set_favorites(EXAMPLE, &ids(&["cs2", "bg3"])).unwrap();
        assert_eq!(favorites_of(&new), ["cs2", "bg3"]);
        // En remettant l'ancienne ligne, on retrouve le fichier d'origine à l'octet près.
        let back = new.replace(
            r#""favorites": ["cs2", "bg3"]"#,
            r#""favorites": ["cs2", "discord", "instagram"]"#,
        );
        assert_eq!(back, EXAMPLE);
    }

    #[test]
    fn liste_vide() {
        let new = set_favorites(EXAMPLE, &[]).unwrap();
        assert!(new.contains(r#""favorites": [],"#), "{new}");
        assert!(favorites_of(&new).is_empty());
    }

    const MULTI: &str = "{\r\n  \"version\": 1,\r\n  \"categories\": [{ \"id\": \"c\", \"name\": \"C\" }],\r\n  \"favorites\": [\r\n      \"a\"\r\n  ],\r\n  \"tiles\": [\r\n    { \"id\": \"a\", \"name\": \"A\", \"category\": \"c\", \"type\": \"web\", \"url\": \"https://a\" },\r\n    { \"id\": \"b\", \"name\": \"B\", \"category\": \"c\", \"type\": \"web\", \"url\": \"https://b\" }\r\n  ]\r\n}\r\n";

    #[test]
    fn garde_un_id_par_ligne_et_les_fins_de_ligne_windows() {
        let new = set_favorites(MULTI, &ids(&["a", "b"])).unwrap();
        assert!(
            new.contains("\"favorites\": [\r\n      \"a\",\r\n      \"b\"\r\n  ],\r\n"),
            "{new:?}"
        );
        assert_eq!(favorites_of(&new), ["a", "b"]);
    }

    #[test]
    fn ajoute_le_champ_avant_tiles_s_il_manque() {
        let text = MULTI.replace("  \"favorites\": [\r\n      \"a\"\r\n  ],\r\n", "");
        let new = set_favorites(&text, &ids(&["b"])).unwrap();
        assert!(
            new.contains("}],\r\n  \"favorites\": [\"b\"],\r\n  \"tiles\": [\r\n"),
            "{new:?}"
        );
        assert_eq!(favorites_of(&new), ["b"]);
    }

    #[test]
    fn fichier_compact_bom_et_cle_echappee() {
        // BOM en tête, tout sur une ligne, et une clé écrite `"\u0074iles"`
        // (= "tiles" : valide en JSON, même si personne n'écrit ça).
        let text = "\u{feff}{\"version\":1,\"categories\":[{\"id\":\"c\",\"name\":\"C\"}],\"\\u0074iles\":[{\"id\":\"a\",\"name\":\"A\",\"category\":\"c\",\"type\":\"web\",\"url\":\"u\"}]}";
        let new = set_favorites(text, &ids(&["a"])).unwrap();
        assert!(new.starts_with('\u{feff}'));
        assert!(
            new.contains("\"favorites\": [\"a\"], \"\\u0074iles\""),
            "{new}"
        );
        assert_eq!(favorites_of(&new), ["a"]);
    }

    #[test]
    fn ajoute_puis_retire_et_persiste() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(config::FILE_NAME);
        // Pas de fichier : la config d'exemple est créée.
        assert_eq!(
            set(dir.path(), "bg3", true).unwrap(),
            ["cs2", "discord", "instagram", "bg3"]
        );
        assert_eq!(
            set(dir.path(), "cs2", false).unwrap(),
            ["discord", "instagram", "bg3"]
        );
        // Même demande deux fois (double-clic) : rien ne change.
        assert_eq!(
            set(dir.path(), "cs2", false).unwrap(),
            ["discord", "instagram", "bg3"]
        );
        // Relu depuis le disque, comme après un redémarrage.
        let saved = config::load_or_create(dir.path()).unwrap();
        assert_eq!(saved.favorites, ["discord", "instagram", "bg3"]);
        // Seul le tableau a changé : le reste du fichier est celui d'origine.
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text.replace(
                r#"["discord", "instagram", "bg3"]"#,
                r#"["cs2", "discord", "instagram"]"#
            ),
            EXAMPLE
        );
        // Ni fichier temporaire, ni state.json : seulement config.json.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn tuile_inconnue_ou_config_invalide_ne_touchent_pas_au_fichier() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(config::FILE_NAME);
        fs::write(&path, EXAMPLE).unwrap();
        assert!(matches!(
            set(dir.path(), "inconnue", true),
            Err(AppError::UnknownTile(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), EXAMPLE);

        let broken = EXAMPLE.replacen("\"version\": 1,", "\"version\": 1,,", 1);
        fs::write(&path, &broken).unwrap();
        assert!(matches!(
            set(dir.path(), "bg3", true),
            Err(AppError::Json { .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
    }

    #[test]
    fn retrouve_la_cle_avant_la_valeur() {
        let text = r#"{ "a\"b" : 1 }"#;
        let value = text.find('1').unwrap();
        assert_eq!(key_start(text, value), Some(2));
        assert_eq!(key_start("[1]", 1), None);
    }
}
