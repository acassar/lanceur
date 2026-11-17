// Logique de la fenêtre d'import Steam, sans Vue (testée dans steam.test.ts).
import type { Config, SteamGame, SteamImportResult } from "./types";

/** Un jeu de la liste à cocher. */
export interface SteamChoice extends SteamGame {
  /** Le jeu a déjà une tuile : il est affiché, mais ne peut pas être coché. */
  inConfig: boolean;
}

/** Marque les jeux qui ont déjà une tuile `steam` dans la config. */
export function toChoices(games: SteamGame[], config: Config | null): SteamChoice[] {
  const known = new Set((config?.tiles ?? []).flatMap((t) => (t.type === "steam" ? [t.appId] : [])));
  return games.map((g) => ({ ...g, inConfig: known.has(g.appId) }));
}

/**
 * Nom de la catégorie où l'import rangera les jeux : même règle que `plan`
 * dans steam.rs (id `jeux`, sinon une catégorie nommée « Jeux » ou « Games »,
 * sinon une nouvelle catégorie « Jeux »).
 */
export function targetCategory(config: Config | null): string {
  const categories = config?.categories ?? [];
  const found =
    categories.find((c) => c.id === "jeux") ??
    categories.find((c) => ["jeux", "games"].includes(c.name.trim().toLowerCase()));
  return found?.name ?? "Jeux";
}

/** « 1 jeu », « 3 jeux ». */
export function games(n: number): string {
  return `${n} jeu${n > 1 ? "x" : ""}`;
}

/** Message affiché après l'import. */
export function importMessage(result: SteamImportResult): string {
  const n = result.added.length;
  if (n === 0) return "Aucun nouveau jeu : ils ont tous déjà une tuile";
  return `${games(n)} ajouté${n > 1 ? "s" : ""} dans « ${result.category} »`;
}
