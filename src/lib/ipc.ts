// Point d'accès unique aux commandes Rust. Les composants n'appellent jamais
// invoke() directement : ils passent par cet objet typé.
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
}

// Étape 1 : les commandes Rust n'existent pas encore, on utilise la fausse IPC
// partout (navigateur et app installée). L'étape 2 branchera ici l'implémentation
// réelle, basée sur invoke(), quand l'app tourne dans Tauri.
export const ipc: Ipc = createMockIpc();
