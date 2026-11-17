<script setup lang="ts">
// Fenêtre du lanceur (variante B de la maquette) : recherche en haut,
// rail de catégories à gauche, sections de tuiles à droite, barre d'état en bas.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, watchEffect } from "vue";
import CategoryRail from "./components/CategoryRail.vue";
import ConfigError from "./components/ConfigError.vue";
import DetectApps from "./components/DetectApps.vue";
import HotkeyWarning from "./components/HotkeyWarning.vue";
import LaunchToast from "./components/LaunchToast.vue";
import SearchBar from "./components/SearchBar.vue";
import StatusBar from "./components/StatusBar.vue";
import SteamImport from "./components/SteamImport.vue";
import TileEditor from "./components/TileEditor.vue";
import TileSection from "./components/TileSection.vue";
import WelcomeScreen from "./components/WelcomeScreen.vue";
import { useKeyboardNav, type Direction } from "./composables/useKeyboardNav";
import { useLauncher } from "./composables/useLauncher";
import { ipc, toIpcError } from "./lib/ipc";
import { applyTileOrder } from "./lib/reorder";
import { importMessage } from "./lib/steam";
import { kindLabel, targetText } from "./lib/tiles";
import type { SteamImportResult, Tile } from "./lib/types";

const L = useLauncher();
const scroller = ref<HTMLElement | null>(null);
const searchBar = ref<InstanceType<typeof SearchBar> | null>(null);
const { move, reveal } = useKeyboardNav(scroller, L.selected, () => L.flat.value.length);

const toast = ref<{ message: string; detail?: string; error?: boolean } | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | undefined;
function showToast(t: NonNullable<typeof toast.value>) {
  toast.value = t;
  clearTimeout(toastTimer);
  // Une erreur reste affichée plus longtemps, le temps de la lire.
  toastTimer = setTimeout(() => (toast.value = null), t.error ? 6000 : 2200);
}

const status = computed(() => {
  const tile = L.current.value;
  const settings = L.config.value?.settings;
  return tile && settings ? { kind: kindLabel(tile), target: targetText(tile, settings) } : { kind: "", target: "" };
});

// Couleur d'accent lue dans config.json.
watchEffect(() => {
  const accent = L.config.value?.settings.accent;
  if (accent) document.documentElement.style.setProperty("--accent", accent);
});

// Nouvelle recherche ou catégorie : retour en haut de la liste.
watch([L.query, L.rail], () => scroller.value?.scrollTo({ top: 0 }));
// Changer de catégorie ne doit ni voler le focus de la recherche, ni sélectionner
// son texte (sinon la lettre tapée juste après Tab effacerait la recherche).
watch(L.rail, () => searchBar.value?.focus(false));

// Le champ de recherche doit toujours garder le focus : c'est lui qui reçoit les
// touches. Un clic sur une zone vide (barre d'état, fond de la grille…) le lui
// retirerait et le clavier ne répondrait plus ; on annule donc ce mousedown.
function keepSearchFocus(e: MouseEvent) {
  const el = e.target as HTMLElement;
  if (el === searchBar.value?.input) return;
  // Texte qu'on peut vouloir sélectionner pour le copier (exemple de l'écran
  // d'accueil, message d'erreur) : on laisse faire la sélection.
  if (el.closest(".selectable")) return;
  // Clic sur la barre de défilement de la grille : on laisse faire le glisser.
  if (el === scroller.value && e.offsetX >= el.clientWidth) return;
  e.preventDefault();
}
// Retour sur la fenêtre (Alt+Tab, clic sur l'icône de la barre des tâches) :
// focus + sélection du texte, pour pouvoir taper aussitôt.
//
// Cet événement arrive aussi sans vrai départ : quand la liste d'un <select>
// se referme, ou quand Windows rend le focus à la page. On ne touche donc à
// rien si une fenêtre (éditeur, import…) est ouverte, ni si la recherche a
// déjà le clavier : sinon on fermait les listes, et sélectionner le texte à
// chaque fois faisait remplacer la recherche par la lettre suivante.
function onWindowFocus() {
  if (editor.value || detectOpen.value || steamImportOpen.value) return;
  const input = searchBar.value?.input;
  if (input && document.activeElement === input) return;
  searchBar.value?.focus();
}
onMounted(() => window.addEventListener("focus", onWindowFocus));
onBeforeUnmount(() => window.removeEventListener("focus", onWindowFocus));

// Fenêtre rappelée par le raccourci global ou en relançant l'exe : on repart
// d'une fenêtre « neuve » (recherche vide, tout afficher, première tuile).
let stopListening: (() => void)[] = [];
let unmounted = false;
/** Garde la fonction d'arrêt d'une écoute, ou l'arrête aussitôt si le composant a disparu entre-temps. */
function keep(stop: () => void) {
  if (unmounted) stop();
  else stopListening.push(stop);
}
onMounted(async () => {
  // config.json enregistré (à la main ou par un favori) : on recharge tout de
  // suite, même si la fenêtre est réduite. Un JSON invalide affiche l'erreur
  // et laisse la dernière config valide à l'écran.
  keep(await ipc.onConfigChanged(() => load()));
  // Menu de l'icône de notification : « Importer les jeux Steam… ».
  // Ignoré si l'éditeur de tuiles est ouvert (une seule fenêtre à la fois).
  keep(await ipc.onSteamImportRequested(() => (steamImportOpen.value = !editor.value && !detectOpen.value)));
  // « Détecter les applications… » : de même.
  keep(await ipc.onDetectAppsRequested(() => (detectOpen.value = !editor.value && !steamImportOpen.value)));
  const stop = await ipc.onWindowShown(() => {
    L.query.value = "";
    L.rail.value = "all";
    L.selected.value = 0;
    scroller.value?.scrollTo({ top: 0 });
    searchBar.value?.focus();
    // On relit config.json : une modification à la main (tuiles, raccourci,
    // hideOnBlur…) s'applique ainsi dès le prochain affichage.
    load();
  });
  // Le composant a pu disparaître pendant l'attente (rechargement à chaud).
  keep(stop);
});
onBeforeUnmount(() => {
  unmounted = true;
  stopListening.forEach((stop) => stop());
  stopListening = [];
});

async function launch(index?: number) {
  const tile = L.flat.value[index ?? L.selected.value];
  if (!tile || !L.config.value) return;
  try {
    await L.launch(index);
    showToast({ message: `Lancement de ${tile.name}`, detail: targetText(tile, L.config.value.settings) });
  } catch (e) {
    showToast({ message: `Impossible de lancer ${tile.name}`, detail: toIpcError(e).message, error: true });
  }
}

async function openConfigFolder() {
  try {
    await ipc.openConfigFolder();
  } catch (e) {
    showToast({ message: "Impossible d'ouvrir le dossier", detail: toIpcError(e).message, error: true });
  }
}

async function toggleFavorite(id: string) {
  try {
    await L.toggleFavorite(id);
  } catch (e) {
    showToast({ message: "Favori non enregistré", detail: toIpcError(e).message, error: true });
  }
}

// Fenêtre d'import Steam (écran d'accueil ou menu de l'icône de notification).
const steamImportOpen = ref(false);
function closeSteamImport() {
  steamImportOpen.value = false;
  // Le reste de la fenêtre redevient actif : le clavier retourne à la recherche.
  nextTick(() => searchBar.value?.focus());
}
function onSteamImported(result: SteamImportResult) {
  closeSteamImport();
  showToast({ message: importMessage(result) });
  // config.json vient d'être réécrit : la surveillance du fichier rechargerait
  // aussi la config, mais on n'attend pas son délai.
  if (result.added.length > 0) load();
}

// Fenêtre « Détecter les applications » (écran d'accueil ou menu de l'icône de notification).
const detectOpen = ref(false);
function closeDetect() {
  detectOpen.value = false;
  nextTick(() => searchBar.value?.focus());
}
function onDetectImported(message: string, added: boolean) {
  closeDetect();
  showToast({ message });
  if (added) load();
}

// Éditeur de tuiles : `tile` absent pour une nouvelle tuile. La clé force un
// nouveau formulaire à chaque ouverture.
const editor = ref<{ key: number; tile?: Tile; category?: string } | null>(null);
let editorKey = 0;
function addTile() {
  if (steamImportOpen.value || detectOpen.value) return;
  // Nouvelle tuile rangée par défaut dans la catégorie affichée.
  editor.value = { key: ++editorKey, category: L.rail.value };
}
function editTile(tile: Tile) {
  if (steamImportOpen.value || detectOpen.value) return;
  editor.value = { key: ++editorKey, tile };
}
function closeEditor() {
  editor.value = null;
  nextTick(() => searchBar.value?.focus());
}
async function onTileSaved(tile: Tile, added: boolean) {
  closeEditor();
  showToast({ message: added ? `« ${tile.name} » ajoutée` : `« ${tile.name} » enregistrée` });
  // Comme après l'import Steam : on n'attend pas la surveillance du fichier.
  await load();
  // Une nouvelle tuile est sélectionnée (et visible), pour la lancer aussitôt.
  if (added && !L.query.value.trim()) {
    const i = L.flat.value.findIndex((t) => t.id === tile.id);
    if (i >= 0) {
      L.selected.value = i;
      nextTick(reveal);
    }
  }
}
function onTileDeleted(tile: Tile) {
  closeEditor();
  showToast({ message: `« ${tile.name} » supprimée` });
  load();
}

// Glisser-déposer. Pas pendant une recherche : la liste affichée n'est alors
// qu'une partie des tuiles. Ni Récents (ordre des lancements).
const canReorder = computed(() => !!L.config.value && !L.query.value.trim());
function sectionReorderable(key: string) {
  return canReorder.value && (key === "favorites" || !!L.config.value?.categories.some((c) => c.id === key));
}
/** Les enregistrements d'ordre partent l'un après l'autre, dans l'ordre des glisser. */
let reorderQueue: Promise<void> = Promise.resolve();
/**
 * Applique tout de suite le nouvel ordre à l'écran, puis l'enregistre. En cas
 * d'échec (fichier modifié à la main entre-temps…), on recharge le fichier.
 * `keep` : la tuile déplacée, sélectionnée à sa nouvelle place.
 */
function reorder(save: () => Promise<void>, apply: () => void, keep?: { section: string; id: string }) {
  apply();
  if (keep) {
    const section = L.sections.value.find((s) => s.key === keep.section);
    const item = section?.items.find((i) => i.tile.id === keep.id);
    if (item) L.selected.value = item.index;
  }
  reorderQueue = reorderQueue.then(async () => {
    try {
      await save();
    } catch (e) {
      showToast({ message: "Nouvel ordre non enregistré", detail: toIpcError(e).message, error: true });
      await load();
    }
  });
}
function reorderSection(key: string, ids: string[], moved: string) {
  const cfg = L.config.value;
  if (!cfg) return;
  const keep = { section: key, id: moved };
  if (key === "favorites") reorder(() => ipc.reorderFavorites(ids), () => (cfg.favorites = ids), keep);
  else reorder(() => ipc.reorderTiles(key, ids), () => applyTileOrder(cfg, key, ids), keep);
}
function reorderCategories(ids: string[]) {
  const cfg = L.config.value;
  if (!cfg) return;
  const byId = new Map(cfg.categories.map((c) => [c.id, c]));
  if (ids.some((id) => !byId.has(id))) return load();
  reorder(
    () => ipc.reorderCategories(ids),
    () => (cfg.categories = ids.map((id) => byId.get(id)!)),
  );
}

const ARROWS: Record<string, Direction> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" };

function onKeydown(e: KeyboardEvent) {
  // Composition en cours (accents morts, IME) : la touche appartient à la saisie.
  if (e.isComposing) return;
  const input = e.target instanceof HTMLInputElement ? e.target : null;
  const dir = ARROWS[e.key];
  if (dir) {
    // Gauche/droite déplacent le curseur dans le texte, sauf s'il est déjà au bord.
    if (input && dir === "left" && input.selectionStart !== 0) return;
    if (input && dir === "right" && input.selectionEnd !== input.value.length) return;
    e.preventDefault();
    move(dir);
    nextTick(reveal);
  } else if (e.key === "Enter") {
    e.preventDefault();
    // Entrée maintenue : un seul lancement, pas une rafale.
    if (!e.repeat) launch();
  } else if (e.key === "Escape") {
    e.preventDefault();
    if (L.query.value) L.query.value = "";
    else ipc.hideWindow().catch(console.error);
  } else if (e.key === "Tab") {
    e.preventDefault();
    L.cycleRail(e.shiftKey ? -1 : 1);
  } else if (e.ctrlKey && e.key.toLowerCase() === "d") {
    e.preventDefault();
    if (!e.repeat && L.current.value) toggleFavorite(L.current.value.id);
  } else if (e.ctrlKey && e.key.toLowerCase() === "n") {
    e.preventDefault();
    if (!e.repeat && L.config.value) addTile();
  } else if ((e.ctrlKey && e.key.toLowerCase() === "e") || e.key === "F2") {
    e.preventDefault();
    if (!e.repeat && L.current.value) editTile(L.current.value);
  }
}

/**
 * Charge la config, puis relit l'état du raccourci global. S'il n'a pas pu
 * être enregistré, un bandeau reste affiché tant que le problème dure.
 */
const hotkeyError = ref<string | null>(null);
async function load() {
  await L.load();
  try {
    hotkeyError.value = (await ipc.getHotkeyStatus()).error;
  } catch (e) {
    console.error(e);
  }
}

onMounted(load);
</script>

<template>
  <div class="launcher" @keydown="onKeydown" @mousedown="keepSearchFocus">
    <SearchBar ref="searchBar" v-model="L.query.value" :count="L.railItems.value[0]?.count ?? 0" :total="L.config.value?.tiles.length ?? 0" />

    <div class="body">
      <CategoryRail
        v-model="L.rail.value"
        :items="L.railItems.value"
        :tile-count="L.config.value?.tiles.length ?? 0"
        :reorderable="!!L.config.value"
        @add-tile="L.config.value && addTile()"
        @reorder="reorderCategories"
      />

      <div ref="scroller" class="scroll">
        <ConfigError
          v-if="L.error.value"
          :error="L.error.value"
          :compact="!!L.config.value"
          @retry="load"
          @open-folder="openConfigFolder"
        />
        <WelcomeScreen
          v-if="L.config.value && L.config.value.tiles.length === 0"
          @open-folder="openConfigFolder"
          @copy-failed="showToast({ message: 'Impossible de copier l’exemple', error: true })"
          @detect="detectOpen = true"
          @add-tile="addTile"
        />
        <p v-else-if="L.config.value && L.sections.value.length === 0" class="empty">
          <template v-if="L.query.value.trim()">Aucune tuile pour <strong>« {{ L.query.value.trim() }} »</strong>.</template>
          <template v-else-if="L.rail.value === 'favorites'">Aucun favori pour l'instant. Sélectionnez une tuile et appuyez sur <kbd>Ctrl</kbd> <kbd>D</kbd>.</template>
          <template v-else>Cette catégorie est vide.</template>
          <br /><a href="#" @click.prevent="addTile">Ajouter une tuile</a> (<kbd>Ctrl</kbd> <kbd>N</kbd>)
          ou éditer <code>config.json</code> (<a href="#" @click.prevent="openConfigFolder">ouvrir le dossier</a>).
        </p>
        <TileSection
          v-for="section in L.sections.value"
          :key="section.key"
          :section="section"
          :selected="L.selected.value"
          :favorites="L.favoriteIds.value"
          :query="L.query.value"
          @launch="launch"
          @hover="(i) => (L.selected.value = i)"
          @toggle-favorite="toggleFavorite"
          :reorderable="sectionReorderable(section.key)"
          @edit="editTile"
          @reorder="(ids, moved) => reorderSection(section.key, ids, moved)"
        />
      </div>
    </div>

    <HotkeyWarning v-if="hotkeyError" :message="hotkeyError" @open-folder="openConfigFolder" />
    <StatusBar :kind="status.kind" :target="status.target" />
    <TileEditor
      v-if="editor && L.config.value"
      :key="editor.key"
      :tile="editor.tile"
      :categories="L.config.value.categories"
      :category="editor.category"
      @close="closeEditor"
      @saved="onTileSaved"
      @deleted="onTileDeleted"
    />
    <DetectApps v-if="detectOpen" :config="L.config.value" @close="closeDetect" @imported="onDetectImported" @partial="load" />
    <SteamImport v-if="steamImportOpen" :config="L.config.value" @close="closeSteamImport" @imported="onSteamImported" />
    <LaunchToast :message="toast?.message ?? null" :detail="toast?.detail" :error="toast?.error" />
  </div>
</template>

<style scoped>
.launcher { height: 100%; display: flex; flex-direction: column; }
.body { flex: 1; min-height: 0; display: flex; }
.scroll {
  flex: 1; min-width: 0; overflow-y: auto; padding: 0 10px 14px;
  scrollbar-width: thin; scrollbar-color: var(--line) transparent;
}
.empty { padding: 48px 16px; margin: 0; text-align: center; color: var(--muted); }
.empty strong { color: var(--fg); font-weight: 500; }
.empty code { font: 12px var(--font-mono); }
.empty a { color: var(--accent); }
</style>
