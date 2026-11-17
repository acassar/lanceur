//! Icônes des tuiles : extraction, téléchargement et cache.
//!
//! Pour chaque tuile, on cherche une image selon sa *source* ([`Source`]) :
//!
//! | Tuile | Source |
//! |---|---|
//! | `"icon"` renseigné | ce fichier (PNG, JPEG, ICO…) ou cette URL ; un `.exe`/`.lnk` donne son icône |
//! | `app` (exe, lnk) | l'icône que Windows affiche pour ce fichier (API Win32, voir `win32.rs`) |
//! | `steam` | la jaquette du jeu : cache local de Steam, sinon le CDN de Steam |
//! | `web` | le favicon lu dans la page (voir `favicon.rs`) |
//!
//! Le résultat est toujours *réencodé* (PNG, ou JPEG pour les jaquettes) et
//! réduit, puis rangé dans `icons\` à côté de `config.json`. À la 2ᵉ ouverture,
//! on lit simplement ce fichier : c'est instantané.
//!
//! **Nom du fichier de cache** : `<id>-<empreinte>.png`. L'empreinte résume la
//! source (chemin, URL…) et la date de modification du fichier source. Si vous
//! changez le chemin d'une tuile, ou si l'exe est mis à jour, l'empreinte change
//! et l'icône est recalculée ; l'ancien fichier est supprimé.
//!
//! **Échec** : si aucune image n'est trouvée sur Internet (site hors ligne…),
//! on écrit un petit fichier `<id>-<empreinte>.none` pour ne pas retenter à
//! chaque ouverture. On réessaie au bout de [`RETRY_AFTER`]. Le front affiche
//! alors le monogramme.
//!
//! **Affichage** : la commande renvoie une `data:` URL
//! (`data:image/png;base64,…`), que la CSP autorise déjà pour les images. Pas
//! besoin d'ouvrir au front l'accès aux fichiers du disque (protocole `asset:`).

mod favicon;
#[cfg(windows)]
mod win32;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, UNIX_EPOCH};

use base64::Engine;
use image::{DynamicImage, ImageFormat, imageops::FilterType};

use crate::config::{Target, Tile};
use crate::fsutil;
use crate::launch::{check_web_url, expand_env};

/// Sous-dossier du cache, à côté de `config.json`.
pub const DIR_NAME: &str = "icons";

/// Taille (en pixels) des icônes stockées. Les tuiles les affichent en 46 px :
/// 96 px restent nets sur un écran à 200 %.
const ICON_SIZE: u32 = 96;

/// Taille des jaquettes Steam stockées (format portrait 2:3, comme Steam).
const COVER_SIZE: (u32, u32) = (160, 240);

/// Délai avant de retenter une icône introuvable sur Internet.
const RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// À changer si le format du cache change : toutes les empreintes changent
/// alors, et les icônes sont recalculées.
const CACHE_VERSION: u32 = 1;

/// D'où vient l'image d'une tuile.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Une image sur le disque (champ `"icon"`).
    ImageFile(PathBuf),
    /// Une image sur Internet (champ `"icon"` en `https://…`).
    ImageUrl(String),
    /// L'icône Windows d'un exe, d'un raccourci `.lnk`… (API Win32).
    Shell(PathBuf),
    /// La jaquette d'un jeu Steam.
    SteamCover(u32),
    /// Le favicon d'un site.
    Favicon(String),
}

impl Source {
    /// `true` si la source est sur Internet : un échec y est peut-être
    /// passager (pas de réseau), on le mémorise donc pour [`RETRY_AFTER`].
    fn is_remote(&self) -> bool {
        matches!(
            self,
            Source::ImageUrl(_) | Source::SteamCover(_) | Source::Favicon(_)
        )
    }

    /// Fichier source local, dont la date de modification entre dans l'empreinte.
    fn local_file(&self) -> Option<&Path> {
        match self {
            Source::ImageFile(p) | Source::Shell(p) => Some(p),
            _ => None,
        }
    }
}

/// Pourquoi une icône n'a pas été trouvée.
#[derive(Debug)]
pub struct Failure {
    pub message: String,
    /// `true` si le serveur n'a pas pu être joint (pas de réseau, délai
    /// dépassé). Cet échec est sans doute passager : on ne le mémorise pas.
    pub offline: bool,
}

/// Permet d'utiliser `?` sur un `Result<_, String>` dans une fonction qui
/// renvoie `Result<_, Failure>` : Rust convertit l'erreur avec ce `From`.
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure {
            message,
            offline: false,
        }
    }
}

/// Une image prête à être mise en cache et affichée.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub format: Format,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
        }
    }
    fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
        }
    }
}

// ---------------------------------------------------------------------------
// Point d'entrée : la commande `get_icon` (lib.rs) appelle `icon_data_url`.
// ---------------------------------------------------------------------------

/// Renvoie l'icône de la tuile en `data:` URL, ou `None` (le front affiche
/// alors le monogramme). Ne renvoie jamais d'erreur : une icône manquante n'est
/// pas grave, on l'écrit seulement dans la console.
///
/// `async fn` : la fonction peut *attendre* (un téléchargement…) sans bloquer
/// le thread. Chaque `.await` est un point où Rust peut faire autre chose en
/// attendant, comme `await` en TypeScript.
pub async fn icon_data_url(config_dir: PathBuf, tile: Tile) -> Option<String> {
    let source = source_for(&tile, &config_dir, |name| std::env::var(name).ok())?;
    let cache_dir = config_dir.join(DIR_NAME);

    // 1. Déjà en cache ? (lecture de fichiers : sur un thread d'attente)
    let id = tile.id.clone();
    let lookup = {
        let (cache_dir, source) = (cache_dir.clone(), source.clone());
        blocking(move || cache_lookup(&cache_dir, &id, &source)).await?
    };
    let key = match lookup {
        Lookup::Hit(encoded) => return Some(to_data_url(&encoded)),
        Lookup::KnownMissing => return None,
        Lookup::Miss(key) => key,
    };

    // 2. Sinon, on la produit.
    let produced = produce(&source).await;
    // On ne mémorise un échec que s'il vient d'Internet *et* que le serveur a
    // répondu (page sans icône, image illisible…). Sans réseau (PC qui démarre
    // avant le Wi-Fi), on réessaiera à la prochaine ouverture.
    let remember_failure = match &produced {
        Ok(_) => false,
        Err(failure) => {
            eprintln!("icône de « {} » introuvable : {}", tile.id, failure.message);
            source.is_remote() && !failure.offline
        }
    };
    let encoded = produced.ok();

    // 3. Et on la range dans le cache (ou on note l'échec).
    let id = tile.id.clone();
    let to_store = encoded.clone();
    blocking(move || {
        if let Err(e) = cache_store(&cache_dir, &id, &key, to_store.as_ref(), remember_failure) {
            eprintln!("cache d'icône non écrit : {e}");
        }
    })
    .await;

    encoded.map(|e| to_data_url(&e))
}

/// Exécute `f` sur un thread prévu pour les opérations bloquantes (fichiers,
/// appels Windows), et attend son résultat sans bloquer le thread `async`.
/// `None` si le thread a paniqué.
async fn blocking<T, F>(f: F) -> Option<T>
where
    // `Send + 'static` : `f` et son résultat peuvent passer d'un thread à
    // l'autre, et ne contiennent pas d'emprunt qui pourrait expirer avant.
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.ok()
}

/// Comme [`blocking`], pour un travail qui peut échouer. Sert aussi au
/// décodage et au redimensionnement des images : ce sont des calculs lourds,
/// qui ne doivent pas occuper les threads `async` (ceux qui répondent aux
/// autres commandes, comme `launch`).
async fn on_blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    blocking(f)
        .await
        .unwrap_or_else(|| Err("erreur interne".into()))
}

/// Choisit la source de l'image d'une tuile (fonction pure, testée plus bas).
///
/// `config_dir` : un chemin `"icon"` relatif (`images\\amazon.png`) se lit
/// par rapport au dossier de `config.json`.
pub fn source_for(
    tile: &Tile,
    config_dir: &Path,
    env: impl Fn(&str) -> Option<String>,
) -> Option<Source> {
    // `"icon"` remplace toujours la source automatique.
    if let Some(icon) = tile
        .icon
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if is_http_url(icon) {
            return check_web_url(icon)
                .ok()
                .map(|()| Source::ImageUrl(icon.to_string()));
        }
        let expanded = expand_env(icon, &env);
        let path = match is_absolute_windows_path(&expanded) {
            true => PathBuf::from(expanded),
            false => config_dir.join(expanded),
        };
        // Un exe ou un raccourci donné comme icône : on prend son icône Windows.
        let is_shell = path.extension().is_some_and(|e| {
            ["exe", "lnk", "dll", "url"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        });
        return Some(match is_shell {
            true => Source::Shell(path),
            false => Source::ImageFile(path),
        });
    }

    match &tile.target {
        Target::Steam { app_id } => Some(Source::SteamCover(*app_id)),
        Target::App { path, .. } => {
            let path = PathBuf::from(expand_env(path, &env));
            Some(Source::Shell(resolve_in_path(path, &env)))
        }
        // Une URL invalide ne se lance pas non plus : pas d'icône à chercher.
        // (même vérification qu'au lancement, dans launch.rs).
        Target::Web { url } => check_web_url(url)
            .ok()
            .map(|()| Source::Favicon(url.clone())),
    }
}

/// `true` pour un texte qui commence par `http://` ou `https://`.
fn is_http_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// `C:\…`, `\\serveur\…` ou `\…` : chemin absolu au sens de Windows. On teste
/// le texte plutôt que `Path::is_absolute`, pour que les tests donnent le même
/// résultat sous Linux (où `C:\…` n'est pas un chemin absolu).
fn is_absolute_windows_path(s: &str) -> bool {
    let b = s.as_bytes();
    let drive =
        b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/');
    drive || s.starts_with(['\\', '/'])
}

/// Un nom seul (`wt.exe`) est cherché dans les dossiers du `PATH`, comme le
/// fait Windows au lancement. Un vrai chemin est renvoyé tel quel.
fn resolve_in_path(path: PathBuf, env: impl Fn(&str) -> Option<String>) -> PathBuf {
    if path.components().count() != 1 {
        return path;
    }
    let Some(dirs) = env("PATH") else {
        return path;
    };
    // Sans extension (`notepad`), Windows essaie aussi `notepad.exe`.
    let mut names = vec![path.clone()];
    if path.extension().is_none() {
        names.push(path.with_extension("exe"));
    }
    // `std::env::split_paths` découpe le PATH (`;` sous Windows, `:` ailleurs).
    std::env::split_paths(&dirs)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|candidate| candidate.is_file())
        .unwrap_or(path)
}

// ---------------------------------------------------------------------------
// Cache sur disque
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Lookup {
    /// L'icône est en cache.
    Hit(Encoded),
    /// L'icône a été cherchée récemment, sans succès.
    KnownMissing,
    /// Rien en cache : il faut la produire. Contient le début du nom de fichier.
    Miss(String),
}

/// Début du nom des fichiers de cache d'une tuile : `<id>-<empreinte 16 hex>`.
fn cache_key(id: &str, source: &Source) -> String {
    // La date de modification du fichier source : si l'exe est mis à jour (ou
    // l'image remplacée), l'empreinte change et l'icône est recalculée.
    let modified = source
        .local_file()
        .and_then(|p| fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let fingerprint = fnv1a(format!("{CACHE_VERSION}|{source:?}|{modified}").as_bytes());
    format!("{id}-{fingerprint:016x}")
}

/// Empreinte FNV-1a sur 64 bits : un calcul très simple et *stable* (il donne
/// le même résultat sur toutes les versions de Rust, contrairement au hachage
/// interne des `HashMap`). Pas besoin d'un hachage cryptographique ici.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn cache_lookup(cache_dir: &Path, id: &str, source: &Source) -> Lookup {
    let key = cache_key(id, source);
    for format in [Format::Png, Format::Jpeg] {
        if let Ok(bytes) = fs::read(cache_dir.join(format!("{key}.{}", format.ext()))) {
            return Lookup::Hit(Encoded { bytes, format });
        }
    }
    let marker = cache_dir.join(format!("{key}.none"));
    let recent_failure = fs::metadata(&marker)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|age| age < RETRY_AFTER));
    if recent_failure {
        Lookup::KnownMissing
    } else {
        Lookup::Miss(key)
    }
}

/// Écrit l'image (ou le marqueur d'échec) et supprime les anciens fichiers de
/// la tuile.
fn cache_store(
    cache_dir: &Path,
    id: &str,
    key: &str,
    encoded: Option<&Encoded>,
    remember_failure: bool,
) -> Result<(), crate::error::AppError> {
    let file_name = match encoded {
        Some(e) => format!("{key}.{}", e.format.ext()),
        // Échec à ne pas mémoriser : en local (fichier absent…), c'est rapide à
        // revérifier, et l'icône apparaîtra dès que le fichier existera ; sans
        // réseau, on réessaiera à la prochaine ouverture.
        None if !remember_failure => {
            remove_stale(cache_dir, id, None);
            return Ok(());
        }
        None => format!("{key}.none"),
    };
    let bytes = encoded.map_or(&[][..], |e| &e.bytes[..]);
    fsutil::write_atomic(&cache_dir.join(&file_name), bytes)?;
    remove_stale(cache_dir, id, Some(&file_name));
    Ok(())
}

/// Supprime les fichiers de cache de la tuile `id`, sauf `keep`.
fn remove_stale(cache_dir: &Path, id: &str, keep: Option<&str>) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if Some(name) != keep && is_cache_file_of(name, id) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Supprime du cache les icônes des tuiles qui n'existent plus dans la config.
///
/// Appelée après chaque lecture réussie de `config.json` (commande
/// `get_config`). Seuls les fichiers au format du cache
/// (`<id>-<16 hex>.png|jpg|none`) sont concernés : un autre fichier posé dans
/// `icons\` n'est jamais touché.
pub fn remove_orphans(config_dir: &Path, tiles: &[Tile]) {
    let Ok(entries) = fs::read_dir(config_dir.join(DIR_NAME)) else {
        return; // pas encore de cache : rien à faire
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // `let … else` : si le nom n'a pas la forme d'un fichier de cache, on passe.
        let Some(id) = cache_file_id(name) else {
            continue;
        };
        if !tiles.iter().any(|t| t.id == id) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// L'id de la tuile d'un fichier de cache (`cs2-0123456789abcdef.png` → `cs2`),
/// ou `None` si `name` n'est pas un fichier de cache.
fn cache_file_id(name: &str) -> Option<&str> {
    // `rsplit_once` coupe au *dernier* `-` : un id peut lui-même contenir des `-`.
    let (id, _) = name.rsplit_once('-')?;
    is_cache_file_of(name, id).then_some(id)
}

/// `true` si `name` est exactement `<id>-<16 hex>.(png|jpg|none)`. On vérifie
/// la forme complète : l'id `cs` ne doit pas toucher aux fichiers de `cs-2`.
fn is_cache_file_of(name: &str, id: &str) -> bool {
    let Some(rest) = name.strip_prefix(id).and_then(|r| r.strip_prefix('-')) else {
        return false;
    };
    let Some((hex, ext)) = rest.split_once('.') else {
        return false;
    };
    hex.len() == 16
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
        && ["png", "jpg", "none"].contains(&ext)
}

fn to_data_url(encoded: &Encoded) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(&encoded.bytes);
    format!("data:{};base64,{b64}", encoded.format.mime())
}

// ---------------------------------------------------------------------------
// Production de l'image selon la source
// ---------------------------------------------------------------------------

async fn produce(source: &Source) -> Result<Encoded, Failure> {
    // Les lectures de fichiers, appels Windows, décodages et redimensionnements
    // passent par `on_blocking` ; seuls les téléchargements restent `async`.
    match source {
        Source::ImageFile(path) => {
            let path = path.clone();
            Ok(on_blocking(move || {
                let bytes = fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))?;
                encode_icon(decode(&bytes)?)
            })
            .await?)
        }
        Source::ImageUrl(url) => {
            let bytes = favicon::download(url).await?;
            Ok(on_blocking(move || encode_icon(decode(&bytes)?)).await?)
        }
        Source::Shell(path) => {
            let path = path.clone();
            Ok(on_blocking(move || encode_icon(shell_icon(&path)?)).await?)
        }
        Source::SteamCover(app_id) => steam_cover(*app_id).await,
        Source::Favicon(url) => {
            let image = favicon::fetch(url).await?;
            Ok(on_blocking(move || encode_icon(image)).await?)
        }
    }
}

/// Décode une image (PNG, ICO, JPEG…), avec des limites : un fichier piégé de
/// quelques Ko ne doit pas pouvoir réclamer des Go de mémoire.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|e| e.to_string())
}

/// Réduit l'image à [`ICON_SIZE`] (sans l'agrandir) et l'encode en PNG.
fn encode_icon(image: DynamicImage) -> Result<Encoded, String> {
    let image = fit(image, ICON_SIZE, ICON_SIZE);
    let mut bytes = Vec::new();
    // `to_rgba8` : 4 octets par pixel (rouge, vert, bleu, transparence).
    DynamicImage::ImageRgba8(image.to_rgba8())
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(Encoded {
        bytes,
        format: Format::Png,
    })
}

/// Réduit la jaquette et l'encode en JPEG (bien plus léger qu'un PNG pour une
/// photo, et une jaquette n'a pas de transparence).
fn encode_cover(image: DynamicImage) -> Result<Encoded, String> {
    let image = fit(image, COVER_SIZE.0, COVER_SIZE.1);
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85);
    DynamicImage::ImageRgb8(image.to_rgb8())
        .write_with_encoder(encoder)
        .map_err(|e| e.to_string())?;
    Ok(Encoded {
        bytes,
        format: Format::Jpeg,
    })
}

/// Réduit l'image pour qu'elle tienne dans `w`×`h` en gardant ses proportions.
/// Une image déjà plus petite n'est pas agrandie (le navigateur s'en charge).
fn fit(image: DynamicImage, w: u32, h: u32) -> DynamicImage {
    if image.width() <= w && image.height() <= h {
        image
    } else {
        // Lanczos3 : filtre de réduction de bonne qualité (net, sans crénelage).
        image.resize(w, h, FilterType::Lanczos3)
    }
}

// ---------------------------------------------------------------------------
// Icônes Windows (exe, lnk)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn shell_icon(path: &Path) -> Result<DynamicImage, String> {
    if !path.exists() {
        return Err(format!("fichier introuvable : {}", path.display()));
    }
    let (width, height, mut pixels) = win32::shell_image_bgra(path, ICON_SIZE as i32)?;
    bgra_to_rgba(&mut pixels);
    image::RgbaImage::from_raw(width, height, pixels)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| "image Windows de taille inattendue".to_string())
}

#[cfg(not(windows))]
fn shell_icon(_path: &Path) -> Result<DynamicImage, String> {
    Err("icônes d'exe disponibles uniquement sous Windows".into())
}

/// Convertit les pixels renvoyés par Windows en RGBA « normal ».
///
/// Windows range les octets dans l'ordre bleu, vert, rouge, alpha (BGRA), et
/// donne en général des couleurs *prémultipliées* : chaque couleur est déjà
/// multipliée par la transparence (un rouge à moitié transparent vaut
/// `128, 0, 0, 128` au lieu de `255, 0, 0, 128`). Le PNG attend l'inverse.
///
/// - Si toute l'image a une transparence nulle, c'est une vieille icône sans
///   canal alpha : on la rend opaque.
/// - Si une couleur dépasse sa transparence, l'image ne peut pas être
///   prémultipliée : on ne touche pas aux couleurs.
#[cfg_attr(not(windows), allow(dead_code))] // utilisé seulement sous Windows
pub fn bgra_to_rgba(pixels: &mut [u8]) {
    // `chunks_exact_mut(4)` : on parcourt les pixels 4 octets par 4 octets.
    let no_alpha = pixels.chunks_exact(4).all(|p| p[3] == 0);
    let premultiplied = !no_alpha
        && pixels
            .chunks_exact(4)
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    for p in pixels.chunks_exact_mut(4) {
        p.swap(0, 2); // BGRA → RGBA
        if no_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                // Arrondi au plus proche : (c × 255 + a/2) / a.
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Jaquettes Steam
// ---------------------------------------------------------------------------

/// Jaquette d'un jeu : d'abord le cache de Steam sur le disque (pas de réseau),
/// sinon le CDN de Steam (la même image que dans la bibliothèque Steam).
async fn steam_cover(app_id: u32) -> Result<Encoded, Failure> {
    let local = on_blocking(move || {
        let path = steam_dir()
            .and_then(|steam| find_local_cover(&steam, app_id))
            .ok_or_else(|| "pas de jaquette dans le cache de Steam".to_string())?;
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        encode_cover(decode(&bytes)?)
    })
    .await;
    if let Ok(cover) = local {
        return Ok(cover);
    }

    // `offline` reste `true` tant que *tous* les serveurs étaient injoignables.
    let mut failure = Failure {
        message: "aucune jaquette".into(),
        offline: true,
    };
    for url in steam_cover_urls(app_id) {
        let result = match favicon::download(&url).await {
            Ok(bytes) => on_blocking(move || encode_cover(decode(&bytes)?))
                .await
                .map_err(Failure::from),
            Err(e) => Err(e),
        };
        match result {
            Ok(cover) => return Ok(cover),
            Err(e) => {
                failure = Failure {
                    message: format!("{url} : {}", e.message),
                    offline: failure.offline && e.offline,
                }
            }
        }
    }
    Err(failure)
}

/// URL des jaquettes sur les serveurs de Steam, de la plus adaptée à la moins
/// adaptée (`header.jpg` existe pour tous les jeux, mais il est en paysage).
fn steam_cover_urls(app_id: u32) -> Vec<String> {
    vec![
        format!(
            "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900.jpg"
        ),
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/library_600x900.jpg"),
        format!(
            "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/{app_id}/header.jpg"
        ),
    ]
}

/// Cherche la jaquette dans le cache de Steam (`appcache\librarycache`).
/// Steam a changé d'organisation en 2024, on essaie donc les deux :
/// - ancien : `librarycache\<appId>_library_600x900.jpg` ;
/// - nouveau : `librarycache\<appId>\library_600x900.jpg`, parfois dans un
///   sous-dossier au nom aléatoire.
pub fn find_local_cover(steam_dir: &Path, app_id: u32) -> Option<PathBuf> {
    const NAME: &str = "library_600x900.jpg";
    let cache = steam_dir.join("appcache").join("librarycache");
    let old = cache.join(format!("{app_id}_{NAME}"));
    if old.is_file() {
        return Some(old);
    }
    let game = cache.join(app_id.to_string());
    let direct = game.join(NAME);
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(&game)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(NAME))
        .find(|p| p.is_file())
}

/// Dossier d'installation de Steam, lu dans le registre (valeur `SteamPath`),
/// avec le dossier par défaut en secours.
#[cfg(windows)]
pub(crate) fn steam_dir() -> Option<PathBuf> {
    win32::steam_path_from_registry()
        .or_else(|| Some(PathBuf::from(r"C:\Program Files (x86)\Steam")))
        .filter(|p| p.is_dir())
}

#[cfg(not(windows))]
pub(crate) fn steam_dir() -> Option<PathBuf> {
    None
}

/// Client HTTP partagé par tous les téléchargements.
///
/// `OnceLock` : la valeur est créée au premier appel de `get_or_init`, puis
/// réutilisée (comme un singleton). Garder un seul client réutilise les
/// connexions déjà ouvertes vers un même serveur.
///
/// `Option` : si le client ne peut pas être créé (TLS indisponible…), on n'a
/// simplement pas d'icônes Internet, au lieu d'arrêter l'app.
fn http_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                // Certains sites refusent les clients qui ne ressemblent pas à un
                // navigateur : on se présente comme Edge (le navigateur des tuiles).
                .user_agent(concat!(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
                    "(KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0"
                ))
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .build()
                .ok()
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "APPDATA" => Some(r"C:\Users\A\AppData\Roaming".into()),
            _ => None,
        }
    }

    fn source_for_test(tile: &Tile, env: impl Fn(&str) -> Option<String>) -> Option<Source> {
        source_for(tile, Path::new("/config"), env)
    }

    fn tile(icon: Option<&str>, target: Target) -> Tile {
        Tile {
            id: "t".into(),
            name: "T".into(),
            category: "c".into(),
            icon: icon.map(String::from),
            target,
        }
    }

    fn app(path: &str) -> Target {
        Target::App {
            path: path.into(),
            args: vec![],
        }
    }

    #[test]
    fn source_selon_le_type_de_tuile() {
        assert_eq!(
            source_for_test(&tile(None, Target::Steam { app_id: 730 }), env),
            Some(Source::SteamCover(730))
        );
        assert_eq!(
            source_for_test(&tile(None, app(r"%APPDATA%\x.lnk")), env),
            Some(Source::Shell(PathBuf::from(
                r"C:\Users\A\AppData\Roaming\x.lnk"
            )))
        );
        let web = Target::Web {
            url: "https://a.fr".into(),
        };
        assert_eq!(
            source_for_test(&tile(None, web), env),
            Some(Source::Favicon("https://a.fr".into()))
        );
        let bad = Target::Web {
            url: "file:///c".into(),
        };
        assert_eq!(source_for_test(&tile(None, bad), env), None);
    }

    #[test]
    fn le_champ_icon_remplace_la_source_automatique() {
        let steam = || Target::Steam { app_id: 1 };
        assert_eq!(
            source_for_test(&tile(Some(r"%APPDATA%\i.png"), steam()), env),
            Some(Source::ImageFile(PathBuf::from(
                r"C:\Users\A\AppData\Roaming\i.png"
            )))
        );
        assert_eq!(
            source_for_test(&tile(Some("https://x/i.png"), steam()), env),
            Some(Source::ImageUrl("https://x/i.png".into()))
        );
        assert_eq!(
            source_for_test(&tile(Some(r"C:\a\Jeu.EXE"), steam()), env),
            Some(Source::Shell(PathBuf::from(r"C:\a\Jeu.EXE")))
        );
        // Chemin relatif : par rapport au dossier de config.json.
        assert_eq!(
            source_for_test(&tile(Some(r"images\a.png"), steam()), env),
            Some(Source::ImageFile(
                Path::new("/config").join(r"images\a.png")
            ))
        );
        // Un `"icon": ""` est ignoré.
        assert_eq!(
            source_for_test(&tile(Some("  "), steam()), env),
            Some(Source::SteamCover(1))
        );
    }

    #[test]
    fn un_nom_seul_est_cherche_dans_le_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("outil.exe"), b"").unwrap();
        let path_var = dir.path().to_string_lossy().to_string();
        let env = |name: &str| (name == "PATH").then(|| path_var.clone());
        assert_eq!(
            resolve_in_path("outil".into(), env),
            dir.path().join("outil.exe")
        );
        assert_eq!(
            resolve_in_path("outil.exe".into(), env),
            dir.path().join("outil.exe")
        );
        // Introuvable : on garde le nom (l'extraction échouera proprement).
        assert_eq!(
            resolve_in_path("absent.exe".into(), env),
            PathBuf::from("absent.exe")
        );
    }

    #[test]
    fn l_empreinte_change_avec_la_source() {
        let a = cache_key("cs2", &Source::SteamCover(730));
        assert_eq!(a, cache_key("cs2", &Source::SteamCover(730)));
        assert_ne!(a, cache_key("cs2", &Source::SteamCover(731)));
        assert!(a.starts_with("cs2-") && a.len() == "cs2-".len() + 16);
        // FNV-1a de la chaîne vide : valeur de référence de l'algorithme.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn reconnait_seulement_les_fichiers_de_la_tuile() {
        assert!(is_cache_file_of("cs-0123456789abcdef.png", "cs"));
        assert!(is_cache_file_of("cs-0123456789abcdef.none", "cs"));
        assert!(!is_cache_file_of("cs-2-0123456789abcdef.png", "cs"));
        assert!(!is_cache_file_of("cs-0123456789abcdef.txt", "cs"));
        assert!(!is_cache_file_of("cs-0123.png", "cs"));
    }

    #[test]
    fn supprime_les_icones_des_tuiles_retirees() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join(DIR_NAME);
        fs::create_dir_all(&cache).unwrap();
        let files = [
            "t-0123456789abcdef.png",
            "t-2-0123456789abcdef.jpg",
            "old-0123456789abcdef.none",
            "notes.txt",
        ];
        for f in files {
            fs::write(cache.join(f), b"x").unwrap();
        }
        // Seule la tuile `t` existe encore (pas `t-2` ni `old`).
        let tiles = [tile(None, Target::Steam { app_id: 1 })];
        remove_orphans(dir.path(), &tiles);

        let mut left: Vec<String> = fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["notes.txt", "t-0123456789abcdef.png"]);
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            w,
            h,
            image::Rgba([200, 10, 10, 255]),
        ))
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
        out
    }

    #[test]
    fn cache_ecrit_relu_puis_remplace() {
        let dir = tempfile::tempdir().unwrap();
        let src1 = Source::SteamCover(1);
        let Lookup::Miss(key1) = cache_lookup(dir.path(), "t", &src1) else {
            panic!("le cache devrait être vide");
        };
        let img = encode_icon(decode(&png_bytes(4, 4)).unwrap()).unwrap();
        cache_store(dir.path(), "t", &key1, Some(&img), true).unwrap();
        assert_eq!(
            cache_lookup(dir.path(), "t", &src1),
            Lookup::Hit(img.clone())
        );

        // Autre source : nouvelle entrée, l'ancienne est supprimée.
        let src2 = Source::SteamCover(2);
        let Lookup::Miss(key2) = cache_lookup(dir.path(), "t", &src2) else {
            panic!("source différente : pas en cache");
        };
        cache_store(dir.path(), "t", &key2, None, true).unwrap();
        assert_eq!(cache_lookup(dir.path(), "t", &src2), Lookup::KnownMissing);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn un_echec_local_n_est_pas_memorise() {
        let dir = tempfile::tempdir().unwrap();
        let src = Source::Shell("absent.exe".into());
        let Lookup::Miss(key) = cache_lookup(dir.path(), "t", &src) else {
            panic!()
        };
        cache_store(dir.path(), "t", &key, None, false).unwrap();
        assert!(matches!(
            cache_lookup(dir.path(), "t", &src),
            Lookup::Miss(_)
        ));
    }

    #[test]
    fn les_grandes_images_sont_reduites() {
        let img = encode_icon(decode(&png_bytes(300, 150)).unwrap()).unwrap();
        let back = decode(&img.bytes).unwrap();
        assert_eq!((back.width(), back.height()), (96, 48));
        // Une petite image garde sa taille.
        let small = decode(
            &encode_icon(decode(&png_bytes(16, 16)).unwrap())
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(small.width(), 16);

        let cover = encode_cover(decode(&png_bytes(600, 900)).unwrap()).unwrap();
        assert_eq!(cover.format, Format::Jpeg);
        let back = decode(&cover.bytes).unwrap();
        assert_eq!((back.width(), back.height()), (160, 240));
    }

    #[test]
    fn decode_refuse_ce_qui_n_est_pas_une_image() {
        assert!(decode(b"<html></html>").is_err());
    }

    #[test]
    fn data_url() {
        let e = Encoded {
            bytes: vec![1, 2, 3],
            format: Format::Png,
        };
        assert_eq!(to_data_url(&e), "data:image/png;base64,AQID");
    }

    #[test]
    fn conversion_des_pixels_windows() {
        // Prémultiplié : rouge à 50 % (BGRA 0,0,128,128) → RGBA 255,0,0,128.
        let mut px = vec![0, 0, 128, 128, 0, 0, 0, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![255, 0, 0, 128, 0, 0, 0, 0]);

        // Sans canal alpha (tout à 0) : rendu opaque.
        let mut px = vec![10, 20, 30, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![30, 20, 10, 255]);

        // Couleur > alpha : pas prémultiplié, couleurs gardées.
        let mut px = vec![200, 0, 0, 100];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![0, 0, 200, 100]);
    }

    #[test]
    fn jaquette_steam_locale_ancien_et_nouveau_format() {
        let steam = tempfile::tempdir().unwrap();
        let cache = steam.path().join("appcache").join("librarycache");
        fs::create_dir_all(cache.join("20").join("abc123")).unwrap();
        fs::write(cache.join("10_library_600x900.jpg"), b"").unwrap();
        fs::write(
            cache.join("20").join("abc123").join("library_600x900.jpg"),
            b"",
        )
        .unwrap();

        assert_eq!(
            find_local_cover(steam.path(), 10),
            Some(cache.join("10_library_600x900.jpg"))
        );
        assert_eq!(
            find_local_cover(steam.path(), 20),
            Some(cache.join("20").join("abc123").join("library_600x900.jpg"))
        );
        assert_eq!(find_local_cover(steam.path(), 30), None);
    }
}
