import { describe, expect, it } from "vitest";
import { hotkeyFromEvent, hotkeyLabel, isLoneWin, mainKey } from "./hotkey";

const key = (code: string, key: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code,
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("hotkeyFromEvent", () => {
  it("écrit les modificateurs dans l'ordre attendu par Rust", () => {
    expect(hotkeyFromEvent(key("Space", " ", { metaKey: true, altKey: true }))).toEqual({ hotkey: "Super+Alt+Space" });
    expect(hotkeyFromEvent(key("KeyK", "k", { ctrlKey: true, shiftKey: true }))).toEqual({ hotkey: "Ctrl+Shift+K" });
    expect(hotkeyFromEvent(key("F9", "F9", { altKey: true }))).toEqual({ hotkey: "Alt+F9" });
  });

  it("prend la lettre imprimée (AZERTY) plutôt que la position", () => {
    // Touche A d'un clavier AZERTY : position Q d'un QWERTY.
    expect(hotkeyFromEvent(key("KeyQ", "a", { ctrlKey: true }))).toEqual({ hotkey: "Ctrl+A" });
    // Avec Ctrl+Alt (AltGr), la lettre devient un symbole : on retombe sur la position.
    expect(hotkeyFromEvent(key("KeyE", "€", { ctrlKey: true, altKey: true }))).toEqual({ hotkey: "Ctrl+Alt+E" });
    // Rangée des chiffres en AZERTY : « & » sans Maj, mais c'est bien 1.
    expect(hotkeyFromEvent(key("Digit1", "&", { altKey: true }))).toEqual({ hotkey: "Alt+1" });
  });

  it("attend la touche principale tant que seuls des modificateurs sont enfoncés", () => {
    expect(hotkeyFromEvent(key("ControlLeft", "Control", { ctrlKey: true }))).toEqual({ pending: "Ctrl" });
    expect(hotkeyFromEvent(key("AltLeft", "Alt", { ctrlKey: true, altKey: true }))).toEqual({ pending: "Ctrl+Alt" });
  });

  it("refuse une touche seule ou inconnue", () => {
    expect(hotkeyFromEvent(key("KeyA", "a"))).toHaveProperty("error");
    expect(hotkeyFromEvent(key("IntlBackslash", "<", { ctrlKey: true }))).toHaveProperty("error");
  });
});

describe("mainKey", () => {
  it("garde les noms compris par Rust", () => {
    expect(mainKey({ code: "Numpad5", key: "5" })).toBe("Numpad5");
    expect(mainKey({ code: "ArrowUp", key: "ArrowUp" })).toBe("ArrowUp");
    expect(mainKey({ code: "F24", key: "F24" })).toBe("F24");
    expect(mainKey({ code: "F25", key: "F25" })).toBeNull();
    expect(mainKey({ code: "MetaLeft", key: "Meta" })).toBeNull();
  });
});

describe("hotkeyLabel et isLoneWin", () => {
  it("affiche le raccourci en français", () => {
    expect(hotkeyLabel("Super+Alt+Space")).toEqual(["Win", "Alt", "Espace"]);
    expect(hotkeyLabel("ctrl+shift+KeyK")).toEqual(["Ctrl", "Maj", "K"]);
    expect(hotkeyLabel("Super")).toEqual(["Win"]);
  });

  it("reconnaît la touche Windows seule comme Rust", () => {
    expect(isLoneWin(" super ")).toBe(true);
    expect(isLoneWin("Win")).toBe(true);
    expect(isLoneWin("Super+Space")).toBe(false);
  });
});
