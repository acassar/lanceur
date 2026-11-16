<script setup lang="ts">
import { onMounted, ref } from "vue";

defineProps<{ count: number; total: number }>();
const query = defineModel<string>({ required: true });
const input = ref<HTMLInputElement | null>(null);

/** Met le focus dans la recherche. `select` sélectionne aussi le texte (utile à l'ouverture). */
function focus(select = true) {
  input.value?.focus();
  if (select) input.value?.select();
}
onMounted(focus);
defineExpose({ focus, input });
</script>

<template>
  <div class="search">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
      <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
    </svg>
    <input
      ref="input"
      v-model="query"
      type="text"
      placeholder="Rechercher un jeu, une app, un site…"
      autocomplete="off"
      spellcheck="false"
      aria-label="Rechercher"
    />
    <span v-if="query.trim()" class="count">{{ count }} / {{ total }}</span>
  </div>
</template>

<style scoped>
.search { position: relative; margin: 12px 14px 6px; }
svg { position: absolute; left: 14px; top: 50%; transform: translateY(-50%); width: 18px; height: 18px; color: var(--muted); }
input {
  width: 100%;
  font: 400 17px var(--font-ui);
  color: var(--fg);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 12px 90px 12px 42px;
  outline: none;
}
input::placeholder { color: var(--faint); }
input:focus {
  border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
}
.count { position: absolute; right: 14px; top: 50%; transform: translateY(-50%); font: 12px var(--font-mono); color: var(--faint); }
</style>
