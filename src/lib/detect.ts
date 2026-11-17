// Logique de la fenêtre « Détecter les applications », sans Vue (testée dans detect.test.ts).
import { games } from "./steam";
import { normalize } from "./search";
import type { AppsImportResult, Config, DetectedApp, SteamImportResult } from "./types";

/** « 1 application », « 3 applications ». */
export function apps(n: number): string {
  return `${n} application${n > 1 ? "s" : ""}`;
}

/** Ce qui sera ajouté : « 2 applications et 1 jeu », « 3 jeux », « rien ». */
export function selection(nApps: number, nGames: number): string {
  const parts = [nApps > 0 ? apps(nApps) : "", nGames > 0 ? games(nGames) : ""].filter(Boolean);
  return parts.join(" et ") || "rien";
}

/**
 * Applications dont le nom ou le dossier contient le texte cherché (sans tenir
 * compte des majuscules ni des accents). Un filtre vide garde tout.
 */
export function filterApps(list: DetectedApp[], query: string): DetectedApp[] {
  const q = normalize(query.trim());
  if (!q) return list;
  return list.filter((a) => normalize(a.name).includes(q) || normalize(a.folder ?? "").includes(q));
}

/** Message affiché après l'ajout (un import peut manquer : rien de coché de ce côté). */
export function detectMessage(appsResult: AppsImportResult | null, gamesResult: SteamImportResult | null): string {
  const nApps = appsResult?.added.length ?? 0;
  const nGames = gamesResult?.added.length ?? 0;
  if (nApps + nGames === 0) return "Rien de nouveau : tout avait déjà une tuile";
  const where = [
    nApps > 0 && appsResult ? `« ${appsResult.category} »` : "",
    nGames > 0 && gamesResult ? `« ${gamesResult.category} »` : "",
  ].filter(Boolean);
  // Accord : « 1 application ajoutée », mais « 2 applications et 1 jeu ajoutés ».
  const ending = (nGames === 0 ? "e" : "") + (nApps + nGames > 1 ? "s" : "");
  return `${selection(nApps, nGames)} ajouté${ending} dans ${where.join(" et ")}`;
}

/** Noms de catégorie qui conviennent aux applications (`APPS_CATEGORIES` dans apps.rs). */
const APPS_CATEGORIES = ["applications", "apps", "programmes", "logiciels", "outils", "tools"];

/**
 * Nom de la catégorie où l'import rangera les applications : même règle que
 * `plan` dans apps.rs (id connu, sinon nom connu, sinon nouvelle « Applications »).
 */
export function appsCategory(config: Config | null): string {
  const cats = config?.categories ?? [];
  const found =
    cats.find((c) => APPS_CATEGORIES.includes(c.id)) ?? cats.find((c) => APPS_CATEGORIES.includes(c.name.trim().toLowerCase()));
  return found?.name ?? "Applications";
}
