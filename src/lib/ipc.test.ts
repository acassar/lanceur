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
        settings: { hotkey: "", hideAfterLaunch: true, hideOnBlur: true, accent: "#fff", browser: "msedge", recentsCount: 4 },
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
    expect(await ipc.getHotkeyStatus()).toEqual({ hotkey: "Ctrl+Alt+Space", error: null });
    const stop = await ipc.onWindowShown(() => {});
    expect(typeof stop).toBe("function");
  });
});
