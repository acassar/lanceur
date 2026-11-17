<script setup lang="ts">
// Écran d'accueil affiché tant que config.json ne contient aucune tuile (premier
// démarrage). Deux façons de commencer :
// - « Détecter mes applications » : le lanceur cherche les applications du
//   menu Démarrer et les jeux Steam, on coche ce qu'on veut garder ;
// - « Partir de zéro » : ajouter ses tuiles une à une, ou éditer config.json
//   (où est le fichier, exemple à copier ; il est relu dès qu'il est enregistré).
import { ref } from "vue";

const emit = defineEmits<{ openFolder: []; copyFailed: []; detect: []; addTile: [] }>();

/** « Partir de zéro » choisi : on affiche les explications pour ajouter ses tuiles. */
const manual = ref(false);

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
    <h2 id="welcome-title">Bienvenue ! Par quoi commencer ?</h2>
    <div class="choices">
      <button type="button" class="choice primary" @click="$emit('detect')">
        <strong>Détecter mes applications</strong>
        <span>Le lanceur cherche les applications du menu Démarrer et vos jeux Steam. Vous cochez celles à garder.</span>
      </button>
      <button type="button" class="choice" :aria-expanded="manual" @click="manual = true">
        <strong>Partir de zéro</strong>
        <span>Ajoutez vos tuiles une à une, ou éditez <code>config.json</code> à la main.</span>
      </button>
    </div>
    <p v-if="!manual" class="hint">
      Vous pourrez détecter les applications plus tard : clic droit sur l'icône du lanceur, dans la zone de notification.
    </p>
    <template v-else>
    <div class="steam">
      <p>Un programme, un raccourci ou un site : ajoutez-le depuis la fenêtre.</p>
      <button type="button" class="primary" @click="$emit('addTile')">Ajouter une tuile</button>
    </div>
    <p>
      Vous pouvez aussi éditer à la main <code>config.json</code>, où le lanceur range ses tuiles
      (utile pour en ajouter beaucoup d'un coup) :
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
    </template>
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
.steam {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  margin: 0 0 12px; padding: 10px 12px;
  background: var(--win); border: 1px solid var(--line); border-radius: 7px;
}
.steam p { margin: 0; color: var(--fg); }
.steam button { flex: none; }
.actions { display: flex; gap: 8px; margin: 0 0 10px; }
.choices { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin: 4px 0 12px; }
.choice {
  display: flex; flex-direction: column; align-items: flex-start; gap: 4px;
  padding: 12px 14px; text-align: left; background: var(--win);
}
.choice strong { font-size: 14px; font-weight: 600; }
.choice span { color: var(--muted); font-size: 12.5px; line-height: 1.45; }
button {
  font: inherit; font-size: 13px; color: var(--fg); cursor: pointer;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 12px;
}
button:hover { border-color: var(--muted); }
button.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
</style>
