<script setup lang="ts">
// Écran d'accueil affiché tant que config.json ne contient aucune tuile (premier
// démarrage). Il explique où est le fichier et donne un exemple à copier ; le
// fichier étant relu dès qu'il est enregistré, les tuiles apparaissent aussitôt.
import { ref } from "vue";

const emit = defineEmits<{ openFolder: []; copyFailed: [] }>();

/**
 * Exemple à coller dans config.json à la place des trois listes vides : trois
 * catégories et une tuile de chaque type.
 */
const SNIPPET = `"categories": [
  { "id": "jeux", "name": "Jeux" },
  { "id": "outils", "name": "Outils" },
  { "id": "web", "name": "Web" }
],
"favorites": [],
"tiles": [
  { "id": "cs2", "name": "Counter-Strike 2", "category": "jeux",
    "type": "steam", "appId": 730 },
  { "id": "discord", "name": "Discord", "category": "outils",
    "type": "app", "path": "%LOCALAPPDATA%\\\\Discord\\\\Update.exe",
    "args": ["--processStart", "Discord.exe"] },
  { "id": "youtube", "name": "YouTube", "category": "web",
    "type": "web", "url": "https://www.youtube.com" }
]`;

const copied = ref(false);
let copiedTimer: ReturnType<typeof setTimeout> | undefined;
async function copy() {
  try {
    await navigator.clipboard.writeText(SNIPPET);
    copied.value = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied.value = false), 2000);
  } catch (e) {
    console.error(e);
    emit("copyFailed");
  }
}
</script>

<template>
  <section class="welcome" aria-labelledby="welcome-title">
    <h2 id="welcome-title">Aucune tuile pour l'instant</h2>
    <p>
      Le lanceur affiche les applications listées dans <code>config.json</code>.
      Ajoutez vos jeux Steam, programmes et sites dans ce fichier :
    </p>
    <p class="path"><code>%APPDATA%\io.github.acassar.lanceur\config.json</code></p>
    <ol>
      <li>Ouvrez le dossier et éditez <code>config.json</code> (VS Code propose l'autocomplétion).</li>
      <li>Remplacez les trois lignes <code>"categories"</code>, <code>"favorites"</code> et <code>"tiles"</code> de la fin, par exemple par :</li>
    </ol>
    <div class="actions">
      <button type="button" class="primary" @click="$emit('openFolder')">Ouvrir le dossier</button>
      <button type="button" @click="copy">{{ copied ? "Copié" : "Copier l'exemple" }}</button>
    </div>
    <pre class="snippet selectable">{{ SNIPPET }}</pre>
    <p class="hint">
      Enregistrez : les tuiles apparaissent aussitôt, sans redémarrer.
      Pour un jeu Steam, l'<code>appId</code> est le nombre dans l'adresse de sa page du magasin.
    </p>
  </section>
</template>

<style scoped>
.welcome {
  margin: 24px auto; max-width: 600px; padding: 18px 20px;
  background: var(--surface); border: 1px solid var(--line); border-radius: 10px;
  font-size: 13.5px;
}
h2 { margin: 0 0 8px; font-size: 16px; font-weight: 600; }
p, ol { margin: 0 0 10px; color: var(--muted); line-height: 1.5; }
ol { padding-left: 20px; }
code { font: 12px var(--font-mono); color: var(--fg); }
.path code { word-break: break-all; }
.snippet {
  margin: 0 0 10px; padding: 10px 12px; overflow-x: auto;
  background: var(--win); border: 1px solid var(--line); border-radius: 7px;
  font: 12px/1.5 var(--font-mono); color: var(--fg);
  user-select: text; cursor: text;
}
.hint { margin: 0; font-size: 12.5px; }
.actions { display: flex; gap: 8px; margin: 0 0 10px; }
button {
  font: inherit; font-size: 13px; color: var(--fg); cursor: pointer;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 12px;
}
button:hover { border-color: var(--muted); }
button.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
</style>
