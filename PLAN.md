# Lanceur : plan de développement V1

Un lanceur d'apps Windows personnel : une icône dans la barre des tâches ouvre une fenêtre compacte. Elle permet de lancer en un clic des jeux Steam, des applications installées et des apps web.

- **Stack** : Tauri 2, Vue 3, Vite, TypeScript (front) ; Rust (back).
- **Compilation** : GitHub Actions (`tauri-action`, runner Windows).
- **Maquette validée** : variante B, avec un rail de catégories, des favoris et des récents. Voir [`docs/maquette.html`](docs/maquette.html).

> Statut : **validé le 16/11/2026**. Décisions retenues : 1A, 2A, 3A, 4A, 5A, 6A (voir section 7).

---

## 1. Principe général

```
┌──────────────── Front (WebView, Vue 3) ───────────────┐
│  Affichage, recherche, navigation clavier, favoris UI │
│          │ invoke("get_config"), invoke("launch")…    │
└──────────┼────────────────────────────────────────────┘
           ▼  IPC Tauri (commandes typées, JSON)
┌──────────────── Back (Rust, processus natif) ─────────┐
│  Lire/écrire le JSON · lancer les cibles · extraire   │
│  les icônes · raccourci global · afficher/masquer     │
└───────────────────────────────────────────────────────┘
```

La règle de séparation : **le front ne touche jamais au système**. Il ne lit aucun fichier et ne lance aucun processus. Il appelle des *commandes* Rust, c'est-à-dire des fonctions Rust exposées au JavaScript, un peu comme les routes d'un contrôleur NestJS. Tout ce qui demande Windows (fichiers, processus, icônes, raccourci clavier) vit côté Rust.

Ce découpage a un avantage pratique. Le front peut se développer et se tester dans un simple navigateur avec `npm run dev`, grâce à une fausse couche IPC, sans installer Rust en local.

## 2. Structure des dossiers

```
lanceur/
├─ .github/workflows/build.yml   # CI : tests + build Windows (+ release sur tag)
├─ docs/
│  └─ maquette.html              # maquette interactive validée
├─ PLAN.md                       # ce document
├─ config.schema.json            # schéma pour éditer config.json dans VS Code
├─ src/                          # FRONT (Vue 3 + TS)
│  ├─ main.ts
│  ├─ App.vue                    # assemble la fenêtre
│  ├─ components/
│  │  ├─ SearchBar.vue           # input, autofocus, compteur
│  │  ├─ CategoryRail.vue        # Tout / Favoris / catégories, Tab pour changer
│  │  ├─ TileSection.vue         # titre + grille (Favoris, Récents, Jeux…)
│  │  ├─ Tile.vue                # tuile (icône, nom, étoile)
│  │  ├─ TileIcon.vue            # jaquette / icône exe / favicon, ou monogramme
│  │  ├─ StatusBar.vue           # cible de la tuile sélectionnée + aides clavier
│  │  └─ Toast.vue               # « Lancement de… », erreurs
│  ├─ composables/
│  │  ├─ useConfig.ts            # charge la config, favoris, recharge
│  │  ├─ useSearch.ts            # filtre + score (préfixe, initiales « bg3 »…)
│  │  ├─ useKeyboardNav.ts       # flèches, Entrée, Échap, Tab, Ctrl+D
│  │  └─ useLauncher.ts          # appelle launch, gère masquage et toast
│  ├─ lib/
│  │  ├─ ipc.ts                  # wrappers typés autour de invoke()
│  │  ├─ ipc.mock.ts             # fausse IPC pour npm run dev dans le navigateur
│  │  ├─ search.ts (+ search.test.ts)
│  │  └─ types.ts                # types partagés (miroir des structs Rust)
│  └─ styles/tokens.css          # couleurs, typo (issus de la maquette)
├─ src-tauri/                    # BACK (Rust)
│  ├─ Cargo.toml                 # dépendances Rust (≈ package.json)
│  ├─ tauri.conf.json            # fenêtre, identifiant, bundle
│  ├─ capabilities/default.json  # permissions accordées au front
│  ├─ icons/                     # icônes de l'app (générées par `tauri icon`)
│  └─ src/
│     ├─ main.rs                 # point d'entrée, appelle lib::run()
│     ├─ lib.rs                  # construit l'app : plugins, commandes, setup
│     ├─ config.rs               # structs serde, lecture/écriture, config par défaut
│     ├─ state.rs                # récents (state.json)
│     ├─ launch.rs               # steam / exe / lnk / web
│     ├─ icons.rs                # extraction d'icône exe, favicon, cache
│     ├─ window.rs               # afficher/masquer, centrer, raccourci global
│     └─ error.rs                # type d'erreur unique, sérialisable vers le front
├─ index.html
├─ package.json
├─ vite.config.ts
└─ tsconfig.json
```

**Plugins Tauri prévus**

| Plugin | Rôle |
|---|---|
| `tauri-plugin-global-shortcut` | raccourci clavier global pour afficher ou masquer la fenêtre |
| `tauri-plugin-single-instance` | relancer l'app réaffiche la fenêtre existante au lieu d'en ouvrir une 2ᵉ |

Crates Rust en plus : `serde` et `serde_json` (JSON), `thiserror` (erreurs), `windows` (API Win32 pour les lancements et les icônes), `reqwest` (favicons), `image` (encodage PNG).

## 3. Fichiers de données

Ils sont stockés dans `%APPDATA%\io.github.acassar.lanceur\`, le dossier que Tauri fournit via `app_config_dir()`.

| Fichier | Qui l'écrit | Contenu |
|---|---|---|
| `config.json` | **vous** (à la main) + l'app pour les favoris | réglages, catégories, tuiles, favoris |
| `state.json` | l'app uniquement | récents (change à chaque lancement) |
| `icons\<id>-<empreinte>.png` | l'app uniquement | cache des icônes extraites (`.jpg` pour les jaquettes Steam) |

L'empreinte résume la source de l'icône (chemin, URL, date de modification de l'exe…) : si elle change, l'icône est recalculée. Une icône introuvable sur un site qui a répondu laisse un fichier `.none`, retenté au bout de 24 h ; sans réseau, on réessaie à l'ouverture suivante. Supprimer le dossier `icons` vide le cache.

Les récents vont dans un fichier séparé pour que l'app ne réécrive pas votre `config.json` à chaque lancement.

Quand vous ajoutez ou retirez un favori, l'app ne remplace que le tableau `"favorites"` dans `config.json` (ou l'ajoute avant `"tiles"` s'il manque) : votre mise en forme, vos lignes vides et l'ordre des champs sont conservés. Si le fichier est invalide à ce moment-là, il n'est pas touché et l'erreur s'affiche.

### `config.json`

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/acassar/lanceur/main/config.schema.json",
  "version": 1,
  "settings": {
    "hotkey": "Ctrl+Alt+Space",
    "hideAfterLaunch": true,
    "hideOnBlur": true,
    "accent": "#f0a35e",
    "browser": "msedge",          // msedge | chrome | brave | chemin complet
    "recentsCount": 4
  },
  "categories": [
    { "id": "jeux",   "name": "Jeux" },
    { "id": "outils", "name": "Outils" },
    { "id": "web",    "name": "Web" }
  ],
  "favorites": ["cs2", "discord", "instagram"],   // ordre = ordre d'affichage
  "tiles": [
    { "id": "cs2", "name": "Counter-Strike 2", "category": "jeux",
      "type": "steam", "appId": 730 },

    { "id": "discord", "name": "Discord", "category": "outils",
      "type": "app", "path": "%LOCALAPPDATA%\\Discord\\Update.exe",
      "args": ["--processStart", "Discord.exe"] },

    { "id": "vscode", "name": "VS Code", "category": "outils",
      "type": "app", "path": "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Visual Studio Code.lnk" },

    { "id": "instagram", "name": "Instagram", "category": "web",
      "type": "web", "url": "https://www.instagram.com" },

    { "id": "amazon", "name": "Amazon", "category": "web",
      "type": "web", "url": "https://www.amazon.fr", "icon": "C:\\Images\\amazon.png" }
  ]
}
```

Règles :
- `id` est unique, en minuscules et stable : il sert aux favoris, aux récents et au cache d'icônes.
- `type` vaut `steam`, `app` (exe ou .lnk) ou `web`. Les champs requis dépendent du type.
- `icon` est optionnel et remplace l'icône trouvée automatiquement.
- Les variables d'environnement (`%APPDATA%`, `%LOCALAPPDATA%`…) sont développées au lancement.
- Un `config.schema.json` (JSON Schema) est fourni à la racine du repo. VS Code proposera alors l'autocomplétion et signalera les erreurs pendant l'édition à la main.
- Au premier démarrage, si le fichier n'existe pas, l'app crée une config d'exemple.
- Si le JSON est invalide, l'app affiche l'erreur (ligne et colonne) dans la fenêtre au lieu de planter, et garde la dernière config valide.

Côté Rust, le champ `type` se traduit très naturellement par une *enum* :

```rust
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Target {
    Steam { #[serde(rename = "appId")] app_id: u32 },
    App   { path: String, #[serde(default)] args: Vec<String> },
    Web   { url: String },
}
```

`serde` lit le champ `"type"` et choisit la bonne variante. Si `appId` manque sur une tuile `steam`, la lecture échoue avec un message précis. C'est l'équivalent d'une union discriminée TypeScript, mais vérifiée à la lecture du fichier.

### `state.json`

```json
{ "recents": [ { "id": "bg3", "at": "2026-11-16T13:40:00Z" } ] }
```

## 4. Commandes exposées au front

| Commande | Entrée | Sortie | Rôle |
|---|---|---|---|
| `get_config` | – | `Config` | lit (ou crée) `config.json` |
| `get_recents` | – | `string[]` | ids des derniers lancements |
| `launch` | `id` | `()` ou erreur | lance la tuile, met à jour les récents |
| `set_favorite` | `id`, `favorite` | `string[]` | met la tuile en favori (`true`) ou l'en retire (`false`) et réécrit `config.json` ; répéter l'appel ne change rien |
| `get_icon` | `id` | `data:` URL ou `null` | extrait ou télécharge l'icône, avec cache ; `null` → monogramme |
| `open_config_folder` | – | `()` | ouvre le dossier dans l'Explorateur |
| `hide_window` | – | `()` | réduit la fenêtre (Échap, après lancement) ; l'icône reste dans la barre des tâches |
| `get_hotkey_status` | – | `{ hotkey, error }` | raccourci global configuré, et l'erreur s'il n'a pas pu être enregistré (invalide, ou déjà pris par une autre app) |

**Icônes selon la source**
- `icon` renseigné : ce fichier image (PNG, JPEG, ICO…) ou cette URL ; un `.exe` ou `.lnk` donne son icône Windows. Un chemin relatif se lit depuis le dossier de `config.json`.
- `app` : l'icône que l'Explorateur affiche pour le fichier (`IShellItemImageFactory`, qui suit aussi les `.lnk`). Un nom seul (`wt.exe`) est cherché dans le `PATH`.
- `steam` : la jaquette du jeu, lue dans le cache de Steam (`appcache\librarycache`), sinon téléchargée sur le CDN de Steam.
- `web` : le favicon lu dans la page (`<link rel="icon">`, `apple-touch-icon`), puis `/favicon.ico` en secours.

Les icônes arrivent au front en `data:` URL, que la CSP autorise déjà (`img-src … data:`). Le protocole `asset:` n'a donc pas besoin d'être ouvert : le front ne peut toujours lire aucun fichier du disque.

Événement émis par Rust vers le front : `window-shown`. Le front y vide la recherche et redonne le focus au champ.

**Lancement selon le type**
- `steam` : ouverture de `steam://rungameid/<appId>` avec `ShellExecuteExW` (API Windows), qui trouve Steam comme gestionnaire de l'URL. Le plugin opener n'est finalement pas utilisé : la même fonction sert déjà aux `.lnk`.
- `app` (.exe) : `std::process::Command`, avec le dossier de l'exe comme dossier de travail, sans attendre la fin du processus.
- `app` (.lnk ou autre fichier) : `ShellExecuteExW` (API Windows), qui résout le raccourci comme un double-clic.
- `web` : `<browser> --app=<url>`, ce qui ouvre une fenêtre dédiée sans barre d'adresse. `msedge`, `chrome` et `brave` sont retrouvés par Windows sans chemin complet ; l'URL doit être en `http(s)://`.

## 5. Étapes de développement

Chaque étape se termine par quelque chose que vous pouvez **tester vous-même**, et chaque étape fait l'objet d'une PR séparée. Les parties Rust sont expliquées dans la PR et en commentaires dans le code.

| # | Étape | Comment tester | Rust abordé |
|---|---|---|---|
| 0 | **Squelette + CI** : projet `create-tauri-app` (vue-ts), workflow GitHub Actions | télécharger l'installeur depuis l'onglet Actions, l'installer : une fenêtre « Hello » s'ouvre | structure d'un projet Cargo, `main.rs` / `lib.rs` |
| 1 | **UI statique** : la maquette portée en composants Vue, données factices via `ipc.mock.ts` | `npm run dev` dans le navigateur : recherche, flèches, Tab, Ctrl+D, Entrée (toast simulé) | aucun |
| 2 | **Config réelle** : `config.rs`, commande `get_config`, config d'exemple, erreurs de JSON affichées | modifier `config.json`, rouvrir l'app : les tuiles suivent ; casser le JSON : message clair | structs, `serde`, `Result` et `?`, ownership de base |
| 3 | **Lancement** : `launch.rs` pour steam, exe, lnk et web ; récents dans `state.json` | lancer un jeu, Discord, VS Code (.lnk) et Instagram en fenêtre d'app | `enum` + `match`, `std::process`, appel Win32 |
| 4 | **Fenêtre** : raccourci global, afficher/masquer, focus auto, masquage après lancement et à la perte de focus, instance unique, centrage sur l'écran actif | Ctrl+Alt+Espace depuis un jeu ou un navigateur ; relancer l'exe réaffiche la fenêtre | plugins, état partagé (`State<Mutex<…>>`), closures |
| 5 | **Favoris** : `set_favorite` réécrit `config.json` | étoile ou Ctrl+D, fermer et rouvrir : les favoris sont toujours là | écriture atomique de fichier, gestion d'erreurs |
| 6 | **Icônes** : extraction depuis les exe/lnk, favicon pour le web, cache PNG, monogramme de secours | les vraies icônes apparaissent ; 2ᵉ ouverture instantanée grâce au cache | `unsafe` et FFI Win32 (encadré), async avec `reqwest` |
| 7 | **Finitions + v0.1.0** : rechargement de la config, `config.schema.json`, README, tag `v0.1.0` | une Release GitHub contient l'installeur `.msi` / `-setup.exe` | – |

Les tests automatiques tournent dans la CI :
- **Vitest** pour la recherche et la navigation clavier (logique pure) ;
- **`cargo test`** pour la lecture et la validation de la config, le développement des variables d'environnement et la construction des commandes de lancement.

## 6. Compilation (GitHub Actions)

Pas de toolchain Rust nécessaire sur votre PC : chaque push sur `main` et chaque PR compilent l'app sur un runner Windows (et *Run workflow* permet de lancer un build sur n'importe quelle branche).

Le workflow est dans [`.github/workflows/build.yml`](.github/workflows/build.yml), commenté. En résumé :
- **job `build`** (push sur `main`, PR, lancement manuel) : `npm ci`, build du front et de l'app avec `tauri-action`, `cargo test`, puis envoi des installeurs en artefact. Le jeton GitHub y est en lecture seule ;
- **job `release`** (tag `v*`) : vérifie que le tag correspond aux versions de `package.json` et `Cargo.toml`, puis crée une Release brouillon. C'est le seul job autorisé à écrire dans le repo.

**Récupérer l'app**
- À chaque build : ouvrez l'onglet *Actions*, puis le dernier run et l'artefact `lanceur-windows` (zip contenant le `.msi` et le `-setup.exe`).
- Pour une version : `git tag v0.1.0 && git push --tags`, puis publiez la Release brouillon créée.

**À savoir** : l'installeur n'est pas signé, donc Windows SmartScreen affichera « Windows a protégé votre ordinateur ». Cliquez sur *Informations complémentaires*, puis *Exécuter quand même*. C'est normal pour un projet perso. La signature est payante et peut venir plus tard.

**Optionnel, toolchain locale** (pour itérer plus vite sur le Rust à partir de l'étape 3) :
1. installer `rustup` (rustup.rs) ;
2. installer *Visual Studio Build Tools* avec « Développement Desktop en C++ » ;
3. WebView2 est déjà présent sur Windows 11 ;
4. lancer `npm run tauri dev`.

## 7. Décisions

Toutes les options A ont été retenues (16/11/2026) : fenêtre réduite plutôt que masquée, `Ctrl+Alt+Espace`, icônes via Win32 en Rust, favicons lus sur la page, CSS maison, repo `acassar/lanceur`.

1. **Comportement de l'icône de barre des tâches quand la fenêtre est cachée**
   - **A. Réduire** au lieu de masquer : l'icône reste dans la barre, un clic restaure instantanément. *(recommandé : c'est exactement « une icône dans ma barre des tâches »)*
   - B. Masquer vraiment : l'icône disparaît. Il faut épingler l'app ; un clic relance l'exe, qui réaffiche la fenêtre existante (un peu plus lent).

2. **Raccourci global par défaut**
   - **A. `Ctrl+Alt+Espace`** *(recommandé : libre sous Windows et rarement pris par les jeux)*
   - B. `Alt+Espace` : déjà utilisé par Windows (menu de fenêtre) et par PowerToys Run.
   - C. Autre choix. Dans tous les cas, il reste modifiable dans `config.json`.

3. **Extraction des icônes d'exe**
   - **A. API Win32 en Rust** (crate `windows`) : rapide, sans dépendance externe, et une bonne occasion d'apprendre (encadré). *(recommandé)*
   - B. Script PowerShell appelé par Rust : moins de Rust, mais environ 300 ms par icône et une fenêtre console à masquer.

4. **Favicons des apps web**
   - **A. Lire la page** (`<link rel="icon">`), puis `/favicon.ico` en secours, sans service tiers. *(recommandé)*
   - B. Service de favicons (Google, DuckDuckGo) : plus simple, mais chaque domaine est envoyé à un tiers.

5. **Style de l'interface**
   - **A. CSS maison** avec les tokens de la maquette, sans bibliothèque : léger et fidèle au design. *(recommandé)*
   - B. Vuetify, que vous connaissez déjà : plus lourd, avec un look Material à retravailler pour ressembler à la maquette.

6. **Nom du repo et identifiant de l'app** : `acassar/lanceur` et `io.github.acassar.lanceur`, sauf si vous préférez un autre nom.

## 8. Hors périmètre V1 (pistes V2)

- Import automatique de la bibliothèque Steam (`libraryfolders.vdf` + manifests) : la structure `type: "steam"` le permet déjà.
- Éditeur de tuiles dans l'interface (ajout, glisser-déposer).
- Icône dans la zone de notification et lancement au démarrage de Windows (`tauri-plugin-autostart`).
- Apps web intégrées dans l'app sous forme d'onglets.
