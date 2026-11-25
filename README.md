# NavForensics (Tauri + React)

Réécriture de NavForensics en **Rust (Tauri 2)** + **React/TypeScript** : lecture
forensique des bases de navigateurs (Chromium, Firefox, Safari) extraites via
X-Ways, avec timeline unifiée, regroupement par domaine, recherche inter-sources,
exports CSV/XLSX et rapport HTML.

## Architecture

```
navforensics-tauri/
├── engine/            Coeur Rust (crate pur, testable) — toute la logique d'analyse
│   ├── src/
│   │   ├── lib.rs        detection, load_artifact, scan_folder, sha256
│   │   ├── model.rs      Cell / Row / Category / Artifact (serialisables)
│   │   ├── time.rs       conversions WebKit / PRTime / CFAbsolute / FILETIME
│   │   ├── db.rs         ouverture SQLite lecture seule sur copie (-wal/-shm)
│   │   ├── parsers.rs    Chromium / Firefox / Safari + fichiers JSON/plist
│   │   ├── sessions.rs   SNSS (carving) + sessionstore mozLz4
│   │   ├── views.rs      timeline, domaines, recherche, carving de cache
│   │   └── report.rs     export CSV / XLSX / rapport HTML
│   └── tests/            tests d'integration (bases synthetiques)
├── src-tauri/         Coquille Tauri : commandes exposant le coeur au front
│   ├── src/main.rs
│   ├── tauri.conf.json
│   └── capabilities/default.json
└── src/               Interface React + TypeScript
    ├── api.ts            typage + appels invoke()
    └── App.tsx           navigation, table virtualisee, tri/filtre, themes, exports
```

Le **cœur Rust est indépendant de Tauri** : `cargo test -p navforensics-engine`
valide toute la logique (conversions, parsing, timeline, domaines, recherche)
sans dépendance graphique.

## Principes forensiques (identiques à la version Python)

- Lecture **seule sur copie**, avec les sidecars `-wal` / `-shm`.
- **SHA-256** de chaque source.
- Horodatages convertis par navigateur ; valeurs brutes conservées (détail).
- **Aucun secret déchiffré** (mots de passe, cookies chiffrés) — métadonnées seules.

## Installation rapide (scripts)

Des scripts automatisent la vérification/installation de Rust et Node, les
dépendances système, `npm install`, la génération des icônes puis le build.

Windows (PowerShell) :

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1          # installe + construit l'installeur .msi
powershell -ExecutionPolicy Bypass -File install.ps1 -Action dev   # installe + lance en dev
```

Linux / macOS :

```bash
chmod +x install.sh
./install.sh build     # installe + construit
./install.sh           # installe + lance en dev
```

Un logo par défaut (`assets/logo.png`) est fourni : les scripts génèrent
automatiquement les icônes de l'application. Remplacez-le par le vôtre au besoin.

## Prérequis de build

- **Rust** (stable) : https://rustup.rs
- **Node.js 18+** et npm
- **Tauri 2** (prérequis système) :
  - Windows : *Microsoft Edge WebView2* (préinstallé sur Win 11) + *Visual Studio
    Build Tools* (C++).
  - Linux : `webkit2gtk`, `libayatana-appindicator`, `librsvg` (voir doc Tauri).
  - macOS : Xcode Command Line Tools.
- CLI Tauri : fournie en devDependency (`npm run tauri …`).

## Lancer en développement

```bash
npm install
npm run tauri dev
```

## Construire l'exécutable / l'installeur

1. Générer les icônes (une fois) à partir d'un logo PNG :

```bash
npx tauri icon chemin/vers/logo.png
```

2. Construire :

```bash
npm run tauri build
```

Les binaires et installeurs sont produits dans
`src-tauri/target/release/` et `src-tauri/target/release/bundle/`
(`.msi` / `.exe` NSIS sous Windows).

> Un `.msi` **signé** (certificat de code) évite les faux positifs antivirus.

## Tester le cœur Rust

```bash
cargo test -p navforensics-engine
```

## Fonctionnalités

- Chromium : History (historique, visites, téléchargements, recherches), Cookies,
  Web Data (autofill), Login Data (métadonnées), Bookmarks, Shortcuts, Top Sites,
  Favicons, Preferences, Session (SNSS).
- Firefox : places, cookies, formhistory, logins.json, favicons, sessionstore.
- Safari : History.db, Downloads.plist, Bookmarks.plist.
- Vues : **Timeline unifiée**, **Regroupement par domaine**, **Recherche globale**.
- Table **virtualisée** (TanStack) : tri par colonne, filtre plein texte, détail
  par double-clic ; 3 thèmes (sombre / clair / contraste) ; fuseau UTC / local.
- Exports : CSV, XLSX, rapport HTML (en-tête de dossier, empreintes, timeline,
  domaines, détail par source).

## Limite connue

- **WebCacheV01.dat (IE / Edge Legacy, base ESE)** : détecté, mais le parseur ESE
  n'est pas encore porté en Rust (le chargement renvoie un message explicite).
  À ajouter via un crate ESE (ex. `ese_parser`) ou des bindings `libesedb`, puis
  valider sur un échantillon réel. La version Python couvre déjà ce format.
"# NavForensic" 
