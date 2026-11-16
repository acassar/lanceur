<script setup lang="ts">
// Étape 0 : écran de vérification. Il prouve que :
//  1. le front Vue s'affiche dans la fenêtre Tauri ;
//  2. le front sait appeler une commande Rust (`ping`).
// Il sera remplacé par l'interface de la maquette à l'étape 1.
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";

const reponse = ref("…");
const version = ref("");
const erreur = ref(false);

// Tauri injecte cet objet dans la fenêtre. Il est absent dans un simple navigateur (npm run dev).
const dansTauri = "__TAURI_INTERNALS__" in window;

onMounted(async () => {
  if (!dansTauri) {
    erreur.value = true;
    reponse.value = "Pas de Rust ici : lancez l'app via Tauri pour tester l'appel.";
    return;
  }
  try {
    reponse.value = await invoke<string>("ping", { name: "Adrien" });
    version.value = await getVersion();
  } catch (e) {
    // Dans Tauri, une erreur ici est un vrai problème (permission, nom de commande…) : on l'affiche.
    erreur.value = true;
    reponse.value = `Échec de l'appel Rust : ${String(e)}`;
  }
});
</script>

<template>
  <main class="hello">
    <div class="logo" aria-hidden="true">
      <span></span><span></span><span></span><span class="rond"></span>
    </div>
    <h1>Lanceur</h1>
    <p :class="{ erreur }">{{ reponse }}</p>
    <p v-if="version" class="note">Version {{ version }}</p>
    <p class="note">Étape 0 : squelette Tauri 2 + Vue 3. L'interface arrive à l'étape 1.</p>
  </main>
</template>

<style>
:root {
  --bg: #15171c;
  --surface: #1d2027;
  --fg: #ebe8e2;
  --muted: #8d919b;
  --accent: #f0a35e;
  color-scheme: dark;
  font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;
}
html, body { margin: 0; height: 100%; background: var(--bg); color: var(--fg); }
#app { height: 100%; }
</style>

<style scoped>
.hello {
  height: 100%;
  display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px;
  text-align: center; padding: 0 24px;
}
.logo { display: grid; grid-template-columns: repeat(2, 22px); gap: 6px; margin-bottom: 8px; }
.logo span { width: 22px; height: 22px; border-radius: 6px; background: var(--surface); }
.logo .rond { border-radius: 50%; background: var(--accent); }
h1 { margin: 0; font-size: 22px; font-weight: 600; }
p { margin: 0; color: var(--fg); }
p.erreur { color: var(--accent); }
.note { color: var(--muted); font-size: 13px; }
</style>
