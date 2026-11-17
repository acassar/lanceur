<script setup lang="ts">
// Fenêtre d'ajout ou de modification d'une tuile. Elle envoie la tuile à Rust
// (commandes add_tile / update_tile / delete_tile), qui ne modifie que les
// lignes concernées de config.json : le fichier reste éditable à la main.
//
// Même principe que SteamImport.vue : <dialog> natif ouvert avec showModal(),
// déplacé dans <body> pour que ses touches n'atteignent pas les raccourcis de
// la fenêtre (Entrée lance une tuile, Tab change de catégorie…).
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { NEW_CATEGORY, emptyForm, formFromTile, nameFromPath, toDraft, type TileForm } from "../lib/editor";
import { ipc, toIpcError } from "../lib/ipc";
import type { Category, Tile, TileType } from "../lib/types";

const props = defineProps<{
  /** Tuile à modifier ; absente pour une nouvelle tuile. */
  tile?: Tile;
  categories: Category[];
  /** Catégorie proposée pour une nouvelle tuile (celle affichée dans le rail). */
  category?: string;
}>();
const emit = defineEmits<{
  close: [];
  saved: [tile: Tile, added: boolean];
  deleted: [tile: Tile];
}>();

const dialog = ref<HTMLDialogElement | null>(null);
const nameInput = ref<HTMLInputElement | null>(null);
const form = ref<TileForm>(props.tile ? formFromTile(props.tile) : emptyForm(props.categories, props.category));
const errors = ref<Partial<Record<keyof TileForm, string>>>({});
const error = ref<string | null>(null);
const busy = ref(false);
/** Deuxième étape de la suppression : « Supprimer définitivement ? ». */
const confirmDelete = ref(false);

const TYPES: { type: TileType; label: string }[] = [
  { type: "app", label: "Application" },
  { type: "web", label: "App web" },
  { type: "steam", label: "Jeu Steam" },
];
const title = computed(() => (props.tile ? `Modifier « ${props.tile.name} »` : "Ajouter une tuile"));

/** Le formulaire tel qu'il était lors du dernier « Ajouter » refusé. */
let checked: TileForm | null = null;
// Une erreur disparaît dès que le champ concerné est modifié.
watch(
  form,
  (now) => {
    for (const key of Object.keys(errors.value) as (keyof TileForm)[]) {
      if (now[key] !== checked?.[key]) delete errors.value[key];
    }
    confirmDelete.value = false;
  },
  { deep: true },
);

function close() {
  if (!busy.value) emit("close");
}

async function browse(images: boolean) {
  if (busy.value) return;
  // Pendant que la boîte « Ouvrir » est ouverte, le formulaire est figé.
  busy.value = true;
  try {
    const path = await ipc.pickFile(images);
    if (!path) return;
    if (images) {
      form.value.icon = path;
    } else {
      form.value.path = path;
      // Nom proposé d'après le fichier, si aucun n'a encore été tapé.
      if (!form.value.name.trim()) form.value.name = nameFromPath(path);
    }
  } catch (e) {
    error.value = toIpcError(e).message;
  } finally {
    busy.value = false;
  }
}

async function submit() {
  if (busy.value) return;
  const result = toDraft(form.value);
  if ("errors" in result) {
    checked = { ...form.value };
    errors.value = result.errors;
    // Focus sur le premier champ en erreur, pour corriger tout de suite.
    await nextTick();
    dialog.value?.querySelector<HTMLElement>("[aria-invalid='true']")?.focus();
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    const saved = props.tile ? await ipc.updateTile(props.tile.id, result.draft) : await ipc.addTile(result.draft);
    emit("saved", saved, !props.tile);
  } catch (e) {
    error.value = toIpcError(e).message;
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (!props.tile || busy.value) return;
  if (!confirmDelete.value) {
    confirmDelete.value = true;
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    await ipc.deleteTile(props.tile.id);
    emit("deleted", props.tile);
  } catch (e) {
    error.value = toIpcError(e).message;
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  dialog.value?.showModal();
  nameInput.value?.focus();
});
</script>

<template>
  <Teleport to="body">
  <dialog ref="dialog" class="tile-editor" aria-labelledby="tile-editor-title" @cancel.prevent="close">
    <form novalidate @submit.prevent="submit">
      <h2 id="tile-editor-title">{{ title }}</h2>

      <div class="types" role="radiogroup" aria-label="Type de tuile">
        <label v-for="t in TYPES" :key="t.type" :class="{ on: form.type === t.type }">
          <input v-model="form.type" type="radio" name="type" :value="t.type" :disabled="busy" />
          {{ t.label }}
        </label>
      </div>

      <label class="field">
        <span>Nom</span>
        <input ref="nameInput" v-model="form.name" type="text" :aria-invalid="!!errors.name" :disabled="busy" />
        <small v-if="errors.name" class="err">{{ errors.name }}</small>
      </label>

      <template v-if="form.type === 'app'">
        <label class="field">
          <span>Programme ou raccourci</span>
          <span class="row">
            <input
              v-model="form.path"
              type="text"
              class="mono"
              spellcheck="false"
              placeholder="C:\Program Files\…\app.exe, un .lnk, %LOCALAPPDATA%\…"
              :aria-invalid="!!errors.path"
              :disabled="busy"
            />
            <button type="button" :disabled="busy" @click="browse(false)">Parcourir…</button>
          </span>
          <small v-if="errors.path" class="err">{{ errors.path }}</small>
        </label>
        <label class="field">
          <span>Arguments <em>(facultatif)</em></span>
          <input v-model="form.args" type="text" class="mono" spellcheck="false" placeholder='--profile "Mon profil"' :disabled="busy" />
        </label>
      </template>

      <label v-else-if="form.type === 'web'" class="field">
        <span>Adresse</span>
        <input
          v-model="form.url"
          type="text"
          class="mono"
          spellcheck="false"
          placeholder="https://www.youtube.com"
          :aria-invalid="!!errors.url"
          :disabled="busy"
        />
        <small v-if="errors.url" class="err">{{ errors.url }}</small>
      </label>

      <label v-else class="field">
        <span>appId Steam</span>
        <input
          v-model="form.appId"
          type="text"
          class="mono"
          spellcheck="false"
          placeholder="620, ou l'adresse de la page du jeu sur le magasin"
          :aria-invalid="!!errors.appId"
          :disabled="busy"
        />
        <small v-if="errors.appId" class="err">{{ errors.appId }}</small>
      </label>

      <div class="field">
        <label for="tile-editor-category">Catégorie</label>
        <span class="row">
          <select id="tile-editor-category" v-model="form.category" :disabled="busy">
            <option v-for="c in categories" :key="c.id" :value="c.id">{{ c.name }}</option>
            <option :value="NEW_CATEGORY">Nouvelle catégorie…</option>
          </select>
          <input
            v-if="form.category === NEW_CATEGORY"
            v-model="form.newCategory"
            type="text"
            placeholder="Nom de la catégorie"
            aria-label="Nom de la nouvelle catégorie"
            :aria-invalid="!!errors.newCategory"
            :disabled="busy"
          />
        </span>
        <small v-if="errors.newCategory" class="err">{{ errors.newCategory }}</small>
      </div>

      <label class="field">
        <span>Icône <em>(facultatif : sinon trouvée automatiquement)</em></span>
        <span class="row">
          <input v-model="form.icon" type="text" class="mono" spellcheck="false" placeholder="C:\…\image.png" :disabled="busy" />
          <button type="button" :disabled="busy" @click="browse(true)">Parcourir…</button>
        </span>
      </label>

      <p v-if="error" class="error selectable" role="alert">{{ error }}</p>

      <div class="actions">
        <button v-if="tile" type="button" class="danger" :disabled="busy" @click="remove">
          {{ confirmDelete ? "Confirmer la suppression" : "Supprimer" }}
        </button>
        <span class="spacer" />
        <button type="button" :disabled="busy" @click="close">Annuler</button>
        <button type="submit" class="primary" :disabled="busy">
          {{ busy ? "Enregistrement…" : tile ? "Enregistrer" : "Ajouter" }}
        </button>
      </div>
    </form>
  </dialog>
  </Teleport>
</template>

<style scoped>
.tile-editor {
  width: min(540px, calc(100vw - 32px)); max-height: calc(100vh - 32px);
  padding: 14px 18px; overflow-y: auto;
  color: var(--fg); background: var(--surface);
  border: 1px solid var(--line); border-radius: 10px;
  font-size: 13.5px;
  scrollbar-width: thin; scrollbar-color: var(--line) transparent;
}
.tile-editor::backdrop { background: rgb(0 0 0 / 55%); }
form { display: flex; flex-direction: column; gap: 8px; }
h2 { margin: 0; font-size: 16px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.types { display: flex; gap: 4px; padding: 3px; background: var(--win); border: 1px solid var(--line); border-radius: 8px; }
.types label {
  flex: 1; text-align: center; padding: 5px 8px; border-radius: 6px; cursor: pointer; color: var(--muted);
}
.types label:hover { color: var(--fg); }
.types label.on { background: var(--surface-2); color: var(--fg); box-shadow: inset 0 -2px 0 var(--accent); }
.types label:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: -2px; }
.types input { position: absolute; opacity: 0; pointer-events: none; }

.field { display: flex; flex-direction: column; gap: 4px; }
.field > span:first-child, .field > label { font-size: 12.5px; color: var(--muted); }
em { font-style: normal; color: var(--faint); }
.row { display: flex; gap: 6px; }
.row > input, .row > select { flex: 1; min-width: 0; }
input[type="text"], select {
  font: inherit; font-size: 13px; color: var(--fg);
  background: var(--win); border: 1px solid var(--line); border-radius: 7px; padding: 6px 9px;
  user-select: text;
}
input.mono { font: 12.5px var(--font-mono); }
input::placeholder { color: var(--faint); }
input:focus, select:focus { outline: none; border-color: color-mix(in srgb, var(--accent) 70%, var(--line)); }
input[aria-invalid="true"] { border-color: #e06c6c; }
.err { color: #e78f8f; font-size: 12px; }
.error {
  margin: 0; white-space: pre-wrap; word-break: break-word; color: #e78f8f;
  font: 12.5px/1.5 var(--font-mono); user-select: text; cursor: text;
}

.actions { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.spacer { flex: 1; }
button {
  font: inherit; font-size: 13px; color: var(--fg); cursor: pointer; flex: none;
  background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 12px;
}
button:hover:not(:disabled) { border-color: var(--muted); }
button:disabled { opacity: 0.5; cursor: default; }
button.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
button.danger { color: #e78f8f; }
button.danger:hover:not(:disabled) { border-color: #e06c6c; }
</style>
