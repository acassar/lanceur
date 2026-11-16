// Fausse couche IPC : elle imite les commandes Rust avec des données d'exemple
// gardées en mémoire. Elle permet de développer et tester toute l'interface dans
// un navigateur (npm run dev), sans Rust.
//
// Dans l'app (Tauri), ipc.ts utilise les vraies commandes Rust à la place.
import type { Ipc } from "./ipc";
import type { Config } from "./types";

const SAMPLE: Config = {
  version: 1,
  settings: {
    hotkey: "Ctrl+Alt+Space",
    hideAfterLaunch: true,
    hideOnBlur: true,
    accent: "#f0a35e",
    browser: "msedge",
    recentsCount: 4,
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

  return {
    async getConfig() {
      config = await loadConfig();
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
    // Pas d'extraction d'icônes dans le navigateur : les monogrammes restent.
    async getIcon() {
      return null;
    },
    async hideWindow() {
      console.info("[ipc mock] hide_window");
    },
    async getHotkeyStatus() {
      return { hotkey: config?.settings.hotkey ?? "Ctrl+Alt+Space", error: null };
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
