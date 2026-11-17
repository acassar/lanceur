// Fausse couche IPC : elle imite les commandes Rust avec des données d'exemple
// gardées en mémoire. Elle permet de développer et tester toute l'interface dans
// un navigateur (npm run dev), sans Rust.
//
// Dans l'app (Tauri), ipc.ts utilise les vraies commandes Rust à la place.
import type { Ipc } from "./ipc";
import { appsCategory } from "./detect";
import { applyTileOrder } from "./reorder";
import type { Category, Config, DetectedApp, SteamGame, Tile, TileDraft } from "./types";

export const SAMPLE: Config = {
  version: 1,
  settings: {
    hotkey: "Super+Alt+Space",
    hideAfterLaunch: true,
    hideOnBlur: true,
    accent: "#f0a35e",
    browser: "msedge",
    recentsCount: 4,
    launchAtStartup: false,
  },
  categories: [
    { id: "jeux", name: "Jeux" },
    { id: "outils", name: "Outils" },
    { id: "web", name: "Web" },
  ],
  favorites: ["cs2", "hades2", "discord", "vscode", "youtube"],
  tiles: [
    { id: "cs2", name: "Counter-Strike 2", category: "jeux", type: "steam", appId: 730 },
    { id: "bg3", name: "Baldur's Gate 3", category: "jeux", type: "steam", appId: 1086940 },
    { id: "hades2", name: "Hades II", category: "jeux", type: "steam", appId: 1145350 },
    { id: "eldenring", name: "Elden Ring", category: "jeux", type: "steam", appId: 1245620 },
    { id: "rocketleague", name: "Rocket League", category: "jeux", type: "steam", appId: 252950 },
    { id: "factorio", name: "Factorio", category: "jeux", type: "steam", appId: 427520 },
    { id: "hollowknight", name: "Hollow Knight", category: "jeux", type: "steam", appId: 367520 },
    { id: "steam", name: "Steam", category: "outils", type: "app", path: "C:\\Program Files (x86)\\Steam\\steam.exe" },
    {
      id: "discord",
      name: "Discord",
      category: "outils",
      type: "app",
      path: "%LOCALAPPDATA%\\Discord\\Update.exe",
      args: ["--processStart", "Discord.exe"],
    },
    {
      id: "vscode",
      name: "VS Code",
      category: "outils",
      type: "app",
      path: "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Visual Studio Code.lnk",
    },
    { id: "firefox", name: "Firefox", category: "outils", type: "app", path: "C:\\Program Files\\Mozilla Firefox\\firefox.exe" },
    { id: "obs", name: "OBS Studio", category: "outils", type: "app", path: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe" },
    { id: "spotify", name: "Spotify", category: "outils", type: "app", path: "%APPDATA%\\Spotify\\Spotify.exe" },
    { id: "terminal", name: "Terminal", category: "outils", type: "app", path: "wt.exe" },
    { id: "instagram", name: "Instagram", category: "web", type: "web", url: "https://www.instagram.com" },
    { id: "amazon", name: "Amazon", category: "web", type: "web", url: "https://www.amazon.fr" },
    { id: "youtube", name: "YouTube", category: "web", type: "web", url: "https://www.youtube.com" },
    { id: "gmail", name: "Gmail", category: "web", type: "web", url: "https://mail.google.com" },
    { id: "github", name: "GitHub", category: "web", type: "web", url: "https://github.com" },
  ],
};

/** Jeux « installés » simulés : certains ont déjà une tuile dans SAMPLE. */
export const SAMPLE_STEAM_GAMES: SteamGame[] = [
  { appId: 1086940, name: "Baldur's Gate 3" },
  { appId: 730, name: "Counter-Strike 2" },
  { appId: 1091520, name: "Cyberpunk 2077" },
  { appId: 1145350, name: "Hades II" },
  { appId: 620, name: "Portal 2" },
  { appId: 413150, name: "Stardew Valley" },
];

/** Raccourcis du menu Démarrer simulés (VS Code a déjà une tuile dans SAMPLE). */
const START_MENU = "%ProgramData%\\Microsoft\\Windows\\Start Menu\\Programs";
const USER_START_MENU = "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs";
export const SAMPLE_APPS: Omit<DetectedApp, "inConfig">[] = [
  { name: "7-Zip File Manager", path: `${START_MENU}\\7-Zip\\7-Zip File Manager.lnk`, folder: "7-Zip" },
  { name: "Discord", path: `${USER_START_MENU}\\Discord Inc\\Discord.lnk`, folder: "Discord Inc" },
  { name: "GIMP 3", path: `${START_MENU}\\GIMP 3.lnk`, folder: null },
  { name: "Notepad++", path: `${START_MENU}\\Notepad++.lnk`, folder: null },
  { name: "Steam", path: `${START_MENU}\\Steam\\Steam.lnk`, folder: "Steam" },
  { name: "Visual Studio Code", path: `${USER_START_MENU}\\Visual Studio Code.lnk`, folder: null },
  { name: "VLC media player", path: `${START_MENU}\\VideoLAN\\VLC media player.lnk`, folder: "VideoLAN" },
];

/**
 * @param loadConfig d'où vient la config : les données d'exemple par défaut
 *   (les tests en passent une autre).
 * @param initialRecents récents simulés au démarrage.
 */
export function createMockIpc(
  loadConfig: () => Promise<Config> = async () => structuredClone(SAMPLE),
  initialRecents = ["bg3", "discord", "instagram", "cs2"],
): Omit<Ipc, "openConfigFolder"> {
  let config: Config | null = null;
  let recents = initialRecents;
  // Ajouts de l'import Steam simulé, réappliqués à chaque rechargement (la
  // config d'exemple, elle, repart de zéro à chaque `loadConfig`).
  let importedCategory: Category | null = null;
  const importedTiles: Tile[] = [];
  // Modifications faites dans l'éditeur, réappliquées de même à chaque rechargement.
  const edits: ((c: Config) => void)[] = [];
  /** Ids uniques même pour plusieurs ajouts dans la même milliseconde (import). */
  let tileCounter = 0;
  function edit(change: (c: Config) => void) {
    edits.push(change);
    if (config) change(config);
  }
  function categoryOf(c: Config, draft: TileDraft): string {
    const name = draft.newCategory?.trim();
    if (!name) return draft.category;
    const found = c.categories.find((x) => x.name.toLowerCase() === name.toLowerCase());
    if (found) return found.id;
    const id = name.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "categorie";
    c.categories.push({ id, name });
    return id;
  }
  function tileOf(id: string, category: string, draft: TileDraft): Tile {
    const { newCategory: _, ...rest } = draft;
    return { ...rest, id, category } as Tile;
  }

  return {
    async getConfig() {
      config = await loadConfig();
      if (importedCategory && !config.categories.some((c) => c.id === importedCategory?.id)) {
        config.categories.push(importedCategory);
      }
      config.tiles.push(...importedTiles.filter((t) => !config?.tiles.some((u) => u.id === t.id)));
      for (const change of edits) change(config);
      // On ignore les récents qui ne correspondent à aucune tuile de cette config.
      const ids = new Set(config.tiles.map((t) => t.id));
      recents = recents.filter((id) => ids.has(id));
      return structuredClone(config);
    },
    async getRecents() {
      return [...recents];
    },
    async launch(id) {
      const tile = config?.tiles.find((t) => t.id === id);
      if (!tile) throw new Error(`Tuile inconnue : ${id}`);
      console.info("[ipc mock] launch", tile);
      recents = [id, ...recents.filter((r) => r !== id)].slice(0, config?.settings.recentsCount ?? 4);
      return [...recents];
    },
    async setFavorite(id, favorite) {
      if (!config) return [];
      const has = config.favorites.includes(id);
      if (favorite && !has) config.favorites.push(id);
      if (!favorite && has) config.favorites = config.favorites.filter((f) => f !== id);
      return [...config.favorites];
    },
    async addTile(draft) {
      const tile = tileOf(`tuile-${Date.now()}-${++tileCounter}`, "", draft);
      edit((c) => {
        if (c.tiles.some((t) => t.id === tile.id)) return;
        tile.category = categoryOf(c, draft);
        c.tiles.push({ ...tile });
      });
      console.info("[ipc mock] add_tile", tile);
      return { ...tile };
    },
    async updateTile(id, draft) {
      if (!config?.tiles.some((t) => t.id === id)) throw new Error(`Tuile inconnue : ${id}`);
      let saved: Tile | undefined;
      edit((c) => {
        const i = c.tiles.findIndex((t) => t.id === id);
        if (i < 0) return;
        const tile = tileOf(id, categoryOf(c, draft), draft);
        // Changement de catégorie : la tuile passe en dernier, comme dans editor.rs.
        if (tile.category === c.tiles[i].category) c.tiles[i] = tile;
        else c.tiles.push(...c.tiles.splice(i, 1).map(() => tile));
        saved = tile;
      });
      return { ...saved! };
    },
    async deleteTile(id) {
      if (!config?.tiles.some((t) => t.id === id)) throw new Error(`Tuile inconnue : ${id}`);
      edit((c) => {
        c.tiles = c.tiles.filter((t) => t.id !== id);
        c.favorites = c.favorites.filter((f) => f !== id);
      });
    },
    async reorderTiles(category, ids) {
      edit((c) => applyTileOrder(c, category, ids));
    },
    async reorderCategories(ids) {
      edit((c) => {
        c.categories = ids.map((id) => c.categories.find((x) => x.id === id)).filter((x): x is Category => !!x);
      });
    },
    async reorderFavorites(ids) {
      edit((c) => {
        c.favorites = [...ids];
      });
    },
    async pickFile(images) {
      return images ? null : "C:\\Program Files\\Exemple\\Exemple.exe";
    },
    async detectSteamGames() {
      return structuredClone(SAMPLE_STEAM_GAMES);
    },
    async importSteamGames(appIds) {
      const cfg = config ?? (await this.getConfig());
      const category =
        cfg.categories.find((c) => c.id === "jeux") ?? (importedCategory = { id: "jeux", name: "Jeux" });
      if (!cfg.categories.includes(category)) cfg.categories.push(category);
      const known = new Set(cfg.tiles.flatMap((t) => (t.type === "steam" ? [t.appId] : [])));
      const added: string[] = [];
      for (const game of SAMPLE_STEAM_GAMES.filter((g) => appIds.includes(g.appId) && !known.has(g.appId))) {
        const tile: Tile = { id: `steam-${game.appId}`, name: game.name, category: category.id, type: "steam", appId: game.appId };
        importedTiles.push(tile);
        cfg.tiles.push(tile);
        added.push(tile.id);
      }
      return { added, category: category.name };
    },
    async onSteamImportRequested() {
      return () => {};
    },
    async detectApps() {
      const cfg = config ?? (await this.getConfig());
      const apps = cfg.tiles.flatMap((t) => (t.type === "app" ? [t] : []));
      const paths = new Set(apps.map((t) => t.path.toLowerCase()));
      const names = new Set(apps.map((t) => t.name.trim().toLowerCase()));
      return SAMPLE_APPS.map((a) => ({ ...a, inConfig: paths.has(a.path.toLowerCase()) || names.has(a.name.toLowerCase()) }));
    },
    async importApps(paths) {
      const fresh = (await this.detectApps()).filter((a) => !a.inConfig && paths.includes(a.path));
      const cfg = config!;
      const category = appsCategory(cfg);
      const existing = cfg.categories.find((c) => c.name === category);
      const added: string[] = [];
      for (const app of fresh) {
        const tile = await this.addTile({ name: app.name, category: existing?.id ?? "", newCategory: existing ? undefined : "Applications", type: "app", path: app.path, args: [] });
        added.push(tile.id);
      }
      return { added, category };
    },
    async onDetectAppsRequested() {
      return () => {};
    },
    // Pas d'extraction d'icônes dans le navigateur : les monogrammes restent.
    async getIcon() {
      return null;
    },
    async hideWindow() {
      console.info("[ipc mock] hide_window");
    },
    async getHotkeyStatus() {
      return { hotkey: config?.settings.hotkey ?? "Super+Alt+Space", error: null };
    },
    // Dans un navigateur, la fenêtre ne « réapparaît » jamais.
    async onWindowShown() {
      return () => {};
    },
    async onConfigChanged() {
      return () => {};
    },
  };
}
