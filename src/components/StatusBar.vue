<script setup lang="ts">
defineProps<{ kind: string; target: string }>();
</script>

<template>
  <footer class="status">
    <div class="target"><span class="kind">{{ kind }}</span><code :title="target">{{ target }}</code></div>
    <div class="keys">
      <span><kbd>↵</kbd> lancer</span>
      <span><kbd>Tab</kbd> catégorie</span>
      <span><kbd>Ctrl D</kbd> favori</span>
      <span><kbd>Échap</kbd> masquer</span>
    </div>
  </footer>
</template>

<style scoped>
.status {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  border-top: 1px solid var(--line); padding: 8px 14px; font-size: 12px; color: var(--faint);
}
.target { display: flex; align-items: center; gap: 8px; min-width: 0; }
.kind { flex: none; color: var(--muted); }
code { font: 12px var(--font-mono); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.keys { display: flex; gap: 10px; flex: none; }
.keys span { display: inline-flex; gap: 4px; align-items: center; }
</style>
