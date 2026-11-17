import { describe, expect, it } from "vitest";
import { NEW_CATEGORY, emptyForm, formFromTile, joinArgs, nameFromPath, normalizeUrl, parseAppId, splitArgs, toDraft } from "./editor";
import type { Tile } from "./types";

const CATEGORIES = [
  { id: "jeux", name: "Jeux" },
  { id: "web", name: "Web" },
];

describe("arguments", () => {
  it("découpe comme une invite de commandes", () => {
    expect(splitArgs("--processStart Discord.exe")).toEqual(["--processStart", "Discord.exe"]);
    expect(splitArgs('  --title "Mon jeu"  -x ')).toEqual(["--title", "Mon jeu", "-x"]);
    expect(splitArgs('"" a\\"b')).toEqual(["", 'a"b']);
    expect(splitArgs("")).toEqual([]);
  });

  it("joinArgs puis splitArgs redonne la même liste", () => {
    for (const args of [["a", "b c", ""], ['--x="y"'], []]) {
      expect(splitArgs(joinArgs(args))).toEqual(args);
    }
    expect(joinArgs(["--processStart", "Discord.exe"])).toBe("--processStart Discord.exe");
  });
});

describe("champs", () => {
  it("appId : nombre ou adresse du magasin", () => {
    expect(parseAppId(" 620 ")).toBe(620);
    expect(parseAppId("https://store.steampowered.com/app/620/Portal_2/")).toBe(620);
    expect(parseAppId("steam://rungameid/730")).toBe(730);
    expect(parseAppId("0")).toBeNull();
    expect(parseAppId("abc")).toBeNull();
    expect(parseAppId("99999999999")).toBeNull();
  });

  it("adresse sans https://", () => {
    expect(normalizeUrl(" youtube.com ")).toBe("https://youtube.com");
    expect(normalizeUrl("http://a.fr")).toBe("http://a.fr");
    expect(normalizeUrl("")).toBe("");
  });

  it("nom tiré du fichier choisi", () => {
    expect(nameFromPath("C:\\Users\\a\\Desktop\\Discord.lnk")).toBe("Discord");
    expect(nameFromPath("D:/Jeux/jeu.v2.exe")).toBe("jeu.v2");
  });
});

describe("formulaire", () => {
  it("nouvelle tuile dans la catégorie affichée, sinon la première", () => {
    expect(emptyForm(CATEGORIES, "web").category).toBe("web");
    expect(emptyForm(CATEGORIES, "favorites").category).toBe("jeux");
    expect(emptyForm([]).category).toBe(NEW_CATEGORY);
  });

  it("aller-retour tuile → formulaire → brouillon", () => {
    const tiles: Tile[] = [
      { id: "d", name: "Discord", category: "web", type: "app", path: "C:\\D.exe", args: ["--a", "b c"], icon: "C:\\i.png" },
      { id: "y", name: "YouTube", category: "web", type: "web", url: "https://youtube.com" },
      { id: "p", name: "Portal 2", category: "jeux", type: "steam", appId: 620 },
    ];
    for (const { id, ...rest } of tiles) {
      const result = toDraft(formFromTile({ id, ...rest } as Tile));
      expect(result).toEqual({ draft: rest });
    }
  });

  it("nouvelle catégorie, guillemets et https:// ajoutés", () => {
    const form = { ...emptyForm(CATEGORIES), type: "web" as const, name: " Twitch ", url: "twitch.tv", category: NEW_CATEGORY, newCategory: " Vidéo " };
    expect(toDraft(form)).toEqual({ draft: { name: "Twitch", category: "", newCategory: "Vidéo", type: "web", url: "https://twitch.tv" } });
    const app = { ...emptyForm(CATEGORIES), name: "A", path: ' "C:\\Program Files\\a.exe" ' };
    expect(toDraft(app)).toMatchObject({ draft: { path: "C:\\Program Files\\a.exe", args: [] } });
  });

  it("signale chaque champ manquant", () => {
    const result = toDraft({ ...emptyForm(CATEGORIES), category: NEW_CATEGORY });
    expect(result).toEqual({ errors: { name: expect.any(String), newCategory: expect.any(String), path: expect.any(String) } });
    expect(toDraft({ ...emptyForm(CATEGORIES), name: "a", type: "web", url: "https://a b" })).toHaveProperty("errors.url");
    expect(toDraft({ ...emptyForm(CATEGORIES), name: "a", type: "steam", appId: "x" })).toHaveProperty("errors.appId");
  });
});
