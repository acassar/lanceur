import { describe, expect, it } from "vitest";
import { createMockIpc } from "./ipc.mock";
import { importMessage, targetCategory, toChoices } from "./steam";

describe("toChoices", () => {
  it("marque les jeux qui ont déjà une tuile", async () => {
    const ipc = createMockIpc();
    const config = await ipc.getConfig();
    const choices = toChoices(
      [
        { appId: 730, name: "Counter-Strike 2" },
        { appId: 620, name: "Portal 2" },
      ],
      config,
    );
    expect(choices.map((c) => c.inConfig)).toEqual([true, false]);
    expect(toChoices([{ appId: 1, name: "X" }], null)[0].inConfig).toBe(false);
  });
});

describe("targetCategory", () => {
  it("suit la règle de Rust", async () => {
    const config = await createMockIpc().getConfig();
    expect(targetCategory(null)).toBe("Jeux");
    expect(targetCategory({ ...config, categories: [{ id: "jeux", name: "Mes jeux" }] })).toBe("Mes jeux");
    expect(targetCategory({ ...config, categories: [{ id: "g", name: " Games " }] })).toBe(" Games ");
    expect(targetCategory({ ...config, categories: [{ id: "o", name: "Outils" }] })).toBe("Jeux");
  });
});

describe("importMessage", () => {
  it("accorde le nombre de jeux", () => {
    expect(importMessage({ added: [], category: "Jeux" })).toMatch(/Aucun nouveau jeu/);
    expect(importMessage({ added: ["a"], category: "Jeux" })).toBe("1 jeu ajouté dans « Jeux »");
    expect(importMessage({ added: ["a", "b"], category: "Jeux" })).toBe("2 jeux ajoutés dans « Jeux »");
  });
});

describe("createMockIpc, import Steam", () => {
  it("ajoute les jeux choisis sans doublon, et les garde au rechargement", async () => {
    const ipc = createMockIpc();
    const before = await ipc.getConfig();
    const detected = await ipc.detectSteamGames();
    const fresh = toChoices(detected, before).filter((g) => !g.inConfig);
    expect(fresh.length).toBeGreaterThan(0);

    const result = await ipc.importSteamGames([730, fresh[0].appId]);
    expect(result.added).toHaveLength(1);
    const after = await ipc.getConfig();
    expect(after.tiles).toHaveLength(before.tiles.length + 1);
    expect((await ipc.importSteamGames([fresh[0].appId])).added).toEqual([]);
  });
});
