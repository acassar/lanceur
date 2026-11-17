//! Lecture du format texte « KeyValues » de Valve (fichiers `.vdf` et `.acf`).
//!
//! C'est l'ancêtre maison du JSON chez Valve : des paires `"clé" "valeur"`, et
//! des blocs `"clé" { … }` pour imbriquer. Par exemple `libraryfolders.vdf` :
//!
//! ```text
//! "libraryfolders"
//! {
//!     "0"
//!     {
//!         "path"    "C:\\Program Files (x86)\\Steam"
//!         "apps"    { "228980" "215180" }
//!     }
//! }
//! ```
//!
//! Il n'existe pas de crate « officielle » pour ce format et on n'en lit que
//! deux fichiers : un petit lecteur fait main suffit (une centaine de lignes).
//!
//! Notions Rust abordées : une `enum` *récursive* (un bloc contient d'autres
//! valeurs), et un *itérateur de caractères* qu'on fait avancer à la main
//! (`Peekable` permet de regarder le caractère suivant sans le consommer).

use std::iter::Peekable;
use std::str::Chars;

/// Une valeur : soit un texte, soit un bloc de paires (clé, valeur).
///
/// Un bloc est une liste et non une table : l'ordre est gardé et une même clé
/// peut apparaître plusieurs fois (rare, mais permis par le format).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Block(Vec<(String, Value)>),
}

impl Value {
    /// Valeur de la première clé `key` du bloc (sans tenir compte des
    /// majuscules : Valve écrit `appid` ou `appID` selon les fichiers).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Block(pairs) => pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Value::Str(_) => None,
        }
    }

    /// Le texte, si c'est un texte.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Block(_) => None,
        }
    }

    /// Les paires du bloc (vide si c'est un texte).
    pub fn pairs(&self) -> &[(String, Value)] {
        match self {
            Value::Block(pairs) => pairs,
            Value::Str(_) => &[],
        }
    }
}

/// Lit tout le fichier. Renvoie le bloc racine (qui contient en général une
/// seule paire, `"libraryfolders" { … }` ou `"AppState" { … }`), ou `None` si
/// le texte est mal formé (fichier tronqué, accolade en trop…).
pub fn parse(text: &str) -> Option<Value> {
    // Certains fichiers commencent par un BOM UTF-8 : on l'ignore.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut chars = text.chars().peekable();
    let root = parse_pairs(&mut chars, false)?;
    Some(Value::Block(root))
}

/// Ce que lit [`next_token`].
#[derive(Debug, PartialEq)]
enum Token {
    Text(String),
    Open,
    Close,
}

/// Lit des paires jusqu'à `}` (si `nested`) ou jusqu'à la fin du texte.
///
/// `&mut Peekable<Chars>` : on *emprunte en écriture* l'itérateur, pour que
/// les appels imbriqués avancent dans le même texte.
fn parse_pairs(chars: &mut Peekable<Chars>, nested: bool) -> Option<Vec<(String, Value)>> {
    let mut pairs = Vec::new();
    loop {
        let key = match next_token(chars) {
            Some(Token::Text(key)) => key,
            // `}` ferme le bloc en cours ; il est interdit au premier niveau.
            Some(Token::Close) if nested => return Some(pairs),
            // Fin du texte : normale au premier niveau, sinon bloc non fermé.
            None if !nested => return Some(pairs),
            _ => return None,
        };
        let value = match next_token(chars)? {
            Token::Text(text) => Value::Str(text),
            // Appel *récursif* : le bloc imbriqué est lu par la même fonction.
            Token::Open => Value::Block(parse_pairs(chars, true)?),
            Token::Close => return None,
        };
        skip_condition(chars);
        pairs.push((key, value));
    }
}

/// Saute les blancs et les commentaires `// …`, puis lit un élément.
fn next_token(chars: &mut Peekable<Chars>) -> Option<Token> {
    loop {
        match chars.peek()? {
            c if c.is_whitespace() => {
                chars.next();
            }
            '/' => {
                // `//` : commentaire jusqu'à la fin de la ligne.
                chars.next();
                if chars.peek() != Some(&'/') {
                    return None;
                }
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
    match chars.next()? {
        '{' => Some(Token::Open),
        '}' => Some(Token::Close),
        '"' => read_quoted(chars).map(Token::Text),
        // Texte sans guillemets (rare) : jusqu'au prochain blanc ou accolade.
        first => {
            let mut text = String::from(first);
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                    break;
                }
                text.push(c);
                chars.next();
            }
            Some(Token::Text(text))
        }
    }
}

/// Lit un texte entre guillemets (le `"` ouvrant est déjà lu), en traitant les
/// échappements `\\`, `\"`, `\n` et `\t`.
fn read_quoted(chars: &mut Peekable<Chars>) -> Option<String> {
    let mut text = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(text),
            '\\' => match chars.next()? {
                'n' => text.push('\n'),
                't' => text.push('\t'),
                c @ ('\\' | '"') => text.push(c),
                // Antislash seul (`D:\SteamLibrary` écrit par un vieil outil) :
                // on le garde, sinon le chemin deviendrait `D:SteamLibrary`.
                c => {
                    text.push('\\');
                    text.push(c);
                }
            },
            c => text.push(c),
        }
    }
}

/// Saute une condition de plateforme placée après une valeur, comme
/// `"clé" "valeur" [$WIN32]`. On ne s'en sert pas, mais elle ne doit pas être
/// prise pour la clé suivante.
fn skip_condition(chars: &mut Peekable<Chars>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
    if chars.peek() == Some(&'[') {
        for c in chars.by_ref() {
            if c == ']' {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_des_blocs_imbriques_et_des_echappements() {
        let root = parse(
            "\u{feff}// commentaire\n\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"apps\" { \"730\" \"123\" }\n\t}\n}\n",
        )
        .unwrap();
        let folder = root.get("LibraryFolders").unwrap().get("0").unwrap();
        assert_eq!(
            folder.get("path").unwrap().as_str(),
            Some(r"C:\Program Files (x86)\Steam")
        );
        assert_eq!(folder.get("apps").unwrap().pairs().len(), 1);
    }

    #[test]
    fn ignore_les_conditions_et_accepte_les_textes_sans_guillemets() {
        let root = parse(r#""a" { "b" "1" [$WIN32] c 2 "q" "dit \"oui\"" }"#).unwrap();
        let a = root.get("a").unwrap();
        assert_eq!(a.get("b").unwrap().as_str(), Some("1"));
        assert_eq!(a.get("c").unwrap().as_str(), Some("2"));
        assert_eq!(a.get("q").unwrap().as_str(), Some(r#"dit "oui""#));
        let root = parse(r#""path" "D:\SteamLibrary""#).unwrap();
        assert_eq!(root.get("path").unwrap().as_str(), Some(r"D:\SteamLibrary"));
    }

    #[test]
    fn refuse_un_fichier_tronque() {
        assert_eq!(parse(r#""a" { "b" "1""#), None);
        assert_eq!(parse(r#""a" { "b" "#), None);
        assert_eq!(parse(r#""a" "1" }"#), None);
        assert_eq!(parse(r#""a" { "b" "non fermé }"#), None);
        assert_eq!(parse(""), Some(Value::Block(vec![])));
    }
}
