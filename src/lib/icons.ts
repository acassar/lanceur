// Icônes des tuiles, demandées à Rust (commande get_icon) une seule fois par
// tuile et par session.
//
// Rust garde déjà les icônes en cache sur le disque. Ce cache-ci, en mémoire,
// évite en plus de redemander l'icône quand une tuile réapparaît (changement
// de catégorie, recherche) : elle s'affiche alors immédiatement, sans clignoter.
import type { Tile } from "./types";

type Fetch = (id: string) => Promise<string | null>;

/**
 * Clé de cache : l'id, plus tout ce qui détermine l'icône. Si la tuile change
 * dans config.json (autre chemin, autre `icon`), l'icône est redemandée.
 */
export function iconKey(tile: Tile): string {
  const { name: _name, category: _category, ...source } = tile;
  return JSON.stringify(source, Object.keys(source).sort());
}

export function createIconCache(fetch: Fetch) {
  const pending = new Map<string, Promise<string | null>>();
  const resolved = new Map<string, string>();

  return {
    /** Icône déjà trouvée (`undefined` sinon : pas encore demandée, en cours ou introuvable). */
    peek(tile: Tile): string | undefined {
      return resolved.get(iconKey(tile));
    },
    /** Demande l'icône (une seule requête par clé, même en parallèle). */
    load(tile: Tile): Promise<string | null> {
      const key = iconKey(tile);
      let promise = pending.get(key);
      if (!promise) {
        promise = fetch(tile.id)
          // Une erreur inattendue ne doit rien casser : monogramme.
          .catch(() => null)
          .then((src) => {
            if (src) {
              resolved.set(key, src);
            } else {
              // Pas d'icône : on n'en garde pas souvenir, pour redemander à la
              // prochaine apparition de la tuile (fichier revenu, réseau
              // rétabli…). Rust répond vite : il sait déjà quoi réessayer.
              pending.delete(key);
            }
            return src;
          });
        pending.set(key, promise);
      }
      return promise;
    },
  };
}
