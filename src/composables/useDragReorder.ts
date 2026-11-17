// Glisser-déposer pour réordonner une liste (tuiles d'une section, catégories
// du rail), avec les événements « pointer » plutôt que le glisser-déposer HTML5 :
// sous Windows, Tauri intercepte ce dernier pour recevoir les fichiers déposés
// depuis l'Explorateur, et il ne marche alors pas dans la page.
//
// Usage : chaque élément appelle `start(event, index)` sur `pointerdown`. Le
// glisser commence après quelques pixels (un simple clic reste un clic) ; les
// éléments frères marqués `data-drag-index` servent de cibles.
import { ref } from "vue";
import { dropTarget, type DropTarget, type Rect } from "../lib/reorder";

/** Distance (px) avant qu'un appui devienne un glisser. */
const THRESHOLD = 5;

export function useDragReorder(options: {
  /** `x` ou `grid` : on compare à la moitié horizontale ; `y` : verticale. */
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

  function start(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const container = el.parentElement;
    if (!container) return;
    const origin = { x: e.clientX, y: e.clientY };
    let active = false;

    const items = () =>
      [...container.querySelectorAll<HTMLElement>(":scope > [data-drag-index]")].map((item) => ({
        index: Number(item.dataset.dragIndex),
        rect: item.getBoundingClientRect() as Rect,
      }));

    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - origin.x;
      const dy = ev.clientY - origin.y;
      if (!active) {
        if (Math.hypot(dx, dy) < THRESHOLD) return;
        active = true;
        dragging.value = index;
      }
      offset.value = { x: dx, y: dy };
      // Les autres éléments, à leur place (l'élément glissé est décalé à l'écran).
      const others = items().filter((i) => i.index !== index);
      target.value = dropTarget(others, { x: ev.clientX, y: ev.clientY }, options.axis) ?? target.value;
    };

    const end = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      if (!active) return;
      // Le relâchement produirait un « click » (lancer la tuile) : on l'avale.
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);

      const t = target.value;
      dragging.value = null;
      target.value = null;
      offset.value = { x: 0, y: 0 };
      if (ev.type === "pointerup" && t) {
        let to = t.index + (t.after ? 1 : 0);
        if (to > index) to--;
        if (to !== index) options.onDrop(index, to);
      }
    };

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  }

  return { dragging, target, offset, start };
}
