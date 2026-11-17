// Logique du formulaire de l'éditeur de tuiles, sans Vue (testée dans editor.test.ts).
import type { Category, Tile, TileDraft, TileType } from "./types";

/** Valeur spéciale du choix de catégorie : « Nouvelle catégorie… ». */
export const NEW_CATEGORY = "__new__";

/** Les champs du formulaire, tous en texte (ce que contiennent les <input>). */
export interface TileForm {
  type: TileType;
  name: string;
  path: string;
  /** Arguments sur une ligne, comme dans une invite de commandes. */
  args: string;
  url: string;
  /** appId, ou une adresse du magasin Steam qui le contient. */
  appId: string;
  /** Id d'une catégorie existante, ou `NEW_CATEGORY`. */
  category: string;
  newCategory: string;
  icon: string;
}

/** Formulaire vide pour une nouvelle tuile, dans `category` si elle existe. */
export function emptyForm(categories: Category[], category?: string): TileForm {
  const known = categories.some((c) => c.id === category);
  return {
    type: "app",
    name: "",
    path: "",
    args: "",
    url: "",
    appId: "",
    category: known ? category! : (categories[0]?.id ?? NEW_CATEGORY),
    newCategory: "",
    icon: "",
  };
}

/** Formulaire pré-rempli pour modifier `tile`. */
export function formFromTile(tile: Tile): TileForm {
  return {
    type: tile.type,
    name: tile.name,
    path: tile.type === "app" ? tile.path : "",
    args: tile.type === "app" ? joinArgs(tile.args ?? []) : "",
    url: tile.type === "web" ? tile.url : "",
    appId: tile.type === "steam" ? String(tile.appId) : "",
    category: tile.category,
    newCategory: "",
    icon: tile.icon ?? "",
  };
}

/**
 * Vérifie le formulaire. Renvoie la tuile à envoyer à Rust, ou un message par
 * champ en erreur (Rust revérifie de toute façon avant d'écrire).
 */
export function toDraft(form: TileForm): { draft: TileDraft } | { errors: Partial<Record<keyof TileForm, string>> } {
  const errors: Partial<Record<keyof TileForm, string>> = {};
  const name = form.name.trim();
  if (!name) errors.name = "Donnez un nom à la tuile.";
  const newCategory = form.category === NEW_CATEGORY ? form.newCategory.trim() : "";
  if (form.category === NEW_CATEGORY && !newCategory) errors.newCategory = "Nommez la nouvelle catégorie.";

  const common = {
    name,
    category: form.category === NEW_CATEGORY ? "" : form.category,
    ...(newCategory ? { newCategory } : {}),
    ...(form.icon.trim() ? { icon: unquote(form.icon) } : {}),
  };
  let draft: TileDraft | null = null;
  switch (form.type) {
    case "app": {
      const path = unquote(form.path);
      if (!path) errors.path = "Indiquez le programme ou le raccourci à lancer.";
      else draft = { ...common, type: "app", path, args: splitArgs(form.args) };
      break;
    }
    case "web": {
      const url = normalizeUrl(form.url);
      if (!url) errors.url = "Indiquez l'adresse du site.";
      else if (!/^https?:\/\/[^\s"]+$/i.test(url)) errors.url = "Adresse invalide (pas d'espace ni de guillemet).";
      else draft = { ...common, type: "web", url };
      break;
    }
    case "steam": {
      const appId = parseAppId(form.appId);
      if (appId === null) errors.appId = "Indiquez l'appId du jeu (le nombre dans l'adresse de sa page du magasin).";
      else draft = { ...common, type: "steam", appId };
      break;
    }
  }
  return Object.keys(errors).length > 0 || !draft ? { errors } : { draft };
}

/** Retire les blancs et les guillemets d'un chemin collé depuis l'Explorateur. */
export function unquote(path: string): string {
  return path.trim().replace(/^"(.*)"$/, "$1").trim();
}

/** Ajoute `https://` à une adresse tapée sans (« youtube.com »). */
export function normalizeUrl(text: string): string {
  const url = text.trim();
  // `localhost:3000` n'a pas de schéma : seul `xxx://` en est un.
  if (!url || /^[a-z][a-z0-9+.-]*:\/\//i.test(url)) return url;
  return `https://${url}`;
}

/**
 * appId Steam : un nombre, ou une adresse qui le contient
 * (`https://store.steampowered.com/app/620/Portal_2/`, `steam://rungameid/620`).
 */
export function parseAppId(text: string): number | null {
  const t = text.trim();
  const m = /^(\d+)$/.exec(t) ?? /(?:\/app\/|rungameid\/|run\/)(\d+)/i.exec(t);
  const n = m ? Number(m[1]) : NaN;
  return Number.isSafeInteger(n) && n > 0 && n <= 0xffffffff ? n : null;
}

/**
 * Découpe une ligne d'arguments avec les règles de Windows
 * (CommandLineToArgvW) : les espaces séparent, les guillemets regroupent
 * (`--title "Mon jeu"` → 2 arguments), `\"` donne un guillemet, et des `\`
 * suivis d'un guillemet sont dédoublés (`"C:\dir\\"` → `C:\dir\`).
 */
export function splitArgs(line: string): string[] {
  const args: string[] = [];
  let current = "";
  let quoted = false;
  let started = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === "\\") {
      let n = 0;
      while (line[i + n] === "\\") n++;
      if (line[i + n] === '"') {
        // 2k `\` + `"` : k `\` puis un guillemet de structure ;
        // 2k+1 `\` + `"` : k `\` puis un guillemet littéral.
        current += "\\".repeat(Math.floor(n / 2));
        if (n % 2 === 1) {
          current += '"';
          i += n;
        } else {
          i += n - 1;
        }
      } else {
        current += "\\".repeat(n);
        i += n - 1;
      }
      started = true;
    } else if (c === '"') {
      quoted = !quoted;
      started = true;
    } else if (/\s/.test(c) && !quoted) {
      if (started) args.push(current);
      current = "";
      started = false;
    } else {
      current += c;
      started = true;
    }
  }
  if (started) args.push(current);
  return args;
}

/** L'inverse de `splitArgs` : guillemets autour des arguments qui en ont besoin. */
export function joinArgs(args: string[]): string {
  return args
    .map((a) => {
      if (a !== "" && !/[\s"]/.test(a)) return a;
      // Les `\` devant un guillemet (ou devant le guillemet final) sont doublés.
      const escaped = a.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/, "$1$1");
      return `"${escaped}"`;
    })
    .join(" ");
}

/** Nom proposé d'après un chemin choisi : `C:\…\Discord.lnk` → « Discord ». */
export function nameFromPath(path: string): string {
  const file = path.split(/[\\/]/).pop() ?? "";
  return file.replace(/\.(exe|lnk|bat|cmd|url)$/i, "").trim();
}
