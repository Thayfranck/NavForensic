# NavForensics 2.9.0 (Tauri + React)

NavForensics est une application de bureau forensique écrite en Rust (Tauri 2) + React/TypeScript. Elle lit **en lecture seule** les bases de données des navigateurs extraites via X-Ways ou d'une image E01 (Chromium, Firefox, Safari, Internet Explorer / Edge Legacy, sur bureau comme sur mobile), en tire des vues d'analyse (chronologie interactive, domaines, identité, géolocalisations, téléchargements, VPN, crypto-actifs, mots de passe Firefox, etc.), **récupère les enregistrements supprimés** par carving SQLite, et génère un rapport horodaté — **HTML ou PDF** — avec empreinte **SHA-256** pour chaque export.

Historique détaillé des versions : voir [CHANGELOG.md](CHANGELOG.md).

## Architecture

```text
navforensics-tauri/
├── engine/            Cœur Rust (crate pur, testable) — toute la logique d'analyse
│   ├── src/
│   │   ├── lib.rs           détection, load_artifact, scan_folder, nommage des navigateurs
│   │   ├── model.rs         Cell / Row / Category / Artifact (sérialisables)
│   │   ├── time.rs          conversions WebKit / PRTime / CFAbsolute / FILETIME
│   │   ├── db.rs            ouverture SQLite lecture seule sur copie (-wal/-shm)
│   │   ├── ese.rs           parseur ESE/EDB pur Rust (WebCacheV01.dat, IE/Edge Legacy)
│   │   ├── e01.rs           image E01 / brute : partitions, NTFS en lecture seule, profils
│   │   ├── carve.rs         carving SQLite : récupération des enregistrements supprimés
│   │   ├── crypto.rs        crypto-actifs (adresses, plateformes, phrases mnémoniques)
│   │   ├── firefox_login.rs déchiffrement des identifiants Firefox (NSS : key4.db, logins.json)
│   │   ├── ioc.rs           liste de surveillance (IOC) et correspondances
│   │   ├── profiles.rs      comptes synchronisés et extensions installées
│   │   ├── parsers.rs       Chromium / Firefox / Safari + fichiers JSON/plist
│   │   ├── sessions.rs      SNSS (carving) + sessionstore mozLz4
│   │   ├── views.rs         chronologie, domaines, recherche, identité, téléchargements,
│   │   │                    géolocalisations, VPN, récupérés, heatmap, points géolocalisés
│   │   ├── taxonomy.rs      liens signalés (catégorisation automatique)
│   │   ├── report.rs        export CSV / XLSX / rapport HTML
│   │   └── pdf.rs           export du rapport en PDF (printpdf)
│   └── tests/               tests d'intégration (bases synthétiques, ESE, carving, crypto)
├── src-tauri/         Coquille Tauri : commandes exposant le cœur au front
│   ├── src/main.rs
│   ├── tauri.conf.json
│   └── capabilities/default.json
└── src/               Interface React + TypeScript
    ├── api.ts             typage + appels invoke()
    └── App.tsx            navigation, frise temporelle, table virtualisée, tri/filtre,
                           carte Leaflet, thèmes, exports
```

Le cœur Rust est indépendant de Tauri : `cargo test -p navforensics-engine` valide toute la logique (conversions, parsing, chronologie, domaines, recherche, identité, ESE, carving, crypto, déchiffrement Firefox, export PDF) sans dépendance graphique.

## Principes forensiques

- Lecture seule sur copie, avec les sidecars `-wal` / `-shm`.
- SHA-256 de chaque source **et de chaque fichier exporté** (CSV, XLSX, rapport HTML, rapport PDF).
- Horodatages convertis par navigateur ; valeurs brutes conservées (détail).
- **Déchiffrement limité aux identifiants Firefox** : seul le module « Mots de passe Firefox » déchiffre des secrets. Il ne s'applique qu'à la demande, sur une copie, avec le mot de passe maître fourni par l'opérateur (vide si le profil n'en a pas). Les autres secrets (cookies chiffrés, mots de passe Chromium) ne sont pas déchiffrés.

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
- Accès à crates.io au premier build (nouvelles dépendances de la version 2.6 : `aes`, `des`, `cbc`, `pbkdf2`, `hmac`, `sha1`, `base64`).

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

### Navigateurs et artefacts

- **Chromium** : History (historique, visites, téléchargements, recherches), Cookies, Web Data (autofill), Login Data (métadonnées), Bookmarks, Shortcuts, Top Sites, Favicons, Preferences, Session (SNSS).
- **Navigateurs reconnus par signature de chemin** : Google Chrome, Microsoft Edge, Brave, Opera / Opera GX, Vivaldi, Yandex, Arc, Chromium, Samsung Internet, UC Browser, y compris les paquets Android (`com.*`) et les conteneurs iOS.
- **Firefox** : places, cookies, formhistory, logins.json, favicons, sessionstore.
- **Safari** : History.db, Downloads.plist, Bookmarks.plist (macOS et iOS).
- **Internet Explorer / Edge Legacy** : `WebCacheV01.dat` (base ESE/EDB) lue par un parseur Rust pur — historique, cache, cookies, stockage DOM, avec le compte Windows isolé dans une colonne dédiée.
- **Mots de passe Firefox** : déchiffrement des identifiants enregistrés (`key4.db` + `logins.json`), schémas 3DES-CBC et AES-256-CBC / PBES2, mot de passe maître optionnel. Lecture seule sur copie.

### Images E01 / brutes

Extraction directe des artefacts de navigateurs depuis une image forensique E01 (EWF, crate `ewf`) ou brute (`.dd`/`.001`/`.raw`) : partitions MBR/GPT, volume NTFS en lecture seule (crate `ntfs-core`), parcours des profils sous `Users/<user>/AppData` (Chromium et variantes Yandex/Opera, Firefox, IE/Edge Legacy).

Limites : une image E01 est lue pour ses volumes NTFS (Windows) et ext4 (Android, données applicatives). Les volumes **APFS** (iOS, macOS) et **F2FS** (Android récent) sont détectés mais ne sont pas lus : pour ces artefacts, extraire le système de fichiers (Cellebrite FFS, GrayKey, checkm8…) puis l'ouvrir avec « Dossier ». Le panneau d'aide liste les chemins à extraire.

### Chemins de profils (aide intégrée)

Le panneau d'aide donne le référentiel des chemins de profils :
- **Bureau** : Windows Vista à 11 (26H2) avec la conversion pour Windows XP, macOS et Linux, pour les navigateurs usuels ;
- **Mobile** : iOS 14 à 27 (conteneurs d'application, identifiants de bundle) et Android 12 à 16 (paquets sous `/data/data/`).

### Vues de synthèse

- **Chronologie interactive** : frise de sélection sous les courbes multi-séries (type d'évènement), zoom à la molette ou au double-clic, sélection à la souris, déplacement au clic droit, infobulles au survol (date, nombre, détail par série), passage aux barres verticales sous 3 jours de fenêtre, axe de dates qui s'adapte au zoom. La frise est disponible sur toutes les vues datées (historiques, visites, termes recherchés, téléchargements…) et filtre le tableau.
- Regroupement par domaine, statistiques, histogramme horaire, termes recherchés, cookies Google Analytics, services en ligne, liens signalés.
- **Pattern of life** : heatmap jour × heure (24 h), selon le fuseau de la barre d'outils (UTC / locale).

### Colonne Résultats

Chaque entrée affiche son nombre d'éléments entre parenthèses.

- **Éléments d'identité** : reconstitués depuis l'autofill et les formulaires, avec validation (Luhn pour les cartes, NIR, IBAN, plaque, téléphone). Ils sont regroupés en catégories dépliantes — moyens de paiement, téléphones, emails, adresses, identité civile, sécurité sociale, véhicules, autres — et chaque catégorie se déplie sur ses types, avec leur nombre.
- **Recherches / mots-clés** : n'affiche que les **éléments marqués** issus de la recherche par mots-clés et de la recherche globale. Le libellé indique le nombre d'éléments tagués et le nombre trouvés. Marquer un résultat en retient l'intégralité (mot-clé, liste, colonne, valeur trouvée) ; le rapport comporte une section listant les mots-clés à réponse positive marqués.
- **Mots de passe Firefox** : identifiants déchiffrés (URL, identifiant, mot de passe, source), marquables et exportables.
- **Liste de surveillance (IOC)** : import d'une liste d'indicateurs (domaines, URLs, emails, adresses crypto, numéros, mots-clés ; un par ligne, type déduit ou imposé par `valeur | type`), recherche automatique dans toutes les sources, vue « Correspondances IOC » et section dédiée dans le rapport.
- **Comptes synchronisés & extensions** : compte de synchronisation lié au profil (Chromium `account_info`, Firefox Sync) et extensions installées avec leurs permissions (`Extensions/` + manifestes, `extensions.json` Firefox).
- **Téléchargements**, **Géolocalisations** (services de cartographie + adresses), **VPN détectés**, **Récupérés (supprimés)**, **Crypto-actifs**, **Points géolocalisés** (coordonnées extraites des URLs de cartographie, avec carte Leaflet/OpenStreetMap intégrée ; les liens OSM s'ouvrent dans le navigateur).
- **Éléments marqués** : récapitulatif de tous les marquages, avec chemin complet de la source et date du fichier.

### Navigation et interface

- Arborescence repliable des navigateurs trouvés (badges de couleur), sous-catégories dépliantes (historiques, favoris…).
- Colonne de navigation redimensionnable à la souris.
- Table virtualisée avec tri par colonne, filtre plein texte, détail par ligne, marquage des éléments probants, colonnes qui s'adaptent à la largeur de la fenêtre.
- Thèmes sombre / clair / contraste ; fuseau UTC ou local (local par défaut).

### Recherche par mots-clés

Recherche libre et listes thématiques éditables. Les listes sont conservées dans le navigateur de l'application.

### Récupération des enregistrements supprimés

Carving de l'espace libre SQLite (freelist, blocs libres intra-page, zone non allouée) et des versions anciennes de pages du journal `-wal`, pour reconstituer l'historique, les téléchargements et cookies effacés (URL, titre, horodatage quand reconstituables).

### Crypto-actifs

Extraction des adresses (Bitcoin legacy/P2SH/bech32, Ethereum/EVM, Litecoin, Tron, Monero, XRP, Dogecoin, Bitcoin Cash), détection des plateformes fréquentées (exchanges, wallets web, explorateurs de blocs, mixeurs) et repérage de phrases mnémoniques (seed) possibles dans les champs de formulaire.

### Sources et exports

- Version et date d'installation du navigateur déduites du profil ; forçage manuel du navigateur attribué à un scellé.
- Exports : menu groupé CSV / XLSX / rapport. Rapport **HTML** ou **PDF** (en-tête de dossier, tableau des sources avec version/installation/SHA-256, sections sélectionnables — dont récupérés, crypto-actifs, IOC, comptes et extensions, pattern of life, carte — et éléments d'identité retenus). Empreinte SHA-256 calculée pour chaque fichier produit.

> **Note sur les éléments déduits.** Le carving ne récupère qu'une fraction du supprimé (pages recyclées et en-têtes écrasés définitivement perdus — limite commune à tous les outils). Les adresses crypto sont validées par motif, non confrontées à la chaîne ; les phrases mnémoniques sont signalées par heuristique. Les éléments d'identité sont reconstitués par motif et validation. Tous ces éléments sont des indices à recouper avec les artefacts bruts.

> **Note sur le déchiffrement Firefox.** Le déchiffrement suit l'algorithme NSS de référence. Il doit être vérifié sur un profil réel avant usage probatoire.

## Avertissement d'usage

Outil destiné à un cadre d'investigation judiciaire. Travaillez sur une copie de travail du scellé, documentez les empreintes SHA-256 dans la procédure, et recoupez les éléments déduits (identité, VPN, géolocalisations, mots de passe déchiffrés) avec les artefacts bruts avant toute qualification.
