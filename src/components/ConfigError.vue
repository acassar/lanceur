<script setup lang="ts">
// Affiche une erreur de lecture de config.json à la place des tuiles (ou en
// bandeau au-dessus si une config valide est déjà affichée).
import type { IpcError } from "../lib/ipc";

defineProps<{ error: IpcError; compact?: boolean }>();
defineEmits<{ retry: []; openFolder: [] }>();
</script>

<template>
  <div class="config-error" :class="{ compact }" role="alert">
    <p class="title">
      {{ compact ? "La nouvelle configuration est invalide : la précédente reste affichée." : "Impossible de lire la configuration." }}
    </p>
    <pre class="message selectable">{{ error.message }}</pre>
    <p v-if="error.path" class="path"><code>{{ error.path }}</code></p>
    <div class="actions">
      <button type="button" @click="$emit('openFolder')">Ouvrir le dossier</button>
      <button type="button" class="primary" @click="$emit('retry')">Réessayer</button>
    </div>
  </div>
</template>

<style scoped>
.config-error {
  margin: 32px auto; max-width: 560px; padding: 16px 18px;
  background: var(--surface); border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--line));
  border-radius: 10px;
}
.config-error.compact { margin: 10px 0; }
.title { margin: 0 0 8px; font-weight: 500; }
.message {
  margin: 0; white-space: pre-wrap; word-break: break-word;
  font: 12.5px/1.5 var(--font-mono); color: var(--fg);
  user-select: text; cursor: text;
}
.path { margin: 8px 0 0; }
.path code { font: 12px var(--font-mono); color: var(--muted); word-break: break-all; }
.actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 14px; }
button {
  font: inherit; font-size: 13px; color: var(--fg); cursor: pointer;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 12px;
}
button:hover { border-color: var(--muted); }
button.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
</style>
