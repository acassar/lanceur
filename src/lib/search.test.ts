import { describe, expect, it } from "vitest";
import { filterTiles, highlight, initials, normalize, score } from "./search";
import type { Tile } from "./types";

const tile = (id: string, name: string): Tile => ({ id, name, category: "jeux", type: "steam", appId: 1 });

describe("normalize", () => {
  it("retire accents et majuscules", () => {
    expect(normalize("Éditeur Vidéo")).toBe("editeur video");
  });
});

describe("initials", () => {
  it("prend la première lettre de chaque mot et garde les nombres", () => {
    expect(initials("Baldur's Gate 3")).toBe("bg3");
    expect(initials("Counter-Strike 2")).toBe("cs2");
  });
});

describe("score", () => {
  it("classe préfixe > début de mot > initiales > contenu", () => {
    expect(score("Discord", "dis")).toBe(4);
    expect(score("Rocket League", "lea")).toBe(3);
    expect(score("Baldur's Gate 3", "bg3")).toBe(2);
    expect(score("Factorio", "tor")).toBe(1);
    expect(score("Factorio", "xyz")).toBe(0);
  });

  it("ignore les accents dans la recherche", () => {
    expect(score("Elden Ring", "élden")).toBe(4);
  });

  it("une recherche vide correspond à tout", () => {
    expect(score("Steam", "  ")).toBe(1);
  });
});

describe("filterTiles", () => {
  const tiles = [tile("a", "Hades II"), tile("b", "Hollow Knight"), tile("c", "Shadow of Hades")];

  it("trie par pertinence puis par ordre d'origine", () => {
    expect(filterTiles(tiles, "ha").map((t) => t.id)).toEqual(["a", "c"]);
    expect(filterTiles(tiles, "h").map((t) => t.id)).toEqual(["a", "b", "c"]);
  });

  it("renvoie toutes les tuiles sans recherche", () => {
    expect(filterTiles(tiles, "")).toHaveLength(3);
  });
});

describe("highlight", () => {
  it("isole la partie trouvée", () => {
    expect(highlight("Discord", "sco")).toEqual([
      { text: "Di", match: false },
      { text: "sco", match: true },
      { text: "rd", match: false },
    ]);
  });

  it("ne surligne rien si la correspondance vient des initiales", () => {
    expect(highlight("Baldur's Gate 3", "bg3")).toEqual([{ text: "Baldur's Gate 3", match: false }]);
  });
});
