# Lanceur

Lanceur d'apps Windows personnel (Tauri 2 + Vue 3 + TypeScript) : une icône dans la barre des tâches, une fenêtre de lancement rapide pour les jeux Steam, les applications installées et les apps web.

- Plan de développement : [PLAN.md](PLAN.md)
- Maquette interactive : [docs/maquette.html](docs/maquette.html) (à ouvrir dans un navigateur)

## Récupérer l'app

L'app est compilée par GitHub Actions sur un runner Windows, pas besoin de Rust sur votre PC.

1. Onglet **Actions** → dernier run « build » réussi.
2. En bas de la page, artefact **lanceur-windows** → zip contenant `Lanceur_x.y.z_x64_fr-FR.msi` / `_en-US.msi` et `Lanceur_x.y.z_x64-setup.exe`.
3. Lancer l'un des deux installeurs. L'installeur n'étant pas signé, SmartScreen affiche un avertissement : *Informations complémentaires* → *Exécuter quand même*.

Pour une version : `git tag v0.1.0 && git push --tags`, puis publier la Release brouillon créée par la CI.

## Développer

| Commande | Effet | Prérequis |
|---|---|---|
| `npm install` | installe les dépendances front | Node 22 |
| `npm run dev` | front seul dans le navigateur (http://localhost:1420) | Node |
| `npm run tauri dev` | app complète avec rechargement à chaud | Node + Rust + VS Build Tools C++ |
| `cargo test --manifest-path src-tauri/Cargo.toml` | tests Rust | Rust |

## Organisation

```
src/          front Vue 3 (interface)
src-tauri/    back Rust (système : fichiers, lancement, icônes, raccourci)
assets/       source de l'icône de l'app (régénérer avec `npx tauri icon assets/app-icon.png`)
docs/         maquette
```

Statut : étape 0 (squelette + CI). Voir l'avancement dans [PLAN.md](PLAN.md#5-étapes-de-développement).
