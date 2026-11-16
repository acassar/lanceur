<script setup lang="ts">
// Icône de secours (monogramme coloré) en attendant les vraies icônes de l'étape 6.
// Trois styles selon le type : jaquette (Steam), icône pleine (app), favicon sur plaque (web).
import { computed } from "vue";
import { hueOf, monogram } from "../lib/tiles";
import type { Tile } from "../lib/types";

const props = defineProps<{ tile: Tile }>();
const background = computed(() => {
  const h = hueOf(props.tile.id);
  return `linear-gradient(145deg, hsl(${h} 55% 52%), hsl(${(h + 25) % 360} 60% 22%))`;
});
const letters = computed(() => monogram(props.tile.name));
</script>

<template>
  <div class="ico" :class="tile.type">
    <div v-if="tile.type === 'web'" class="face"><div class="fav" :style="{ background }">{{ letters }}</div></div>
    <div v-else class="face" :style="{ background }">{{ letters }}</div>
    <span class="badge" aria-hidden="true">
      <svg v-if="tile.type === 'steam'" viewBox="0 0 10 10" fill="currentColor"><path d="M2.5 1.5v7l6-3.5z" /></svg>
      <svg v-else-if="tile.type === 'web'" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2">
        <circle cx="5" cy="5" r="3.6" /><path d="M1.4 5h7.2M5 1.4c1.4 1.6 1.4 5.6 0 7.2M5 1.4c-1.4 1.6-1.4 5.6 0 7.2" />
      </svg>
      <svg v-else viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.4">
        <rect x="1.5" y="2" width="7" height="6" rx="1" /><path d="M1.5 4h7" />
      </svg>
    </span>
  </div>
</template>

<style scoped>
.ico { position: relative; width: var(--ico, 46px); height: var(--ico, 46px); flex: none; }
.face {
  width: 100%; height: 100%; border-radius: 12px;
  display: grid; place-items: center;
  font-weight: 700; font-size: calc(var(--ico, 46px) * 0.36); letter-spacing: -0.02em; color: #fff;
}
.steam .face { border-radius: 9px; box-shadow: inset 0 -14px 18px -10px rgba(0, 0, 0, 0.45); }
.web .face { background: var(--surface-2); border: 1px solid var(--line); }
.fav {
  width: 48%; height: 48%; border-radius: 6px;
  display: grid; place-items: center; font-size: calc(var(--ico, 46px) * 0.22); color: #fff;
}
.badge {
  position: absolute; right: -4px; bottom: -4px; width: 18px; height: 18px; border-radius: 6px;
  background: var(--win); border: 1px solid var(--line);
  display: grid; place-items: center; color: var(--muted);
}
.badge svg { width: 10px; height: 10px; }
</style>
