// Calculs du glisser-déposer, sans DOM ni Vue (testés dans reorder.test.ts).
import type { Config } from "./types";

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Dépôt avant (`after: false`) ou après l'élément `index`. */
export interface DropTarget {
  index: number;
  after: boolean;
}

/**
 * Élément le plus proche du pointeur, et de quel côté déposer. `x` : grille ou
 * ligne (moitié gauche/droite) ; `y` : liste verticale (moitié haut/bas).
 * Dans une grille, on cherche d'abord la bonne ligne, puis la colonne.
 */
export function dropTarget(items: { index: number; rect: Rect }[], p: { x: number; y: number }, axis: "x" | "y"): DropTarget | null {
  if (items.length === 0) return null;
  const distance = (r: Rect, v: number, lo: "left" | "top", hi: "right" | "bottom") => (v < r[lo] ? r[lo] - v : v > r[hi] ? v - r[hi] : 0);
  const [main, cross] = axis === "x" ? (["x", "y"] as const) : (["y", "x"] as const);
  const [lo, hi] = main === "x" ? (["left", "right"] as const) : (["top", "bottom"] as const);
  const [clo, chi] = cross === "x" ? (["left", "right"] as const) : (["top", "bottom"] as const);
  // Ligne (ou colonne) la plus proche sur l'axe secondaire, puis l'élément le plus proche dedans.
  const best = Math.min(...items.map((i) => distance(i.rect, p[cross], clo, chi)));
  const row = items.filter((i) => distance(i.rect, p[cross], clo, chi) === best);
  const item = row.reduce((a, b) => (distance(b.rect, p[main], lo, hi) < distance(a.rect, p[main], lo, hi) ? b : a));
  const center = (item.rect[lo] + item.rect[hi]) / 2;
  return { index: item.index, after: p[main] > center };
}

/** `list` avec l'élément `from` déplacé à la position `to`. */
export function move<T>(list: T[], from: number, to: number): T[] {
  const copy = [...list];
  const [item] = copy.splice(from, 1);
  copy.splice(to, 0, item);
  return copy;
}

/**
 * Applique un nouvel ordre des tuiles d'une catégorie à `config.tiles`, comme
 * `reorder_tiles` dans editor.rs : seules les places de cette catégorie changent.
 */
export function applyTileOrder(config: Config, category: string, ids: string[]): void {
  const byId = new Map(config.tiles.map((t) => [t.id, t]));
  let k = 0;
  config.tiles = config.tiles.map((t) => (t.category === category ? (byId.get(ids[k++]) ?? t) : t));
}
