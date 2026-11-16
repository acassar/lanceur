// État central de la fenêtre : config, recherche, catégorie, sélection,
// et sections calculées à afficher (variante B de la maquette).
import { computed, ref, watch } from "vue";
import { ipc } from "../lib/ipc";
import { filterTiles } from "../lib/search";
import type { Config, Tile } from "../lib/types";

/** Entrée du rail : « Tout », « Favoris » ou l'id d'une catégorie. */
export type RailKey = "all" | "favorites" | string;

export interface RailItem {
  key: RailKey;
  label: string;
  count: number;
}

export interface Section {
  key: string;
  title: string;
  /** Tuiles larges sur une ligne (Récents). */
  compact: boolean;
  /** Chaque tuile porte son index global dans l'ordre de navigation. */
  items: { tile: Tile; index: number }[];
}

export function useLauncher() {
  const config = ref<Config | null>(null);
  const recents = ref<string[]>([]);
  const query = ref("");
  const rail = ref<RailKey>("all");
  const selected = ref(0);
  const error = ref<string | null>(null);

  async function load() {
    try {
      config.value = await ipc.getConfig();
      recents.value = await ipc.getRecents();
    } catch (e) {
      error.value = String(e);
    }
  }

  const byId = computed(() => new Map((config.value?.tiles ?? []).map((t) => [t.id, t])));
  const pick = (ids: string[]) => ids.map((id) => byId.value.get(id)).filter((t): t is Tile => !!t);

  /** Tuiles qui correspondent à la recherche, toutes catégories confondues. */
  const matching = computed(() => filterTiles(config.value?.tiles ?? [], query.value));
  const favoriteIds = computed(() => new Set(config.value?.favorites ?? []));
  const favoriteTiles = computed(() =>
    query.value.trim()
      ? matching.value.filter((t) => favoriteIds.value.has(t.id))
      : pick(config.value?.favorites ?? []),
  );

  const railItems = computed<RailItem[]>(() => [
    { key: "all", label: "Tout", count: matching.value.length },
    { key: "favorites", label: "Favoris", count: favoriteTiles.value.length },
    ...(config.value?.categories ?? []).map((c) => ({
      key: c.id,
      label: c.name,
      count: matching.value.filter((t) => t.category === c.id).length,
    })),
  ]);

  const sections = computed<Section[]>(() => {
    const cfg = config.value;
    if (!cfg) return [];
    const searching = query.value.trim() !== "";
    const raw: { key: string; title: string; compact?: boolean; tiles: Tile[] }[] = [];

    if (rail.value === "favorites") {
      raw.push({ key: "favorites", title: "Favoris", tiles: favoriteTiles.value });
    } else if (rail.value === "all") {
      if (searching) {
        raw.push({ key: "results", title: "Résultats", tiles: matching.value });
      } else {
        raw.push({ key: "favorites", title: "Favoris", tiles: favoriteTiles.value });
        raw.push({ key: "recents", title: "Récents", compact: true, tiles: pick(recents.value) });
        for (const c of cfg.categories) {
          raw.push({ key: c.id, title: c.name, tiles: matching.value.filter((t) => t.category === c.id) });
        }
      }
    } else {
      const cat = cfg.categories.find((c) => c.id === rail.value);
      raw.push({ key: rail.value, title: cat?.name ?? rail.value, tiles: matching.value.filter((t) => t.category === rail.value) });
    }

    let index = 0;
    return raw
      .filter((s) => s.tiles.length > 0)
      .map((s) => ({
        key: s.key,
        title: s.title,
        compact: !!s.compact,
        items: s.tiles.map((tile) => ({ tile, index: index++ })),
      }));
  });

  /** Ordre de navigation : la même tuile peut apparaître deux fois (Favoris + catégorie). */
  const entries = computed(() => sections.value.flatMap((s) => s.items.map((i) => ({ section: s.key, tile: i.tile }))));
  const flat = computed(() => entries.value.map((e) => e.tile));
  const current = computed<Tile | undefined>(() => flat.value[selected.value]);

  // Nouvelle recherche ou nouvelle catégorie : on revient sur la première tuile.
  watch([query, rail], () => (selected.value = 0));
  watch(flat, (list) => {
    if (selected.value >= list.length) selected.value = Math.max(0, list.length - 1);
  });

  function cycleRail(step: 1 | -1) {
    const keys = railItems.value.map((r) => r.key);
    const i = keys.indexOf(rail.value);
    rail.value = keys[(i + step + keys.length) % keys.length];
  }

  async function toggleFavorite(id: string) {
    if (!config.value) return;
    // Ajouter/retirer un favori change la section Favoris et décale les index :
    // on mémorise la tuile sélectionnée pour la retrouver ensuite.
    const before = entries.value[selected.value];
    config.value.favorites = await ipc.toggleFavorite(id);
    if (!before) return;
    const list = entries.value;
    let i = list.findIndex((e) => e.section === before.section && e.tile.id === before.tile.id);
    if (i < 0) i = list.findIndex((e) => e.tile.id === before.tile.id);
    if (i >= 0) selected.value = i;
  }

  /** Lance une tuile. Renvoie la tuile lancée pour afficher le toast. */
  async function launch(index = selected.value): Promise<Tile | undefined> {
    const tile = flat.value[index];
    if (!tile) return;
    recents.value = await ipc.launch(tile.id);
    if (config.value?.settings.hideAfterLaunch) {
      query.value = "";
      await ipc.hideWindow();
    }
    return tile;
  }

  return {
    config,
    error,
    query,
    rail,
    selected,
    railItems,
    sections,
    flat,
    current,
    favoriteIds,
    load,
    cycleRail,
    toggleFavorite,
    launch,
  };
}
