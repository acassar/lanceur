// Met la même version partout où l'app la déclare, pour que le tag de release corresponde.
// Usage : npm run bump -- 0.3.0
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('Usage : npm run bump -- X.Y.Z   (ex. npm run bump -- 0.3.0)');
  process.exit(1);
}

function edit(path, replace) {
  const before = readFileSync(path, 'utf8');
  const after = replace(before);
  if (after === before) throw new Error(`${path} : version introuvable`);
  writeFileSync(path, after);
  console.log(`${path} → ${version}`);
}

// package.json et package-lock.json : le premier "version" est celui de l'app,
// et dans le lock, celui de packages[""] (premier "version" après "packages").
edit('package.json', (s) => s.replace(/"version": "[^"]*"/, `"version": "${version}"`));
edit('package-lock.json', (s) =>
  s
    .replace(/"version": "[^"]*"/, `"version": "${version}"`)
    .replace(/("packages": \{\s*"": \{[^}]*?"version": ")[^"]*"/, `$1${version}"`),
);
// Cargo.toml : la ligne version de [package] (la première du fichier).
edit('src-tauri/Cargo.toml', (s) => s.replace(/^version = "[^"]*"/m, `version = "${version}"`));
// Cargo.lock : l'entrée du paquet lanceur.
edit('src-tauri/Cargo.lock', (s) =>
  s.replace(/(name = "lanceur"\r?\nversion = ")[^"]*"/, `$1${version}"`),
);

console.log(`\nEnsuite : PR + merge sur main, puis git tag v${version} && git push origin v${version}`);
