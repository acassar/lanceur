//! Favicon d'un site, lu dans sa page (décision 4A : pas de service tiers).
//!
//! 1. On télécharge la page et on lit ses balises `<link rel="icon" …>`
//!    (et `apple-touch-icon`, souvent plus grande et plus nette).
//! 2. On essaie ces images de la plus grande à la plus petite.
//! 3. En dernier recours, `/favicon.ico` à la racine du site.
//!
//! Tout est `async` : pendant qu'on attend un serveur, le thread sert à autre
//! chose (les autres icônes, les autres commandes). `reqwest` est l'équivalent
//! de `fetch` en JavaScript : `client.get(url).send().await` ressemble beaucoup
//! à `await fetch(url)`.

use image::DynamicImage;
use reqwest::Url;

use super::{decode, http_client, is_http_url};

/// Taille maximale lue pour une page HTML. Les balises `<link>` sont dans le
/// `<head>`, au début : inutile de télécharger toute la page.
const MAX_HTML: usize = 512 * 1024;

/// Taille maximale d'une image téléchargée.
const MAX_IMAGE: usize = 2 * 1024 * 1024;

/// Trouve et décode le favicon de la page `page_url`.
pub async fn fetch(page_url: &str) -> Result<DynamicImage, String> {
    let url = Url::parse(page_url).map_err(|e| format!("{page_url} : {e}"))?;

    // Si la page ne répond pas, on tente quand même `/favicon.ico`.
    let mut candidates = Vec::new();
    let mut base = url.clone();
    match get_limited(url.clone(), MAX_HTML, true).await {
        Ok((final_url, bytes)) => {
            // Après une redirection (`amazon.fr` → `www.amazon.fr`), les
            // chemins relatifs se lisent par rapport à l'adresse finale.
            base = final_url;
            // `from_utf8_lossy` : une page mal encodée ne fait pas échouer la
            // lecture, les caractères invalides deviennent `�`.
            candidates = icon_links(&String::from_utf8_lossy(&bytes), &base);
        }
        Err(e) => eprintln!("favicon : page {page_url} illisible : {e}"),
    }
    for root in [&base, &url] {
        if let Ok(ico) = root.join("/favicon.ico")
            && !candidates.contains(&ico)
        {
            candidates.push(ico);
        }
    }

    let mut last_error = String::from("aucune icône trouvée");
    for candidate in candidates {
        // `and_then` enchaîne : si le téléchargement réussit, on décode.
        match download(candidate.as_str()).await.and_then(|b| decode(&b)) {
            Ok(image) => return Ok(image),
            Err(e) => last_error = format!("{candidate} : {e}"),
        }
    }
    Err(last_error)
}

/// Télécharge une image (au plus [`MAX_IMAGE`] octets).
pub async fn download(url: &str) -> Result<Vec<u8>, String> {
    if !is_http_url(url) {
        return Err(format!("URL refusée : {url}"));
    }
    let url = Url::parse(url).map_err(|e| e.to_string())?;
    Ok(get_limited(url, MAX_IMAGE, false).await?.1)
}

/// Requête GET qui s'arrête à `max` octets. Renvoie l'URL finale (après les
/// redirections) et le contenu.
///
/// Avec `truncate`, un contenu trop long est coupé (une page HTML : le début
/// suffit). Sinon, il est refusé (une image coupée serait illisible).
async fn get_limited(url: Url, max: usize, truncate: bool) -> Result<(Url, Vec<u8>), String> {
    let mut response = http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        // Une réponse 404 ou 500 devient une erreur.
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let final_url = response.url().clone();

    let mut bytes = Vec::new();
    // On lit la réponse morceau par morceau (`chunk`), pour pouvoir s'arrêter
    // dès qu'on dépasse la limite au lieu de tout charger en mémoire.
    // `Ok(Some(chunk))` : un morceau ; `Ok(None)` : fin de la réponse.
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > max {
            if truncate {
                bytes.truncate(max);
                break;
            }
            return Err(format!("fichier trop gros (plus de {} Ko)", max / 1024));
        }
    }
    Ok((final_url, bytes))
}

/// Une icône annoncée par la page, avec sa taille estimée.
#[derive(Debug, PartialEq)]
struct Candidate {
    url: Url,
    size: u32,
}

/// Lit les balises `<link rel="…icon…" href="…">` de `html` et renvoie leurs
/// URL absolues, de la plus grande icône à la plus petite.
///
/// Ce n'est pas un vrai analyseur HTML (ce serait une grosse dépendance pour
/// peu de chose) : on repère les `<link`, puis on lit leurs attributs.
fn icon_links(html: &str, base: &Url) -> Vec<Url> {
    let mut found: Vec<Candidate> = Vec::new();
    // `to_ascii_lowercase` ne change que les lettres A-Z : les positions dans
    // `lower` sont exactement celles de `html`, on peut donc chercher dans
    // l'une et découper l'autre.
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("<link") {
        let start = from + pos + "<link".len();
        let attrs = parse_attributes(&html[start..]);
        from = start;

        let get = |name: &str| {
            attrs
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str())
        };
        let rel = get("rel").unwrap_or("").to_ascii_lowercase();
        let rels: Vec<&str> = rel.split_ascii_whitespace().collect();
        let is_icon = rels
            .iter()
            .any(|r| *r == "icon" || r.starts_with("apple-touch-icon"));
        let Some(href) = get("href").filter(|_| is_icon) else {
            continue;
        };
        // SVG : la crate `image` ne sait pas le dessiner, on l'ignore.
        let is_svg = get("type").is_some_and(|t| t.contains("svg"))
            || href
                .to_ascii_lowercase()
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .ends_with(".svg");
        if is_svg {
            continue;
        }
        let Ok(url) = base.join(&decode_entities(href)) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        let apple = rels.iter().any(|r| r.starts_with("apple-touch-icon"));
        // Sans `sizes`, une apple-touch-icon fait en général 180 px, un
        // favicon classique 32 px.
        let size = get("sizes")
            .and_then(largest_size)
            .unwrap_or(if apple { 180 } else { 32 });
        if !found.iter().any(|c| c.url == url) {
            found.push(Candidate { url, size });
        }
    }
    // Tri du plus grand au plus petit. `sort_by_key` est *stable* : à taille
    // égale, l'ordre de la page est gardé.
    found.sort_by_key(|c| std::cmp::Reverse(c.size));
    found.into_iter().map(|c| c.url).collect()
}

/// Plus grande taille d'un attribut `sizes="16x16 32x32"` (`any` → 256).
fn largest_size(sizes: &str) -> Option<u32> {
    sizes
        .split_ascii_whitespace()
        .filter_map(|s| {
            if s.eq_ignore_ascii_case("any") {
                return Some(256);
            }
            // `48x48` → on garde la largeur.
            let lower = s.to_ascii_lowercase();
            let (width, _height) = lower.split_once('x')?;
            width.parse().ok()
        })
        .max()
}

/// Lit les attributs d'une balise jusqu'au `>` : `rel="icon" href=/a.png`.
/// Renvoie des paires (nom en minuscules, valeur).
fn parse_attributes(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    // On avance octet par octet. Tous les séparateurs cherchés (`=`, `"`, `>`,
    // espaces) sont des caractères ASCII : découper `s` à ces positions ne
    // coupe donc jamais un caractère accentué en deux.
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && !matches!(bytes[i], b'=' | b'>' | b'/')
            && !bytes[i].is_ascii_whitespace()
        {
            i += 1;
        }
        let name = s[name_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                let value_start = i + 1;
                let end = s[value_start..]
                    .bytes()
                    .position(|b| b == quote)
                    .map_or(s.len(), |p| value_start + p);
                value = s[value_start..end].to_string();
                i = (end + 1).min(s.len());
            } else {
                let value_start = i;
                while i < bytes.len() && bytes[i] != b'>' && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = s[value_start..i].to_string();
            }
        }
        if !name.is_empty() {
            attrs.push((name, value));
        }
    }
    attrs
}

/// Remplace les entités HTML courantes dans un `href` (`&amp;` → `&`).
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://www.exemple.fr/accueil/").unwrap()
    }

    #[test]
    fn lit_les_attributs() {
        assert_eq!(
            parse_attributes(r#" REL="Icon" href='/a b.png' sizes=32x32 async />"#),
            vec![
                ("rel".to_string(), "Icon".to_string()),
                ("href".to_string(), "/a b.png".to_string()),
                ("sizes".to_string(), "32x32".to_string()),
                ("async".to_string(), String::new()),
            ]
        );
        // Guillemet non fermé : pas de panique, on lit jusqu'au bout.
        assert_eq!(parse_attributes(r#" href="/é"#).len(), 1);
    }

    #[test]
    fn trouve_les_icones_de_la_plus_grande_a_la_plus_petite() {
        let html = r#"<html><HEAD>
            <link rel="stylesheet" href="/style.css">
            <link rel="shortcut icon" href="/favicon.ico">
            <link rel="icon" type="image/svg+xml" href="/logo.svg">
            <link rel="mask-icon" href="/mask.png">
            <link rel="icon" sizes="16x16 192x192" href="img/192.png?v=1&amp;x=2">
            <link rel="apple-touch-icon" href="https://cdn.exemple.fr/apple.png">
            <link rel="icon" href="javascript:alert(1)">
        </head>"#;
        let urls: Vec<String> = icon_links(html, &base())
            .iter()
            .map(|u| u.to_string())
            .collect();
        assert_eq!(
            urls,
            vec![
                "https://www.exemple.fr/accueil/img/192.png?v=1&x=2",
                "https://cdn.exemple.fr/apple.png",
                "https://www.exemple.fr/favicon.ico",
            ]
        );
    }

    #[test]
    fn tailles() {
        assert_eq!(largest_size("16x16 48X48"), Some(48));
        assert_eq!(largest_size("any"), Some(256));
        assert_eq!(largest_size("n'importe quoi"), None);
    }

    #[test]
    fn page_sans_icone() {
        assert!(icon_links("<html><body>Bonjour</body></html>", &base()).is_empty());
        // Un `<link` à la toute fin du texte ne fait pas planter.
        assert!(icon_links("<link", &base()).is_empty());
    }
}
