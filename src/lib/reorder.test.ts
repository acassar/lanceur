import { describe, expect, it } from "vitest";
import { applyTileOrder, dropTarget, move } from "./reorder";
import { SAMPLE } from "./ipc.mock";

// Grille 3 × 2 de cases 100 × 100 : index 0 1 2 / 3 4 5.
const grid = [0, 1, 2, 3, 4, 5].map((index) => {
  const left = (index % 3) * 100;
  const top = Math.floor(index / 3) * 100;
  return { index, rect: { left, top, right: left + 90, bottom: top + 90 } };
});

describe("dropTarget", () => {
  it("grille : bonne ligne, puis côté de la case", () => {
    expect(dropTarget(grid, { x: 120, y: 40 }, "x")).toEqual({ index: 1, after: false });
    expect(dropTarget(grid, { x: 180, y: 140 }, "x")).toEqual({ index: 4, after: true });
    // Entre deux lignes ou au-delà du bord : l'élément le plus proche.
    expect(dropTarget(grid, { x: 400, y: 150 }, "x")).toEqual({ index: 5, after: true });
    expect(dropTarget([], { x: 0, y: 0 }, "x")).toBeNull();
  });

  it("liste verticale", () => {
    const list = [0, 1, 2].map((index) => ({ index, rect: { left: 0, right: 100, top: index * 30, bottom: index * 30 + 28 } }));
    expect(dropTarget(list, { x: 50, y: 35 }, "y")).toEqual({ index: 1, after: false });
    expect(dropTarget(list, { x: 50, y: 80 }, "y")).toEqual({ index: 2, after: true });
  });
});

describe("ordre", () => {
  it("move", () => {
    expect(move(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(move(["a", "b", "c", "d"], 3, 0)).toEqual(["d", "a", "b", "c"]);
  });

  it("applyTileOrder ne touche qu'aux places de la catégorie", () => {
    const config = structuredClone(SAMPLE);
    const web = config.tiles.filter((t) => t.category === "web").map((t) => t.id);
    applyTileOrder(config, "web", [...web].reverse());
    expect(config.tiles.filter((t) => t.category === "web").map((t) => t.id)).toEqual([...web].reverse());
    expect(config.tiles.filter((t) => t.category !== "web")).toEqual(SAMPLE.tiles.filter((t) => t.category !== "web"));
  });
});
