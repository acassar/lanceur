<script setup lang="ts">
import { useDragReorder } from "../composables/useDragReorder";
import type { Section } from "../composables/useLauncher";
import { move } from "../lib/reorder";
import type { Tile } from "../lib/types";
import TileButton from "./TileButton.vue";

const props = defineProps<{
  section: Section;
  selected: number;
  favorites: Set<string>;
  query: string;
  /** Les tuiles peuvent être réordonnées par glisser-déposer. */
  reorderable: boolean;
}>();
const emit = defineEmits<{ launch: [index: number]; hover: [index: number]; toggleFavorite: [id: string]; edit: [tile: Tile]; reorder: [ids: string[], moved: string] }>();

/** Ids de la section au début du glisser : la config peut être rechargée entre-temps. */
let snapshot: string[] = [];
const drag = useDragReorder({
  axis: "x",
  onDrop: (from, to) => emit("reorder", move(snapshot, from, to), snapshot[from]),
});
function startDrag(e: PointerEvent, i: number) {
  snapshot = props.section.items.map((item) => item.tile.id);
  drag.start(e, i);
}
</script>

<template>
  <section class="group">
    <h2>{{ section.title }}<span>{{ section.items.length }}</span></h2>
    <div class="grid" :class="{ compact: section.compact }">
      <TileButton
        v-for="(item, i) in section.items"
        :key="item.index"
        :data-drag-index="reorderable ? i : undefined"
        :class="{
          dragging: drag.dragging.value === i,
          'drop-before': drag.target.value?.index === i && !drag.target.value.after,
          'drop-after': drag.target.value?.index === i && drag.target.value.after,
        }"
        :style="drag.dragging.value === i ? { transform: `translate(${drag.offset.value.x}px, ${drag.offset.value.y}px)` } : undefined"
        @pointerdown="reorderable && startDrag($event, i)"
        :tile="item.tile"
        :index="item.index"
        :selected="item.index === selected"
        :favorite="favorites.has(item.tile.id)"
        :compact="section.compact"
        :query="query"
        @launch="emit('launch', item.index)"
        @hover="drag.dragging.value === null && emit('hover', item.index)"
        @toggle-favorite="emit('toggleFavorite', item.tile.id)"
        @edit="emit('edit', item.tile)"
      />
    </div>
  </section>
</template>

<style scoped>
.group { display: flex; flex-direction: column; gap: 4px; padding-top: 10px; }
h2 {
  margin: 0 6px; display: flex; align-items: baseline; gap: 8px;
  font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.09em; color: var(--muted);
}
h2 span { font: 11px var(--font-mono); color: var(--faint); letter-spacing: 0; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(92px, 1fr)); gap: 2px; }
/* Glisser-déposer : la tuile suit la souris, un trait montre où elle ira. */
.dragging { opacity: 0.6; z-index: 2; pointer-events: none; box-shadow: 0 6px 18px rgb(0 0 0 / 45%); }
.drop-before::before, .drop-after::after {
  content: ""; position: absolute; top: 6px; bottom: 6px; width: 3px;
  border-radius: 2px; background: var(--accent);
}
.drop-before::before { left: -2px; }
.drop-after::after { right: -2px; }
.grid.compact { grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 6px; }
</style>
