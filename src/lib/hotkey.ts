// Raccourci global : capture au clavier dans l'écran de paramètres, et
// affichage lisible. Le texte produit est celui que Rust lit dans config.json
// (window.rs, `parse_trigger`) : "Super+Alt+Space", ou "Super" pour la touche
// Windows seule.

/** Texte du réglage pour la touche Windows seule. */
export const LONE_WIN = "Super";

/** Le réglage désigne-t-il la touche Windows seule ? (mêmes mots que Rust) */
export function isLoneWin(hotkey: string): boolean {
  return ["super", "win", "windows"].includes(hotkey.trim().toLowerCase());
}

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight",
]);

/** Touches nommées acceptées par Rust (crate global-hotkey), par `KeyboardEvent.code`. */
const NAMED = new Set([
  "Space", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown",
  "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PrintScreen", "ScrollLock", "Pause", "CapsLock", "NumLock",
  "Minus", "Equal", "Comma", "Period", "Slash", "Semicolon", "Quote", "Backquote", "BracketLeft", "BracketRight", "Backslash",
  "NumpadAdd", "NumpadSubtract", "NumpadMultiply", "NumpadDivide", "NumpadDecimal", "NumpadEnter", "NumpadEqual",
]);

/** Modificateurs enfoncés, dans l'ordre d'écriture du réglage. */
export function modifiers(e: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string[] {
  const mods: string[] = [];
  if (e.metaKey) mods.push("Super");
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  return mods;
}

/**
 * Nom de la touche principale pour Rust, ou `null` si ce n'est pas une touche
 * utilisable (modificateur seul, touche inconnue).
 *
 * Les lettres sont lues dans `e.key` (la lettre imprimée, AZERTY compris :
 * Windows enregistre le raccourci par lettre, pas par position) ; le reste
 * dans `e.code` (position physique), qui ne change pas avec Maj ou AltGr.
 */
export function mainKey(e: Pick<KeyboardEvent, "key" | "code">): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  if (/^[a-z]$/i.test(e.key)) return e.key.toUpperCase();
  let m = /^Key([A-Z])$/.exec(e.code);
  if (m) return m[1];
  m = /^Digit(\d)$/.exec(e.code);
  if (m) return m[1];
  m = /^Numpad(\d)$/.exec(e.code);
  if (m) return `Numpad${m[1]}`;
  if (/^F([1-9]|1\d|2[0-4])$/.test(e.code)) return e.code;
  if (NAMED.has(e.code)) return e.code;
  return null;
}

/**
 * Raccourci complet d'après une touche enfoncée : `{ hotkey }` s'il est
 * complet, `{ pending }` (modificateurs seuls, pour l'affichage pendant la
 * saisie), ou `{ error }` s'il n'est pas utilisable.
 */
export function hotkeyFromEvent(
  e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
): { hotkey: string } | { pending: string } | { error: string } {
  const mods = modifiers(e);
  const key = mainKey(e);
  if (!key) {
    if (MODIFIER_CODES.has(e.code)) return { pending: mods.join("+") };
    return { error: "Cette touche ne peut pas servir de raccourci." };
  }
  if (mods.length === 0) {
    return { error: "Ajoutez au moins Ctrl, Alt, Maj ou Win : une touche seule serait volée à toutes les autres applications." };
  }
  return { hotkey: [...mods, key].join("+") };
}

const LABELS: Record<string, string> = {
  SUPER: "Win", WIN: "Win", WINDOWS: "Win", CTRL: "Ctrl", CONTROL: "Ctrl", ALT: "Alt", SHIFT: "Maj",
  SPACE: "Espace", ENTER: "Entrée", ESCAPE: "Échap", ESC: "Échap", DELETE: "Suppr", BACKSPACE: "Retour arrière",
  ARROWUP: "↑", ARROWDOWN: "↓", ARROWLEFT: "←", ARROWRIGHT: "→", UP: "↑", DOWN: "↓", LEFT: "←", RIGHT: "→",
  PAGEUP: "Page préc.", PAGEDOWN: "Page suiv.", HOME: "Début", END: "Fin", INSERT: "Inser", TAB: "Tab",
};

/** Le réglage tel qu'on l'affiche : "Super+Alt+Space" → ["Win", "Alt", "Espace"]. */
export function hotkeyLabel(hotkey: string): string[] {
  if (isLoneWin(hotkey)) return ["Win"];
  return hotkey
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => LABELS[part.toUpperCase()] ?? part.replace(/^Key(?=[A-Z]$)|^Digit(?=\d$)/, ""));
}
