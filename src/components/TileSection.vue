<script setup lang="ts">
import type { Section } from "../composables/useLauncher";
import TileButton from "./TileButton.vue";

defineProps<{ section: Section; selected: number; favorites: Set<string>; query: string }>();
const emit = defineEmits<{ launch: [index: number]; hover: [index: number]; toggleFavorite: [id: string] }>();
</script>

<template>
  <section class="group">
    <h2>{{ section.title }}<span>{{ section.items.length }}</span></h2>
    <div class="grid" :class="{ compact: section.compact }">
      <TileButton
        v-for="item in section.items"
        :key="item.index"
        :tile="item.tile"
        :index="item.index"
        :selected="item.index === selected"
        :favorite="favorites.has(item.tile.id)"
        :compact="section.compact"
        :query="query"
        @launch="emit('launch', item.index)"
        @hover="emit('hover', item.index)"
        @toggle-favorite="emit('toggleFavorite', item.tile.id)"
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
.grid.compact { grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 6px; }
</style>
