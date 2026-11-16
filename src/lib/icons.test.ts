import { describe, expect, it, vi } from "vitest";
import { createIconCache, iconKey } from "./icons";
import type { Tile } from "./types";

const tile: Tile = { id: "a", name: "A", category: "c", type: "app", path: "C:\\a.exe" };

describe("iconKey", () => {
  it("ignore le nom et la catégorie, mais pas la source", () => {
    expect(iconKey({ ...tile, name: "Autre", category: "d" })).toBe(iconKey(tile));
    expect(iconKey({ ...tile, path: "C:\\b.exe" })).not.toBe(iconKey(tile));
    expect(iconKey({ ...tile, icon: "C:\\i.png" })).not.toBe(iconKey(tile));
  });
});

describe("createIconCache", () => {
  it("ne demande chaque icône qu'une fois", async () => {
    const fetch = vi.fn(async (id: string) => `data:${id}`);
    const cache = createIconCache(fetch);
    expect(cache.peek(tile)).toBeUndefined();
    const [a, b] = await Promise.all([cache.load(tile), cache.load(tile)]);
    expect(a).toBe("data:a");
    expect(b).toBe("data:a");
    expect(cache.peek(tile)).toBe("data:a");
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("une erreur donne null (monogramme)", async () => {
    const cache = createIconCache(async () => {
      throw new Error("boom");
    });
    expect(await cache.load(tile)).toBeNull();
    expect(cache.peek(tile)).toBeNull();
  });
});
