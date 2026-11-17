// Glisser-déposer pour réordonner une liste (tuiles d'une section, catégories
// du rail), avec les événements « pointer » plutôt que le glisser-déposer HTML5 :
// sous Windows, Tauri intercepte ce dernier pour recevoir les fichiers déposés
// depuis l'Explorateur, et il ne marche alors pas dans la page.
//
// Usage : chaque élément appelle `start(event, index)` sur `pointerdown`. Le
// glisser commence après quelques pixels (un simple clic reste un clic) ; les
// éléments frères marqués `data-drag-index` servent de cibles.
import { getCurrentInstance, onBeforeUnmount, ref } from "vue";
import { dropTarget, type DropTarget, type Rect } from "../lib/reorder";

/** Distance (px) avant qu'un appui devienne un glisser. */
const THRESHOLD = 5;

export function useDragReorder(options: {
  /** `x` : grille ou ligne (moitié gauche/droite) ; `y` : liste verticale. */
  axis: "x" | "y";
  /** Appelé au lâcher, si l'élément a changé de place. */
  onDrop: (from: number, to: number) => void;
}) {
  /** Index de l'élément en cours de glisser, ou `null`. */
  const dragging = ref<number | null>(null);
  /** Où l'élément serait déposé (pour afficher le trait d'insertion). */
  const target = ref<DropTarget | null>(null);
  /** Déplacement de l'élément glissé depuis l'appui, pour qu'il suive la souris. */
  const offset = ref({ x: 0, y: 0 });
  /** Arrête le glisser en cours sans rien déposer (composant démonté…). */
  let cancel: (() => void) | null = null;
  if (getCurrentInstance()) onBeforeUnmount(() => cancel?.());

  function start(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const container = el.parentElement;
    if (!container) return;
    const origin = { x: e.clientX, y: e.clientY };
    let active = false;
    /** Les autres éléments et leur place, mesurés une fois au début du glisser. */
    let others: { index: number; rect: Rect }[] = [];
    /** Défilement au moment de la mesure (la liste peut défiler pendant le glisser). */
    let scroll = { x: 0, y: 0 };
    const scroller = el.closest<HTMLElement>(".scroll");
    const scrolled = () => ({ x: scroller?.scrollLeft ?? 0, y: scroller?.scrollTop ?? 0 });

    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - origin.x;
      const dy = ev.clientY - origin.y;
      if (!active) {
        if (Math.hypot(dx, dy) < THRESHOLD) return;
        active = true;
        dragging.value = index;
        others = [...container.querySelectorAll<HTMLElement>(":scope > [data-drag-index]")]
          .map((item) => ({ index: Number(item.dataset.dragIndex), rect: item.getBoundingClientRect() as Rect }))
          .filter((i) => i.index !== index);
        scroll = scrolled();
      }
      const now = scrolled();
      offset.value = { x: dx + now.x - scroll.x, y: dy + now.y - scroll.y };
      // Position du pointeur ramenée au défilement de la mesure.
      const p = { x: ev.clientX + now.x - scroll.x, y: ev.clientY + now.y - scroll.y };
      target.value = dropTarget(others, p, options.axis) ?? target.value;
    };

    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      cancel = null;
      dragging.value = null;
      target.value = null;
      offset.value = { x: 0, y: 0 };
    };
    const end = (ev: PointerEvent) => {
      const t = target.value;
      stop();
      if (!active) return;
      // Le relâchement produirait un « click » (lancer la tuile) : on l'avale.
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);

      if (ev.type === "pointerup" && t) {
        let to = t.index + (t.after ? 1 : 0);
        if (to > index) to--;
        if (to !== index) options.onDrop(index, to);
      }
    };

    cancel?.();
    cancel = stop;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  }

  return { dragging, target, offset, start };
}
