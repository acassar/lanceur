// Point d'accès unique aux commandes Rust. Les composants n'appellent jamais
// invoke() directement : ils passent par cet objet typé.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { createMockIpc } from "./ipc.mock";
import type { Config } from "./types";

export interface Ipc {
  getConfig(): Promise<Config>;
  getRecents(): Promise<string[]>;
  /** Lance la tuile et renvoie la nouvelle liste des récents. */
  launch(id: string): Promise<string[]>;
  /** Ajoute ou retire un favori et renvoie la nouvelle liste. */
  toggleFavorite(id: string): Promise<string[]>;
  hideWindow(): Promise<void>;
  /** Ouvre le dossier de config.json dans l'Explorateur. */
  openConfigFolder(): Promise<void>;
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
  // Étape 2 : seule la lecture de la config est réelle. Lancement, récents,
  // favoris et masquage restent simulés en mémoire jusqu'aux étapes 3 à 5.
  const simulated = createMockIpc(() => invoke<Config>("get_config"), []);
  return {
    ...simulated,
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
