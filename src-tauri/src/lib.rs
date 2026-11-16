//! Cœur de l'application côté Rust.
//!
//! Pour l'instant (étape 0), il ne contient qu'une commande de test, `ping`,
//! qui prouve que le front Vue sait appeler du Rust. Les vrais modules
//! (config, lancement, icônes, fenêtre) arrivent aux étapes suivantes.

/// Une *commande* Tauri : une fonction Rust que le front peut appeler avec
/// `invoke("ping", { name: "Adrien" })`. C'est l'équivalent d'une route d'API.
///
/// - `#[tauri::command]` est une macro qui génère le code de « plomberie »
///   (lecture des arguments JSON, sérialisation de la réponse).
/// - `name: &str` est une *référence* vers une chaîne : on lit la chaîne
///   sans en devenir propriétaire (pas de copie).
/// - La fonction renvoie un `String`, une chaîne qui lui appartient et qui sera
///   envoyée au front en JSON.
#[tauri::command]
fn ping(name: &str) -> String {
    format!("Bonjour {name}, le Rust répond !")
}

/// Construit et démarre l'application.
///
/// `#[cfg_attr(mobile, ...)]` ne concerne que les builds mobiles : on peut l'ignorer.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // On déclare la liste des commandes appelables depuis le front.
        .invoke_handler(tauri::generate_handler![ping])
        // `generate_context!` lit tauri.conf.json au moment de la compilation.
        .run(tauri::generate_context!())
        // `expect` : si le démarrage échoue, on arrête l'app avec ce message.
        .expect("erreur au démarrage de l'application");
}

// Les tests vivent dans le même fichier, dans un module compilé seulement
// pour `cargo test` (grâce à `#[cfg(test)]`).
#[cfg(test)]
mod tests {
    use super::*; // importe `ping` depuis le module parent

    #[test]
    fn ping_contient_le_nom() {
        assert!(ping("Adrien").contains("Adrien"));
    }
}
