# Lanceur

Lanceur d'apps Windows personnel (Tauri 2 + Vue 3 + TypeScript) : une icône dans la barre des tâches, une fenêtre de lancement rapide pour les jeux Steam, les applications installées et les apps web.

- Plan de développement : [PLAN.md](PLAN.md)
- Maquette interactive : [docs/maquette.html](docs/maquette.html) (à ouvrir dans un navigateur)

## Installer

1. Onglet **Releases** du repo → dernière version → télécharger `Lanceur_x.y.z_x64-setup.exe` (ou le `.msi`, l'un ou l'autre suffit).
2. Lancer l'installeur. Il n'est pas signé : SmartScreen affiche « Windows a protégé votre ordinateur » → *Informations complémentaires* → *Exécuter quand même*.
3. Épingler *Lanceur* à la barre des tâches (clic droit sur l'icône pendant qu'il tourne → *Épingler à la barre des tâches*).

Pour essayer une version de développement : onglet **Actions** → dernier run « build » réussi → artefact **lanceur-windows** (zip avec les deux installeurs).

## Utiliser

| Touche | Effet |
|---|---|
| `Ctrl+Alt+Espace` | afficher / masquer la fenêtre, de n'importe où (réglable : `hotkey`) |
| taper du texte | rechercher (nom, début de mot, initiales : « bg3 » trouve *Baldur's Gate 3*) |
| `Entrée` | lancer la tuile sélectionnée |
| flèches | se déplacer dans la grille |
| `Tab` / `Maj+Tab` | catégorie suivante / précédente |
| `Ctrl+D` (ou l'étoile) | ajouter / retirer des favoris |
| `Échap` | vider la recherche, puis masquer la fenêtre |

« Masquer » réduit la fenêtre : l'icône reste dans la barre des tâches, un clic la rouvre. Relancer l'exe réaffiche la fenêtre existante au lieu d'en ouvrir une deuxième.

## Configurer

Toute la configuration est dans un fichier JSON lisible :

```
%APPDATA%\io.github.acassar.lanceur\config.json
```

Il est créé avec des exemples au premier démarrage. Le lien « ouvrir le dossier » de l'app (affiché quand une catégorie est vide ou que le fichier contient une erreur) ouvre ce dossier dans l'Explorateur.

**Le fichier est relu dès qu'il est enregistré** : les tuiles, les catégories et les réglages (raccourci compris) changent aussitôt, sans redémarrer. S'il contient une erreur, l'app l'affiche (ligne, colonne, tuile concernée) et garde la dernière version valide.

### Autocomplétion dans VS Code

Ajoutez cette ligne au début du fichier (c'est déjà le cas d'une config créée par la v0.1.0) :

```json
{
  "$schema": "https://raw.githubusercontent.com/acassar/lanceur/main/config.schema.json",
  "version": 1,
  ...
}
```

VS Code propose alors les champs, affiche leur description au survol et souligne les erreurs pendant la frappe. Le schéma est [config.schema.json](config.schema.json).

### Réglages (`settings`)

| Réglage | Défaut | Rôle |
|---|---|---|
| `hotkey` | `"Ctrl+Alt+Space"` | raccourci global ; au moins un modificateur (`Ctrl`, `Alt`, `Shift`, `Super`) + une touche (`Space`, `A`…`Z`, `0`…`9`, `F1`…`F24`) |
| `hideAfterLaunch` | `true` | réduire la fenêtre après un lancement |
| `hideOnBlur` | `true` | réduire la fenêtre quand elle perd le focus |
| `accent` | `"#f0a35e"` | couleur d'accent de l'interface |
| `browser` | `"msedge"` | navigateur des apps web : `msedge`, `chrome`, `brave` ou chemin complet d'un navigateur Chromium |
| `recentsCount` | `4` | nombre de tuiles dans « Récents » (`0` pour masquer la section) |

### Tuiles

Chaque tuile a un `id` (minuscules, chiffres, `-`, `_` ; unique et stable), un `name`, une `category` (id d'une entrée de `categories`), un `type`, et un `icon` optionnel.

```jsonc
// Jeu Steam : appId = le nombre dans l'URL de la page du jeu sur le magasin
// (store.steampowered.com/app/730/…).
{ "id": "cs2", "name": "Counter-Strike 2", "category": "jeux", "type": "steam", "appId": 730 }

// Application : un .exe ou un raccourci .lnk, avec des arguments optionnels.
// %APPDATA%, %LOCALAPPDATA%, %USERPROFILE%… sont développés.
{ "id": "discord", "name": "Discord", "category": "outils", "type": "app",
  "path": "%LOCALAPPDATA%\\Discord\\Update.exe", "args": ["--processStart", "Discord.exe"] }

// App web : ouverte dans une fenêtre d'application dédiée (msedge --app=…).
{ "id": "instagram", "name": "Instagram", "category": "web", "type": "web", "url": "https://www.instagram.com" }
```

Astuces :
- dans un chemin JSON, chaque `\` s'écrit `\\` ;
- pour une app du menu Démarrer, le plus simple est de viser son raccourci : `%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\…\\Nom.lnk` (ou `C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\…` pour les apps installées pour tous les utilisateurs) ;
- un programme qui demande les droits administrateur affiche l'invite UAC de Windows ;
- `icon` accepte une image (PNG, JPEG, ICO…), un `.exe`/`.lnk` dont on prend l'icône, ou une URL `https://`.

`favorites` liste les ids des favoris dans l'ordre d'affichage. L'étoile et `Ctrl+D` le modifient en ne touchant qu'à ce tableau : votre mise en forme est conservée.

### Fichiers de l'app

Dans le même dossier que `config.json` :
- `state.json` : les récents (écrit par l'app) ;
- `icons\` : le cache des icônes. Les icônes des tuiles retirées de la config sont supprimées automatiquement ; supprimer le dossier force la recherche de toutes les icônes.

## Limites connues (v0.1.0)

- L'installeur n'est pas signé (avertissement SmartScreen à l'installation).
- Pas d'éditeur de tuiles dans l'interface, pas d'import automatique de la bibliothèque Steam, pas d'icône dans la zone de notification ni de lancement au démarrage de Windows : ce sont les pistes V2 du [plan](PLAN.md#8-hors-périmètre-v1-pistes-v2).
- Si `config.json` est un lien symbolique, ses modifications ne sont pas détectées tout de suite : elles s'appliquent au prochain affichage de la fenêtre.

## Développer

| Commande | Effet | Prérequis |
|---|---|---|
| `npm install` | installe les dépendances front | Node 22 |
| `npm run dev` | front seul dans le navigateur (http://localhost:1420), données factices | Node |
| `npm test` | tests du front (Vitest) | Node |
| `npm run tauri dev` | app complète avec rechargement à chaud | Node + Rust + VS Build Tools C++ |
| `cargo test --manifest-path src-tauri/Cargo.toml` | tests Rust (après `npm run build`) | Rust |

La CI (GitHub Actions, runner Windows) compile et teste chaque push sur `main` et chaque PR : pas besoin de Rust sur votre PC.

### Publier une version

1. Mettre la même `version` dans `package.json` **et** `src-tauri/Cargo.toml` (puis `npm install` et `cargo check` pour mettre à jour les fichiers `lock`), et merger sur `main`.
2. `git tag v0.2.0 && git push origin v0.2.0`.
3. Le job `release` vérifie que le tag correspond aux versions, compile et crée une Release **brouillon** avec les installeurs. Il reste à la publier depuis l'onglet *Releases*.

## Organisation

```
src/                 front Vue 3 (interface)
src-tauri/           back Rust (système : fichiers, lancement, icônes, raccourci)
config.schema.json   schéma JSON de config.json (autocomplétion VS Code)
assets/              source de l'icône de l'app (régénérer avec `npx tauri icon assets/app-icon.png`)
docs/                maquette
```
