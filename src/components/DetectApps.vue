<script setup lang="ts">
// Fenêtre « Détecter les applications » : Rust liste les raccourcis du menu
// Démarrer et les jeux Steam installés, on coche ce qu'on veut garder, puis
// Rust ajoute les tuiles à config.json. Rien n'est supprimé ni remplacé : ce
// qui a déjà une tuile est affiché grisé, sans case à cocher active.
//
// Ouverte depuis l'écran d'accueil (premier démarrage) ou le menu de l'icône
// de notification, à tout moment. Même principe que SteamImport.vue : un
// <dialog> natif modal, déplacé dans <body>.
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { appsCategory, detectMessage, filterApps, selection } from "../lib/detect";
import { ipc, toIpcError } from "../lib/ipc";
import { targetCategory, toChoices } from "../lib/steam";
import type { Config, DetectedApp, SteamGame, SteamImportResult } from "../lib/types";

const props = defineProps<{ config: Config | null }>();
// `partial` : une partie a été ajoutée avant une erreur ; le parent recharge
// la config (les jeux ajoutés passent alors en « déjà ajouté »).
const emit = defineEmits<{ close: []; imported: [message: string, added: boolean]; partial: [] }>();

const dialog = ref<HTMLDialogElement | null>(null);
const submitButton = ref<HTMLButtonElement | null>(null);
const filterInput = ref<HTMLInputElement | null>(null);

/** `null` pendant la recherche. */
const detectedApps = ref<DetectedApp[] | null>(null);
const detectedGames = ref<SteamGame[] | null>(null);
/** Steam absent ou illisible : la section des jeux l'explique, le reste marche. */
const steamError = ref<string | null>(null);
const appsError = ref<string | null>(null);
const error = ref<string | null>(null);
const busy = ref(false);
const loading = computed(() => detectedApps.value === null && !appsError.value);

const query = ref("");
const checkedApps = ref(new Set<string>());
const checkedGames = ref(new Set<number>());

const shownApps = computed(() => filterApps(detectedApps.value ?? [], query.value));
const freshApps = computed(() => (detectedApps.value ?? []).filter((a) => !a.inConfig));
const shownFreshApps = computed(() => shownApps.value.filter((a) => !a.inConfig));
// Jeux recalculés si la config est rechargée pendant que la fenêtre est ouverte.
const gameChoices = computed(() => detectedGames.value && toChoices(detectedGames.value, props.config));
const freshGames = computed(() => (gameChoices.value ?? []).filter((g) => !g.inConfig));

const nApps = computed(() => freshApps.value.filter((a) => checkedApps.value.has(a.path)).length);
const nGames = computed(() => freshGames.value.filter((g) => checkedGames.value.has(g.appId)).length);
const allAppsShownChecked = computed(
  () => shownFreshApps.value.length > 0 && shownFreshApps.value.every((a) => checkedApps.value.has(a.path)),
);
const allGamesChecked = computed(() => freshGames.value.length > 0 && freshGames.value.every((g) => checkedGames.value.has(g.appId)));
const nothingNew = computed(() => !loading.value && freshApps.value.length === 0 && freshGames.value.length === 0);


async function detect() {
  error.value = appsError.value = steamError.value = null;
  detectedApps.value = detectedGames.value = null;
  // Les deux recherches en parallèle ; l'échec de l'une n'empêche pas l'autre.
  const [a, g] = await Promise.allSettled([ipc.detectApps(), ipc.detectSteamGames()]);
  if (g.status === "fulfilled") {
    detectedGames.value = g.value;
    // Les jeux sont cochés d'office : ce sont tous des choses qu'on lance.
    checkedGames.value = new Set(freshGames.value.map((x) => x.appId));
  } else {
    steamError.value = toIpcError(g.reason).message;
  }
  if (a.status === "fulfilled") {
    // Les applications ne sont pas cochées : le menu Démarrer en contient
    // souvent beaucoup, on coche celles qu'on veut garder.
    detectedApps.value = a.value;
  } else {
    appsError.value = toIpcError(a.reason).message;
  }
  await nextTick();
  if (freshApps.value.length > 8) filterInput.value?.focus();
  else submitButton.value?.focus();
}

// config.json rechargé pendant que la fenêtre est ouverte (modifié à la main,
// ou jeux ajoutés avant une erreur) : Rust recalcule quelles applications ont
// déjà une tuile. Les cases cochées restent cochées.
watch(
  () => props.config,
  async () => {
    if (!detectedApps.value || busy.value) return;
    try {
      detectedApps.value = await ipc.detectApps();
    } catch (e) {
      console.error(e);
    }
  },
);

function close() {
  if (!busy.value) emit("close");
}

/** Copie du Set avec `key` ajouté ou retiré (un nouveau Set, pour que Vue voie le changement). */
function toggled<T>(set: Set<T>, key: T): Set<T> {
  const next = new Set(set);
  if (!next.delete(key)) next.add(key);
  return next;
}

function toggleAllApps() {
  const next = new Set(checkedApps.value);
  const check = !allAppsShownChecked.value;
  for (const a of shownFreshApps.value) {
    if (check) next.add(a.path);
    else next.delete(a.path);
  }
  checkedApps.value = next;
}

function toggleAllGames() {
  checkedGames.value = allGamesChecked.value ? new Set() : new Set(freshGames.value.map((g) => g.appId));
}

async function submit() {
  if (busy.value || nApps.value + nGames.value === 0) return;
  busy.value = true;
  error.value = null;
  const paths = freshApps.value.filter((a) => checkedApps.value.has(a.path)).map((a) => a.path);
  const appIds = freshGames.value.filter((g) => checkedGames.value.has(g.appId)).map((g) => g.appId);
  // L'un après l'autre (chaque import relit config.json avant d'écrire), les
  // jeux d'abord : la catégorie « Jeux » arrive ainsi avant « Applications ».
  let gamesResult: SteamImportResult | null = null;
  try {
    if (appIds.length > 0) gamesResult = await ipc.importSteamGames(appIds);
    const appsResult = paths.length > 0 ? await ipc.importApps(paths) : null;
    const added = (appsResult?.added.length ?? 0) + (gamesResult?.added.length ?? 0) > 0;
    emit("imported", detectMessage(appsResult, gamesResult), added);
  } catch (e) {
    const message = toIpcError(e).message;
    if (gamesResult) {
      // Les jeux sont déjà ajoutés : on le dit, avec l'erreur des applications,
      // et la liste des jeux est recalculée (ils passent en « déjà ajouté »).
      error.value = `${detectMessage(null, gamesResult)}, mais pas les applications : ${message}`;
      checkedGames.value = new Set();
      emit("partial");
    } else {
      error.value = message;
    }
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
  <dialog ref="dialog" class="detect" aria-labelledby="detect-title" @cancel.prevent="close">
    <h2 id="detect-title">Détecter les applications</h2>

    <p v-if="loading" class="info">Recherche des applications et des jeux installés…</p>
    <template v-else>
      <p class="info">
        Cochez ce que vous voulez garder : une tuile est ajoutée à <code>config.json</code> pour chacun.
        Rien n'est retiré, et ce qui a déjà une tuile est grisé.
      </p>

      <div class="lists">
        <section aria-labelledby="detect-apps">
          <div class="head">
            <h3 id="detect-apps">Applications <span class="where">→ « {{ appsCategory(config) }} »</span></h3>
            <button v-if="shownFreshApps.length > 1" type="button" class="link" :disabled="busy" @click="toggleAllApps">
              {{ allAppsShownChecked ? "Tout décocher" : "Tout cocher" }}
            </button>
          </div>
          <p v-if="appsError" class="note selectable">Lecture du menu Démarrer impossible : {{ appsError }}</p>
          <p v-else-if="detectedApps && detectedApps.length === 0" class="note">Aucun raccourci trouvé dans le menu Démarrer.</p>
          <template v-else-if="detectedApps">
            <input
              v-if="detectedApps.length > 8"
              ref="filterInput"
              v-model="query"
              class="filter"
              type="search"
              placeholder="Filtrer les applications…"
              aria-label="Filtrer les applications"
              @keydown.enter.prevent
            />
            <ul class="list">
              <li v-for="a in shownApps" :key="a.path">
                <label :class="{ done: a.inConfig }" :title="a.path">
                  <input
                    type="checkbox"
                    :checked="a.inConfig || checkedApps.has(a.path)"
                    :disabled="a.inConfig || busy"
                    @change="checkedApps = toggled(checkedApps, a.path)"
                  />
                  <span class="name">{{ a.name }}</span>
                  <span v-if="a.inConfig" class="tag">déjà ajoutée</span>
                  <span v-else-if="a.folder" class="tag">{{ a.folder }}</span>
                </label>
              </li>
              <li v-if="shownApps.length === 0" class="none">Aucune application pour « {{ query.trim() }} ».</li>
            </ul>
          </template>
        </section>

        <section aria-labelledby="detect-games">
          <div class="head">
            <h3 id="detect-games">Jeux Steam <span class="where">→ « {{ targetCategory(config) }} »</span></h3>
            <button v-if="freshGames.length > 1" type="button" class="link" :disabled="busy" @click="toggleAllGames">
              {{ allGamesChecked ? "Tout décocher" : "Tout cocher" }}
            </button>
          </div>
          <p v-if="steamError" class="note selectable">{{ steamError }}</p>
          <p v-else-if="gameChoices && gameChoices.length === 0" class="note">Aucun jeu installé trouvé dans les bibliothèques Steam.</p>
          <ul v-else-if="gameChoices" class="list">
            <li v-for="g in gameChoices" :key="g.appId">
              <label :class="{ done: g.inConfig }">
                <input
                  type="checkbox"
                  :checked="g.inConfig || checkedGames.has(g.appId)"
                  :disabled="g.inConfig || busy"
                  @change="checkedGames = toggled(checkedGames, g.appId)"
                />
                <span class="name">{{ g.name }}</span>
                <span v-if="g.inConfig" class="tag">déjà ajouté</span>
              </label>
            </li>
          </ul>
        </section>
      </div>
    </template>

    <p v-if="error" class="error selectable" role="alert">{{ error }}</p>

    <div class="actions">
      <span class="spacer" />
      <button type="button" :disabled="busy" @click="close">{{ nothingNew ? "Fermer" : "Annuler" }}</button>
      <button
        v-if="!nothingNew"
        ref="submitButton"
        type="button"
        class="primary"
        :disabled="loading || busy || nApps + nGames === 0"
        @click="submit"
      >
        {{ busy ? "Ajout…" : nApps + nGames === 0 ? "Ajouter" : `Ajouter ${selection(nApps, nGames)}` }}
      </button>
    </div>
  </dialog>
  </Teleport>
</template>

<style scoped>
.detect {
  width: min(620px, calc(100vw - 32px)); max-height: calc(100vh - 48px);
  padding: 18px 20px; display: flex; flex-direction: column;
  color: var(--fg); background: var(--surface);
  border: 1px solid var(--line); border-radius: 10px;
  font-size: 13.5px;
}
.detect::backdrop { background: rgb(0 0 0 / 55%); }
h2 { margin: 0 0 8px; font-size: 16px; font-weight: 600; }
h3 { margin: 0; font-size: 13.5px; font-weight: 600; }
.where { font-weight: 400; color: var(--faint); font-size: 12.5px; }
.info { margin: 0 0 10px; color: var(--muted); line-height: 1.5; }
.note { margin: 0 0 10px; color: var(--muted); font-size: 12.5px; user-select: text; cursor: text; }
code { font: 12px var(--font-mono); color: var(--fg); }
.lists { min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 6px; scrollbar-width: thin; scrollbar-color: var(--line) transparent; }
.head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; margin: 0 0 6px; }
.filter {
  width: 100%; box-sizing: border-box; margin: 0 0 6px; padding: 5px 9px;
  font: inherit; font-size: 13px; color: var(--fg);
  background: var(--win); border: 1px solid var(--line); border-radius: 7px; outline: none;
}
.filter:focus { border-color: var(--accent); }
.list {
  list-style: none; margin: 0 0 10px; padding: 4px; max-height: 240px; overflow-y: auto;
  background: var(--win); border: 1px solid var(--line); border-radius: 7px;
  scrollbar-width: thin; scrollbar-color: var(--line) transparent;
}
.none { padding: 5px 8px; color: var(--muted); }
label { display: flex; align-items: center; gap: 10px; padding: 5px 8px; border-radius: 5px; cursor: pointer; }
label:hover { background: var(--surface-2); }
label.done { cursor: default; color: var(--muted); }
label.done:hover { background: none; }
input[type="checkbox"] { margin: 0; accent-color: var(--accent); }
.name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--faint); }
.error {
  margin: 0 0 10px; white-space: pre-wrap; word-break: break-word;
  font: 12.5px/1.5 var(--font-mono); user-select: text; cursor: text;
}
.actions { display: flex; align-items: center; gap: 8px; padding-top: 4px; }
.spacer { flex: 1; }
button {
  font: inherit; font-size: 13px; color: var(--fg); cursor: pointer;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 12px;
}
button:hover:not(:disabled) { border-color: var(--muted); }
button:disabled { opacity: 0.5; cursor: default; }
button.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
button.link { background: none; border: none; padding: 0; color: var(--accent); font-size: 12.5px; }
</style>
