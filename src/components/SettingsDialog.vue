<script setup lang="ts">
// Écran de paramètres : la section "settings" de config.json, modifiable sans
// ouvrir le fichier. Rust n'écrit que les réglages modifiés (settings.rs) :
// le reste du fichier, mise en forme comprise, ne bouge pas.
//
// Même principe que TileEditor.vue : <dialog> natif ouvert avec showModal(),
// déplacé dans <body> pour que ses touches n'atteignent pas les raccourcis de
// la fenêtre (Entrée lance une tuile, Tab change de catégorie…).
import { computed, nextTick, onMounted, ref } from "vue";
import { LONE_WIN, hotkeyFromEvent, hotkeyLabel, isLoneWin, modifiers } from "../lib/hotkey";
import { ipc, toIpcError } from "../lib/ipc";
import type { Settings } from "../lib/types";

const props = defineProps<{ settings: Settings }>();
const emit = defineEmits<{ close: []; saved: [hotkeyError: string | null]; openFolder: [] }>();

const DEFAULT_HOTKEY = "Super+Alt+Space";
const BROWSERS = [
  { value: "msedge", label: "Microsoft Edge" },
  { value: "chrome", label: "Google Chrome" },
  { value: "brave", label: "Brave" },
];
const OTHER_BROWSER = "__autre__";
const ACCENTS = ["#f0a35e", "#e06c75", "#c678dd", "#5e9ef0", "#56b6c2", "#98c379", "#e5c07b"];

const dialog = ref<HTMLDialogElement | null>(null);
const form = ref<Settings>({ ...props.settings });
const error = ref<string | null>(null);
const busy = ref(false);

// --- Raccourci ---------------------------------------------------------------

/** Dernière combinaison choisie, reproposée en revenant de « Touche Windows seule ». */
let lastCombo = isLoneWin(props.settings.hotkey) ? DEFAULT_HOTKEY : props.settings.hotkey;
const loneWin = computed({
  get: () => isLoneWin(form.value.hotkey),
  set: (on) => {
    if (on && !isLoneWin(form.value.hotkey)) lastCombo = form.value.hotkey;
    form.value.hotkey = on ? LONE_WIN : lastCombo;
    stopCapture();
  },
});
/** Saisie du raccourci en cours : ce qui est déjà enfoncé (`""` au début). */
const capturing = ref<string | null>(null);
const captureError = ref<string | null>(null);
const captureButton = ref<HTMLButtonElement | null>(null);
const shown = computed(() => hotkeyLabel(capturing.value ?? form.value.hotkey));

function startCapture() {
  capturing.value = "";
  captureError.value = null;
  captureButton.value?.focus();
}
/**
 * Clic sur le champ : commence la saisie. Un « clic » venu du clavier
 * (`detail` 0 : Espace ou Entrée relâchées juste après avoir saisi un
 * raccourci qui les contient) ne la relance pas.
 */
function onCaptureClick(e: MouseEvent) {
  if (capturing.value === null && e.detail > 0) startCapture();
}
function stopCapture() {
  capturing.value = null;
}
function onCaptureKey(e: KeyboardEvent) {
  if (capturing.value === null) return;
  // Toutes les touches vont au raccourci, aucune à la fenêtre.
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
    stopCapture();
    return;
  }
  const result = hotkeyFromEvent(e);
  if ("pending" in result) {
    capturing.value = result.pending;
  } else if ("error" in result) {
    captureError.value = result.error;
  } else {
    form.value.hotkey = result.hotkey;
    captureError.value = null;
    stopCapture();
  }
}
function onCaptureKeyUp(e: KeyboardEvent) {
  if (capturing.value === null) return;
  e.preventDefault();
  // Modificateur relâché sans touche principale : on repart de ce qui reste enfoncé.
  capturing.value = modifiers(e).join("+");
}

// --- Navigateur --------------------------------------------------------------

const browserChoice = ref(BROWSERS.some((b) => b.value === props.settings.browser) ? props.settings.browser : OTHER_BROWSER);
const customBrowser = ref(browserChoice.value === OTHER_BROWSER ? props.settings.browser : "");
const browserInput = ref<HTMLInputElement | null>(null);
async function onBrowserChoice() {
  if (browserChoice.value === OTHER_BROWSER) {
    await nextTick();
    browserInput.value?.focus();
  }
}

// --- Enregistrement ----------------------------------------------------------

/** Les réglages modifiés par rapport à ceux reçus (eux seuls sont envoyés). */
function changes(): Partial<Settings> {
  const wanted: Settings = {
    ...form.value,
    browser: browserChoice.value === OTHER_BROWSER ? customBrowser.value.trim() : browserChoice.value,
    accent: form.value.accent.trim(),
    recentsCount: Math.max(0, Math.min(50, Math.round(Number(form.value.recentsCount) || 0))),
  };
  const diff: Partial<Settings> = {};
  for (const key of Object.keys(wanted) as (keyof Settings)[]) {
    if (wanted[key] !== props.settings[key]) (diff as Record<string, unknown>)[key] = wanted[key];
  }
  return diff;
}

async function submit() {
  if (busy.value) return;
  if (capturing.value !== null) {
    captureError.value = "Appuyez sur la combinaison voulue, ou Échap pour annuler la saisie.";
    return;
  }
  if (browserChoice.value === OTHER_BROWSER && !customBrowser.value.trim()) {
    error.value = "Indiquez le chemin du navigateur (basé sur Chromium : Edge, Chrome, Brave, Vivaldi…).";
    browserInput.value?.focus();
    return;
  }
  const diff = changes();
  if (Object.keys(diff).length === 0) {
    emit("close");
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    const status = await ipc.updateSettings(diff);
    emit("saved", status.error);
  } catch (e) {
    error.value = toIpcError(e).message;
  } finally {
    busy.value = false;
  }
}

function close() {
  if (busy.value) return;
  // Échap pendant la saisie du raccourci : n'annule que la saisie.
  if (capturing.value !== null) return stopCapture();
  emit("close");
}

onMounted(() => dialog.value?.showModal());
</script>

<template>
  <Teleport to="body">
  <dialog ref="dialog" class="settings" aria-labelledby="settings-title" @cancel.prevent="close">
    <form novalidate @submit.prevent="submit">
      <h2 id="settings-title">Paramètres</h2>

      <fieldset>
        <legend>Raccourci pour afficher le lanceur</legend>
        <div class="choices" role="radiogroup" aria-label="Type de raccourci">
          <label :class="{ on: !loneWin }">
            <input v-model="loneWin" type="radio" :value="false" :disabled="busy" /> Combinaison de touches
          </label>
          <label :class="{ on: loneWin }">
            <input v-model="loneWin" type="radio" :value="true" :disabled="busy" /> Touche Windows seule
          </label>
        </div>

        <div v-if="!loneWin" class="row capture-row">
          <button
            ref="captureButton"
            type="button"
            class="capture"
            :class="{ active: capturing !== null }"
            :disabled="busy"
            :aria-label="capturing !== null ? 'Appuyez sur la combinaison voulue' : 'Raccourci actuel, cliquer pour le changer'"
            @click="onCaptureClick"
            @keydown="onCaptureKey"
            @keyup="onCaptureKeyUp"
            @blur="stopCapture"
          >
            <template v-if="shown.length">
              <kbd v-for="(k, i) in shown" :key="i">{{ k }}</kbd>
            </template>
            <span v-if="capturing !== null" class="hint">{{ shown.length ? "…" : "Appuyez sur la combinaison" }}</span>
          </button>
          <button v-if="capturing === null" type="button" :disabled="busy" @click="startCapture">Modifier</button>
          <button v-else type="button" :disabled="busy" @mousedown.prevent @click="stopCapture">Annuler</button>
          <button
            v-if="capturing === null && form.hotkey !== DEFAULT_HOTKEY"
            type="button"
            class="link"
            :disabled="busy"
            @click="form.hotkey = DEFAULT_HOTKEY"
          >Par défaut</button>
        </div>
        <small v-if="!loneWin && captureError" class="err">{{ captureError }}</small>
        <small v-if="!loneWin" class="help">
          Au moins Ctrl, Alt, Maj ou Win, plus une touche. Certaines combinaisons avec Win sont gardées par Windows
          et n'arrivent pas jusqu'ici.
        </small>
        <small v-else class="help">
          Appuyer puis relâcher <kbd>Win</kbd> seule ouvre le lanceur <strong>à la place du menu Démarrer</strong>.
          Les raccourcis Windows (<kbd>Win</kbd>+<kbd>E</kbd>, <kbd>Win</kbd>+<kbd>D</kbd>…) marchent comme avant, et le
          menu Démarrer reste à portée avec <kbd>Ctrl</kbd>+<kbd>Échap</kbd> ou le bouton Démarrer. Quand une fenêtre
          administrateur est au premier plan, Windows garde la touche pour lui.
        </small>
      </fieldset>

      <fieldset>
        <legend>Fenêtre</legend>
        <label class="check">
          <input v-model="form.hideAfterLaunch" type="checkbox" :disabled="busy" />
          <span>Réduire la fenêtre après un lancement</span>
        </label>
        <label class="check">
          <input v-model="form.hideOnBlur" type="checkbox" :disabled="busy" />
          <span>Réduire la fenêtre quand on clique ailleurs</span>
        </label>
        <label class="check">
          <input v-model="form.launchAtStartup" type="checkbox" :disabled="busy" />
          <span>Lancer au démarrage de Windows <em>(fenêtre cachée, icône dans la zone de notification)</em></span>
        </label>
        <label class="inline">
          Tuiles dans « Récents »
          <input v-model.number="form.recentsCount" type="number" min="0" max="50" class="num" :disabled="busy" />
          <em>(0 pour masquer la section)</em>
        </label>
      </fieldset>

      <fieldset>
        <legend>Apps web</legend>
        <div class="row">
          <select v-model="browserChoice" aria-label="Navigateur des apps web" :disabled="busy" @change="onBrowserChoice">
            <option v-for="b in BROWSERS" :key="b.value" :value="b.value">{{ b.label }}</option>
            <option :value="OTHER_BROWSER">Autre navigateur…</option>
          </select>
          <input
            v-if="browserChoice === OTHER_BROWSER"
            ref="browserInput"
            v-model="customBrowser"
            type="text"
            class="mono"
            spellcheck="false"
            placeholder="C:\…\vivaldi.exe"
            aria-label="Chemin du navigateur"
            :disabled="busy"
          />
        </div>
        <small class="help">Ouvertes dans une fenêtre d'application, sans barre d'adresse (navigateur basé sur Chromium).</small>
      </fieldset>

      <fieldset>
        <legend>Couleur d'accent</legend>
        <div class="row swatches">
          <button
            v-for="c in ACCENTS"
            :key="c"
            type="button"
            class="swatch"
            :class="{ on: form.accent.toLowerCase() === c }"
            :style="{ background: c }"
            :aria-label="`Couleur ${c}`"
            :aria-pressed="form.accent.toLowerCase() === c"
            :disabled="busy"
            @click="form.accent = c"
          />
          <input v-model="form.accent" type="text" class="mono accent" spellcheck="false" aria-label="Couleur CSS" :disabled="busy" />
        </div>
      </fieldset>

      <p v-if="error" class="error selectable" role="alert">{{ error }}</p>

      <div class="actions">
        <button type="button" class="link" :disabled="busy" @click="emit('openFolder')">Ouvrir le dossier de config.json</button>
        <span class="spacer" />
        <button type="button" :disabled="busy" @click="close">Annuler</button>
        <button type="submit" class="primary" :disabled="busy">{{ busy ? "Enregistrement…" : "Enregistrer" }}</button>
      </div>
    </form>
  </dialog>
  </Teleport>
</template>

<style scoped>
.settings {
  width: min(560px, calc(100vw - 32px)); max-height: calc(100vh - 32px);
  padding: 14px 18px; overflow-y: auto;
  color: var(--fg); background: var(--surface);
  border: 1px solid var(--line); border-radius: 10px;
  font-size: 13.5px;
  scrollbar-width: thin; scrollbar-color: var(--line) transparent;
}
.settings::backdrop { background: rgb(0 0 0 / 55%); }
form { display: flex; flex-direction: column; gap: 10px; }
h2 { margin: 0; font-size: 16px; font-weight: 600; }
fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
legend { padding: 0; margin-bottom: 6px; font-size: 12.5px; color: var(--muted); }
em { font-style: normal; color: var(--faint); }
.row { display: flex; gap: 6px; align-items: center; }
.row > input[type="text"], .row > select { flex: 1; min-width: 0; }

.choices { display: flex; gap: 4px; padding: 3px; background: var(--win); border: 1px solid var(--line); border-radius: 8px; }
.choices label { flex: 1; text-align: center; padding: 5px 8px; border-radius: 6px; cursor: pointer; color: var(--muted); }
.choices label:hover { color: var(--fg); }
.choices label.on { background: var(--surface-2); color: var(--fg); box-shadow: inset 0 -2px 0 var(--accent); }
.choices label:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: -2px; }
.choices input { position: absolute; opacity: 0; pointer-events: none; }

.capture {
  flex: 1; min-width: 0; min-height: 34px; display: flex; gap: 5px; align-items: center; justify-content: flex-start;
  background: var(--win); text-align: left;
}
.capture.active { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent); }
.hint { color: var(--faint); }
kbd {
  font: 12px var(--font-mono); padding: 1px 6px; border-radius: 5px;
  background: var(--surface-2); border: 1px solid var(--line); border-bottom-width: 2px; color: var(--fg);
}
.help { color: var(--faint); font-size: 12px; line-height: 1.5; }
.help strong { color: var(--muted); font-weight: 500; }
.help kbd { font-size: 11px; padding: 0 4px; }
.err { color: #e78f8f; font-size: 12px; }

.check, .inline { display: flex; gap: 8px; align-items: center; cursor: pointer; }
input[type="checkbox"] { margin: 0; accent-color: var(--accent); }
input[type="text"], input[type="number"], select {
  font: inherit; font-size: 13px; color: var(--fg);
  background: var(--win); border: 1px solid var(--line); border-radius: 7px; padding: 6px 9px;
  user-select: text;
}
input.num { width: 64px; padding: 4px 6px; }
input.mono { font: 12.5px var(--font-mono); }
input.accent { flex: 0 0 96px; }
input::placeholder { color: var(--faint); }
input:focus, select:focus { outline: none; border-color: color-mix(in srgb, var(--accent) 70%, var(--line)); }

.swatches { flex-wrap: wrap; }
.swatch { width: 24px; height: 24px; padding: 0; border-radius: 50%; border: 2px solid transparent; }
.swatch.on { border-color: var(--fg); }
.swatch:hover:not(:disabled) { border-color: var(--muted); }

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
button.link { background: none; border: none; padding: 5px 0; color: var(--accent); font-size: 12.5px; }
</style>
