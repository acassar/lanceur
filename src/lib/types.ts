// Types partagés du front. Ils décrivent le contenu de config.json, tel que
// la commande Rust get_config le renvoie (structs de src-tauri/src/config.rs).

/** Réglages généraux (section "settings" de config.json). */
export interface Settings {
  hotkey: string;
  hideAfterLaunch: boolean;
  hideOnBlur: boolean;
  accent: string;
  browser: string;
  recentsCount: number;
  launchAtStartup: boolean;
}

export interface Category {
  id: string;
  name: string;
}

interface TileBase {
  id: string;
  name: string;
  category: string;
  /** Chemin d'une image qui remplace l'icône trouvée automatiquement. */
  icon?: string;
}

/**
 * Une tuile. Le champ `type` sert de discriminant : TypeScript sait par exemple
 * qu'une tuile `steam` a forcément un `appId`. Côté Rust, ce sera une `enum`.
 */
export type Tile = TileBase &
  (
    | { type: "steam"; appId: number }
    | { type: "app"; path: string; args?: string[] }
    | { type: "web"; url: string }
  );

export type TileType = Tile["type"];

export interface Config {
  version: 1;
  settings: Settings;
  categories: Category[];
  /** Ids des tuiles favorites, dans l'ordre d'affichage. */
  favorites: string[];
  tiles: Tile[];
}

/** Jeu Steam installé, trouvé par la commande Rust `detect_steam_games` (steam.rs). */
export interface SteamGame {
  appId: number;
  name: string;
}

/** Résultat de la commande `import_steam_games`. */
export interface SteamImportResult {
  /** Ids des tuiles ajoutées (vide si tous les jeux avaient déjà une tuile). */
  added: string[];
  /** Nom de la catégorie où elles ont été rangées. */
  category: string;
}

/**
 * Application installée (raccourci du menu Démarrer), trouvée par la commande
 * Rust `detect_apps` (`DetectedApp` dans apps.rs).
 */
export interface DetectedApp {
  name: string;
  /** Chemin du raccourci, tel qu'il sera écrit dans la tuile (`%APPDATA%\…\X.lnk`). */
  path: string;
  /** Sous-dossier du menu Démarrer, `null` à la racine. */
  folder: string | null;
  /** Une tuile lance déjà cette application (même raccourci ou même nom). */
  inConfig: boolean;
}

/** Résultat de la commande `import_apps` : même forme que l'import Steam. */
export type AppsImportResult = SteamImportResult;

/**
 * Tuile envoyée par l'éditeur aux commandes `add_tile` / `update_tile`
 * (`TileDraft` dans editor.rs) : tout sauf l'id, que Rust choisit à l'ajout
 * à partir du nom, et garde à la modification.
 */
export type TileDraft = {
  name: string;
  /** Id d'une catégorie existante (ignoré si `newCategory` est rempli). */
  category: string;
  /** Nom d'une catégorie à créer (réutilisée si ce nom existe déjà). */
  newCategory?: string;
  icon?: string;
} & (
  | { type: "steam"; appId: number }
  | { type: "app"; path: string; args: string[] }
  | { type: "web"; url: string }
);
