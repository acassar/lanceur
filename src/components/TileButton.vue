<script setup lang="ts">
import { computed } from "vue";
import { highlight } from "../lib/search";
import type { Tile } from "../lib/types";
import TileIcon from "./TileIcon.vue";

const props = defineProps<{
  tile: Tile;
  index: number;
  selected: boolean;
  favorite: boolean;
  compact: boolean;
  query: string;
}>();
const emit = defineEmits<{ launch: []; hover: []; toggleFavorite: []; edit: [] }>();
const segments = computed(() => highlight(props.tile.name, props.query));
</script>

<template>
  <button
    type="button"
    tabindex="-1"
    class="tile"
    :class="{ sel: selected, compact }"
    :data-index="index"
    @mousemove="emit('hover')"
    @click="emit('launch')"
    @contextmenu.prevent="emit('edit')"
  >
    <TileIcon :tile="tile" />
    <span class="name"
      ><template v-for="(s, i) in segments" :key="i"
        ><mark v-if="s.match">{{ s.text }}</mark
        ><template v-else>{{ s.text }}</template></template
      ></span
    >
    <span v-if="!compact" class="edit" title="Modifier la tuile (Ctrl+E ou clic droit)" @click.stop="emit('edit')">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M4 20h4L19 9l-4-4L4 16z" /><path d="m13.5 6.5 4 4" />
      </svg>
    </span>
    <span
      v-if="!compact"
      class="star"
      :class="{ on: favorite }"
      :title="favorite ? 'Retirer des favoris (Ctrl+D)' : 'Ajouter aux favoris (Ctrl+D)'"
      @click.stop="emit('toggleFavorite')"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" aria-hidden="true">
        <path d="m12 3 2.7 5.6 6.1.8-4.5 4.2 1.1 6.1L12 16.8l-5.4 2.9 1.1-6.1-4.5-4.2 6.1-.8z" />
      </svg>
    </span>
  </button>
</template>

<style scoped>
.tile {
  position: relative;
  font: inherit; color: var(--fg); background: none;
  border: 1px solid transparent; border-radius: 10px;
  padding: 12px 6px 10px; cursor: pointer; min-width: 0;
  display: flex; flex-direction: column; align-items: center; gap: 8px;
}
.tile:hover { background: var(--surface); }
.tile.sel { background: var(--surface-2); border-color: color-mix(in srgb, var(--accent) 60%, transparent); }
.name {
  font-size: 12.5px; line-height: 1.25; text-align: center; max-width: 100%;
  display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; line-clamp: 2; overflow: hidden;
}
mark { background: none; color: var(--accent); }

.compact { flex-direction: row; justify-content: flex-start; padding: 8px 10px; gap: 10px; background: var(--surface); --ico: 30px; }
.compact .name { text-align: left; -webkit-line-clamp: 1; line-clamp: 1; }
.compact :deep(.badge) { display: none; }
.compact :deep(.face) { border-radius: 8px; }

.star, .edit {
  position: absolute; top: 4px; right: 4px; width: 22px; height: 22px; border-radius: 6px;
  display: grid; place-items: center; color: var(--faint); opacity: 0; transition: opacity 0.12s;
}
.edit { right: auto; left: 4px; }
.star svg, .edit svg { width: 13px; height: 13px; }
.star:hover, .edit:hover { color: var(--fg); background: var(--surface); }
.tile:hover .star, .tile.sel .star, .star.on, .tile:hover .edit, .tile.sel .edit { opacity: 1; }
.star.on { color: var(--accent); }
.star.on svg { fill: currentColor; }
</style>
