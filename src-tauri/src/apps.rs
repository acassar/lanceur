//! Détection des applications installées (écran d'accueil, menu de l'icône
//! de notification « Détecter les applications… »).
//!
//! Comme pour Steam, deux étapes séparées :
//!
//! 1. [`detect`] *lit* les raccourcis du menu Démarrer, sans rien modifier :
//!    - celui de tous les utilisateurs : `%ProgramData%\Microsoft\Windows\Start Menu\Programs` ;
//!    - le vôtre : `%APPDATA%\Microsoft\Windows\Start Menu\Programs`.
//!
//!    Ce sont les raccourcis que les installeurs créent : c'est la liste la plus
//!    fiable des « vraies » applications. On écarte ce qui n'en est pas
//!    (désinstalleurs, aides, sites web, outils d'administration de Windows).
//! 2. [`import`] ajoute les raccourcis choisis à `config.json` : une tuile `app`
//!    par raccourci (le `.lnk` lui-même, que le lanceur sait déjà ouvrir), dans
//!    une catégorie « Applications » (ou « Outils » si elle existe). Ce qui a
//!    déjà une tuile est ignoré ; rien n'est supprimé ni remplacé.
//!
//! Notions Rust abordées : la récursion sur des dossiers, `strip_prefix` sur
//! des chemins, et `#[cfg(windows)]` pour du code qui n'existe que sous Windows.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{self, Config, Target};
use crate::editor::{self, TileDraft};
use crate::error::AppError;
use crate::launch::expand_env;
use crate::steam::ImportResult;

/// Un raccourci trouvé, tel que le front l'affiche dans la liste à cocher.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedApp {
    /// Nom du raccourci, sans `.lnk` (« Discord »).
    pub name: String,
    /// Chemin enregistré dans la tuile, avec `%APPDATA%` ou `%ProgramData%`
    /// au début : plus court à lire dans config.json, et toujours valable si
    /// le dossier de l'utilisateur change de nom.
    pub path: String,
    /// Sous-dossier du menu Démarrer (« Discord Inc »), `None` à la racine.
    pub folder: Option<String>,
    /// Une tuile lance déjà cette application (même raccourci ou même nom).
    pub in_config: bool,
}

/// Catégorie utilisée quand la config n'en a aucune pour les applications.
const APPS_NAME: &str = "Applications";

/// Noms de catégorie existante qui conviennent aux applications (en minuscules).
const APPS_CATEGORIES: &[&str] = &[
    "applications",
    "apps",
    "programmes",
    "logiciels",
    "outils",
    "tools",
];

/// Un dossier du menu Démarrer à parcourir : son chemin réel, et la façon
/// de l'écrire dans config.json.
struct Root {
    dir: PathBuf,
    /// Début du chemin enregistré (`%APPDATA%\Microsoft\…\Programs`).
    written: String,
}

const PROGRAMS: &str = r"Microsoft\Windows\Start Menu\Programs";

// ---------------------------------------------------------------------------
// Détection
// ---------------------------------------------------------------------------

/// Applications installées (raccourcis du menu Démarrer), triées par nom.
/// Ne modifie rien. `config` sert seulement à marquer celles déjà présentes.
pub fn detect(config: Option<&Config>) -> Vec<DetectedApp> {
    // `filter_map` : on garde les variables d'environnement qui existent.
    // Hors Windows (développement), aucune n'existe : la liste est vide.
    let roots: Vec<Root> = [("ProgramData", "%ProgramData%"), ("APPDATA", "%APPDATA%")]
        .into_iter()
        .filter_map(|(var, written)| {
            let base = std::env::var_os(var)?;
            Some(Root {
                dir: Path::new(&base).join(PROGRAMS),
                written: format!(r"{written}\{PROGRAMS}"),
            })
        })
        .collect();
    let mut apps = shortcuts(&roots, &win::shortcut_target);
    if let Some(config) = config {
        mark_in_config(&mut apps, config);
    }
    apps
}

/// Le travail de [`detect`], testable sans Windows : `target` donne la cible
/// d'un raccourci (`None` si elle est inconnue).
fn shortcuts(roots: &[Root], target: &dyn Fn(&Path) -> Option<PathBuf>) -> Vec<DetectedApp> {
    let mut found = Vec::new();
    for root in roots {
        collect(&root.dir, &root.dir, root, target, 0, &mut found);
    }
    // Même nom dans les deux menus (installé pour tous *et* pour vous) : on
    // n'en garde qu'un. `insert` renvoie `false` pour un nom déjà vu.
    let mut seen = HashSet::new();
    found.retain(|a| seen.insert(a.name.to_lowercase()));
    found.sort_by_cached_key(|a| a.name.to_lowercase());
    found
}

/// Profondeur maximale de sous-dossiers parcourus (le menu Démarrer n'en a
/// en pratique qu'un ou deux niveaux ; la limite évite toute boucle).
const MAX_DEPTH: usize = 4;

/// Parcourt `dir` (et ses sous-dossiers) à la recherche de `.lnk`.
///
/// Fonction *récursive* : elle s'appelle elle-même pour chaque sous-dossier.
/// `found` est emprunté en écriture (`&mut`) : tous les appels remplissent la
/// même liste, sans la copier.
fn collect(
    dir: &Path,
    base: &Path,
    root: &Root,
    target: &dyn Fn(&Path) -> Option<PathBuf>,
    depth: usize,
    found: &mut Vec<DetectedApp>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if depth < MAX_DEPTH && !is_skipped_folder(&file_name) {
                collect(&path, base, root, target, depth + 1, found);
            }
            continue;
        }
        // Seulement les `.lnk` : les `.url` sont des liens web ou des jeux
        // Steam (ceux-ci sont proposés à part, avec leur vrai nom).
        let Some(name) = strip_suffix_ignore_case(&file_name, ".lnk") else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || is_junk_name(name) {
            continue;
        }
        if target(&path).is_some_and(|t| is_junk_target(&t)) {
            continue;
        }
        // `strip_prefix` : le chemin relatif au dossier du menu Démarrer
        // (`Discord Inc\Discord.lnk`). Il ne peut pas échouer ici, mais on
        // passe le raccourci plutôt que de paniquer si c'était le cas.
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let parts: Vec<String> = relative
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let folder = (parts.len() > 1).then(|| parts[..parts.len() - 1].join(" › "));
        found.push(DetectedApp {
            name: name.to_string(),
            path: format!(r"{}\{}", root.written, parts.join(r"\")),
            folder,
            in_config: false,
        });
    }
}

/// `name` sans `suffix` à la fin, sans tenir compte des majuscules (`.LNK`).
fn strip_suffix_ignore_case<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = name.len().checked_sub(suffix.len())?;
    // `is_char_boundary` : on ne coupe pas au milieu d'une lettre accentuée.
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(suffix)).then(|| &name[..cut])
}

/// Dossiers du menu Démarrer qui ne contiennent pas d'applications à lancer
/// au quotidien (outils de Windows, programmes lancés à l'ouverture de session).
fn is_skipped_folder(name: &str) -> bool {
    const SKIPPED: &[&str] = &[
        "startup",
        "démarrage",
        "administrative tools",
        "outils d'administration",
        "outils d'administration windows",
        "windows tools",
        "outils windows",
        "accessibility",
        "accessibilité",
        "options d'ergonomie",
        "system tools",
        "outils système",
        "windows powershell",
        "maintenance",
    ];
    SKIPPED.contains(&name.trim().to_lowercase().as_str())
}

/// Raccourci qui n'est pas une application : désinstalleur, aide, site web…
fn is_junk_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Morceaux de texte qui suffisent, où qu'ils soient dans le nom.
    const PHRASES: &[&str] = &[
        "uninstall",
        "désinstall",
        "desinstall",
        "readme",
        "read me",
        "lisez-moi",
        "lisezmoi",
        "documentation",
        "release notes",
        "notes de version",
        "changelog",
        "what's new",
        "nouveautés",
        "website",
        "web site",
        "site web",
        "site internet",
        "online help",
        "aide en ligne",
        "user guide",
        "guide de l'utilisateur",
    ];
    // Mots entiers seulement : « Aide » mais pas « Aide-mémoire »… et surtout
    // pas « Help » dans un nom d'application comme « Helpdesk ».
    const WORDS: &[&str] = &[
        "help", "aide", "manual", "manuel", "faq", "license", "licence", "support", "uninst",
    ];
    if PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| WORDS.contains(&word))
}

/// Cible qui n'est pas un programme : document, page web, dossier, désinstalleur.
fn is_junk_target(target: &Path) -> bool {
    // Découpage à la main (et non `file_name()`) : les tests tournent aussi
    // sous Linux, où `\` n'est pas un séparateur de dossiers.
    let text = target.to_string_lossy().to_lowercase();
    let file = text.rsplit(['\\', '/']).next().unwrap_or("");
    // `unins000.exe` (Inno Setup), `uninstall.exe`, `uninst.exe`…
    if file.starts_with("unins") {
        return true;
    }
    let ext = file.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    const DOCUMENTS: &[&str] = &[
        "chm", "hlp", "pdf", "txt", "rtf", "md", "doc", "docx", "xps", "htm", "html", "url", "ini",
        "log",
    ];
    DOCUMENTS.contains(&ext) || target.is_dir()
}

/// Marque les applications qui ont déjà une tuile : même raccourci (une fois
/// les `%VARIABLES%` remplacées), ou une tuile `app` du même nom (Discord
/// ajouté à la main avec le chemin de son .exe, par exemple).
fn mark_in_config(apps: &mut [DetectedApp], config: &Config) {
    let env = |name: &str| std::env::var(name).ok();
    let mut paths = HashSet::new();
    let mut names = HashSet::new();
    for tile in &config.tiles {
        if let Target::App { path, .. } = &tile.target {
            paths.insert(normalize(&expand_env(path, env)));
            names.insert(tile.name.trim().to_lowercase());
        }
    }
    for app in apps {
        app.in_config = paths.contains(&normalize(&expand_env(&app.path, env)))
            || names.contains(&app.name.to_lowercase());
    }
}

/// Chemin comparable : Windows ne distingue ni les majuscules ni `/` de `\`.
fn normalize(path: &str) -> String {
    path.trim().replace('/', "\\").to_lowercase()
}

// ---------------------------------------------------------------------------
// Import dans config.json
// ---------------------------------------------------------------------------

/// Ajoute `apps` à `config.json` (dans `dir`) et renvoie ce qui a été ajouté.
///
/// L'écriture passe par l'éditeur de tuiles (`editor::add_tiles_with`) : même
/// verrou, même vérification du fichier avant de l'écrire, et la liste des
/// tuiles à ajouter est calculée sur la config lue *sous* ce verrou.
pub fn import(dir: &Path, apps: &[DetectedApp]) -> Result<ImportResult, AppError> {
    // Rempli par la closure ci-dessous (`move` n'est pas utilisé : la closure
    // *emprunte* `category` en écriture le temps de l'appel).
    let mut category = String::new();
    let added = editor::add_tiles_with(dir, |config| {
        let (name, drafts) = plan(config, apps);
        category = name;
        drafts
    })?;
    Ok(ImportResult {
        added: added.into_iter().map(|t| t.id).collect(),
        category,
    })
}

/// Catégorie (son nom) et brouillons de tuiles, en sautant ce qui existe déjà.
fn plan(config: &Config, apps: &[DetectedApp]) -> (String, Vec<TileDraft>) {
    let existing = config
        .categories
        .iter()
        .find(|c| APPS_CATEGORIES.contains(&c.id.as_str()))
        .or_else(|| {
            config
                .categories
                .iter()
                .find(|c| APPS_CATEGORIES.contains(&c.name.trim().to_lowercase().as_str()))
        });
    let (category_id, new_category, name) = match existing {
        Some(c) => (c.id.clone(), None, c.name.clone()),
        // Nouvelle catégorie : l'éditeur choisit son id (`applications`).
        None => (
            String::new(),
            Some(APPS_NAME.to_string()),
            APPS_NAME.to_string(),
        ),
    };

    let mut fresh: Vec<DetectedApp> = apps.to_vec();
    mark_in_config(&mut fresh, config);
    // Deux fois le même raccourci dans la demande : un seul ajout.
    let mut seen = HashSet::new();
    let drafts = fresh
        .into_iter()
        .filter(|a| !a.in_config && seen.insert(normalize(&a.path)))
        .map(|a| TileDraft {
            name: a.name,
            category: category_id.clone(),
            new_category: new_category.clone(),
            icon: None,
            target: Target::App {
                path: a.path,
                args: Vec::new(),
            },
        })
        .collect();
    (name, drafts)
}

/// Lit `config.json` sans le créer ni échouer : pour marquer les applications
/// déjà présentes, une config absente ou invalide revient à « aucune tuile ».
pub fn current_config(dir: &Path) -> Option<Config> {
    config::load_cached(dir).ok()
}

// ---------------------------------------------------------------------------
// Cible d'un raccourci (Windows uniquement)
// ---------------------------------------------------------------------------

/// `#[cfg(windows)]` : ce module n'est compilé que pour Windows. L'autre
/// version, en dessous, sert au développement sous Linux/macOS.
#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{Interface, PCWSTR};

    /// Cible d'un `.lnk` (`C:\…\Discord.exe`), lue avec `IShellLinkW`, l'objet
    /// COM de Windows qui décrit un raccourci. `None` si elle est inconnue :
    /// c'est le cas des raccourcis « annoncés » d'Office (installés par MSI),
    /// qu'on garde donc.
    pub fn shortcut_target(lnk: &Path) -> Option<PathBuf> {
        // Chemin en UTF-16 terminé par 0, le format des API Windows.
        let lnk_w: Vec<u16> = lnk.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY : `lnk_w` vit jusqu'à la fin de la fonction ; COM est
        // initialisé avant les objets COM, et libéré (si on l'a initialisé)
        // après leur destruction, à la fin de la closure.
        unsafe {
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let result = (|| -> windows::core::Result<Option<PathBuf>> {
                let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                // `cast` : demander à l'objet une autre de ses interfaces
                // (comme un `instanceof` qui renverrait l'objet sous ce type).
                let file: IPersistFile = link.cast()?;
                file.Load(PCWSTR(lnk_w.as_ptr()), STGM_READ)?;
                let mut buf = [0u16; 1024];
                link.GetPath(&mut buf, std::ptr::null_mut(), 0)?;
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                Ok((len > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len]))))
            })();
            if com.is_ok() {
                CoUninitialize();
            }
            result.ok().flatten()
        }
    }
}

#[cfg(not(windows))]
mod win {
    use std::path::{Path, PathBuf};

    /// Hors Windows, pas de raccourcis à lire : cible toujours inconnue.
    pub fn shortcut_target(_lnk: &Path) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    fn root(dir: &Path, written: &str) -> Root {
        Root {
            dir: dir.to_path_buf(),
            written: written.into(),
        }
    }

    #[test]
    fn noms_ecartes() {
        for junk in [
            "Uninstall Discord",
            "Désinstaller VLC",
            "VLC Readme",
            "Lisez-moi",
            "GIMP Help",
            "Aide de 7-Zip",
            "Notepad++ Website",
            "License",
            "Release Notes",
        ] {
            assert!(is_junk_name(junk), "{junk}");
        }
        for app in [
            "Discord",
            "Helpdesk Pro",
            "Visual Studio Code",
            "Aidez-moi",
            "OBS Studio",
        ] {
            assert!(!is_junk_name(app), "{app}");
        }
    }

    #[test]
    fn cibles_ecartees() {
        assert!(is_junk_target(Path::new(
            r"C:\Program Files\X\unins000.exe"
        )));
        assert!(is_junk_target(Path::new(r"C:\Program Files\X\manual.PDF")));
        assert!(is_junk_target(Path::new(r"C:\Program Files\X\aide.chm")));
        assert!(!is_junk_target(Path::new(r"C:\Program Files\X\x.exe")));
        // Dossier : un raccourci qui ouvre un dossier n'est pas une application.
        assert!(is_junk_target(&std::env::temp_dir()));
    }

    #[test]
    fn suffixe_lnk() {
        assert_eq!(
            strip_suffix_ignore_case("Discord.lnk", ".lnk"),
            Some("Discord")
        );
        assert_eq!(strip_suffix_ignore_case("X.LNK", ".lnk"), Some("X"));
        assert_eq!(strip_suffix_ignore_case("X.url", ".lnk"), None);
        assert_eq!(strip_suffix_ignore_case("é", ".lnk"), None);
        assert_eq!(strip_suffix_ignore_case("aé.lnk", ".lnk"), Some("aé"));
    }

    #[test]
    fn parcourt_les_deux_menus() {
        let tmp = tempfile::tempdir().unwrap();
        let all = tmp.path().join("all");
        let me = tmp.path().join("me");
        touch(&all.join("Steam").join("Steam.lnk"));
        touch(&all.join("Steam").join("Counter-Strike 2.url"));
        touch(&all.join("VLC").join("VLC media player.lnk"));
        touch(&all.join("VLC").join("Documentation.lnk"));
        touch(&all.join("VLC").join("Uninstall VLC.lnk"));
        touch(&all.join("Windows Tools").join("Services.lnk"));
        touch(&all.join("Firefox.lnk"));
        touch(&all.join("Outil.lnk")); // cible : unins000.exe (voir `target`)
        touch(&me.join("Discord Inc").join("Discord.lnk"));
        touch(&me.join("Firefox.lnk")); // déjà dans le menu de tous
        touch(&me.join("Startup").join("Agent.lnk"));
        touch(
            &me.join("a")
                .join("b")
                .join("c")
                .join("d")
                .join("e")
                .join("Trop loin.lnk"),
        );
        touch(&me.join("a").join("Jeu.lnk"));

        let roots = [root(&all, "%ProgramData%\\P"), root(&me, "%APPDATA%\\P")];
        let target = |p: &Path| {
            p.ends_with("Outil.lnk")
                .then(|| PathBuf::from(r"C:\X\unins000.exe"))
        };
        let apps = shortcuts(&roots, &target);
        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["Discord", "Firefox", "Jeu", "Steam", "VLC media player"]
        );
        let discord = &apps[0];
        assert_eq!(discord.path, r"%APPDATA%\P\Discord Inc\Discord.lnk");
        assert_eq!(discord.folder.as_deref(), Some("Discord Inc"));
        assert_eq!(apps[1].path, r"%ProgramData%\P\Firefox.lnk");
        assert_eq!(apps[1].folder, None);
        // Dossier absent : rien, sans erreur.
        assert!(shortcuts(&[root(&tmp.path().join("absent"), "x")], &target).is_empty());
    }

    fn app(name: &str, path: &str) -> DetectedApp {
        DetectedApp {
            name: name.into(),
            path: path.into(),
            folder: None,
            in_config: false,
        }
    }

    fn parse(text: &str) -> Config {
        config::parse(text, Path::new("config.json")).unwrap()
    }

    #[test]
    fn marque_ce_qui_a_deja_une_tuile() {
        let config = parse(
            r#"{ "version": 1, "categories": [{ "id": "o", "name": "Outils" }], "tiles": [
              { "id": "d", "name": "discord", "category": "o", "type": "app", "path": "C:\\D\\Update.exe" },
              { "id": "v", "name": "Lecteur", "category": "o", "type": "app", "path": "c:/menu/vlc.LNK" },
              { "id": "s", "name": "Steam", "category": "o", "type": "steam", "appId": 1 }
            ] }"#,
        );
        let mut apps = vec![
            app("Discord", r"C:\menu\Discord.lnk"),
            app("VLC media player", r"C:\Menu\VLC.lnk"),
            // Une tuile Steam du même nom ne lance pas l'app Steam.
            app("Steam", r"C:\menu\Steam.lnk"),
        ];
        mark_in_config(&mut apps, &config);
        let marks: Vec<bool> = apps.iter().map(|a| a.in_config).collect();
        assert_eq!(marks, [true, true, false]);
    }

    #[test]
    fn import_cree_applications_sans_doublon() {
        let dir = tempfile::tempdir().unwrap();
        let firefox = app("Firefox", r"%ProgramData%\P\Firefox.lnk");
        let result = import(
            dir.path(),
            &[
                firefox.clone(),
                firefox.clone(),
                app("Discord", r"%APPDATA%\P\Discord.lnk"),
            ],
        )
        .unwrap();
        assert_eq!(result.added, ["firefox", "discord"]);
        assert_eq!(result.category, "Applications");

        let config = config::load_or_create(dir.path()).unwrap();
        assert_eq!(config.categories.len(), 1);
        assert_eq!(config.categories[0].id, "applications");
        assert_eq!(
            config.tiles[0].target,
            Target::App {
                path: r"%ProgramData%\P\Firefox.lnk".into(),
                args: vec![]
            }
        );

        // Second passage : déjà là, rien n'est ajouté ni réécrit.
        let path = dir.path().join(config::FILE_NAME);
        let before = fs::read_to_string(&path).unwrap();
        let again = import(dir.path(), &[firefox]).unwrap();
        assert!(again.added.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn reprend_la_categorie_outils() {
        let config =
            parse(r#"{ "version": 1, "categories": [{ "id": "o", "name": " Outils " }] }"#);
        let (name, drafts) = plan(&config, &[app("X", "x.lnk")]);
        assert_eq!(name, " Outils ");
        assert_eq!(drafts[0].category, "o");
        assert_eq!(drafts[0].new_category, None);
    }
}
