//! Retouches *dans le texte* de `config.json`, sans réécrire le reste du fichier.
//!
//! `config.json` est aussi votre fichier, édité à la main : quand l'app le
//! modifie (favoris, import Steam), on ne veut perdre ni sa mise en forme, ni
//! l'ordre des champs. Plutôt que de convertir la config en JSON et de tout
//! réécrire, on repère *où* se trouve un tableau dans le texte, et on ne change
//! que ce morceau ; tout le reste est recopié octet pour octet.
//!
//! Utilisé par favorites.rs (remplacer `"favorites"`), steam.rs (ajouter des
//! éléments à la fin de `"categories"` et de `"tiles"`) et editor.rs
//! (ajouter, remplacer ou retirer une tuile).

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

/// Positions (début, fin) des éléments du tableau `key` de l'objet racine.
/// `None` si le fichier n'a pas la forme attendue ou si le tableau manque.
fn element_spans(text: &str, key: &str) -> Option<Vec<(usize, usize)>> {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let root: HashMap<String, &RawValue> = serde_json::from_str(body).ok()?;
    let elements: Vec<&RawValue> = serde_json::from_str(root.get(key)?.get()).ok()?;
    Some(
        elements
            .iter()
            .map(|e| {
                let start = offset(text, e.get());
                (start, start + e.get().len())
            })
            .collect(),
    )
}

/// Remplace l'élément n°`index` du tableau `key` par `item` (du JSON déjà
/// écrit). Tout le reste du fichier, y compris les blancs autour de
/// l'élément, est recopié tel quel.
pub fn replace_element(text: &str, key: &str, index: usize, item: &str) -> Option<String> {
    let (start, end) = *element_spans(text, key)?.get(index)?;
    Some(format!("{}{item}{}", &text[..start], &text[end..]))
}

/// Retire l'élément n°`index` du tableau `key`, avec la virgule et les blancs
/// qui le séparent de son voisin. Le dernier élément retiré laisse `[]`.
pub fn remove_element(text: &str, key: &str, index: usize) -> Option<String> {
    let spans = element_spans(text, key)?;
    let (start, end) = *spans.get(index)?;
    let (cut_start, cut_end) = match (index.checked_sub(1), spans.get(index + 1)) {
        // Pas le premier : on retire depuis la fin du précédent (sa virgule,
        // le saut de ligne et le retrait qui suivent) jusqu'à la fin de celui-ci.
        (Some(prev), _) => (spans[prev].1, end),
        // Le premier, suivi d'un autre : de son début au début du suivant.
        (None, Some(&(next, _))) => (start, next),
        // Le seul : le tableau devient `[]`.
        (None, None) => {
            let open = text[..start].rfind('[')?;
            let close = end + text[end..].find(']')?;
            return Some(format!("{}[]{}", &text[..open], &text[close + 1..]));
        }
    };
    Some(format!("{}{}", &text[..cut_start], &text[cut_end..]))
}

/// Réordonne les éléments du tableau `key` : la place n°`i` reçoit l'élément
/// qui était n°`order[i]`. Les séparateurs (virgules, sauts de ligne, lignes
/// vides, retraits) restent à leur place ; seuls les éléments changent de case.
/// `order` doit être une permutation de `0..n`, sinon `None`.
pub fn permute_elements(text: &str, key: &str, order: &[usize]) -> Option<String> {
    let spans = element_spans(text, key)?;
    let mut seen = vec![false; spans.len()];
    if order.len() != spans.len()
        || order
            .iter()
            .any(|&i| i >= spans.len() || std::mem::replace(&mut seen[i], true))
    {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut copied = 0; // tout ce qui précède `copied` est déjà dans `out`
    for (slot, &(start, end)) in spans.iter().enumerate() {
        let (src_start, src_end) = spans[order[slot]];
        out.push_str(&text[copied..start]);
        out.push_str(&text[src_start..src_end]);
        copied = end;
    }
    out.push_str(&text[copied..]);
    Some(out)
}

/// Donne la valeur `value` (du JSON déjà écrit) au champ `field` de l'objet
/// `object` de l'objet racine (par exemple `"settings"` → `"hotkey"`).
///
/// - Champ présent : seule sa valeur est remplacée, la clé et les blancs autour
///   restent tels quels.
/// - Champ absent : il est ajouté après le dernier champ de l'objet, avec le
///   même retrait (ou sur la même ligne si l'objet tient sur une ligne).
/// - Objet absent : il est ajouté juste après `"version"`.
///
/// Renvoie `None` si le fichier n'a pas la forme attendue (rien n'est alors écrit).
pub fn set_object_field(text: &str, object: &str, field: &str, value: &str) -> Option<String> {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let root: HashMap<String, &RawValue> = serde_json::from_str(body).ok()?;
    let nl = newline(text);
    let multiline = body.trim().contains('\n');
    let quoted = serde_json::to_string(field).ok()?;

    let (at, end, insert) = match root.get(object) {
        Some(raw) => {
            let start = offset(text, raw.get());
            // Les champs de l'objet, chacun avec sa position dans le texte.
            let fields: HashMap<String, &RawValue> = serde_json::from_str(raw.get()).ok()?;
            if let Some(current) = fields.get(field) {
                // Champ présent : on ne remplace que sa valeur.
                let at = offset(text, current.get());
                (at, at + current.get().len(), value.to_string())
            } else if let Some(last) = fields.values().max_by_key(|v| offset(text, v.get())) {
                // Champ absent : après la valeur du dernier champ (le plus loin
                // dans le texte), avant les blancs et le `}` qui restent en place.
                let last_start = offset(text, last.get());
                let last_end = last_start + last.get().len();
                let sep = if raw.get().contains('\n') {
                    let key = key_start(text, last_start)?;
                    let own_line = text[line_start(text, key)..key].trim().is_empty();
                    let indent = match own_line {
                        true => indent_at(text, key),
                        false => one_more(&indent_at(text, start)),
                    };
                    format!(",{nl}{indent}")
                } else {
                    ", ".to_string()
                };
                (last_end, last_end, format!("{sep}{quoted}: {value}"))
            } else {
                // Objet vide `{}` : on le remplace entièrement.
                let object = new_object(text, start, &quoted, value, nl, multiline);
                (start, start + raw.get().len(), object)
            }
        }
        // Objet absent : juste après la valeur de `"version"` (obligatoire).
        None => {
            let version = root.get("version")?;
            let value_pos = offset(text, version.get());
            let key_pos = key_start(text, value_pos)?;
            let object_text = new_object(text, key_pos, &quoted, value, nl, multiline);
            let object_key = serde_json::to_string(object).ok()?;
            let own_line = text[line_start(text, key_pos)..key_pos].trim().is_empty();
            let insert = if multiline && own_line {
                format!(
                    ",{nl}{}{object_key}: {object_text}",
                    indent_at(text, key_pos)
                )
            } else {
                format!(", {object_key}: {object_text}")
            };
            let end = value_pos + version.get().len();
            (end, end, insert)
        }
    };
    Some(format!("{}{insert}{}", &text[..at], &text[end..]))
}

/// Un nouvel objet `{ "field": value }`, pour la clé située sur la ligne de `pos`.
fn new_object(
    text: &str,
    pos: usize,
    quoted: &str,
    value: &str,
    nl: &str,
    multiline: bool,
) -> String {
    if !multiline {
        return format!("{{ {quoted}: {value} }}");
    }
    let outer = indent_at(text, pos);
    let inner = one_more(&outer);
    format!("{{{nl}{inner}{quoted}: {value}{nl}{outer}}}")
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

    #[test]
    fn remplace_un_element_sans_toucher_au_reste() {
        let text =
            "{\n  \"version\": 1,\n  \"tiles\": [\n    { \"a\": 1 },\n\n    { \"b\": 2 }\n  ]\n}";
        let new = replace_element(text, "tiles", 1, "{ \"c\": 3 }").unwrap();
        assert_eq!(new, text.replace("{ \"b\": 2 }", "{ \"c\": 3 }"));
        assert_eq!(replace_element(text, "tiles", 2, "1"), None);
        assert_eq!(replace_element(text, "absent", 0, "1"), None);
    }

    #[test]
    fn retire_un_element_et_sa_virgule() {
        let text =
            "{\r\n  \"version\": 1,\r\n  \"tiles\": [\r\n    1,\r\n    2,\r\n    3\r\n  ]\r\n}";
        let middle = remove_element(text, "tiles", 1).unwrap();
        assert_eq!(middle, text.replace("    2,\r\n", ""));
        let first = remove_element(text, "tiles", 0).unwrap();
        assert_eq!(first, text.replace("    1,\r\n", ""));
        let last = remove_element(text, "tiles", 2).unwrap();
        assert_eq!(last, text.replace(",\r\n    3", ""));

        let compact = "{\"version\":1,\"tiles\":[1, 2]}";
        assert_eq!(
            remove_element(compact, "tiles", 0).unwrap(),
            "{\"version\":1,\"tiles\":[2]}"
        );
        let one = "{\n  \"version\": 1,\n  \"tiles\": [\n    { \"a\": [1] }\n  ],\n  \"x\": []\n}";
        assert_eq!(
            remove_element(one, "tiles", 0).unwrap(),
            "{\n  \"version\": 1,\n  \"tiles\": [],\n  \"x\": []\n}"
        );
        assert_eq!(remove_element(one, "tiles", 1), None);
    }

    #[test]
    fn remplace_la_valeur_d_un_reglage_existant() {
        let text = "{\r\n  \"version\": 1,\r\n  \"settings\": {\r\n    \"hotkey\": \"Super+Alt+Space\", // pas de commentaire en JSON\r\n    \"hideOnBlur\": true\r\n  }\r\n}";
        // (le commentaire rend le JSON invalide : on vérifie qu'il est refusé)
        assert_eq!(
            set_object_field(text, "settings", "hotkey", "\"Super\""),
            None
        );

        let text = "{\r\n  \"version\": 1,\r\n  \"settings\": {\r\n    \"hotkey\" :  \"Super+Alt+Space\",\r\n    \"hideOnBlur\": true\r\n  }\r\n}";
        let new = set_object_field(text, "settings", "hotkey", "\"Super\"").unwrap();
        assert_eq!(new, text.replace("\"Super+Alt+Space\"", "\"Super\""));
        let new = set_object_field(text, "settings", "hideOnBlur", "false").unwrap();
        assert_eq!(new, text.replace("true", "false"));
    }

    #[test]
    fn ajoute_un_reglage_absent_apres_le_dernier() {
        let text = "{\n\t\"version\": 1,\n\t\"settings\": {\n\t\t\"hotkey\": \"Ctrl+K\"\n\t},\n\t\"tiles\": []\n}";
        let new = set_object_field(text, "settings", "hideOnBlur", "false").unwrap();
        assert_eq!(
            new,
            "{\n\t\"version\": 1,\n\t\"settings\": {\n\t\t\"hotkey\": \"Ctrl+K\",\n\t\t\"hideOnBlur\": false\n\t},\n\t\"tiles\": []\n}"
        );

        let compact = "{\"version\":1,\"settings\":{\"hotkey\":\"Ctrl+K\"}}";
        assert_eq!(
            set_object_field(compact, "settings", "hideOnBlur", "false").unwrap(),
            "{\"version\":1,\"settings\":{\"hotkey\":\"Ctrl+K\", \"hideOnBlur\": false}}"
        );
    }

    #[test]
    fn remplit_un_objet_vide_ou_absent() {
        let text = "{\n  \"version\": 1,\n  \"settings\": {},\n  \"tiles\": []\n}";
        assert_eq!(
            set_object_field(text, "settings", "hotkey", "\"Super\"").unwrap(),
            "{\n  \"version\": 1,\n  \"settings\": {\n    \"hotkey\": \"Super\"\n  },\n  \"tiles\": []\n}"
        );

        let text = "{\n  \"version\": 1,\n  \"tiles\": []\n}";
        assert_eq!(
            set_object_field(text, "settings", "hotkey", "\"Super\"").unwrap(),
            "{\n  \"version\": 1,\n  \"settings\": {\n    \"hotkey\": \"Super\"\n  },\n  \"tiles\": []\n}"
        );
        assert_eq!(
            set_object_field("{\"version\":1}", "settings", "hotkey", "\"Super\"").unwrap(),
            "{\"version\":1, \"settings\": { \"hotkey\": \"Super\" }}"
        );
        assert_eq!(set_object_field("{}", "settings", "hotkey", "1"), None);
        assert_eq!(
            set_object_field("{\"version\":1,\"settings\":[]}", "settings", "hotkey", "1"),
            None
        );
    }

    #[test]
    fn permute_les_elements_sans_toucher_aux_separateurs() {
        let text =
            "{\n  \"version\": 1,\n  \"tiles\": [\n    { \"a\": 1 },\n\n    \"b\",\n    3\n  ]\n}";
        let new = permute_elements(text, "tiles", &[2, 0, 1]).unwrap();
        assert_eq!(
            new,
            "{\n  \"version\": 1,\n  \"tiles\": [\n    3,\n\n    { \"a\": 1 },\n    \"b\"\n  ]\n}"
        );
        assert_eq!(permute_elements(text, "tiles", &[0, 1, 2]).unwrap(), text);
        assert_eq!(permute_elements(text, "tiles", &[0, 0, 1]), None);
        assert_eq!(permute_elements(text, "tiles", &[0, 1]), None);
        assert_eq!(permute_elements(text, "tiles", &[0, 1, 3]), None);
    }
}
