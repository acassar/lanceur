import { describe, expect, it } from "vitest";
import { createMockIpc } from "./ipc.mock";
import { toIpcError } from "./ipc";

describe("toIpcError", () => {
  it("garde les champs envoyés par Rust", () => {
    const e = toIpcError({ message: "config.json, ligne 3, colonne 1 : x", path: "C:\\c.json", line: 3, column: 1 });
    expect(e).toEqual({ message: "config.json, ligne 3, colonne 1 : x", path: "C:\\c.json", line: 3, column: 1 });
  });

  it("accepte une erreur JS ou une simple chaîne", () => {
    expect(toIpcError(new Error("boom")).message).toBe("boom");
    expect(toIpcError("oups")).toEqual({ message: "oups", path: null, line: null, column: null });
  });
});

describe("createMockIpc", () => {
  it("utilise la config fournie et ignore les récents inconnus", async () => {
    const ipc = createMockIpc(
      async () => ({
        version: 1,
        settings: { hotkey: "", hideAfterLaunch: true, hideOnBlur: true, accent: "#fff", browser: "msedge", recentsCount: 4, launchAtStartup: false },
        categories: [{ id: "c", name: "C" }],
        favorites: [],
        tiles: [{ id: "a", name: "A", category: "c", type: "web", url: "https://a" }],
      }),
      ["a", "inconnu"],
    );
    expect((await ipc.getConfig()).tiles).toHaveLength(1);
    expect(await ipc.getRecents()).toEqual(["a"]);
    expect(await ipc.launch("a")).toEqual(["a"]);
  });
});

describe("createMockIpc, fenêtre", () => {
  it("renvoie le raccourci de la config sans erreur", async () => {
    const ipc = createMockIpc();
    await ipc.getConfig();
    expect(await ipc.getHotkeyStatus()).toEqual({ hotkey: "Super+Alt+Space", error: null });
    const stop = await ipc.onWindowShown(() => {});
    expect(typeof stop).toBe("function");
  });
});

describe("createMockIpc, favoris", () => {
  it("setFavorite est idempotent", async () => {
    const ipc = createMockIpc();
    const { favorites } = await ipc.getConfig();
    expect(await ipc.setFavorite("bg3", true)).toEqual([...favorites, "bg3"]);
    expect(await ipc.setFavorite("bg3", true)).toEqual([...favorites, "bg3"]);
    expect(await ipc.setFavorite("bg3", false)).toEqual(favorites);
    expect(await ipc.setFavorite("bg3", false)).toEqual(favorites);
  });
});

describe("createMockIpc, éditeur", () => {
  it("ajoute, modifie et supprime, et garde les changements au rechargement", async () => {
    const ipc = createMockIpc();
    await ipc.getConfig();
    const tile = await ipc.addTile({ name: "Twitch", category: "", newCategory: "Vidéo", type: "web", url: "https://twitch.tv" });
    expect(tile.category).toBe("video");
    await ipc.updateTile(tile.id, { name: "Twitch TV", category: "web", type: "web", url: "https://twitch.tv" });
    await ipc.deleteTile("cs2");
    const config = await ipc.getConfig();
    expect(config.tiles[config.tiles.length - 1]).toMatchObject({ id: tile.id, name: "Twitch TV", category: "web" });
    expect(config.tiles.some((t) => t.id === "cs2")).toBe(false);
    expect(config.favorites).not.toContain("cs2");
    expect(config.categories.map((c) => c.id)).toContain("video");
  });
});
