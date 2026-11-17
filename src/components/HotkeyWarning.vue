<script setup lang="ts">
// Bandeau permanent quand le raccourci global n'a pas pu être enregistré
// (souvent parce qu'une autre application, Claude par exemple, utilise déjà le
// même). Il reste affiché tant que le problème dure : un message éphémère passe
// trop facilement inaperçu.
defineProps<{ message: string }>();
defineEmits<{ settings: [] }>();
</script>

<template>
  <div class="hotkey-warning" role="alert">
    <span class="icon" aria-hidden="true">!</span>
    <p>{{ message }}</p>
    <button type="button" @click="$emit('settings')">Changer le raccourci</button>
  </div>
</template>

<style scoped>
.hotkey-warning {
  display: flex; align-items: center; gap: 10px;
  border-top: 1px solid color-mix(in srgb, var(--accent) 45%, var(--line));
  background: color-mix(in srgb, var(--accent) 10%, var(--surface));
  padding: 8px 14px; font-size: 12.5px;
}
.icon {
  flex: none; width: 18px; height: 18px; border-radius: 50%;
  display: grid; place-items: center; font-weight: 700; font-size: 12px;
  background: var(--accent); color: var(--win);
}
p { margin: 0; flex: 1; min-width: 0; line-height: 1.4; }
button {
  flex: none; font: inherit; font-size: 12px; color: var(--fg); cursor: pointer;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 6px; padding: 4px 10px;
}
button:hover { border-color: var(--accent); }
</style>
