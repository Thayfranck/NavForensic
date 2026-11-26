# Changelog — NavForensics

Toutes les modifications notables de NavForensics sont consignées ici.
Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) ;
le versionnement suit [SemVer](https://semver.org/lang/fr/) à partir de 2.x.

## [2.8.0]

### Ajouté
- **Éléments d'identité par catégories dépliantes** dans la colonne Résultats :
  moyens de paiement, téléphones, emails, adresses, identité civile, sécurité
  sociale, véhicules, et « Autres » pour les types non reconnus.
- Chaque catégorie se déplie sur ses types réels, triés par nombre décroissant,
  avec le compte entre parenthèses. Un clic sur une catégorie ou un type filtre la vue.

## [2.7.1]

### Ajouté
- **Compteurs entre parenthèses** sur chaque entrée de la colonne Résultats
  (identité, téléchargements, géolocalisations, VPN, récupérés, crypto, mots de
  passe Firefox, IOC, comptes synchronisés, extensions, points géolocalisés,
  Pattern of life). Calcul en un seul appel moteur (`results_counts`).

### Corrigé
- Les marquages faits par lot (menu contextuel sur plusieurs lignes) ne
  enregistraient pas leur origine : ils n'apparaissaient donc pas dans la vue
  Recherches / mots-clés.
- Les marquages existants sans origine sont rattachés à la recherche globale ou
  aux mots-clés d'après le titre de la vue où ils ont été posés.

## [2.7.0]

### Modifié
- La vue Résultats « Recherches / mots-clés » n'affiche plus que les éléments
  marqués issus de la recherche par mots-clés et de la recherche globale.
- Le libellé indique le nombre d'éléments tagués et le nombre trouvés
  (`(X tagués · Y trouvés)`).
- Chaque marquage enregistre son origine (`kw`, `search`).
- L'export direct de cette vue est désactivé, comme pour « Éléments marqués » :
  utiliser le rapport HTML ou PDF.

## [2.6.1]

### Corrigé
- Suppression de l'avertissement `field tag is never read` dans le module
  `firefox_login` (champ inutilisé dans la structure `Tlv`).

## [2.6.0]

### Ajouté
- **Déchiffrement des identifiants Firefox** (`engine/src/firefox_login.rs`) :
  lecture de `key4.db` (table `metaData` et `nssPrivate`), prise en charge du
  mot de passe maître, des schémas 3DES-CBC et AES-256-CBC via PBES2/PBKDF2,
  puis déchiffrement des champs de `logins.json`. Lecture seule sur copie.
- Entrée « 🔓 Mots de passe Firefox » dans Résultats, avec invite pour le mot de
  passe maître et choix du dossier de profil.
- Tests unitaires : vecteur PBKDF2-HMAC-SHA256, allers-retours 3DES et AES-256,
  parsing DER (formes courte et longue), retrait du padding PKCS#7.

## [2.5.0]

### Ajouté
- **Catalogue des chemins de profils** dans le panneau d'aide : tableau bureau
  (Windows XP à 11 26H2, macOS, Linux) et tableau mobile (iOS 14 à 27,
  Android 12 à 16).
- Nommage élargi des navigateurs par signature de chemin : Samsung Internet,
  UC Browser, Arc, Yandex, Opera GX, paquets Android (`com.*`) et conteneurs iOS.
- Extraction E01 : Yandex, et profils Opera / Opera GX sous `Roaming`.

### Corrigé
- Ascenseur du tableau : lignes à hauteur fixe de 24 px, pour que la hauteur
  réelle corresponde à l'estimation du virtualiseur (fin de la dérive).
- Résultats mots-clés : chaque terme recherché est mémorisé, et une ligne
  « (aucun résultat) » s'affiche pour les termes sans correspondance.

## [2.4.3]

### Ajouté
- Frise de sélection : déplacement de la fenêtre au clic droit (curseur main),
  infobulles au survol (date, nombre, détail par série), et bascule en barres
  verticales quand la fenêtre passe sous 3 jours.
- Colonnes du tableau étirées pour remplir la largeur de la fenêtre, pour tous
  les onglets.

### Modifié
- Dates affichées au format `dd-mm-yyyy` au-dessus de la frise.

## [2.4.2]

### Ajouté
- Frise interactive disponible sur **toutes les vues datées** (historiques,
  visites, termes recherchés, téléchargements, résultats agrégés), pas seulement
  sur la Chronologie. La sélection filtre le tableau et se réinitialise au
  changement de vue.

## [2.4.1]

### Ajouté
- Frise interactive de la Chronologie : courbes multi-séries façon Google Trends,
  axe de dates qui s'adapte au zoom (années, mois, jours, heures), zoom à la
  molette et au double-clic.
- Carte Leaflet intégrée dans l'application pour les points géolocalisés, et
  liens OpenStreetMap cliquables (ouverture dans le navigateur système).
- Colonne de navigation gauche redimensionnable.
- Sous-catégories dépliantes par navigateur (historiques, favoris, etc.).

### Modifié
- Thème clair par défaut ; fuseau horaire par défaut « Local ».
- Pattern of life : heures au format 24 h, et prise en compte du réglage de
  fuseau (UTC / locale) de la barre d'outils.
- Récapitulatif des éléments marqués : chemin complet de la source, et date du
  fichier (plutôt que la date de marquage).

### Corrigé
- Noms de navigateurs illisibles (blanc sur blanc) en thème clair.
- Carte passant au-dessus des boîtes de dialogue.
- Tuiles OpenStreetMap refusées (403) : utilisation du domaine
  `tile.openstreetmap.org`.

## [2.3.x]

Intermédiaires 2.3.0 à 2.3.6 (détail reconstitué) :

### Ajouté
- **2.3.0** : chargement d'images E01 / brutes (partitions MBR/GPT, NTFS en lecture
  seule) et extraction des profils de navigateurs.
- **2.3.1 / 2.3.2** : Pattern of life (heatmap jour × heure) et points géolocalisés.
- **2.3.3** : colonne cachée `_chemin` pour retrouver le chemin complet d'une
  ligne issue d'une vue agrégée.
- **2.3.4** : liens OpenStreetMap « Carte (OSM) » dans les points géolocalisés.

### Corrigé
- **2.3.5** : tuiles OpenStreetMap (sous-domaines retirés).
- **2.3.6** : colonnes « Date fichier » et « Marqué le » dans le récapitulatif.

## [2.2.0]

### Ajouté
- **Surveillance IOC** : import d'une liste d'indicateurs (un par ligne),
  recherche dans les sources, vue « Correspondances IOC », section dans le rapport.
- **Comptes synchronisés** et **extensions installées** (profils Chromium et
  Firefox), avec sections dans le rapport.

## [2.1.1]

### Ajouté
- **Carving SQLite** : récupération des enregistrements supprimés (freelist,
  blocs libres, zone non allouée, anciennes versions du `-wal`).
- Vue **Recherches / mots-clés** persistante : les résultats des recherches par
  listes de mots-clés sont accumulés pour la session.
- Section du rapport pour les mots-clés marqués.

### Corrigé
- Les résultats de recherche par mots-clés affichent désormais la valeur trouvée
  et le terme recherché.

## [2.1]

### Ajouté
- **Module crypto-actifs** : adresses (BTC, ETH, LTC, Tron, Monero, XRP,
  Dogecoin, BCH), plateformes d'échange et phrases mnémoniques possibles.
- Mode d'emploi et README mis à jour pour cette version.

## Antérieur à 2.1

Export PDF du rapport avec empreinte SHA-256 par fichier produit ; rapport HTML
avec sections sélectionnables ; lecture en lecture seule sur copie.
