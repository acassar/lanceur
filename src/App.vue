<script setup lang="ts">
// Fenêtre du lanceur (variante B de la maquette) : recherche en haut,
// rail de catégories à gauche, sections de tuiles à droite, barre d'état en bas.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, watchEffect } from "vue";
import CategoryRail from "./components/CategoryRail.vue";
import ConfigError from "./components/ConfigError.vue";
import LaunchToast from "./components/LaunchToast.vue";
import SearchBar from "./components/SearchBar.vue";
import StatusBar from "./components/StatusBar.vue";
import TileSection from "./components/TileSection.vue";
import { useKeyboardNav, type Direction } from "./composables/useKeyboardNav";
import { useLauncher } from "./composables/useLauncher";
import { ipc, toIpcError } from "./lib/ipc";
import { kindLabel, targetText } from "./lib/tiles";

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
  // Clic sur la barre de défilement de la grille : on laisse faire le glisser.
  if (el === scroller.value && e.offsetX >= el.clientWidth) return;
  e.preventDefault();
}
// Retour sur la fenêtre (Alt+Tab, clic sur l'icône de la barre des tâches) :
// focus + sélection du texte, pour pouvoir taper aussitôt.
const onWindowFocus = () => searchBar.value?.focus();
onMounted(() => window.addEventListener("focus", onWindowFocus));
onBeforeUnmount(() => window.removeEventListener("focus", onWindowFocus));

// Fenêtre rappelée par le raccourci global ou en relançant l'exe : on repart
// d'une fenêtre « neuve » (recherche vide, tout afficher, première tuile).
let stopShown: (() => void) | undefined;
let unmounted = false;
onMounted(async () => {
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
  if (unmounted) stop();
  else stopShown = stop;
});
onBeforeUnmount(() => {
  unmounted = true;
  stopShown?.();
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
  }
}

/**
 * Charge la config, puis signale un raccourci global qui n'a pas pu être
 * enregistré. Le même message n'est montré qu'une fois, pas à chaque affichage.
 */
let lastHotkeyError: string | null = null;
async function load() {
  await L.load();
  try {
    const { error } = await ipc.getHotkeyStatus();
    if (error && error !== lastHotkeyError) {
      showToast({ message: "Raccourci global indisponible", detail: error, error: true });
    }
    lastHotkeyError = error;
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
      <CategoryRail v-model="L.rail.value" :items="L.railItems.value" :tile-count="L.config.value?.tiles.length ?? 0" />

      <div ref="scroller" class="scroll">
        <ConfigError
          v-if="L.error.value"
          :error="L.error.value"
          :compact="!!L.config.value"
          @retry="load"
          @open-folder="openConfigFolder"
        />
        <p v-if="L.config.value && L.sections.value.length === 0" class="empty">
          <template v-if="L.query.value.trim()">Aucune tuile pour <strong>« {{ L.query.value.trim() }} »</strong>.</template>
          <template v-else-if="L.rail.value === 'favorites'">Aucun favori pour l'instant. Sélectionnez une tuile et appuyez sur <kbd>Ctrl</kbd> <kbd>D</kbd>.</template>
          <template v-else>Cette catégorie est vide.</template>
          <br />Ajoutez des tuiles dans <code>config.json</code>
          (<a href="#" @click.prevent="openConfigFolder">ouvrir le dossier</a>).
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
        />
      </div>
    </div>

    <StatusBar :kind="status.kind" :target="status.target" />
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
