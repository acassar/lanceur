// Déplacement de la sélection au clavier dans des grilles de largeur variable.
// Gauche/droite : tuile précédente/suivante. Haut/bas : on cherche la tuile
// visuellement la plus proche sur la ligne du dessus/dessous, d'après sa
// position à l'écran, ce qui marche aussi entre deux sections.
import type { Ref } from "vue";

export type Direction = "left" | "right" | "up" | "down";

export function useKeyboardNav(container: Ref<HTMLElement | null>, selected: Ref<number>, count: () => number) {
  function rectOf(el: Element) {
    const r = el.getBoundingClientRect();
    return { top: r.top, cx: r.left + r.width / 2 };
  }

  function move(dir: Direction) {
    const n = count();
    if (n === 0) return;
    if (dir === "left" || dir === "right") {
      selected.value = Math.min(n - 1, Math.max(0, selected.value + (dir === "right" ? 1 : -1)));
      return;
    }
    const tiles = Array.from(container.value?.querySelectorAll<HTMLElement>("[data-index]") ?? []);
    const cur = tiles.find((el) => Number(el.dataset.index) === selected.value);
    if (!cur) return;
    const from = rectOf(cur);
    const candidates = tiles
      .map((el) => ({ index: Number(el.dataset.index), ...rectOf(el) }))
      .filter((t) => (dir === "down" ? t.top > from.top + 4 : t.top < from.top - 4));
    if (candidates.length === 0) return;
    const rowTop =
      dir === "down" ? Math.min(...candidates.map((t) => t.top)) : Math.max(...candidates.map((t) => t.top));
    const row = candidates.filter((t) => Math.abs(t.top - rowTop) < 4);
    row.sort((a, b) => Math.abs(a.cx - from.cx) - Math.abs(b.cx - from.cx));
    selected.value = row[0].index;
  }

  /** Fait défiler la liste pour garder la tuile sélectionnée visible. */
  function reveal() {
    container.value
      ?.querySelector(`[data-index="${selected.value}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }

  return { move, reveal };
}
