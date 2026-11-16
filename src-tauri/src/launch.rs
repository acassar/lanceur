//! Lancement d'une tuile : jeu Steam, exécutable, raccourci `.lnk` ou app web.
//!
//! Le travail est coupé en deux temps :
//!
//! 1. [`plan`] traduit la cible de la tuile (`Target`) en une [`Action`]
//!    concrète : quel programme, quels arguments, quel dossier. C'est une
//!    fonction *pure* (elle ne touche pas au système), donc facile à tester.
//! 2. [`run`] exécute l'action : `std::process::Command` pour un `.exe`,
//!    l'API Windows `ShellExecuteExW` pour le reste (URL `steam://`, `.lnk`,
//!    navigateur).
//!
//! Pourquoi deux façons de lancer ?
//! - `Command` démarre directement un exécutable. Il ne sait pas ouvrir un
//!   `.lnk` (ce n'est pas un programme, c'est un fichier qui *décrit* un
//!   programme) ni une URL `steam://`.
//! - `ShellExecuteExW` fait exactement ce que fait l'Explorateur sur un
//!   double-clic : il résout les raccourcis, trouve le programme associé aux
//!   URL `steam://` et connaît le chemin de `msedge.exe` via le registre
//!   (« App Paths »), même s'il n'est pas dans le PATH.

use std::path::{Path, PathBuf};

use crate::config::Target;
use crate::error::AppError;

/// Ce qu'il faut exécuter concrètement. Une `enum` avec un cas par façon de
/// lancer : `run` fera un `match` dessus.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Ouvrir une URL avec le programme associé (`steam://…` → Steam).
    OpenUrl(String),
    /// Démarrer un exécutable, avec son propre dossier comme dossier de travail.
    Spawn {
        program: PathBuf,
        args: Vec<String>,
        dir: Option<PathBuf>,
        /// `true` si `program` est un chemin à vérifier avant de lancer.
        must_exist: bool,
    },
    /// Ouvrir un fichier comme un double-clic (`.lnk`, `.bat`, `.url`…) ou un
    /// programme connu de Windows (`msedge`), avec des paramètres optionnels.
    ShellOpen {
        file: PathBuf,
        /// Ligne de paramètres déjà assemblée (voir [`join_args`]).
        params: String,
        dir: Option<PathBuf>,
        /// `true` si `file` est un chemin à vérifier avant de lancer.
        must_exist: bool,
    },
}

/// Traduit une cible en action.
///
/// - `browser` : réglage `settings.browser` (`msedge`, `chrome`, `brave` ou chemin).
/// - `env` : fonction qui donne la valeur d'une variable d'environnement.
///   En vrai on passe `std::env::var` ; dans les tests, une fausse liste.
///   `impl Fn(&str) -> Option<String>` veut dire « n'importe quelle fonction
///   ou closure qui prend un `&str` et renvoie peut-être une `String` ».
pub fn plan(
    target: &Target,
    browser: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Action, AppError> {
    // `match` sur l'enum `Target` : un bras par type de tuile. Si une variante
    // est ajoutée un jour, le compilateur refusera de compiler tant qu'elle
    // n'est pas traitée ici. C'est le gros avantage sur une suite de `if`.
    match target {
        // `{ app_id }` *déstructure* la variante : on récupère son champ.
        Target::Steam { app_id } => Ok(Action::OpenUrl(format!("steam://rungameid/{app_id}"))),

        Target::App { path, args } => {
            let path = PathBuf::from(expand_env(path, &env));
            let args: Vec<String> = args.iter().map(|a| expand_env(a, &env)).collect();
            // Le dossier de travail d'un `.exe` est le sien : beaucoup de jeux et
            // d'outils cherchent leurs fichiers « à côté » d'eux.
            // `.map(Path::to_path_buf)` transforme `Option<&Path>` en `Option<PathBuf>`.
            let dir = path
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .map(Path::to_path_buf);

            // Un nom seul (`notepad.exe`) est cherché par Windows dans le PATH :
            // on ne vérifie l'existence que pour un vrai chemin.
            let must_exist = path.components().count() > 1;

            if is_exe(&path) {
                Ok(Action::Spawn {
                    program: path,
                    args,
                    dir,
                    must_exist,
                })
            } else {
                Ok(Action::ShellOpen {
                    file: path,
                    params: join_args(&args),
                    // Pas de dossier imposé : un `.lnk` a son propre champ
                    // « Démarrer dans », comme lors d'un double-clic.
                    dir: None,
                    must_exist,
                })
            }
        }

        Target::Web { url } => {
            check_web_url(url)?;
            let (file, must_exist) = browser_program(&expand_env(browser, &env));
            Ok(Action::ShellOpen {
                file,
                // `--app=<url>` : fenêtre dédiée, sans onglets ni barre d'adresse.
                params: join_args(&[format!("--app={url}")]),
                dir: None,
                must_exist,
            })
        }
    }
}

/// Exécute l'action. Ne attend pas la fin du programme lancé.
pub fn run(action: Action) -> Result<(), AppError> {
    match action {
        Action::OpenUrl(url) => shell_execute(Path::new(&url), "", None),

        Action::Spawn {
            program,
            args,
            dir,
            must_exist,
        } => {
            if must_exist {
                ensure_exists(&program)?;
            }
            let mut cmd = std::process::Command::new(&program);
            cmd.args(&args)
                // On ne transmet pas nos entrées/sorties au programme lancé :
                // il vit sa vie, indépendamment du lanceur.
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            // `if let Some(dir) = …` : on n'exécute le bloc que si `dir` a une valeur.
            if let Some(dir) = &dir {
                cmd.current_dir(dir);
            }
            // `spawn` démarre le processus et rend la main tout de suite
            // (contrairement à `output()` ou `status()`, qui attendent la fin).
            // Le `Child` renvoyé est ignoré : le programme continue de tourner
            // même quand ce handle disparaît.
            cmd.spawn().map_err(|e| AppError::Launch {
                target: program.display().to_string(),
                reason: e.to_string(),
            })?;
            Ok(())
        }

        Action::ShellOpen {
            file,
            params,
            dir,
            must_exist,
        } => {
            if must_exist {
                ensure_exists(&file)?;
            }
            shell_execute(&file, &params, dir.as_deref())
        }
    }
}

/// Raccourci : planifie puis exécute.
pub fn launch(target: &Target, browser: &str) -> Result<(), AppError> {
    // `|name| std::env::var(name).ok()` : closure qui lit une variable
    // d'environnement ; `.ok()` transforme le `Result` en `Option`.
    let action = plan(target, browser, |name| std::env::var(name).ok())?;
    run(action)
}

// ---------------------------------------------------------------------------
// Petites fonctions utilitaires (pures, testées plus bas)
// ---------------------------------------------------------------------------

/// Remplace les `%VARIABLE%` par leur valeur, comme l'Explorateur Windows.
/// Une variable inconnue est laissée telle quelle (`%INCONNUE%`).
pub fn expand_env(text: &str, env: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // `while let` : on boucle tant qu'on trouve un `%` dans ce qui reste.
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        // `%NOM%` : on cherche le `%` fermant puis la valeur de la variable.
        // `and_then` n'appelle `env` que si le `%` fermant existe.
        let end = after.find('%');
        let value = end.filter(|&e| e > 0).and_then(|e| env(&after[..e]));
        match (end, value) {
            // Variable connue : on remplace `%NOM%` par sa valeur.
            (Some(end), Some(value)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            // Variable inconnue (ou `%%`) : on recopie `%NOM%` tel quel.
            (Some(end), None) => {
                out.push_str(&rest[start..start + end + 2]);
                rest = &after[end + 1..];
            }
            // Pas de `%` fermant : la fin du texte est recopiée telle quelle.
            (None, _) => {
                rest = &rest[start..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `true` si le chemin se termine par `.exe` (sans tenir compte des majuscules).
fn is_exe(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

/// Programme à lancer pour le réglage `browser`, et s'il faut vérifier qu'il existe.
///
/// Les noms connus deviennent `msedge.exe`, `chrome.exe`… que Windows sait
/// retrouver tout seul. Un chemin complet est utilisé tel quel.
fn browser_program(browser: &str) -> (PathBuf, bool) {
    let name = browser.trim();
    match name.to_ascii_lowercase().as_str() {
        "msedge" | "edge" | "" => ("msedge.exe".into(), false),
        "chrome" => ("chrome.exe".into(), false),
        "brave" => ("brave.exe".into(), false),
        _ => {
            let is_path = name.contains(['\\', '/']);
            (PathBuf::from(name), is_path)
        }
    }
}

/// Refuse les URL qui ne sont pas `http(s)://`, ou qui contiennent des espaces
/// ou des guillemets : elles pourraient ajouter des options au navigateur.
pub fn check_web_url(url: &str) -> Result<(), AppError> {
    let lower = url.to_ascii_lowercase();
    let scheme_ok = lower.starts_with("https://") || lower.starts_with("http://");
    let chars_ok = !url.chars().any(|c| c.is_whitespace() || c == '"');
    if scheme_ok && chars_ok {
        Ok(())
    } else {
        Err(AppError::InvalidUrl(url.to_string()))
    }
}

fn ensure_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        Ok(())
    } else {
        Err(AppError::NotFound(path.to_path_buf()))
    }
}

/// Assemble des arguments en une seule ligne de commande Windows, en ajoutant
/// les guillemets nécessaires (`ShellExecuteExW` attend une seule chaîne).
pub fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Met un argument entre guillemets si besoin, selon les règles de Windows :
/// un `"` dans l'argument devient `\"`, et les `\` placés juste avant un `"`
/// sont doublés. Ex. : `C:\Mon dossier\` → `"C:\Mon dossier\\"`.
fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{b}', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Appel à l'API Windows
// ---------------------------------------------------------------------------

/// Ouvre `file` comme un double-clic dans l'Explorateur.
///
/// `#[cfg(windows)]` : cette version n'est compilée que pour Windows. Sur
/// Linux (où tournent certains tests), c'est la version du dessous qui existe.
#[cfg(windows)]
fn shell_execute(file: &Path, params: &str, dir: Option<&Path>) -> Result<(), AppError> {
    // Les fonctions Windows qui finissent par `W` attendent du texte en UTF-16
    // terminé par un zéro (le format natif de Windows), pas des `String` Rust
    // (UTF-8). On convertit avant l'appel.
    use std::os::windows::ffi::OsStrExt;
    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    let file_w = wide(file.as_os_str());
    let params_w = wide(params.as_ref());
    let dir_w = dir.map(|d| wide(d.as_os_str()));
    let label = file.display().to_string();

    // `ShellExecuteExW` peut passer par des composants COM (extensions de
    // l'Explorateur), qui veulent un thread initialisé pour COM. Les threads
    // de Tauri ne le sont pas forcément : on fait l'appel dans un petit thread
    // dédié, qu'on initialise, puis on attend sa réponse avec `join`.
    // `move` : la closure prend possession des chaînes (le thread peut vivre
    // plus longtemps que la fonction, du point de vue du compilateur).
    let result = std::thread::spawn(move || -> Result<(), String> {
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        };
        use windows::Win32::UI::Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
        };
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::PCWSTR;

        // `unsafe` : le compilateur ne peut pas vérifier ce que fait une
        // fonction écrite en C. On promet ici que les pointeurs passés sont
        // valides : ils viennent des `Vec<u16>` ci-dessus, qui vivent jusqu'à
        // la fin de ce bloc, donc pendant tout l'appel.
        unsafe {
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);

            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                // NO_UI : pas de boîte de dialogue d'erreur Windows, on affiche
                // l'erreur nous-mêmes dans le toast. NOASYNC : on attend que
                // l'ouverture soit vraiment lancée avant de rendre la main.
                fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
                lpVerb: PCWSTR::null(), // verbe par défaut, comme un double-clic
                lpFile: PCWSTR(file_w.as_ptr()),
                lpParameters: PCWSTR(params_w.as_ptr()),
                lpDirectory: dir_w
                    .as_ref()
                    .map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
                nShow: SW_SHOWNORMAL.0,
                // `..Default::default()` : tous les autres champs à zéro.
                ..Default::default()
            };
            let outcome = ShellExecuteExW(&mut info).map_err(|e| e.message());

            // On ne libère COM que si notre appel l'a bien initialisé.
            if com.is_ok() {
                CoUninitialize();
            }
            outcome
        }
    })
    .join()
    // `join` échoue seulement si le thread a paniqué.
    .unwrap_or_else(|_| Err("erreur interne".to_string()));

    result.map_err(|reason| AppError::Launch {
        target: label,
        reason,
    })
}

/// Version hors Windows : le lanceur ne vise que Windows. Elle existe pour
/// que le projet compile (et que les tests tournent) sur Linux/macOS.
#[cfg(not(windows))]
fn shell_execute(file: &Path, _params: &str, _dir: Option<&Path>) -> Result<(), AppError> {
    Err(AppError::Launch {
        target: file.display().to_string(),
        reason: "disponible uniquement sous Windows".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fausse table de variables d'environnement pour les tests.
    fn env(name: &str) -> Option<String> {
        match name {
            "APPDATA" => Some(r"C:\Users\A\AppData\Roaming".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\A\AppData\Local".into()),
            _ => None,
        }
    }

    fn app(path: &str, args: &[&str]) -> Target {
        Target::App {
            path: path.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn steam_ouvre_l_url_rungameid() {
        let action = plan(&Target::Steam { app_id: 730 }, "msedge", env).unwrap();
        assert_eq!(action, Action::OpenUrl("steam://rungameid/730".into()));
    }

    // Chemins Windows (`\`) : ce test ne tourne que sous Windows (dans la CI).
    #[test]
    #[cfg_attr(not(windows), ignore)]
    fn exe_lance_directement_dans_son_dossier() {
        let target = app(
            r"%LOCALAPPDATA%\Discord\Update.exe",
            &["--processStart", "Discord.exe"],
        );
        assert_eq!(
            plan(&target, "msedge", env).unwrap(),
            Action::Spawn {
                program: r"C:\Users\A\AppData\Local\Discord\Update.exe".into(),
                args: vec!["--processStart".into(), "Discord.exe".into()],
                dir: Some(r"C:\Users\A\AppData\Local\Discord".into()),
                must_exist: true,
            }
        );
        // L'extension est reconnue quelle que soit la casse.
        assert!(matches!(
            plan(&app("X.EXE", &[]), "", env).unwrap(),
            Action::Spawn { .. }
        ));
    }

    // Chemins Windows (`\`) : ce test ne tourne que sous Windows (dans la CI).
    #[test]
    #[cfg_attr(not(windows), ignore)]
    fn lnk_passe_par_le_shell() {
        let target = app(
            r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Code.lnk",
            &["--new-window", "C:\\Mon projet"],
        );
        assert_eq!(
            plan(&target, "msedge", env).unwrap(),
            Action::ShellOpen {
                file: r"C:\Users\A\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Code.lnk"
                    .into(),
                params: r#"--new-window "C:\Mon projet""#.into(),
                // Pas de dossier imposé : le raccourci a son « Démarrer dans ».
                dir: None,
                must_exist: true,
            }
        );
    }

    #[test]
    fn web_ouvre_le_navigateur_en_mode_app() {
        let web = Target::Web {
            url: "https://www.instagram.com".into(),
        };
        assert_eq!(
            plan(&web, "msedge", env).unwrap(),
            Action::ShellOpen {
                file: "msedge.exe".into(),
                params: "--app=https://www.instagram.com".into(),
                dir: None,
                must_exist: false,
            }
        );
        let Action::ShellOpen {
            file, must_exist, ..
        } = plan(&web, "Chrome", env).unwrap()
        else {
            panic!("action inattendue");
        };
        assert_eq!((file, must_exist), (PathBuf::from("chrome.exe"), false));

        let Action::ShellOpen {
            file, must_exist, ..
        } = plan(&web, r"%LOCALAPPDATA%\Vivaldi\Application\vivaldi.exe", env).unwrap()
        else {
            panic!("action inattendue");
        };
        assert_eq!(
            file,
            PathBuf::from(r"C:\Users\A\AppData\Local\Vivaldi\Application\vivaldi.exe")
        );
        assert!(must_exist);
    }

    #[test]
    fn un_nom_seul_est_cherche_par_windows() {
        // Pas de vérification d'existence : Windows le cherchera dans le PATH.
        let Action::Spawn {
            must_exist, dir, ..
        } = plan(&app("notepad.exe", &[]), "", env).unwrap()
        else {
            panic!("action inattendue");
        };
        assert!(!must_exist);
        assert_eq!(dir, None);
    }

    #[test]
    fn web_refuse_les_url_dangereuses() {
        for url in [
            "file:///C:/x",
            "javascript:alert(1)",
            "https://x.com --disable-web-security",
            "https://x\"y",
        ] {
            let web = Target::Web { url: url.into() };
            assert!(
                matches!(plan(&web, "msedge", env), Err(AppError::InvalidUrl(_))),
                "{url} aurait dû être refusée"
            );
        }
        assert!(
            plan(
                &Target::Web {
                    url: "HTTP://exemple.fr/a?b=c&d=e".into()
                },
                "msedge",
                env
            )
            .is_ok()
        );
    }

    #[test]
    fn variables_d_environnement() {
        assert_eq!(
            expand_env(r"%APPDATA%\x", env),
            r"C:\Users\A\AppData\Roaming\x"
        );
        assert_eq!(expand_env("%INCONNUE%\\x", env), "%INCONNUE%\\x");
        assert_eq!(expand_env("100% sûr", env), "100% sûr");
        assert_eq!(expand_env("%%APPDATA%", env), "%%APPDATA%");
        assert_eq!(expand_env("%INCONNUE%APPDATA%", env), "%INCONNUE%APPDATA%");
        assert_eq!(expand_env("sans variable", env), "sans variable");
    }

    #[test]
    fn guillemets_windows() {
        assert_eq!(quote_arg("simple"), "simple");
        assert_eq!(quote_arg(""), r#""""#);
        assert_eq!(quote_arg("a b"), r#""a b""#);
        assert_eq!(quote_arg(r#"dit "oui""#), r#""dit \"oui\"""#);
        assert_eq!(quote_arg(r"C:\Mon dossier\"), r#""C:\Mon dossier\\""#);
        assert_eq!(quote_arg(r"C:\sans\espace\"), r"C:\sans\espace\");
    }

    #[test]
    fn un_exe_absent_donne_une_erreur_claire() {
        let action = Action::Spawn {
            program: "/introuvable/jeu.exe".into(),
            args: vec![],
            dir: None,
            must_exist: true,
        };
        assert!(matches!(run(action), Err(AppError::NotFound(_))));
    }
}
