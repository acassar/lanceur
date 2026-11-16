import type { Settings, Tile } from "./types";

/** Libellé du type de tuile, affiché dans la barre d'état. */
export function kindLabel(tile: Tile): string {
  switch (tile.type) {
    case "steam":
      return "Jeu Steam";
    case "web":
      return "App web";
    case "app":
      return tile.path.toLowerCase().endsWith(".lnk") ? "Raccourci .lnk" : "Application";
  }
}

/** La cible exacte qui sera lancée, telle que le Rust l'exécutera. */
export function targetText(tile: Tile, settings: Pick<Settings, "browser">): string {
  switch (tile.type) {
    case "steam":
      return `steam://rungameid/${tile.appId}`;
    case "web":
      return `${settings.browser} --app=${tile.url}`;
    case "app":
      return [tile.path, ...(tile.args ?? [])].join(" ");
  }
}

/** Monogramme de secours tant que les vraies icônes n'existent pas (étape 6). */
export function monogram(name: string): string {
  const words = name.split(/[\s\-:]+/).filter(Boolean);
  if (words.length === 1) return words[0].slice(0, 2);
  return words
    .slice(0, 3)
    .map((w) => (/^\d+$/.test(w) ? w : w[0].toUpperCase()))
    .join("")
    .slice(0, 3);
}

/** Couleur stable dérivée de l'id : la même tuile garde toujours la même teinte. */
export function hueOf(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h % 360;
}
