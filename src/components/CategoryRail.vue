<script setup lang="ts">
import type { RailItem, RailKey } from "../composables/useLauncher";

defineProps<{ items: RailItem[]; tileCount: number }>();
const active = defineModel<RailKey>({ required: true });
</script>

<template>
  <nav class="rail" aria-label="Catégories">
    <button
      v-for="item in items"
      :key="item.key"
      type="button"
      tabindex="-1"
      :aria-pressed="item.key === active"
      @mousedown.prevent
      @click="active = item.key"
    >
      {{ item.label }}<span>{{ item.count }}</span>
    </button>
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
button[aria-pressed="true"] { background: var(--surface-2); color: var(--fg); box-shadow: inset 2px 0 0 var(--accent); }
.foot { margin-top: auto; padding: 8px 10px 0; font-size: 11px; color: var(--faint); line-height: 1.5; }
</style>
