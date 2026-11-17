<script setup lang="ts">
// Fenêtre d'import de la bibliothèque Steam : liste les jeux installés (lus
// par Rust dans les fichiers de Steam), on coche ceux à ajouter, puis Rust
// les ajoute à config.json. Les jeux qui ont déjà une tuile sont grisés.
//
// C'est un <dialog> natif ouvert avec showModal() : le reste de la fenêtre
// devient inactif (ni clic ni clavier), Échap ferme, et le focus reste dedans.
// Il est déplacé dans <body> (Teleport) : ses touches ne remontent ainsi pas
// jusqu'aux raccourcis de la fenêtre (Entrée lance une tuile, Tab change de
// catégorie…), et ses clics ne sont pas renvoyés vers la recherche.
import { computed, nextTick, onMounted, ref } from "vue";
import { ipc, toIpcError } from "../lib/ipc";
import { games, targetCategory, toChoices, type SteamChoice } from "../lib/steam";
import type { Config, SteamGame, SteamImportResult } from "../lib/types";

const props = defineProps<{ config: Config | null }>();
const emit = defineEmits<{ close: []; imported: [result: SteamImportResult] }>();

const dialog = ref<HTMLDialogElement | null>(null);
const submitButton = ref<HTMLButtonElement | null>(null);
/** Jeux trouvés par Rust, `null` pendant la recherche. */
const detected = ref<SteamGame[] | null>(null);
// Recalculé si la config est rechargée pendant que la fenêtre est ouverte.
const choices = computed<SteamChoice[] | null>(() => detected.value && toChoices(detected.value, props.config));
const checked = ref(new Set<number>());
const error = ref<string | null>(null);
const busy = ref(false);

const fresh = computed(() => (choices.value ?? []).filter((c) => !c.inConfig));
/** Nombre de jeux cochés parmi ceux qui n'ont pas encore de tuile. */
const count = computed(() => fresh.value.filter((c) => checked.value.has(c.appId)).length);
const allChecked = computed(() => fresh.value.length > 0 && fresh.value.every((c) => checked.value.has(c.appId)));

async function detect() {
  error.value = null;
  detected.value = null;
  try {
    detected.value = await ipc.detectSteamGames();
    // Par défaut, tous les nouveaux jeux sont cochés.
    checked.value = new Set(fresh.value.map((c) => c.appId));
    // Focus sur « Ajouter » : Entrée suffit pour tout importer.
    await nextTick();
    submitButton.value?.focus();
  } catch (e) {
    error.value = toIpcError(e).message;
  }
}

// Pendant l'écriture, on ne ferme pas : l'import continuerait côté Rust, mais
// la fenêtre fermée ne pourrait plus annoncer son résultat.
function close() {
  if (!busy.value) emit("close");
}

function toggle(appId: number) {
  const next = new Set(checked.value);
  if (!next.delete(appId)) next.add(appId);
  checked.value = next;
}

function toggleAll() {
  checked.value = allChecked.value ? new Set() : new Set(fresh.value.map((c) => c.appId));
}

async function submit() {
  if (busy.value || count.value === 0) return;
  busy.value = true;
  error.value = null;
  try {
    const appIds = fresh.value.filter((c) => checked.value.has(c.appId)).map((c) => c.appId);
    emit("imported", await ipc.importSteamGames(appIds));
  } catch (e) {
    error.value = toIpcError(e).message;
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  dialog.value?.showModal();
  detect();
});
</script>

<template>
  <Teleport to="body">
  <!-- `cancel` : Échap. On ferme par le parent (v-if) plutôt que par le navigateur. -->
  <dialog ref="dialog" class="steam-import" aria-labelledby="steam-import-title" @cancel.prevent="close">
    <h2 id="steam-import-title">Importer les jeux Steam</h2>

    <p v-if="!choices && !error" class="info">Recherche des jeux installés…</p>
    <p v-else-if="choices && choices.length === 0" class="info">Aucun jeu installé trouvé dans les bibliothèques Steam.</p>
    <template v-else-if="choices">
      <p class="info">
        {{ games(choices.length) }} installé{{ choices.length > 1 ? "s" : "" }}.
        Les jeux cochés sont ajoutés à <code>config.json</code>, dans la catégorie « {{ targetCategory(config) }} ».
      </p>
      <ul class="list">
        <li v-for="c in choices" :key="c.appId">
          <label :class="{ done: c.inConfig }">
            <input
              type="checkbox"
              :checked="c.inConfig || checked.has(c.appId)"
              :disabled="c.inConfig || busy"
              @change="toggle(c.appId)"
            />
            <span class="name">{{ c.name }}</span>
            <span v-if="c.inConfig" class="tag">déjà ajouté</span>
          </label>
        </li>
      </ul>
    </template>

    <p v-if="error" class="error selectable" role="alert">{{ error }}</p>

    <div class="actions">
      <button v-if="fresh.length > 1" type="button" class="link" :disabled="busy" @click="toggleAll">
        {{ allChecked ? "Tout décocher" : "Tout cocher" }}
      </button>
      <span class="spacer" />
      <button type="button" :disabled="busy" @click="close">{{ choices && fresh.length === 0 ? "Fermer" : "Annuler" }}</button>
      <button v-if="error && !choices" type="button" class="primary" @click="detect">Réessayer</button>
      <button v-else-if="fresh.length > 0" ref="submitButton" type="button" class="primary" :disabled="busy || count === 0" @click="submit">
        {{ busy ? "Ajout…" : `Ajouter ${games(count)}` }}
      </button>
    </div>
  </dialog>
  </Teleport>
</template>

<style scoped>
.steam-import {
  width: min(520px, calc(100vw - 32px)); max-height: calc(100vh - 48px);
  padding: 18px 20px; display: flex; flex-direction: column;
  color: var(--fg); background: var(--surface);
  border: 1px solid var(--line); border-radius: 10px;
  font-size: 13.5px;
}
.steam-import::backdrop { background: rgb(0 0 0 / 55%); }
h2 { margin: 0 0 8px; font-size: 16px; font-weight: 600; }
.info { margin: 0 0 10px; color: var(--muted); line-height: 1.5; }
code { font: 12px var(--font-mono); color: var(--fg); }
.list {
  list-style: none; margin: 0 0 10px; padding: 4px; min-height: 0; overflow-y: auto;
  background: var(--win); border: 1px solid var(--line); border-radius: 7px;
  scrollbar-width: thin; scrollbar-color: var(--line) transparent;
}
label { display: flex; align-items: center; gap: 10px; padding: 5px 8px; border-radius: 5px; cursor: pointer; }
label:hover { background: var(--surface-2); }
label.done { cursor: default; color: var(--muted); }
label.done:hover { background: none; }
input { margin: 0; accent-color: var(--accent); }
.name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { flex: none; font-size: 12px; color: var(--faint); }
.error {
  margin: 0 0 10px; white-space: pre-wrap; word-break: break-word;
  font: 12.5px/1.5 var(--font-mono); user-select: text; cursor: text;
}
.actions { display: flex; align-items: center; gap: 8px; }
.spacer { flex: 1; }
button {
  font: inherit; font-size: 13px; color: var(--fg); cursor: pointer;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 12px;
}
button:hover:not(:disabled) { border-color: var(--muted); }
button:disabled { opacity: 0.5; cursor: default; }
button.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
button.link { background: none; border: none; padding: 5px 0; color: var(--accent); }
</style>
