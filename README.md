# NavForensics 2.4.1 (Tauri + React)

NavForensics est une application de bureau forensique écrite en Rust (Tauri 2) + React/TypeScript. Elle lit **en lecture seule** les bases de données des navigateurs extraites via X-Ways (Chromium, Firefox, Safari et Internet Explorer / Edge Legacy), en tire des vues d'analyse (chronologie unifiée, domaines, identité, géolocalisations, téléchargements, VPN, crypto-actifs, etc.), **récupère les enregistrements supprimés** par carving SQLite, et génère un rapport horodaté — **HTML ou PDF** — avec empreinte **SHA-256** pour chaque export.

## Architecture

```text
navforensics-tauri/
├── engine/            Cœur Rust (crate pur, testable) — toute la logique d'analyse
│   ├── src/
│   │   ├── lib.rs        detection, load_artifact, scan_folder, sha256, browser_version_install
│   │   ├── model.rs      Cell / Row / Category / Artifact (sérialisables)
│   │   ├── time.rs       conversions WebKit / PRTime / CFAbsolute / FILETIME
│   │   ├── db.rs         ouverture SQLite lecture seule sur copie (-wal/-shm)
│   │   ├── ese.rs        parseur ESE/EDB pur Rust (WebCacheV01.dat, IE/Edge Legacy)
│   │   ├── carve.rs      carving SQLite : récupération des enregistrements supprimés
│   │   ├── crypto.rs     extraction des crypto-actifs (adresses, plateformes, seed)
│   │   ├── parsers.rs    Chromium / Firefox / Safari + fichiers JSON/plist
│   │   ├── sessions.rs   SNSS (carving) + sessionstore mozLz4
│   │   ├── views.rs      chronologie, domaines, recherche, stats, histogramme,
│   │   │                 termes recherchés, cookies GA, services en ligne,
│   │   │                 identité, téléchargements, géolocalisations, VPN, récupérés
│   │   ├── taxonomy.rs   liens signalés (catégorisation automatique)
│   │   ├── report.rs     export CSV / XLSX / rapport HTML
│   │   └── pdf.rs        export du rapport en PDF (printpdf)
│   └── tests/            tests d'intégration (bases synthétiques, ESE, carving, crypto)
├── src-tauri/         Coquille Tauri : commandes exposant le cœur au front
│   ├── src/main.rs
│   ├── tauri.conf.json
│   └── capabilities/default.json
└── src/               Interface React + TypeScript
    ├── api.ts            typage + appels invoke()
    └── App.tsx           navigation, table virtualisée, tri/filtre, thèmes, exports
```

Le cœur Rust est indépendant de Tauri : `cargo test -p navforensics-engine` valide toute la logique (conversions, parsing, chronologie, domaines, recherche, identité, ESE, export PDF) sans dépendance graphique.

## Principes forensiques

- Lecture seule sur copie, avec les sidecars `-wal` / `-shm`.
- SHA-256 de chaque source **et de chaque fichier exporté** (CSV, XLSX, rapport HTML, rapport PDF).
- Horodatages convertis par navigateur ; valeurs brutes conservées (détail).
- Aucun secret déchiffré (mots de passe, cookies chiffrés) — métadonnées seules.

## Installation rapide (scripts)

Des scripts automatisent la vérification/installation de Rust et Node, les dépendances système, `npm install`, la génération des icônes puis le build.

Windows (PowerShell) :

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
powershell -ExecutionPolicy Bypass -File install.ps1 -Action dev
```

Linux / macOS :

```bash
chmod +x install.sh
./install.sh build
./install.sh
```

Un logo par défaut (`assets/logo.png`) est fourni ; les scripts génèrent automatiquement les icônes de l'application. Remplacez-le par le vôtre au besoin.

## Prérequis de build

- Rust (stable) : https://rustup.rs
- Node.js 18+ et npm
- Tauri 2 (prérequis système) :
  - Windows : Microsoft Edge WebView2 (préinstallé sur Win 11) + Visual Studio Build Tools (C++).
  - Linux : webkit2gtk, libayatana-appindicator, librsvg (voir doc Tauri).
  - macOS : Xcode Command Line Tools.
- CLI Tauri : fournie en devDependency (`npm run tauri …`) ou via `cargo tauri`.

## Lancer en développement

```powershell
npm install
npm run tauri dev
```

## Construire l'exécutable / l'installeur

Générer les icônes (une fois) à partir d'un logo PNG :

```powershell
npx tauri icon chemin/vers/logo.png
```

Construire :

```powershell
npm run tauri build
```

ou, pour l'installeur Windows seul :

```powershell
cargo tauri build --bundles msi
```

Les binaires et installeurs sont produits dans `src-tauri/target/release/` et `src-tauri/target/release/bundle/` (`.msi` / `.exe` NSIS sous Windows). Un `.msi` signé (certificat de code) évite les faux positifs antivirus.

## Tester le cœur Rust

```powershell
cargo test -p navforensics-engine
```

## Fonctionnalités

- **Chromium** : History (historique, visites, téléchargements, recherches), Cookies, Web Data (autofill), Login Data (métadonnées), Bookmarks, Shortcuts, Top Sites, Favicons, Preferences, Session (SNSS).
- **Firefox** : places, cookies, formhistory, logins.json, favicons, sessionstore.
- **Safari** : History.db, Downloads.plist, Bookmarks.plist.
- **Internet Explorer / Edge Legacy** : `WebCacheV01.dat` (base ESE/EDB) lue par un parseur Rust pur — historique, cache, cookies, stockage DOM, avec le compte Windows isolé dans une colonne dédiée.
- **Vues de synthèse** : chronologie unifiée, regroupement par domaine, statistiques, histogramme horaire, termes recherchés, cookies Google Analytics, services en ligne, liens signalés.
- **Groupe Résultats** : Identité (reconstituée depuis l'autofill, avec validation Luhn/NIR/IBAN/plaque/téléphone), Téléchargements, Géolocalisations (services de cartographie + adresses), VPN détectés, Récupérés (supprimés) et Crypto-actifs.
- **Recherche par mots-clés** : recherche libre et listes thématiques éditables ; tous les résultats de la session sont conservés dans la vue Résultats « Recherches / mots-clés ». Marquer un résultat en retient l'intégralité (mot-clé, liste, colonne, valeur trouvée), et le rapport comporte une section listant les mots-clés recherchés à réponse positive qui ont été marqués.
- **Liste de surveillance (IOC)** : import d'une liste d'indicateurs (domaines, URLs, emails, adresses crypto, numéros, mots-clés ; un par ligne, type déduit ou imposé par `valeur | type`), recherche automatique des correspondances dans toutes les sources, vue Résultats « Correspondances IOC » et section dédiée dans le rapport.
- **Comptes synchronisés & extensions** : détection du compte de synchronisation lié au profil (Chromium `account_info`, Firefox Sync) et liste des extensions installées avec leurs permissions (dossier `Extensions/` + manifestes, `extensions.json` Firefox).
- **Images E01 / brutes** : extraction directe des artefacts de navigateurs depuis une image forensique E01 (EWF, via la crate `ewf`) ou brute (`.dd`/`.001`/`.raw`), par lecture seule du volume NTFS (crate `ntfs-core`) et parcours des profils (Chromium, Firefox, IE/Edge Legacy) sous `Users/<user>/AppData`.
- **Carte des géolocalisations** : extraction des coordonnées GPS des URLs de cartographie (vue « Points géolocalisés »), avec carte interactive Leaflet/OpenStreetMap dans le rapport.
- **Pattern of life** : heatmap jour × heure de l'activité pour dégager les habitudes.
- **Récupération des enregistrements supprimés** : carving de l'espace libre SQLite (freelist, blocs libres intra-page, zone non allouée) et des versions anciennes de pages du journal `-wal`, pour reconstituer l'historique, les téléchargements et cookies effacés (URL, titre, horodatage quand reconstituables).
- **Crypto-actifs** : extraction des adresses (Bitcoin legacy/P2SH/bech32, Ethereum/EVM, Litecoin, Tron, Monero, XRP, Dogecoin, Bitcoin Cash), détection des plateformes fréquentées (exchanges, wallets web, explorateurs de blocs, mixeurs) et repérage de phrases mnémoniques (seed) possibles dans les champs de formulaire.
- **Navigation** : arborescence repliable des navigateurs trouvés (badges de couleur), table virtualisée (TanStack) avec tri par colonne et filtre plein texte, détail par ligne, marquage des éléments probants, thèmes (sombre / clair / contraste), fuseau UTC / local.
- **Sources** : version et date d'installation du navigateur déduites du profil ; forçage manuel du navigateur attribué à un scellé.
- **Exports** : menu groupé CSV / XLSX / rapport. Rapport **HTML** ou **PDF** (en-tête de dossier, tableau des sources avec version/installation/SHA-256, sections sélectionnables dont récupérés et crypto-actifs, éléments d'identité retenus). Empreinte SHA-256 calculée pour chaque fichier produit.

> **Note sur les éléments déduits.** Le carving ne récupère qu'une fraction du supprimé (pages recyclées et en-têtes écrasés définitivement perdus — limite commune à tous les outils). Les adresses crypto sont validées par motif, non confrontées à la chaîne ; les phrases mnémoniques sont signalées par heuristique. Tous ces éléments sont des indices à recouper avec les artefacts bruts.

## Avertissement d'usage

Outil destiné à un cadre d'investigation judiciaire. Travaillez sur une copie de travail du scellé, documentez les empreintes SHA-256 dans la procédure, et recoupez les éléments déduits (identité, VPN, géolocalisations) avec les artefacts bruts avant toute qualification.
