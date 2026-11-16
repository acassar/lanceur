// Point d'accès unique aux commandes Rust. Les composants n'appellent jamais
// invoke() directement : ils passent par cet objet typé.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createMockIpc } from "./ipc.mock";
import type { Config } from "./types";

export interface Ipc {
  getConfig(): Promise<Config>;
  getRecents(): Promise<string[]>;
  /** Lance la tuile et renvoie la nouvelle liste des récents. */
  launch(id: string): Promise<string[]>;
  /** Ajoute ou retire un favori et renvoie la nouvelle liste. */
  toggleFavorite(id: string): Promise<string[]>;
  /** Réduit la fenêtre dans la barre des tâches (Échap, après un lancement). */
  hideWindow(): Promise<void>;
  /** Raccourci global configuré, et pourquoi il n'a pas pu être enregistré le cas échéant. */
  getHotkeyStatus(): Promise<HotkeyStatus>;
  /**
   * Appelle `callback` chaque fois que la fenêtre réapparaît (raccourci global,
   * exe relancé). Renvoie une fonction qui arrête l'écoute.
   */
  onWindowShown(callback: () => void): Promise<() => void>;
  /** Ouvre le dossier de config.json dans l'Explorateur. */
  openConfigFolder(): Promise<void>;
}

/** Miroir de `HotkeyStatus` dans window.rs. */
export interface HotkeyStatus {
  hotkey: string;
  error: string | null;
}

/**
 * Erreur renvoyée par une commande Rust (miroir de `AppError` dans error.rs).
 * `line` et `column` sont renseignés quand config.json n'est pas du JSON valide.
 */
export interface IpcError {
  message: string;
  path: string | null;
  line: number | null;
  column: number | null;
}

/** Normalise une erreur quelconque (Rust, JS) en IpcError. */
export function toIpcError(e: unknown): IpcError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as Partial<IpcError>;
    return { message: String(o.message), path: o.path ?? null, line: o.line ?? null, column: o.column ?? null };
  }
  return { message: String(e), path: null, line: null, column: null };
}

function createTauriIpc(): Ipc {
  // Étape 4 : tout est réel sauf les favoris, simulés en mémoire jusqu'à l'étape 5.
  const simulated = createMockIpc(() => invoke<Config>("get_config"), []);
  return {
    ...simulated,
    getRecents: () => invoke<string[]>("get_recents"),
    launch: (id) => invoke<string[]>("launch", { id }),
    hideWindow: () => invoke("hide_window"),
    getHotkeyStatus: () => invoke<HotkeyStatus>("get_hotkey_status"),
    // Événement émis par Rust (window.rs, SHOWN_EVENT).
    onWindowShown: (callback) => listen("window-shown", () => callback()),
    openConfigFolder: () => invoke("open_config_folder"),
  };
}

function createBrowserIpc(): Ipc {
  return {
    ...createMockIpc(),
    async openConfigFolder() {
      console.info("[ipc mock] open_config_folder");
    },
  };
}

// Dans l'app (WebView Tauri) : vraies commandes Rust. Dans un navigateur
// (npm run dev) : fausse IPC avec des données d'exemple.
export const ipc: Ipc = isTauri() ? createTauriIpc() : createBrowserIpc();
