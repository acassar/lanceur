//! Retouches *dans le texte* de `config.json`, sans réécrire le reste du fichier.
//!
//! `config.json` est aussi votre fichier, édité à la main : quand l'app le
//! modifie (favoris, import Steam), on ne veut perdre ni sa mise en forme, ni
//! l'ordre des champs. Plutôt que de convertir la config en JSON et de tout
//! réécrire, on repère *où* se trouve un tableau dans le texte, et on ne change
//! que ce morceau ; tout le reste est recopié octet pour octet.
//!
//! Utilisé par favorites.rs (remplacer `"favorites"`) et steam.rs (ajouter des
//! éléments à la fin de `"categories"` et de `"tiles"`).

use std::collections::HashMap;

use serde_json::value::RawValue;

/// Position (en octets) de `part` dans `text`. `part` doit être un morceau de
/// `text` (ce que garantit un `RawValue` emprunté) : on soustrait leurs adresses.
pub fn offset(text: &str, part: &str) -> usize {
    part.as_ptr() as usize - text.as_ptr() as usize
}

/// Début de la ligne qui contient la position `pos`.
pub fn line_start(text: &str, pos: usize) -> usize {
    // `rfind` cherche depuis la fin ; `map(|i| i + 1)` saute le `\n` trouvé.
    text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Retrait (espaces et tabulations) au début de la ligne qui contient `pos`.
pub fn indent_at(text: &str, pos: usize) -> String {
    let line = &text[line_start(text, pos)..];
    let len = line.len() - line.trim_start_matches([' ', '\t']).len();
    line[..len].to_string()
}

/// Un cran de retrait de plus que `outer` : une tabulation si le fichier est
/// indenté avec des tabulations, sinon deux espaces.
pub fn one_more(outer: &str) -> String {
    if outer.contains('\t') {
        format!("{outer}\t")
    } else {
        format!("{outer}  ")
    }
}

/// Fins de ligne du fichier : `\r\n` s'il a été écrit sous Windows.
pub fn newline(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Position du `"` ouvrant de la clé dont la valeur commence en `value`.
/// Entre la clé et la valeur, le JSON n'autorise que des espaces et un `:`.
pub fn key_start(text: &str, value: usize) -> Option<usize> {
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

/// Ajoute `items` (des valeurs JSON déjà écrites, une par ligne) à la fin du
/// tableau `key` de l'objet racine. Renvoie le nouveau texte, ou `None` si le
/// fichier n'a pas la forme attendue (rien n'est alors écrit).
///
/// Le style du fichier est suivi :
/// - tableau non vide : les éléments sont ajoutés après le dernier, avec le
///   même retrait (ou sur la même ligne si le tableau tient sur une ligne) ;
/// - tableau vide `[]` : un élément par ligne, sauf si tout le fichier tient
///   sur une ligne ;
/// - tableau absent : il est ajouté juste après `"version"`.
pub fn append_to_array(text: &str, key: &str, items: &[String]) -> Option<String> {
    if items.is_empty() {
        return Some(text.to_string());
    }
    // Même traitement du BOM que `config::parse`. `body` est un morceau de
    // `text` : les positions calculées plus bas restent valables dans `text`.
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Les champs de premier niveau, chacun sous forme de texte brut *emprunté*
    // à `body` (aucune copie) : on connaît ainsi leur position exacte.
    let root: HashMap<String, &RawValue> = serde_json::from_str(body).ok()?;
    let nl = newline(text);
    let multiline = body.trim().contains('\n');

    let (at, end, insert) = match root.get(key) {
        Some(raw) => {
            let start = offset(text, raw.get());
            let elements: Vec<&RawValue> = serde_json::from_str(raw.get()).ok()?;
            match elements.last() {
                // Tableau non vide : on insère juste après le dernier élément
                // (avant les blancs et le `]`, qui restent en place).
                Some(last) => {
                    let last_start = offset(text, last.get());
                    let last_end = last_start + last.get().len();
                    let sep = if raw.get().contains('\n') {
                        // Retrait du dernier élément s'il commence sa ligne,
                        // sinon (`[{` sur la ligne de la clé) un cran de plus que la clé.
                        let own_line = text[line_start(text, last_start)..last_start]
                            .trim()
                            .is_empty();
                        let indent = match own_line {
                            true => indent_at(text, last_start),
                            false => one_more(&indent_at(text, start)),
                        };
                        format!(",{nl}{indent}")
                    } else {
                        ", ".to_string()
                    };
                    let insert: String = items.iter().map(|i| format!("{sep}{i}")).collect();
                    (last_end, last_end, insert)
                }
                // Tableau vide : on le remplace entièrement.
                None => {
                    let array = new_array(text, start, items, nl, multiline);
                    (start, start + raw.get().len(), array)
                }
            }
        }
        // Tableau absent : on l'ajoute juste après la valeur de `"version"`
        // (toujours présent : il est obligatoire).
        None => {
            let version = root.get("version")?;
            let value = offset(text, version.get());
            let key_pos = key_start(text, value)?;
            let array = new_array(text, key_pos, items, nl, multiline);
            let quoted = serde_json::to_string(key).ok()?;
            let own_line = text[line_start(text, key_pos)..key_pos].trim().is_empty();
            let insert = if multiline && own_line {
                format!(",{nl}{}{quoted}: {array}", indent_at(text, key_pos))
            } else {
                format!(", {quoted}: {array}")
            };
            let end = value + version.get().len();
            (end, end, insert)
        }
    };
    // `[..at]` et `[end..]` : les morceaux avant et après, recopiés tels quels.
    Some(format!("{}{insert}{}", &text[..at], &text[end..]))
}

/// Un nouveau tableau contenant `items`, pour la clé située sur la ligne de `pos`.
fn new_array(text: &str, pos: usize, items: &[String], nl: &str, multiline: bool) -> String {
    if !multiline {
        return format!("[{}]", items.join(", "));
    }
    let outer = indent_at(text, pos);
    let inner = one_more(&outer);
    let lines: Vec<String> = items.iter().map(|i| format!("{inner}{i}")).collect();
    format!("[{nl}{}{nl}{outer}]", lines.join(&format!(",{nl}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn remplit_un_tableau_vide_un_element_par_ligne() {
        let text = "{\r\n  \"version\": 1,\r\n  \"tiles\": []\r\n}\r\n";
        let new =
            append_to_array(text, "tiles", &items(&["{ \"a\": 1 }", "{ \"b\": 2 }"])).unwrap();
        assert_eq!(
            new,
            "{\r\n  \"version\": 1,\r\n  \"tiles\": [\r\n    { \"a\": 1 },\r\n    { \"b\": 2 }\r\n  ]\r\n}\r\n"
        );
    }

    #[test]
    fn ajoute_apres_le_dernier_element_avec_le_meme_retrait() {
        let text = "{\n\t\"version\": 1,\n\t\"tiles\": [\n\t\t{ \"a\": 1 }\n\t]\n}";
        let new = append_to_array(text, "tiles", &items(&["{ \"b\": 2 }"])).unwrap();
        assert_eq!(
            new,
            "{\n\t\"version\": 1,\n\t\"tiles\": [\n\t\t{ \"a\": 1 },\n\t\t{ \"b\": 2 }\n\t]\n}"
        );
    }

    #[test]
    fn element_qui_commence_sur_la_ligne_du_crochet() {
        let text = "{\n  \"version\": 1,\n  \"tiles\": [{\n    \"a\": 1\n  }]\n}";
        let new = append_to_array(text, "tiles", &items(&["{ \"b\": 2 }"])).unwrap();
        assert!(new.contains("  },\n    { \"b\": 2 }]"), "{new}");
    }

    #[test]
    fn tableau_sur_une_ligne_et_fichier_compact() {
        let text = "{\n  \"version\": 1,\n  \"categories\": [{ \"id\": \"a\" }]\n}";
        let new = append_to_array(text, "categories", &items(&["{ \"id\": \"b\" }"])).unwrap();
        assert!(
            new.contains("[{ \"id\": \"a\" }, { \"id\": \"b\" }]"),
            "{new}"
        );

        let text = "\u{feff}{\"version\":1,\"tiles\":[]}";
        let new = append_to_array(text, "tiles", &items(&["1", "2"])).unwrap();
        assert_eq!(new, "\u{feff}{\"version\":1,\"tiles\":[1, 2]}");
    }

    #[test]
    fn ajoute_le_tableau_absent_apres_version() {
        let text = "{\n  \"version\": 1,\n  \"settings\": {}\n}";
        let new = append_to_array(text, "tiles", &items(&["1"])).unwrap();
        assert_eq!(
            new,
            "{\n  \"version\": 1,\n  \"tiles\": [\n    1\n  ],\n  \"settings\": {}\n}"
        );
        let new = append_to_array("{\"version\":1}", "tiles", &items(&["1"])).unwrap();
        assert_eq!(new, "{\"version\":1, \"tiles\": [1]}");
    }

    #[test]
    fn refuse_un_fichier_inattendu() {
        assert_eq!(append_to_array("[]", "tiles", &items(&["1"])), None);
        assert_eq!(
            append_to_array("{\"tiles\": {}}", "tiles", &items(&["1"])),
            None
        );
        assert_eq!(append_to_array("{}", "tiles", &items(&["1"])), None);
    }

    #[test]
    fn retrouve_la_cle_avant_la_valeur() {
        let text = r#"{ "a\"b" : 1 }"#;
        let value = text.find('1').unwrap();
        assert_eq!(key_start(text, value), Some(2));
        assert_eq!(key_start("[1]", 1), None);
    }
}
