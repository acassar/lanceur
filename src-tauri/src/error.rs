//! Type d'erreur unique de l'application.
//!
//! En Rust, une fonction qui peut échouer renvoie un `Result<T, E>` : soit
//! `Ok(valeur)`, soit `Err(erreur)`. Pas d'exception qui « remonte toute seule » :
//! l'erreur est une valeur comme une autre, et le compilateur oblige à la traiter.
//!
//! On regroupe ici toutes les erreurs possibles dans une seule `enum` (`AppError`).
//! Chaque variante décrit un cas précis et porte les informations utiles
//! (chemin du fichier, ligne, colonne…).

use std::path::PathBuf;

use serde::ser::{Serialize, SerializeStruct, Serializer};

/// `#[derive(Debug, thiserror::Error)]` génère automatiquement :
/// - `Debug` : un affichage technique, pour les logs ;
/// - `Display` (via `thiserror`) : le message lisible, écrit dans `#[error("…")]`.
///   Les `{path}`, `{line}`… sont remplacés par les champs de la variante.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Tauri n'a pas su donner le dossier `%APPDATA%\io.github.acassar.lanceur`.
    #[error("Dossier de configuration introuvable : {0}")]
    NoConfigDir(String),

    /// Erreur d'entrée/sortie : fichier illisible, disque plein, droits…
    /// `#[source]` garde l'erreur d'origine (`std::io::Error`) attachée à la nôtre.
    #[error("Impossible {action} {} : {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Le fichier n'est pas encodé en UTF-8 (ex. « ANSI » ou « Unicode » dans le Bloc-notes).
    #[error(
        "config.json n'est pas enregistré en UTF-8. Dans le Bloc-notes : Fichier > Enregistrer sous, Encodage « UTF-8 »."
    )]
    Encoding { path: PathBuf },

    /// Le fichier n'est pas du JSON valide, ou un champ manque / a le mauvais type.
    #[error("config.json, ligne {line}, colonne {column} : {message}")]
    Json {
        path: PathBuf,
        line: usize,
        column: usize,
        message: String,
    },

    /// Le JSON est bien formé mais incohérent (id en double, catégorie inconnue…).
    /// On liste tous les problèmes d'un coup, pour tout corriger en une fois.
    #[error("config.json contient des erreurs :\n- {}", problems.join("\n- "))]
    Invalid {
        path: PathBuf,
        problems: Vec<String>,
    },

    /// Le front a demandé une tuile qui n'existe pas (config modifiée entre-temps).
    #[error("Aucune tuile « {0} » dans config.json (rechargez la fenêtre)")]
    UnknownTile(String),

    /// Le fichier à lancer n'existe pas : chemin mal écrit, app désinstallée…
    #[error("Fichier introuvable : {}", .0.display())]
    NotFound(PathBuf),

    /// URL d'une tuile web refusée (pas en http(s), espaces, guillemets).
    #[error(
        "URL refusée : « {0} ». Elle doit commencer par https:// et ne contenir ni espace ni guillemet"
    )]
    InvalidUrl(String),

    /// La modification de config.json (favoris, import Steam) a été annulée
    /// avant écriture : le fichier n'a pas été touché.
    #[error("config.json non modifié : {0}")]
    ConfigWrite(String),

    /// Bibliothèque Steam introuvable ou illisible (import des jeux).
    #[error("Steam : {0}")]
    Steam(String),

    /// Windows a refusé de lancer la cible (`reason` vient de Windows).
    #[error("Impossible de lancer {target} : {reason}")]
    Launch { target: String, reason: String },
}

impl AppError {
    /// Chemin du fichier concerné, s'il y en a un.
    ///
    /// `&self` : on *emprunte* l'erreur en lecture, sans la consommer.
    /// `Option<&PathBuf>` : soit `Some(référence vers le chemin)`, soit `None`.
    fn path(&self) -> Option<&PathBuf> {
        // `match` doit couvrir toutes les variantes : si on en ajoute une à
        // l'enum, le compilateur nous signalera chaque `match` à compléter.
        match self {
            AppError::NoConfigDir(_)
            | AppError::UnknownTile(_)
            | AppError::InvalidUrl(_)
            | AppError::ConfigWrite(_)
            | AppError::Steam(_)
            | AppError::Launch { .. } => None,
            AppError::NotFound(path) => Some(path),
            AppError::Io { path, .. }
            | AppError::Encoding { path }
            | AppError::Json { path, .. }
            | AppError::Invalid { path, .. } => Some(path),
        }
    }
}

/// Pour qu'une commande Tauri puisse renvoyer `Result<_, AppError>`, l'erreur
/// doit pouvoir être convertie en JSON (`Serialize`). On l'écrit à la main pour
/// envoyer au front un objet simple et stable :
///
/// ```json
/// { "message": "…", "path": "C:\\…\\config.json", "line": 12, "column": 5 }
/// ```
///
/// (`line` et `column` valent `null` quand l'erreur n'est pas liée à une position.)
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (line, column) = match self {
            AppError::Json { line, column, .. } => (Some(*line), Some(*column)),
            _ => (None, None),
        };
        let mut s = serializer.serialize_struct("AppError", 4)?;
        // `self.to_string()` utilise le message défini dans `#[error("…")]`.
        s.serialize_field("message", &self.to_string())?;
        s.serialize_field("path", &self.path().map(|p| p.display().to_string()))?;
        s.serialize_field("line", &line)?;
        s.serialize_field("column", &column)?;
        // Le `?` après chaque appel : si l'appel échoue, on sort tout de suite
        // de la fonction en renvoyant l'erreur. Sinon on continue avec la valeur.
        s.end()
    }
}
