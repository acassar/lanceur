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
| `Win+Alt+Espace` | afficher / masquer la fenêtre, de n'importe où (réglable : `hotkey`) |
| taper du texte | rechercher (nom, début de mot, initiales : « bg3 » trouve *Baldur's Gate 3*) |
| `Entrée` | lancer la tuile sélectionnée |
| flèches | se déplacer dans la grille |
| `Tab` / `Maj+Tab` | catégorie suivante / précédente |
| `Ctrl+D` (ou l'étoile) | ajouter / retirer des favoris |
| `Ctrl+N` | ajouter une tuile |
| `Ctrl+E` ou `F2` (ou clic droit, ou le crayon) | modifier ou supprimer la tuile |
| `Échap` | vider la recherche, puis masquer la fenêtre |

« Masquer » réduit la fenêtre : l'icône reste dans la barre des tâches, un clic la rouvre. Relancer l'exe réaffiche la fenêtre existante au lieu d'en ouvrir une deuxième.

### Icône de notification

Le lanceur a aussi une icône dans la zone de notification (à droite de la barre des tâches, éventuellement sous la flèche `^`) :

- clic gauche : afficher la fenêtre, ou la masquer si elle est devant ;
- clic droit : *Afficher le lanceur*, *Détecter les applications…*, *Importer les jeux Steam…*, *Ouvrir le dossier de config*, *Quitter*.

La croix de la fenêtre la masque au lieu de fermer l'app (le raccourci doit rester actif) : pour quitter, passez par *Quitter* dans ce menu.

### Ajouter, modifier, supprimer une tuile

*+ Ajouter une tuile* (en bas du rail) ou `Ctrl+N` ouvre un formulaire : type (application, app web, jeu Steam), nom, programme (bouton *Parcourir…*, `.exe` ou `.lnk`), adresse ou appId (l'adresse de la page du magasin Steam marche aussi), catégorie (existante ou nouvelle) et icône facultative. La nouvelle tuile se range en dernier dans sa catégorie.

Un clic droit sur une tuile (ou le crayon au survol, `Ctrl+E`, `F2`) ouvre le même formulaire pour la modifier ; *Supprimer*, puis *Confirmer la suppression*, la retire (et de vos favoris). Les catégories ne sont jamais supprimées.

Pour réordonner, glissez une tuile à sa nouvelle place dans sa catégorie ou dans Favoris (pas pendant une recherche, ni dans Récents), ou une catégorie dans le rail. Seul l'ordre des lignes concernées change dans `config.json`.

Comme pour les favoris, seules les lignes concernées de `config.json` changent : une tuile ajoutée ou modifiée est écrite sur une ligne, le reste du fichier (ordre, lignes vides, mise en forme) ne bouge pas, et un fichier invalide n'est jamais modifié. L'id d'une nouvelle tuile est tiré de son nom ; il ne change plus ensuite.

### Détecter les applications installées

Au premier démarrage, l'écran d'accueil propose deux choix : *Détecter mes applications* ou *Partir de zéro* (ajouter ses tuiles une à une, ou éditer `config.json`). La détection est aussi disponible à tout moment par *Détecter les applications…* (menu de l'icône de notification).

Elle liste dans une même fenêtre :

- les **applications** du menu Démarrer (raccourcis `.lnk` de tous les utilisateurs et de votre compte), sans les désinstalleurs, aides, sites web, documents ni outils d'administration de Windows. Elles ne sont pas cochées d'office (le menu Démarrer en contient souvent beaucoup) ; un champ permet de filtrer la liste. Chacune devient une tuile `app` qui ouvre le raccourci (`%APPDATA%\…\Discord.lnk`), dans la catégorie « Outils » ou « Applications » si elle existe, sinon dans une nouvelle catégorie « Applications » ;
- les **jeux Steam** installés, cochés d'office (même règle que l'import ci-dessous).

Rien n'est jamais supprimé ni remplacé : ce qui a déjà une tuile (même raccourci, ou une application du même nom) est grisé et n'est pas ajouté une deuxième fois.

### Importer les jeux Steam

*Importer les jeux Steam…* (menu de l'icône de notification) liste les jeux installés sur le PC, dans toutes vos bibliothèques Steam (un dossier `SteamLibrary` par disque). Cochez ceux à ajouter : chacun devient une tuile `steam` dans la catégorie « Jeux » (créée si besoin). Les jeux qui ont déjà une tuile sont grisés et jamais ajoutés deux fois.

L'import ne fait qu'*ajouter* des lignes à la fin de `"categories"` et `"tiles"` dans `config.json` : vos tuiles, leur ordre et votre mise en forme restent tels quels, et un fichier invalide n'est jamais modifié. Relancez l'import après avoir installé de nouveaux jeux ; un jeu désinstallé garde sa tuile (supprimez-la à la main si besoin).

### Lancement au démarrage de Windows

Mettez `"launchAtStartup": true` dans les réglages de `config.json`. Le lanceur s'ajoute alors aux *Applications de démarrage* de Windows (registre de votre compte, sans droits admin) ; à l'ouverture de session, il démarre fenêtre cachée : seule l'icône de notification apparaît, et `Win+Alt+Espace` l'ouvre (ensuite, comme d'habitude, masquer la réduit dans la barre des tâches). Remettez `false` pour le retirer. Le désactiver dans le Gestionnaire des tâches marche aussi, et ce choix est respecté tant que vous ne repassez pas le réglage de `false` à `true`. La désinstallation (installeur `.exe`) retire l'entrée. (En développement, `npm run tauri dev` n'y touche pas.)

## Configurer

Toute la configuration est dans un fichier JSON lisible :

```
%APPDATA%\io.github.acassar.lanceur\config.json
```

Au premier démarrage, il est créé **sans tuiles** (réglages par défaut, listes `categories`, `favorites` et `tiles` vides) : l'app affiche alors un écran d'accueil qui propose de détecter les applications installées ou de partir de zéro (avec un exemple à copier). Une config déjà existante n'est jamais modifiée. Un exemple complet se trouve dans [src-tauri/config.example.json](src-tauri/config.example.json). Le bouton « Ouvrir le dossier » de l'app (écran d'accueil, catégorie vide ou erreur dans le fichier) ouvre ce dossier dans l'Explorateur.

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
| `hotkey` | `"Super+Alt+Space"` | raccourci global ; au moins un modificateur (`Ctrl`, `Alt`, `Shift`, `Super`) + une touche (`Space`, `A`…`Z`, `0`…`9`, `F1`…`F24`) |
| `hideAfterLaunch` | `true` | réduire la fenêtre après un lancement |
| `hideOnBlur` | `true` | réduire la fenêtre quand elle perd le focus |
| `accent` | `"#f0a35e"` | couleur d'accent de l'interface |
| `browser` | `"msedge"` | navigateur des apps web : `msedge`, `chrome`, `brave` ou chemin complet d'un navigateur Chromium |
| `recentsCount` | `4` | nombre de tuiles dans « Récents » (`0` pour masquer la section) |
| `launchAtStartup` | `false` | lancer l'app à l'ouverture de session Windows, fenêtre cachée (voir « Icône de notification ») |

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
- Pas encore d'éditeur de tuiles dans l'interface : c'est une piste V2 du [plan](PLAN.md#8-hors-périmètre-v1-pistes-v2).
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

1. `npm run bump -- 0.3.0` met cette version dans `package.json`, `package-lock.json`, `src-tauri/Cargo.toml` et `src-tauri/Cargo.lock`. Commiter, puis merger sur `main`.
2. Onglet **Actions** du repo → workflow **release** → **Run workflow** (branche `main`) → **Run workflow**.
3. Le workflow vérifie les versions, lance les tests, crée le tag `v0.3.0` sur le dernier commit de `main`, compile et crée une Release **brouillon** avec les installeurs. Il reste à la publier depuis l'onglet *Releases*.

Si le tag de cette version existe déjà sur un autre commit, le workflow s'arrête : monter la version (étape 1) puis relancer. Si une release échoue pendant la compilation, il suffit de relancer *Run workflow* : le tag déjà posé sur ce commit est réutilisé.

Alternative en ligne de commande : `git tag v0.3.0 && git push origin v0.3.0` (le tag doit être exactement `v` + la version) lance le même workflow, sans créer de tag.

## Organisation

```
src/                 front Vue 3 (interface)
src-tauri/           back Rust (système : fichiers, lancement, icônes, raccourci)
config.schema.json   schéma JSON de config.json (autocomplétion VS Code)
assets/              source de l'icône de l'app (régénérer avec `npx tauri icon assets/app-icon.png`)
docs/                maquette
```
