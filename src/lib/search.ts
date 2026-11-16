import type { Tile } from "./types";

/** Minuscules et sans accents : « Élan » et « elan » se valent. */
export function normalize(s: string): string {
  return s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
}

const WORD_SPLIT = /[\s\-:]+/;

/** Initiales des mots : « Baldur's Gate 3 » → « bg3 ». */
export function initials(name: string): string {
  return normalize(name)
    .split(WORD_SPLIT)
    .filter(Boolean)
    .map((w) => (/^\d+$/.test(w) ? w : w[0]))
    .join("");
}

/**
 * Score de correspondance entre une tuile et la recherche. 0 = pas de correspondance.
 * Plus le score est haut, plus la tuile remonte dans les résultats.
 */
export function score(name: string, query: string): number {
  const q = normalize(query.trim());
  if (!q) return 1;
  const n = normalize(name);
  if (n.startsWith(q)) return 4;
  if (n.split(WORD_SPLIT).some((w) => w.startsWith(q))) return 3;
  if (initials(name).startsWith(q)) return 2;
  if (n.includes(q)) return 1;
  return 0;
}

/** Filtre les tuiles. Avec une recherche, les trie par score (ordre d'origine à égalité). */
export function filterTiles(tiles: Tile[], query: string): Tile[] {
  if (!query.trim()) return tiles;
  return tiles
    .map((tile, i) => ({ tile, i, s: score(tile.name, query) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.tile);
}

export interface Segment {
  text: string;
  match: boolean;
}

/** Découpe un nom pour surligner la partie qui correspond à la recherche. */
export function highlight(name: string, query: string): Segment[] {
  const q = normalize(query.trim());
  if (!q) return [{ text: name, match: false }];
  // normalize() garde la même longueur que l'original pour les lettres accentuées
  // composées (é → e + accent retiré) : on retrouve la position dans le nom d'origine.
  const n = normalize(name);
  const i = n.indexOf(q);
  if (i < 0 || n.length !== name.length) return [{ text: name, match: false }];
  return [
    { text: name.slice(0, i), match: false },
    { text: name.slice(i, i + q.length), match: true },
    { text: name.slice(i + q.length), match: false },
  ].filter((s) => s.text);
}
