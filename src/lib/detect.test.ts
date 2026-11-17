import { describe, expect, it } from "vitest";
import { appsCategory, detectMessage, filterApps, selection } from "./detect";
import { createMockIpc } from "./ipc.mock";
import type { DetectedApp } from "./types";

const app = (name: string, folder: string | null = null): DetectedApp => ({ name, path: `${name}.lnk`, folder, inConfig: false });

describe("selection", () => {
  it("accorde et assemble", () => {
    expect(selection(0, 0)).toBe("rien");
    expect(selection(1, 0)).toBe("1 application");
    expect(selection(0, 3)).toBe("3 jeux");
    expect(selection(2, 1)).toBe("2 applications et 1 jeu");
  });
});

describe("filterApps", () => {
  it("cherche dans le nom et le dossier, sans accents", () => {
    const list = [app("Discord", "Discord Inc"), app("Éditeur"), app("VLC media player", "VideoLAN")];
    expect(filterApps(list, "")).toHaveLength(3);
    expect(filterApps(list, "edit").map((a) => a.name)).toEqual(["Éditeur"]);
    expect(filterApps(list, "videolan").map((a) => a.name)).toEqual(["VLC media player"]);
  });
});

describe("detectMessage", () => {
  it("décrit ce qui a été ajouté, et où", () => {
    expect(detectMessage(null, { added: [], category: "Jeux" })).toMatch(/Rien de nouveau/);
    expect(detectMessage({ added: ["a"], category: "Applications" }, null)).toBe("1 application ajoutée dans « Applications »");
    expect(detectMessage({ added: ["a", "b"], category: "Outils" }, { added: ["c"], category: "Jeux" })).toBe(
      "2 applications et 1 jeu ajoutés dans « Outils » et « Jeux »",
    );
    expect(detectMessage({ added: [], category: "Outils" }, { added: ["c"], category: "Jeux" })).toBe("1 jeu ajouté dans « Jeux »");
  });
});

describe("createMockIpc, détection des applications", () => {
  it("marque les apps déjà présentes et ajoute les autres sans doublon", async () => {
    const ipc = createMockIpc();
    const before = await ipc.getConfig();
    const detected = await ipc.detectApps();
    expect(detected.find((a) => a.name === "Visual Studio Code")?.inConfig).toBe(true);
    expect(detected.find((a) => a.name === "Discord")?.inConfig).toBe(true); // même nom
    const fresh = detected.filter((a) => !a.inConfig);
    const result = await ipc.importApps(fresh.slice(0, 2).map((a) => a.path));
    expect(result.added).toHaveLength(2);
    expect(result.category).toBe("Outils");
    const after = await ipc.getConfig();
    expect(after.tiles).toHaveLength(before.tiles.length + 2);
    expect((await ipc.importApps([fresh[0].path])).added).toEqual([]);
  });
});

describe("appsCategory", () => {
  it("suit la règle de Rust", async () => {
    const config = await createMockIpc().getConfig();
    expect(appsCategory(null)).toBe("Applications");
    expect(appsCategory(config)).toBe("Outils");
    expect(appsCategory({ ...config, categories: [{ id: "x", name: " Logiciels " }] })).toBe(" Logiciels ");
    expect(appsCategory({ ...config, categories: [{ id: "web", name: "Web" }] })).toBe("Applications");
  });
});
