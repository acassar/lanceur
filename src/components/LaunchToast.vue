<script setup lang="ts">
defineProps<{ message: string | null; detail?: string; error?: boolean }>();
</script>

<template>
  <div class="toast" :class="{ on: !!message, error }" role="status" aria-live="polite">
    <span v-if="!error" class="spin" aria-hidden="true"></span>
    <span>{{ message }}</span>
    <code v-if="detail">{{ detail }}</code>
  </div>
</template>

<style scoped>
.toast {
  position: fixed; left: 50%; bottom: 52px; transform: translateX(-50%) translateY(10px);
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 10px;
  padding: 10px 14px; display: flex; gap: 10px; align-items: center; font-size: 13px;
  max-width: calc(100% - 32px);
  opacity: 0; pointer-events: none; transition: opacity 0.2s, transform 0.2s;
}
.toast.on { opacity: 1; transform: translateX(-50%) translateY(0); }
.toast.error { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
code { font: 12px var(--font-mono); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.spin {
  width: 14px; height: 14px; border-radius: 50%; flex: none;
  border: 2px solid var(--line); border-top-color: var(--accent);
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
