import { ref, watch, type Ref } from "vue";
import { createIconCache, iconKey } from "../lib/icons";
import { ipc } from "../lib/ipc";
import type { Tile } from "../lib/types";

// Un seul cache pour toute l'app : toutes les tuiles le partagent.
const cache = createIconCache((id) => ipc.getIcon(id));

/**
 * Icône d'une tuile en `data:` URL, ou `null` tant qu'elle n'est pas connue
 * (ou introuvable) : la tuile affiche alors son monogramme.
 */
export function useTileIcon(tile: () => Tile): Ref<string | null> {
  const src = ref<string | null>(null);
  watch(
    tile,
    (t) => {
      // Déjà connue : affichage immédiat, sans attendre de promesse.
      const known = cache.peek(t);
      src.value = known ?? null;
      if (known) return;
      const key = iconKey(t);
      void cache.load(t).then((loaded) => {
        // La tuile a pu changer entre-temps : on n'écrase pas sa nouvelle icône.
        if (iconKey(tile()) === key) src.value = loaded;
      });
    },
    { immediate: true },
  );
  return src;
}
