<script setup lang="ts">
import { computed } from "vue";
import { useDragReorder } from "../composables/useDragReorder";
import type { RailItem, RailKey } from "../composables/useLauncher";
import { move } from "../lib/reorder";

const props = defineProps<{ items: RailItem[]; tileCount: number; reorderable: boolean }>();
const emit = defineEmits<{ addTile: []; settings: []; reorder: [ids: string[]] }>();
// (même précaution que TileSection : liste figée au début du glisser)
let snapshot: string[] = [];

// Les deux premières entrées (Tout, Favoris) sont fixes : seules les
// catégories se réordonnent, en glissant verticalement.
const FIXED = 2;
const categoryKeys = computed(() => props.items.slice(FIXED).map((i) => i.key));
const drag = useDragReorder({
  axis: "y",
  onDrop: (from, to) => emit("reorder", move(snapshot, from, to)),
});
function startDrag(e: PointerEvent, i: number) {
  snapshot = [...categoryKeys.value];
  drag.start(e, i);
}
const cat = (i: number) => i - FIXED;
const active = defineModel<RailKey>({ required: true });
</script>

<template>
  <nav class="rail" aria-label="Catégories">
    <button
      v-for="(item, i) in items"
      :key="item.key"
      :data-drag-index="reorderable && i >= FIXED ? cat(i) : undefined"
      :class="{
        dragging: drag.dragging.value === cat(i),
        'drop-before': i >= FIXED && drag.target.value?.index === cat(i) && !drag.target.value.after,
        'drop-after': i >= FIXED && drag.target.value?.index === cat(i) && drag.target.value.after,
      }"
      :style="i >= FIXED && drag.dragging.value === cat(i) ? { transform: `translateY(${drag.offset.value.y}px)` } : undefined"
      :title="reorderable && i >= FIXED ? 'Glisser pour réordonner' : undefined"
      @pointerdown="reorderable && i >= FIXED && startDrag($event, cat(i))"
      type="button"
      tabindex="-1"
      :aria-current="item.key === active ? 'true' : undefined"
      @click="active = item.key"
    >
      {{ item.label }}<span>{{ item.count }}</span>
    </button>
    <button type="button" tabindex="-1" class="add" title="Ajouter une tuile (Ctrl+N)" @click="$emit('addTile')">+ Ajouter une tuile</button>
    <button type="button" tabindex="-1" class="tool" title="Paramètres (Ctrl+,)" @click="$emit('settings')">⚙ Paramètres</button>
    <div class="foot">config.json<br />{{ tileCount }} tuiles</div>
  </nav>
</template>

<style scoped>
.rail {
  display: flex; flex-direction: column; gap: 2px;
  width: 168px; flex: none;
  border-right: 1px solid var(--line);
  padding: 10px 8px;
}
button {
  font: inherit; font-size: 13px; color: var(--muted);
  background: none; border: 0; text-align: left;
  padding: 7px 10px; border-radius: 8px; cursor: pointer;
  display: flex; justify-content: space-between; align-items: center;
}
button span { font: 11px var(--font-mono); color: var(--faint); }
button:hover { background: var(--surface); color: var(--fg); }
button[aria-current="true"] { background: var(--surface-2); color: var(--fg); box-shadow: inset 2px 0 0 var(--accent); }
button.dragging { position: relative; z-index: 2; pointer-events: none; background: var(--surface-2); color: var(--fg); box-shadow: 0 6px 18px rgb(0 0 0 / 45%); }
button.drop-before, button[aria-current="true"].drop-before { box-shadow: inset 0 2px 0 var(--accent); }
button.drop-after, button[aria-current="true"].drop-after { box-shadow: inset 0 -2px 0 var(--accent); }
.add { margin-top: auto; color: var(--faint); }
.add:hover, .tool:hover { color: var(--accent); }
.tool { color: var(--faint); }
.foot { padding: 8px 10px 0; font-size: 11px; color: var(--faint); line-height: 1.5; }
</style>
