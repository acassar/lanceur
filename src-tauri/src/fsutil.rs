//! Écriture *atomique* d'un fichier, partagée par `state.json` et `config.json`.
//!
//! Écrire directement dans `config.json` est risqué : si l'app est coupée (ou
//! le PC s'éteint) au milieu de l'écriture, le fichier reste à moitié écrit et
//! votre config est perdue. On écrit donc d'abord le contenu complet dans un
//! fichier temporaire *du même dossier*, puis on le renomme par-dessus
//! l'original. Le renommage est atomique : à tout instant, le fichier contient
//! soit l'ancienne version complète, soit la nouvelle, jamais un mélange.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Remplace le contenu de `path` par `contents`, de façon atomique.
///
/// `contents: &[u8]` : une *tranche d'octets* empruntée. Un `&str` se
/// convertit avec `.as_bytes()`, sans copie.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), AppError> {
    // Si `config.json` est un lien symbolique (vers un dossier synchronisé…),
    // on écrit dans le vrai fichier pour ne pas remplacer le lien par une copie.
    // `canonicalize` échoue si le fichier n'existe pas encore : on garde alors
    // le chemin tel quel.
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();

    // Nom temporaire propre à ce processus (`id()` = numéro du processus) :
    // si deux copies de l'app tournaient, elles n'écriraient pas le même fichier.
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));

    // Fabrique d'erreurs : `io_err(&chemin)` renvoie une closure qui transforme
    // une `std::io::Error` en `AppError::Io` pour ce chemin.
    let io_err = |p: &Path| {
        let p = p.to_path_buf();
        move |source| AppError::Io {
            action: "d'écrire",
            path: p,
            source,
        }
    };

    fs::create_dir_all(dir).map_err(io_err(dir))?;

    // Closure appelée aussitôt (le `()` final) : à l'intérieur, `?` sort de la
    // closure et non de la fonction, ce qui permet de nettoyer juste après.
    // `file` est fermé à la fin de la closure, avant le renommage : Windows
    // refuse de renommer un fichier encore ouvert.
    let written = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        // `sync_all` force l'écriture sur le disque (et pas seulement dans le
        // cache du système) avant le renommage.
        file.sync_all()
    })();
    if let Err(source) = written {
        // On ne laisse pas traîner un fichier temporaire à moitié écrit.
        let _ = fs::remove_file(&tmp);
        return Err(io_err(&tmp)(source));
    }

    if let Err(source) = rename_with_retry(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(&path)(source));
    }
    Ok(())
}

/// Renomme `from` en `to` (en remplaçant `to`). Sous Windows, un antivirus ou
/// l'indexation peuvent garder le fichier ouvert une fraction de seconde : le
/// renommage échoue alors avec « accès refusé ». On réessaie quelques fois.
fn rename_with_retry(from: &Path, to: &PathBuf) -> std::io::Result<()> {
    const ATTEMPTS: u32 = 5;
    let mut attempt = 1;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == ErrorKind::PermissionDenied && attempt < ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(50 * u64::from(attempt)));
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecrit_puis_remplace_sans_laisser_de_fichier_temporaire() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_atomic(&path, b"un").unwrap();
        write_atomic(&path, b"deux").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "deux");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn cree_le_dossier_s_il_manque() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sous").join("a.json");
        write_atomic(&path, b"x").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "x");
    }

    #[test]
    fn une_erreur_laisse_l_original_intact() {
        let dir = tempfile::tempdir().unwrap();
        // Un dossier à la place du fichier : le renommage échoue.
        let path = dir.path().join("a.json");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("garde"), "x").unwrap();
        assert!(write_atomic(&path, b"y").is_err());
        assert!(path.join("garde").exists());
        // Le fichier temporaire a été supprimé.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
