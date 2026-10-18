# ssho

**ssho** (SSH Organizer) est un gestionnaire de connexions SSH pour le terminal, écrit en Rust. Les hôtes restent dans `~/.ssh/config` et ses fichiers `Include`.

Le projet s'est fortement inspiré de [SSHM](https://github.com/Gu1llaum-3/sshm) à ses débuts : j'utilise cet outil au quotidien, mais il lui manquait de nombreuses fonctionnalités, ce qui m'a poussé à en écrire une version en Rust et à la faire évoluer. Les données d'une installation [SSHM](https://github.com/Gu1llaum-3/sshm) existante sont reprises au premier lancement.

Le paquet et l'exécutable s'appellent **`ssho`**.

## Compilation et lancement

Prérequis de compilation : Rust 1.88 ou plus récent, un compilateur C, les outils de compilation usuels et Perl pour OpenSSL embarqué. Les bibliothèques de cryptographie et de compression du contrôle de connectivité sont compilées avec le programme. Le client `ssh` d'OpenSSH doit être accessible dans le `PATH` pour les connexions, les tunnels et la résolution effective de la configuration.

```sh
cargo build --release
./target/release/ssho
```

Installation facultative dans le répertoire Cargo de l'utilisateur :

```sh
cargo install --path . --locked
ssho
```

## Installation depuis un binaire

`install.sh` installe un binaire compilé localement ou une archive de release,
sans privilège :

```sh
sh install.sh --binary ./target/release/ssho
sh install.sh --archive ./ssho-x86_64-unknown-linux-gnu.tar.gz --version 0.1.0
sh install.sh --uninstall
```

Destination : `--bin-dir`, puis `--prefix` + `/bin`, puis `SSHO_INSTALL_DIR`,
sinon `$HOME/.local/bin`. Un remplacement exige `--replace`. Aucun privilège ni
changement de profil ; la désinstallation conserve SSH, clés et données.
Le [guide d'installation](docs/INSTALLATION.md) décrit les checksums, le packaging,
le PATH, le dépôt de téléchargement et les limites Linux/macOS.

## Configuration et premier lancement

Les hôtes restent dans `~/.ssh/config`, avec leurs éventuels fichiers `Include`. L'option `-c` sélectionne une autre configuration SSH.

Les préférences, l'historique et les sauvegardes propres au programme sont dans `~/.config/ssho/`. À la première commande qui utilise ces données :

- Si `ssho/` n'existe pas et que `sshm/` existe, son contenu est copié dans `ssho/`, y compris `config.json`, `sshm_history.json` et `backups/`.
- Si `ssho/` existe déjà, même vide, aucun import ni aucune fusion ne sont effectués.
- Si aucun ancien dossier n'existe, les valeurs par défaut sont utilisées.

La source `sshm/` reste intacte. La copie est préparée à part pour qu'un échec permette de réessayer au lancement suivant. Les liens symboliques de la source sont copiés sous forme de contenu indépendant ; les liens cassés ou cycliques provoquent une erreur sans publier une copie partielle. Le dossier de destination doit être un véritable dossier.

`XDG_CONFIG_HOME` remplace `~/.config` sous Linux/macOS. Sous Windows, la racine est `%APPDATA%`, avec repli sur `%USERPROFILE%/.config`. L'aide, la version et la génération des scripts de complétion ne créent pas de dossier applicatif.

Exemple de préférences, dans `ssho/config.json` :

```json
{
  "check_for_updates": false,
  "focus_search_on_start": true,
  "ping_on_start": false,
  "availability_method": "ssh",
  "ssh_command": ["ssh"],
  "group_aliases": false,
  "group_mode": "flat",
  "key_bindings": {
    "quit_keys": ["q", "ctrl+c"],
    "disable_esc_quit": false,
    "prefix_key": "ctrl+g",
    "shortcuts": {}
  }
}
```

`ssh_command` est un tableau argv configurable uniquement dans ce fichier JSON.
Un champ absent ou `null` conserve `["ssh"]`. Exemple avec un wrapper local :

```json
{"ssh_command": ["/usr/local/bin/my-wrapper", "--profile", "work"]}
```

Le premier élément est l’exécutable ; les suivants précèdent les arguments SSH
(`-F`, chemin de configuration, éventuel `-t`, alias, puis commande distante).
Chaque élément reste un argument littéral : aucun shell, expansion, découpage
sur les espaces ou interprétation de métacaractères n’est ajouté. Le tableau
doit contenir un exécutable non blanc et aucun argument ne peut contenir NUL.
Un argument supplémentaire vide est permis. Le wrapper doit accepter les
arguments SSH et conserver la session au premier plan pour que retour, flux,
codes de sortie et signaux décrivent la connexion. Cet exemple ne certifie
aucun wrapper tiers.

Ce réglage concerne les connexions et commandes distantes individuelles sans forwarding.
Les lots, tout forwarding, y compris avec shell, les tunnels possédés, `ssh -G`, les
prévalidations de copie et les sondes via proxy utilisent toujours `ssh` du
`PATH`. Les sondes directes gardent leur négociation SSH native. Une erreur de
lancement indique l’exécutable configuré et est conservée dans l’historique.

## Utilisation

```sh
ssho                            # Interface terminal
ssho --query 'tag:prod AND user:deploy'
ssho --query ''                  # Recherche vide, focus forcé
ssho production                 # Connexion directe
ssho production uptime          # Commande distante et code de sortie SSH
ssho -t production sudo -v       # Allocation TTY
ssho -c /path/to/ssh_config
ssho add [nom]
ssho edit production
ssho move production
ssho search work --tags
ssho search prod --names --format json
ssho search --format simple
ssho info production --pretty
ssho --no-update-check
```

`--query TEXT` ou `--query=TEXT` ouvre la liste interactive avec la recherche avancée déjà saisie et le curseur actif, même si `focus_search_on_start` est désactivé ou `--search=false` est fourni. La chaîne vide et Unicode sont acceptées ; les caractères de contrôle et séparateurs de ligne sont refusés avant l’initialisation applicative. Une syntaxe de recherche incorrecte ou aucun résultat reste éditable. Cette option appartient à la racine et refuse tout alias direct ou sous-commande ; `--search` garde son rôle booléen historique. Après le début d’une commande distante, `--query` reste un argument littéral de cette commande.

La recherche initiale utilise les notes/favoris chargés et les valeurs effectives natives dès leur arrivée. Elle conserve le navigateur habituel : F2, F5, groupes, colonnes, méthode de disponibilité et retour après SSH. Elle ne déclenche aucune connexion interactive ni écriture d’historique avant un choix explicite, même avec un seul résultat. Les résolutions `ssh -G` et diagnostics suivent les préférences habituelles. Une configuration vide ouvre une liste éditable avec F2 ; le lancement sans `--query` garde sa proposition historique d’ajouter un hôte. La complétion consomme le texte de `--query` et ne propose ensuite ni alias ni sous-commande incompatible ; l’invocation Bash relative reste prise en charge.

`info` conserve le schéma JSON `sshm.info.v1` et les valeurs déclarées (avec leurs valeurs nulles) dans `declared`, les champs historiques et `source`. `target` utilise désormais les valeurs effectives résolues par `ssh -G -F <config> <alias>` ; `effective` contient notamment toutes les identités et les directives répétées. `resolution` distingue réussite et erreur. Codes de sortie : 0 pour une résolution réussie, 2 pour un hôte absent, 1 pour une erreur de configuration ou de résolution. Une erreur de résolution conserve les valeurs déclarées, avec `effective: null` et `RESOLUTION_ERROR`. Cette commande ne migre ni ne crée les données applicatives. La recherche CLI propose les formats `table`, `json` et `simple`.

Une connexion ou un tunnel avec shell lancé depuis l'interface rend le terminal à SSH, puis revient à la liste avec le résultat (code de sortie ou erreur de lancement). La recherche, le focus, les filtres, la sélection et le défilement sont conservés. L'historique est actualisé sans réordonner la liste affichée ; une nouvelle action de tri ou de recherche utilise les dates actualisées. Ctrl+C pendant SSH interrompt la connexion et revient à la liste ; Ctrl+C dans la liste quitte l'application. Une connexion CLI directe conserve stdout, stderr et le code de sortie SSH (128 + signal si le processus est interrompu), sans ouvrir l'interface.

L'historique enregistre les tentatives de connexion, avec date, compteur et dernier tunnel utilisé. `last_result` conserve séparément la date de fin, le code de sortie SSH ou une erreur de lancement ; `last_successful_completion` est la dernière fin de processus avec code 0. Un code de commande distante, même non nul, ne permet pas de conclure à un échec d’authentification, et un tunnel interrompu ne devient jamais un succès supposé. Les anciens fichiers et leurs champs inconnus sont conservés. Les modifications SSH sont précédées d'une sauvegarde dans `ssho/backups/`. Les blocs sans rapport, commentaires, options répétées et clés multiples sont conservés lors de l'édition. Les déplacements préparent les deux écritures et restaurent la destination si l'écriture de la source échoue.


La fiche et le panneau de détails partagent les valeurs effectives (adresse, utilisateur, port, toutes les identités, bastion, commande et TTY) et indiquent séparément la déclaration brute et son fichier/ligne. L’édition conserve toujours la déclaration. La résolution de tous les alias est mise en cache en arrière-plan, même avec `ping_on_start: false` ; les états Pending/Error restent explicites. Le cache est renouvelé après une modification et avec `p` (qui relance aussi les sondes). Une modification externe exige ce rafraîchissement ou une réouverture.

Les symboles `? Unknown`, `~ Connecting`, `+ Online`, `! Offline` restent distincts sans couleurs. Détails/Info donnent durée et cause. Par défaut (`availability_method: "ssh"`, valeur absente ou `null`), une sonde directe exige une négociation SSH réussie sans authentification ; un port HTTP ouvert ne suffit pas. Une sonde via proxy utilise OpenSSH et la commande `exit`, et peut échouer à l’authentification : sa cause est affichée. Les sous-processus du diagnostic SSH ont une limite de cinq secondes. Après capture dans des fichiers temporaires, seuls 256 Kio par flux sont lus en mémoire ; ces fichiers peuvent grossir pendant la fenêtre de diagnostic, sans limite de taille disque imposée. Sous Unix, les descendants du groupe de diagnostic sont arrêtés à la fin du processus, réussie ou non, ainsi qu’à expiration du délai ; aucun shell n’est ajouté autour de `ssh -G` (les directives OpenSSH `Match exec`/`ProxyCommand` conservent leur comportement natif).

**Colonnes** : `v` depuis le tableau ou F2 → **Colonnes** ouvre un brouillon ordonné. `↑/↓` sélectionne, Espace active/désactive une colonne facultative, `Shift+↑/↓` déplace une colonne active, `r` réinitialise au défaut, Entrée sauvegarde et Échap annule. `Name` reste obligatoire et première. Les champs disponibles sont Name, Hostname, User, Port, Tags et Last Attempt ; User/Port suivent la résolution native de l’alias actif, avec valeurs déclarées en repli et `—` si absentes. Les valeurs longues restent consultables par `i` / F4.

La préférence `host_columns` est une liste ordonnée dans `ssho/config.json`, par exemple `["name", "port", "user", "hostname"]`. Le défaut (ou `null`) est `["name", "hostname", "tags", "last_attempt"]` et conserve les largeurs historiques : Name seule jusqu’à 36 cellules de tableau, Hostname dès 37, Tags dès 65, Last Attempt dès 95. Pour une liste personnalisée, les colonnes de fin sont masquées en priorité jusqu’à tenir ; le sélecteur indique **hidden: width**. Minima : Name 12, Hostname 16, User 10, Port 5, Tags 10, Last Attempt 14 cellules, plus un espace entre colonnes ; Name seule s’adapte aux écrans plus étroits. L’espace restant est réparti entre les colonnes affichées. Les tris restent liés à Name / Last Attempt. Les groupes gardent leurs compteurs dans Name. La sauvegarde relit les préférences, préserve champs externes, cible, recherche et replis ; une méthode de disponibilité externe adoptée renouvelle ses diagnostics. Un échec conserve le brouillon et affiche l’erreur. `v` reste du texte dans la recherche. Les listes vides, doublons, noms inconnus ou Name absente/non première sont refusés explicitement.

**Disponibilité** : `P` depuis le tableau ou F2 → **Méthode de disponibilité** choisit SSH (défaut) ou **ICMP local** ; Entrée enregistre `availability_method: "ssh"` / `"icmp"` dans `ssho/config.json`, Échap annule. La sauvegarde relit les préférences et conserve les champs externes ; un échec garde le sélecteur et l’ancienne méthode. La bascule conserve recherche, sélection et groupes, remplace les diagnostics et relance les sondes en arrière-plan. Une méthode modifiée extérieurement et adoptée lors d’un choix de groupes ou d’alias renouvelle également ces diagnostics. Les filtres `is:online`/`is:offline` suivent la méthode affichée. `P` reste du texte dans la recherche.

ICMP vérifie la disponibilité réseau du **Hostname effectif obtenu par `ssh -G`, depuis cette machine** ; il ne traverse ni ProxyJump ni ProxyCommand. Un résultat positif ne démontre ni disponibilité du service SSH ni authentification réussie. La durée affichée est le temps du diagnostic (résolution comprise), **pas un RTT**. Le budget ICMP est de cinq secondes par alias, résolution et ping compris, avec huit workers fixes. L’absence de réponse déclarée par l’outil donne Offline ; outil absent, permission refusée, délai externe dépassé ou erreur donnent Unknown avec cause. Aucun sudo ni changement de capabilities. Sous Unix, stdout/stderr sont limités à 256 Kio par flux, et les processus possédés sont nettoyés après fin, expiration, changement de génération ou fermeture. La fermeture attend cette récolte ICMP ; une attente noyau ne constitue pas une garantie absolue de temps réel.

Linux utilise le contrat `ping` iputils (IPv4/IPv6), macOS `ping` ou `ping6` pour une adresse IPv6 littérale, FreeBSD le `ping` unifié. Les arguments et codes de retour des trois plateformes sont couverts par les tests. Sur macOS, un nom DNS passe par `ping` IPv4 ; `ping6` n’offre pas les mêmes options de délai et une expiration externe reste Unknown. Windows et les autres plateformes donnent Unknown : aucun succès n’est déduit du seul code 0 de `ping.exe`.

## Tags hérités par fichier

Une directive d’en-tête `# FileTags: work, lab` attribue ces tags à tous les hôtes déclarés dans ce fichier. Elle accepte la casse variable du mot `FileTags` et les espaces autour de `#`, du nom et de `:`. Les répétitions s’additionnent dans l’ordre, sans doublons exacts ; `Work` et `work` restent distincts. L’en-tête se termine au premier `Host`, `Include` ou `Match`, même sans argument. Les fichiers inclus appliquent leur propre en-tête, sans recevoir les tags du parent.

Le tableau et la fiche affichent `%work` pour un tag hérité et `#prod` pour un tag propre. La recherche TUI (mots et `tag:`), la recherche CLI et la visibilité utilisent leur union. Le tag hérité `hidden` masque aussi l’hôte ; `H` permet de le montrer dans la TUI, tandis que la complétion de `info` permet toujours de le retrouver explicitement. Ces règles se composent avec le filtre de fichier et les favoris.

Le formulaire édite uniquement les tags propres ; l’héritage de la destination apparaît séparément en lecture seule, y compris pour un fichier vide. Éditer, copier ou déplacer ne matérialise jamais les tags hérités dans `# Tags:`. Après copie ou déplacement, l’hôte reçoit l’en-tête de sa destination. Les en-têtes restent dans leur fichier lors d’une suppression ; les directives tardives sont conservées sous forme de commentaires `# Inactive FileTags: ...` lorsqu’un bloc est retiré, afin qu’elles ne deviennent jamais actives par accident. L’aperçu montre exactement ces changements avant confirmation.

Les directives tardives sont ignorées et signalées avec fichier et ligne : une seule fois sur stderr par commande CLI, dans des lignes persistantes de la TUI. F5 et les sauvegardes renouvellent ces diagnostics ; un échec de rechargement conserve le dernier état. Les avertissements complets sont consultables dans l’aide `h` en faisant défiler, même dans un terminal étroit.

En JSON, `tags` contient l’union sans préfixes graphiques. Lorsqu’un hôte hérite de tags, les clés additives `host_tags` et `file_tags` indiquent la provenance, dans chaque résultat de recherche et dans `info.result` ; `sshm.info.v1` est conservé. Sans héritage, les champs et valeurs historiques sont inchangés, notamment les valeurs nulles de `info`. Avec une directive FileTags reconnue dans le graphe chargé (même vide ou ignorée), une recherche JSON vide renvoie `[]` ; sans FileTags, les anciens messages textuels de recherche vide restent conservés. Les codes de sortie historiques restent inchangés : 1 si aucun hôte n’est déclaré, 0 si le filtre ne trouve rien.

## Navigation au clavier

### Retour, fermeture et confirmations

- **Échap revient d'un niveau**, partout. À la liste, qui est la racine, Échap ne ferme plus l'application : une ligne rappelle les touches de sortie. Ajouter `"esc"` à `key_bindings.quit_keys` rétablit l'ancienne sortie par Échap ; `disable_esc_quit: true` l'interdit toujours. *Changement par rapport aux versions précédentes, où Échap quittait depuis le tableau.*
- **Ctrl+C suit la règle d'Échap** dans tous les sous-écrans, y compris les détails de clé (F4), les tunnels actifs et les résultats d'un lot. À la liste, Ctrl+C et `q` demandent la sortie.
- **Revenir n'arrête rien** : `q`, Échap ou Ctrl+C depuis Tunnels actifs ou Résultats d'un lot ramènent à l'écran précédent ; les tunnels et le lot continuent. *Auparavant `q` y demandait la sortie.* L'arrêt reste l'action explicite `s`.
- **Quitter** avec des tunnels actifs, un lot en cours ou un brouillon non enregistré ouvre un récapitulatif de ce qui sera arrêté ou perdu, avec deux choix nommés : **Revenir** (focus initial) et **Arrêter et quitter**.
- **Brouillon modifié** : Échap propose **Continuer l'édition** (focus initial) et **Abandonner**. Tab ou ←/→ change de choix, Entrée valide le choix marqué `▶`, Échap et Ctrl+C continuent l'édition. Entrée répétée ne peut donc plus abandonner un brouillon. *Auparavant Entrée abandonnait directement.*
- **Bouclage** : les sélecteurs simples (fichier cible, filtre par fichier, alias, groupes, méthode de disponibilité, profils de tunnels) passent du premier au dernier choix et inversement. Le tableau d'hôtes, les journaux, les diffs et l'ordre des colonnes restent bornés.

### Raccourcis, séquence à préfixe et personnalisation

F2 reste l'accès principal à toutes les actions. Trois compléments s'y ajoutent :

- **Aide** : `?` ou F1 depuis le tableau (F1 aussi depuis la recherche) ; `h` reste disponible. La touche qui ouvre l'aide la referme.
- **Séquence à préfixe** : Ctrl+G, relâché, puis une lettre. Un panneau « Actions rapides » liste les lettres et reste ouvert jusqu'à un choix ou Échap, sans délai. Une lettre inconnue n'exécute rien ; une action indisponible affiche son motif. Ces lettres n'appartiennent qu'à la séquence : `d` y duplique, alors que `d` direct supprime toujours depuis le tableau. Lettres : `a` Ajouter, `e` Modifier, `d` Dupliquer, `m` Déplacer, `n` Note, `i` Informations, `t` Profils de tunnels, `k` Clés, `b` Lots, `f` Fichier source, `g` Groupes, `c` Colonnes, `r` Recharger, `?` Aide, `q` Quitter.
- **Raccourcis directs historiques** : inchangés par défaut, actifs seulement lorsque le tableau a le focus. Dans la recherche, chaque caractère et chaque geste d'édition (Ctrl+A/E/K…) reste du texte.

`key_bindings` de `ssho/config.json` règle ces choix :

| Champ | Effet |
| --- | --- |
| `prefix_key` | Touche du préfixe, par exemple `"ctrl+g"` (défaut, aussi pour `null`) ou `"alt+x"`. `""` désactive la séquence. Une touche de navigation, un caractère simple ou une touche déjà attribuée sont refusés. Un geste d'édition comme `"ctrl+a"` est accepté mais reste réservé au texte dans les champs : le préfixe n'agit alors que hors saisie. Ctrl+A et Ctrl+B sont souvent capturés par tmux ou screen. |
| `shortcuts` | Table `identifiant d'action → liste de touches`, qui remplace les touches historiques de cette seule action. `{"host.edit": ["E"]}` libère `e` ; `[]` laisse l'action à F2. Deux actions peuvent échanger leurs touches. |
| `quit_keys` | Touches de sortie depuis la liste (inchangé). |

Les touches s'écrivent `a`, `D` (la casse compte pour un caractère), `f6`, `ctrl+f`, `alt+x`, `enter`. Une préférence en conflit n'est jamais appliquée en silence : elle est ignorée, un bandeau « Raccourcis : n préférence(s) refusée(s) » apparaît dans la liste et l'aide en donne le motif (« d sert déjà à « Supprimer » »). La palette F2, le pied de page et l'aide affichent toujours les touches réellement actives. Ces préférences se modifient dans le fichier JSON.

Identifiants d'action : `host.connect`, `host.add`, `host.edit`, `host.duplicate`, `host.move`, `host.delete`, `host.info`, `host.details`, `host.favorite`, `host.note`, `host.alias`, `tunnel.form`, `tunnel.profiles`, `tunnel.active`, `availability.test`, `availability.method`, `search.focus`, `filter.favorites`, `filter.hidden`, `filter.source`, `filter.source.clear`, `view.aliases`, `view.groups`, `view.columns`, `sort.toggle`, `sort.name`, `sort.recent`, `batch.mark`, `batch.select_visible`, `batch.clear`, `batch.open`, `batch.results`, `keys.open`, `app.backups`, `app.reload`, `app.palette`, `app.help`. La sortie se règle par `quit_keys`.

La reconnaissance d'une combinaison dépend du terminal et d'un éventuel multiplexeur : si le préfixe choisi n'arrive pas jusqu'à l'application, en choisir un autre ou le désactiver ; toutes les actions restent accessibles par F2.

L'écran principal utilise un titre compact et toute la hauteur disponible pour la liste. **Name** reste visible ; **Hostname**, **Tags**, puis **Last Attempt** s'ajoutent selon la largeur. Les textes trop longs se terminent par une ellipse `…`, mesurée en colonnes Unicode. Sous le tableau figurent le nombre d'hôtes filtrés/total (y compris les hôtes masqués), la position sélectionnée et la plage de lignes affichée. Les formulaires reprennent les onglets **General / Advanced** et leurs groupes de champs.

| Contexte | Touches |
| --- | --- |
| Recherche | Saisir pour filtrer ; ↑/↓ ou Ctrl+P/Ctrl+N pour sélectionner ; Entrée pour se connecter |
| Recharger les changements externes | F5 depuis la recherche, le tableau ou la fiche d’information ; ou Recharger dans F2 |
| Regrouper les alias | `A` depuis le tableau ou F2 → Alias regroupés ; préférence persistée, désactivée par défaut |
| Groupes repliables | `g` depuis le tableau ou F2 → Groupes : plat, fichiers, fichiers → tag principal |
| Naviguer dans les groupes | ← replie ou sélectionne le parent ; → déplie ou sélectionne le premier enfant ; Entrée/Espace bascule un groupe ; `i` montre sa fiche |
| Choisir un alias concret | F3 ou F2 → Choisir un alias ; ↑/↓ puis Entrée, Échap annule |
| Palette d'actions | F2 depuis la recherche ou le tableau ; ↑/↓ puis Entrée ; Échap annule sans changer la recherche ni le focus |
| Changer le focus | Tab ou Shift+Tab ; `/` pour revenir à la recherche depuis le tableau |
| Effacer ou revenir | Échap efface d'abord une recherche non vide, puis quitte le focus de recherche ; depuis le tableau, Échap ne quitte pas (`q` ou Ctrl+C, selon `quit_keys`) |
| Tableau | ↑/↓ ou j/k ; a ajouter, e modifier, d supprimer avec confirmation, m déplacer, D dupliquer |
| Défilement du tableau | Page précédente/suivante ; Début/Fin pour le premier/dernier hôte |
| Informations et aide | i pour la fiche de l'hôte, h pour l'aide |
| Détails à côté de la liste | F4 depuis la recherche ou le tableau ; Alt+↑/↓ pour défiler ligne par ligne, Ctrl+Page précédente/suivante par page |
| Connectivité et visibilité | P/F2 choisit SSH ou ICMP local ; p relance les contrôles, H pour afficher les hôtes portant le tag `hidden` |
| Favoris et notes | `*` épingle/désépingle, `N` ouvre la note personnelle ; F2 → Favoris seuls active/désactive le filtre |
| Note personnelle | Ctrl+S enregistre, Entrée ajoute une ligne, ↑/↓ et Début/Fin déplacent le curseur ; Échap/Ctrl+C annule avec confirmation si modifiée |
| Tri | s alterne nom/date, n trie par nom, r par dernière connexion |
| Tunnel | f ; ←/→ pour Local, Remote ou Dynamic, puis ports et adresses |
| Formulaire hôte | Tab/Shift+Tab pour naviguer, Ctrl+J/Ctrl+K pour changer d'onglet, Ctrl+S ou Entrée sur le dernier champ pour enregistrer, Échap ou Ctrl+C pour annuler |
| Assistant de clés | K depuis le tableau ou F2 → Clés OpenSSH ; F9 depuis un formulaire remplit seulement IdentityFile ; ↑/↓, Page précédente/suivante, Début/Fin naviguent ; F4 affiche les détails/erreurs complets |
| Profils de tunnels | F6 ou F2 → Profils ; a crée, Entrée/e/r édite ou renomme, d supprime avec confirmation, Ctrl+L lance |
| Sauvegardes | F8 ou F2 → Sauvegardes ; Entrée ouvre le diff, Entrée confirme, Échap revient |
| Aperçu SSH | Ajout/édition/copie/suppression/déplacement : diff par fichier ; Entrée applique les octets montrés, Échap retrouve la saisie |
| Commandes multi-hôtes (Unix) | x marque l’alias actif ; B prépare le lot ; F2 sélectionne explicitement les résultats du filtre, efface les marques ou ouvre les derniers résultats |
| Préparation du lot | Tab : commande, concurrence, délai, cibles ; ↑/↓ : cible ; Ctrl+L : aperçu, Entrée : confirmation ; Échap : retour |
| Résultats du lot | ↑/↓ : hôte ; Tab : stdout/stderr ; Page précédente/suivante : sortie ; s : annuler le lot ; q, Échap ou Ctrl+C : retour, le lot continue |
| Tunnels actifs | F7 ou F2 → Tunnels actifs ; s arrête le processus sélectionné ; q, Échap ou Ctrl+C reviennent sans rien arrêter |
| Formulaire tunnel | Ctrl+T choisit session shell ou tunnel seul ; Ctrl+S enregistre le profil nommé sans SSH ; Ctrl+L lance le forwarding courant ; Entrée avance sans lancer ; Échap/Ctrl+C annule |
| Édition de texte | Ctrl+A/E ou Début/Fin pour début/fin de ligne ; ←/→ ou Ctrl+B/F pour un caractère ; Ctrl+←/→ ou Alt+B/F pour un mot ; Retour arrière/Ctrl+H efface avant, Suppr/Ctrl+D après ; Ctrl+U efface avant le curseur, Ctrl+K après (hors formulaire hôte où il change d’onglet), Ctrl+W ou Alt+Retour arrière efface le mot précédent |
| Abandon des modifications | Tab ou ←/→ choisit entre Continuer l'édition (focus initial) et Abandonner ; Entrée valide le choix marqué ; Échap ou Ctrl+C reprend exactement le formulaire |
| Alias en édition | Alt+A ajoute un nom, Alt+D supprime le nom actif lorsqu'il en reste plusieurs |

Sur un terminal court, les champs occupent deux lignes (libellé et saisie). Chaque erreur apparaît sous son champ et la validation place le focus sur la première erreur, dans le bon onglet. Le curseur natif reste visible pendant la saisie, y compris avec caractères larges, accents combinés et emoji. Une annulation quitte immédiatement un formulaire inchangé ; seules des valeurs, des alias ou un type de tunnel modifiés demandent confirmation. Revenir aux valeurs initiales enlève cette confirmation. Échap dans la confirmation reprend le formulaire avec son texte, son curseur et sa navigation ; Entrée abandonne et revient à la vue parente.

Les hôtes masqués restent accessibles en connexion directe. La recherche de la TUI combine plusieurs mots sur le nom, l'adresse, l'utilisateur et les tags. La recherche CLI conserve ses propres filtres de champs.

Les actions de lot sont aussi accessibles dans F2, y compris avec une liste vide (sauf marquer un alias). La palette présente trois colonnes **Action**, **Raccourci** et **État** : un libellé ne contient plus de touche, et le raccourci affiché provient de la même table que celle qui exécute les touches de la liste. Elle propose **Ajouter**, **Modifier**, **Tunnel**, **Tester**, **Aide**, **Détails**, **Recharger**, **Favori**, **Note**, **Favoris seuls**, **Dupliquer**, **Profils de tunnels**, **Tunnels actifs**, **Sauvegardes**, **Alias regroupés**, **Choisir un alias**, **Groupes** et **Clés OpenSSH**, ainsi que **Méthode de disponibilité** et **Colonnes** (également disponibles sans hôte). Une action indisponible affiche son motif (« Sélectionnez un hôte »). Modifier, Tunnel, Tester, Favori, Note, Dupliquer, Profils et Choisir un alias sont indisponibles sans hôte sélectionné ; Ajouter, Aide, Détails, Recharger, Tunnels actifs, Sauvegardes, Alias regroupés, Groupes et Favoris seuls restent accessibles avec une liste vide ; ce dernier permet donc de sortir d’un filtre sans résultats. Tester réutilise le contrôle de connectivité de tous les hôtes, comme `p`. La barre sous le tableau adapte ses rappels à la largeur ; sur un écran étroit, **F2**, puis **Aide**, donne accès aux raccourcis détaillés. Les lettres saisies dans la recherche restent du texte.

**Alias regroupés** (`group_aliases: true`, absent/null : `false`) affiche une ligne
par déclaration `Host`, sans modifier SSH. Seuls les alias du même fichier, de
la même ligne fiable et de la même déclaration sont réunis. Le nom principal
reste visible ; `principal→actif` précise une cible secondaire, même en colonne
étroite. Les détails listent les alias. Les compteurs distinguent les blocs
projetés et les alias correspondant aux filtres, sur le total brut.

La recherche examine chaque alias avec ses propres valeurs natives, notes et
favoris. L’alias actif encore admissible est conservé ; sinon le principal est
choisi s’il correspond, puis le premier alias correspondant. F3 permet de choisir
explicitement un autre alias, même quand leurs noms sont ambigus pour la
recherche. Ce choix retire uniquement les contraintes de recherche/favori/hidden
incompatibles, conserve le fichier source exact et ne lance pas de connexion.
Échap conserve le curseur, la requête et le contexte. Les actions, informations,
notes et profils concernent toujours l’alias actif ; CLI et complétion conservent
tous les alias. La bascule relit les préférences avant sauvegarde pour conserver
les autres réglages modifiés pendant la session ; une erreur laisse l’ancien mode
actif et affiche le diagnostic.

**Groupes repliables.** `g` depuis le tableau ou F2 → Groupes choisit la liste plate (`flat`, défaut, y compris champ absent/null), les fichiers (`files`) ou les fichiers puis leur tag principal (`file_tags`). Le tag principal est le premier tag propre, sinon le premier FileTag hérité, sinon « Sans tag ». Chaque hôte, ou chaque bloc si le regroupement d'alias est activé, apparaît une seule fois ; tous ses autres tags restent recherchables. Les favoris et le tri récent ordonnent les hôtes à l’intérieur de leur groupe. Les fichiers sont distincts par chemin exact ; un numéro de source les distingue même si le chemin est tronqué. `i` ouvre une fiche de groupe avec chemin complet, retour à la ligne et défilement sur petit écran ; F4 affiche aussi ces informations sur écran large.

Sur un en-tête, Entrée/Espace déplie ou replie, ← replie ou remonte au parent, → déplie ou descend au premier enfant. Les actions hôte (SSH, édition, suppression, notes, favoris, profils, choix d’alias) sont indisponibles ; Entrée sur un hôte conserve la connexion. Dans la recherche, Espace et ←/→ restent des touches de saisie. Une nouvelle requête ouvre les parents et sélectionne le premier hôte correspondant ; les replis faits pendant cette recherche sont temporaires. Effacer la requête retrouve les replis manuels précédents. F5 et le retour de SSH conservent l’identité sélectionnée et les replis ; une sauvegarde ouvre les parents de l’hôte enregistré seulement si le filtre source exact l’autorise. Les compteurs distinguent hôtes/blocs, alias et groupes ; la pagination compte aussi les en-têtes.

Seul le mode de vue est persisté, après écriture réussie des préférences relues ; les replis durent la session. Les modifications externes du lanceur, du regroupement d’alias et des champs inconnus sont préservées. Un échec laisse le mode et la sélection intacts et affiche son diagnostic dans le sélecteur. Aucun dossier virtuel ni changement de configuration SSH n’est créé par la vue.

**Dupliquer** (`D` depuis le tableau ou F2 → Dupliquer depuis la recherche) demande toujours le fichier cible, y compris le fichier source, puis ouvre un ajout prérempli. Un nouveau nom concret est obligatoire : aucun alias de l’original, motif SSH ou nom déjà présent dans la configuration racine et ses Includes, même masqué et sans distinguer la casse, n’est accepté. La copie conserve les champs SSH représentés, les options et les identités supplémentaires déclarées ; la cible, l’utilisateur et le port viennent de la résolution de l’alias source par `ssh -G`, pour conserver les valeurs héritées. Les chemins de clés implicites de `ssh -G` ne deviennent pas des identités déclarées. Si cette résolution attend encore ou échoue, un message explique de réessayer après résolution ou F5 ; la navigation reste disponible. Avant toute écriture ou sauvegarde de secours, une copie privée du graphe Include permet à `ssh -G` de vérifier la cible, l’utilisateur et le port demandés. Le nouveau bloc est inséré avant le premier `Host`/`Match` du fichier cible, en gardant les commentaires et tags du bloc original ; les directives globales et Includes précédents restent en place, et le port 22 est écrit explicitement. Une destination dont les valeurs antérieures imposent une autre connexion est refusée : choisir un fichier plus tôt dans la configuration (souvent la racine), ou corriger les valeurs globales, puis réessayer. Les fichiers restent inchangés lors d’un refus. Cette vérification accepte les Includes littéraux, les globs, les chemins absolus, `~/` et les chemins relatifs natifs d’OpenSSH ; elle refuse prudemment les chemins Include avec variables `$`, tokens `%`, `~utilisateur`, ou des correspondances qui ne sont pas des fichiers. Hors Unix, cette vérification n’accepte que les Includes sans glob. Un fichier Include sans frontière `Host`/`Match` ne peut pas recevoir la copie ; choisir un autre fichier. L’original et ses alias restent inchangés. Une copie est un nouvel hôte ordinaire : ses favoris et notes personnelles ne sont pas clonés. Ctrl+S sauvegarde puis sélectionne la copie ; Échap annule avec les règles habituelles d’abandon.

Le tri, la visibilité et le rechargement conservent l'hôte sélectionné par son nom et son fichier source. Un ajout, un renommage ou un déplacement sélectionne l'hôte résultant ; si la recherche précédente le masque, elle est effacée, et les hôtes cachés sont affichés si nécessaire. Une édition ouverte depuis la fiche revient à cette fiche actualisée après sauvegarde, ou à la fiche initiale après annulation. Annuler un ajout ouvert depuis le sélecteur de fichiers revient à ce sélecteur. Les formulaires lancés directement par la CLI quittent après sauvegarde ou après annulation de leur vue initiale.

**Profils de tunnels** : F6 ouvre les profils de l’alias sélectionné, même depuis la recherche ; la palette F2 propose aussi « Profils ». Plusieurs profils local (`-L`), distant (`-R`) et SOCKS (`-D`) peuvent coexister. Le nom accepte jusqu’à 100 caractères, sans caractères de contrôle ni espaces de début/fin ; les noms sont sensibles à la casse. Modifier le champ nom puis Ctrl+S renomme le profil ouvert ; un nom déjà occupé est refusé sans écrasement. Ctrl+S reste dans le formulaire, indique « Profile saved » et ne lance aucun SSH. Ctrl+L lance les valeurs actuelles selon le mode choisi, y compris un brouillon non enregistré ou modifié ; le lancement ne sauvegarde pas implicitement un profil. Entrée sur le dernier champ ne lance rien. `d` demande confirmation avant suppression. Échap annule sans écriture, avec confirmation seulement si le formulaire a changé.

La touche `f` garde le brouillon du dernier forwarding de l’historique (également disponible dans la liste des profils). Il peut être lancé directement avec Ctrl+L sans nom, ou nommé puis enregistré avec Ctrl+S. Les profils sont stockés dans `~/.config/ssho/tunnel_profiles.json` (ou le répertoire XDG/APPDATA), par chemin canonique de configuration racine puis alias, avec écritures atomiques, permissions privées à la création et conservation des champs JSON inconnus. Une racine encore absente garde une identité stable après création. Les profils suivent les renommages d’alias réalisés par le formulaire SSH ; une destination déjà occupée reste intacte et l’ancienne association est conservée, avec avertissement distinct après l’enregistrement SSH. Les profils ne modifient ni les fichiers SSH ni le dossier `sshm/`. Chaque profil conserve aussi son mode de lancement ; les profils anciens sans mode utilisent une session shell. Le mode et les ports sont enregistrés dans la même écriture atomique.

**Commandes multi-hôtes (Unix)** : `x` marque/démarque l’alias actif ; aucun en-tête n’est une cible. `B` prépare exclusivement les marques existantes, sans sélectionner implicitement la ligne courante. F2 propose aussi « sélectionner résultats du filtre » (alias actifs projetés, y compris sous les groupes repliés), effacer les marques et revoir les derniers résultats. Recherche, choix d’alias, repli et F5 ne changent pas ces captures. Les cibles conservent l’alias exact, la racine SSH canonique et le fichier source ; un alias disparu ou déplacé est refusé au lancement. Le contenu des fichiers SSH reste celui lu par OpenSSH au moment de chaque connexion : ce n’est pas une copie immuable de la configuration.

La préparation montre la liste exacte et navigable, la commande et les chemins complets défilables. Tab choisit commande/concurrence/délai/cibles ; ↑/↓ parcourt les cibles, Page précédente/suivante les détails. Ctrl+L ouvre l’aperçu, puis Entrée confirme réellement le lancement. Défauts : **4 connexions simultanées**, **30 secondes par cible** ; réglages **1–16** et **1–3600 secondes**. Un seul lot est actif ; `B` revient à ses résultats. La commande est un argument littéral après l’alias, transmis à SSH natif, indépendamment de `ssh_command` ; aucun shell local. SSH est non interactif (`BatchMode=yes`, entrée fermée), sans TTY, multiplexage, forwarding hérité ni détachement, avec `RemoteCommand` remplacée. Il faut des accès déjà utilisables sans invite ; aucune clé hôte n’est acceptée automatiquement.

Les résultats distinguent attente, exécution, réussite, échec, expiration et annulation, avec code, stdout et stderr séparés. Chaque flux conserve au plus **256 Kio par cible**, signale sa troncature et continue à être drainé sans fichier temporaire de sortie. `s` annule les processus possédés et empêche le départ des cibles en attente ; quitter depuis la liste avec un lot actif ouvre le récapitulatif de sortie ; fermer l'écran des résultats ne l'annule pas. Les descendants restant dans le groupe possédé sont nettoyés même si le leader a déjà terminé, avant de récolter son PID. Une commande distante détachée, ou un descendant local qui quitte ce groupe, ne peut pas être garanti arrêté. L’historique reçoit séquentiellement les événements de tentative réelle et de fin observée sur le fil UI ; les cibles annulées avant tentative n’y figurent pas. Les connexions SSH interactives et la prévalidation native de copie sont temporairement refusées tant que le lot tourne, pour garder la supervision réactive. Marques, paramètres et sorties sont propres à la session ; seul l’historique est persisté.

**Assistant de clés (outils OpenSSH natifs)** : `K` depuis le tableau ou F2 → **Clés OpenSSH** inventorie les fichiers publics de `~/.ssh` (sans récursion) et les identités déclarées des hôtes. `F9` ouvre le sélecteur depuis un formulaire : Entrée ne remplace que son IdentityFile, Échap conserve tous les champs, curseurs et parents. L’inventaire affiche chemin, empreinte `ssh-keygen -lf` du public et alias déclarés utilisateurs. Les références `~/` sont résolues ; `%`, variables, `~utilisateur` et chemins relatifs sont signalés sans correspondance inventée. Ce n’est pas une résolution complète des usages effectifs d’OpenSSH. Les privés sans public connus par référence ou par nom `id_*` ont un diagnostic ; aucun contenu privé n’est lu par l’application.

Même à 28×8, la sélection et le champ actif restent visibles. `F4` ouvre les détails complets de la clé et de la dernière erreur ; ↑/↓, Page précédente/suivante et Début/Fin font défiler le chemin, l’empreinte, les usages, le diagnostic et les instructions de récupération. Échap ou F4 revient au même brouillon. Une actualisation conserve la sélection par chemin, y compris dans un sélecteur suspendu ; si la clé disparaît, aucune autre clé n’est sélectionnée avant une navigation explicite. Une revue de déploiement garde son instantané public et son alias.

`n` ouvre la génération Ed25519 : saisir un chemin privé absolu et un commentaire, puis Entrée. `ssh-keygen` demande sa phrase secrète dans le terminal natif ; aucune phrase vide n’est imposée. La paire est créée dans un répertoire privé voisin, puis publiée par créations atomiques sans écrasement ; un nom privé ou public occupé, même apparu pendant la génération, est refusé. Sur Unix le privé reste en mode 0600. Si le privé a été publié mais que la publication du public échoue, le chemin privé reste intact : aucun rollback ne supprime une clé éventuellement remplacée par un autre processus. Le diagnostic nomme les deux chemins et la cause ; les inspecter avant de réessayer, récupérer au besoin le public avec `ssh-keygen -y` natif, ou choisir une autre destination. Seul le staging temporaire est nettoyé après erreur. `a` appelle `ssh-add` pour l’agent déjà indiqué par `SSH_AUTH_SOCK` ; aucun agent persistant n’est démarré.

Avec un alias exact sélectionné avant d’ouvrir Keys, `t` prépare le diff transactionnel de rattachement : la nouvelle identité devient première et les anciennes restent présentes ; alias, autres champs et tags sont conservés. Entrée dans le diff confirme. `d` montre le public, son empreinte, l’alias et la racine SSH ; Entrée lance `ssh-copy-id -i PUBLIC -F ROOT ALIAS`. Un public modifié depuis la revue est refusé. Le public relu est figé dans un chemin temporaire sûr avec un lien vers son privé, pour préserver le contrôle natif des doublons sans exposer les noms originaux à l’évaluation shell interne du script. Le privé correspondant est nécessaire ; aucun mode forcé ou acceptation automatique de clé hôte n’est ajouté. Mots de passe et phrases secrètes restent dans les dialogues natifs, sans capture. Un outil absent est signalé sans installation automatique. Les erreurs conservent le brouillon ; le terminal est restauré après l’outil.

L’inventaire utilise un seul worker, au plus 256 candidats / 1024 entrées du répertoire, un budget de 8 s entre clés et un délai de 2 s par empreinte (donc au plus environ 10 s). F5 pendant une analyse regroupe les demandes en une seule actualisation suivante. Les destinations générées avec succès, même hors de `~/.ssh`, restent candidates pour la session et sont sélectionnées après analyse ; les 256 plus récentes sont prioritaires dans la même limite totale de 256 candidats. Aucun chemin généré supplémentaire n’est enregistré sur disque par l’application. Publics réguliers, non liés symboliquement sur Unix, un seul public par fichier et 64 Kio maximum ; captures d’empreintes bornées à 64 Kio par flux sur Unix. Génération, ajout agent et déploiement sont refusés pendant un lot actif pour garder ses délais et son annulation disponibles. La publication de deux fichiers n’est pas une transaction résistante à une coupure électrique. Le déploiement sûr exige Unix et `/tmp`.

**Tunnels actifs (Unix)** : Ctrl+T dans le formulaire choisit « Shell session » (comportement terminal habituel) ou « Tunnel only ». Ctrl+L démarre explicitement le tunnel seul en arrière-plan et ouvre F7, sans enregistrer le brouillon. SSH reçoit `-N -T -o ExitOnForwardFailure=yes -o BatchMode=yes`, une entrée fermée et un stderr capturé dont seuls les derniers 32 Kio sont conservés. Les clés/hôtes connus et méthodes d’authentification restent ceux d’OpenSSH ; l’authentification exige une clé/agent déjà utilisable sans invite. RemoteCommand, RequestTTY, le fork et le multiplexage sont neutralisés pour que le processus et ses descendants appartiennent à cette session. Aucune acceptation automatique de clé hôte n’est ajoutée.

F7 affiche l’hôte, le profil et les ports au moment du lancement, le chemin canonique de configuration, l’état et la cause de sortie. « Active » signifie uniquement que le processus vit, sans garantie de disponibilité du forwarding. `s` arrête seulement l’enfant sélectionné et ses descendants ; les entrées terminées restent consultables. Renommer/supprimer un profil ou recharger avec F5 ne change pas l’identité des processus possédés. Quitter depuis la liste avec des tunnels actifs ouvre un récapitulatif : **Revenir** a le focus, **Arrêter et quitter** doit être choisi avec Tab puis Entrée ; revenir de F7 n'arrête aucun tunnel. La sortie normale et les erreurs récoltent uniquement les enfants possédés ; aucun PID externe ou historique n’est utilisé pour arrêter un SSH. L’historique conserve tentatives et résultats réels, et une erreur d’écriture affiche un avertissement sans empêcher le tunnel.

**Aperçu et restauration.** Chaque ajout, édition, copie, suppression et déplacement dans la TUI prépare un diff par fichier avant toute écriture SSH ou sauvegarde. Les chemins d’accès et destinations physiques, les permissions attendues et les lignes `+`/`-` sont affichés ; un déplacement montre ses deux fichiers. `␍` représente CR et `→` une tabulation dans le diff. Entrée confirme, Échap retrouve le formulaire ou sélecteur avec sa saisie et son curseur ; les notes, favoris et profils personnels s’enregistrent directement dans leurs fichiers JSON. Les commandes CLI non interactives conservent leur comportement immédiat et utilisent les mêmes transformations.

La confirmation vérifie de nouveau contenu, identité du fichier, permissions et chemin d’accès. Une modification externe refuse l’opération et conserve l’aperçu ; revenir avec Échap puis préparer un nouvel aperçu (F5 depuis la liste si nécessaire). Une copie vérifie aussi le graphe Include et les hypothèses de la prévalidation native `ssh -G`. Aucun fichier ni répertoire SSH n’est créé pendant la préparation ; le miroir de copie est privé et temporaire.

**F8 / Sauvegardes** liste d’abord les versions avec destination explicitement enregistrée, puis les anciennes copies `.backup` sans provenance. Ces dernières restent consultables dans le répertoire de données mais leur restauration automatique est désactivée : aucun chemin n’est déduit du nom. Une version privée `backups/version-*.json` contient les octets, le chemin canonique, le chemin d’accès original, la date et le mode d’origine. L’aperçu de restauration valide la destination enregistrée et le fichier de sauvegarde lui-même ; confirmer sauvegarde d’abord le contenu courant, puis le remplace atomiquement. Les permissions actuelles sont conservées ; une destination absente est recréée au mode enregistré. Des fichiers portant le même nom dans des répertoires différents conservent leurs versions séparées. Les copies historiques « dernière sauvegarde » sont maintenues pour compatibilité.

Revenir à la liste avant F8 lorsqu’un formulaire, sélecteur ou aperçu est ouvert, même sous F7 : une restauration ne doit pas abandonner un brouillon suspendu. F7 peut recouvrir un formulaire ou un aperçu et revient sans le modifier. Fermer F7 (`q`, Échap ou Ctrl+C) retrouve intégralement le brouillon ou l'aperçu suspendu : on ne quitte plus l'application depuis F7. Les sauvegardes ne sont ni chiffrées ni purgées automatiquement ; elles sont privées au compte local et leur provenance repose sur ce répertoire, pas sur une signature cryptographique. Une restauration est par fichier, pas un retour atomique de tout un graphe Include ; elle ne restaure pas les notes, favoris ni profils JSON associés. Le déplacement utilise un retour arrière protégé ; si une intervention externe empêche celui-ci, l’erreur garde les sauvegardes et demande une récupération manuelle.

**F5 / Recharger** relit le fichier SSH racine et ses Includes après une modification externe. La recherche (et son curseur), le tri, la visibilité des hôtes cachés et la sélection par nom et fichier source sont conservés ; le défilement reste borné à la nouvelle liste. La fiche ouverte est actualisée si l’hôte existe encore, sinon la liste réapparaît. Les diagnostics périmés sont effacés et la configuration effective est recalculée, même si le ping au démarrage est désactivé ; le rechargement ne lance pas de sondes de connectivité. Si un fichier ne peut plus être lu, la dernière liste et ses diagnostics restent disponibles avec la cause affichée. Un Include sans correspondance, littéral ou avec glob, reste facultatif ; la disparition du fichier racine et les erreurs de parcours ou de lecture sont signalées. F5 recharge aussi les notes, favoris et profils de tunnels. Si le chemin racine est un lien symbolique dont la cible a changé, ces données sont réassociées ensemble à la nouvelle racine canonique ; toutes les lectures sont préparées avant de publier le nouvel état. Un échec laisse la liste et les données précédentes affichées. F5 ne quitte pas les formulaires ni les confirmations en cours.

Le panneau **Détails** est désactivé au lancement et s'ouvre à droite lorsque le terminal atteint **110 colonnes**. Il suit l'hôte sélectionné sans réduire le nombre de lignes du tableau ; les colonnes du tableau s'adaptent à la place restante. En dessous de 110 colonnes, la liste reprend toute la largeur et un rappel indique **Tab, puis i** depuis la recherche, ou **i** depuis le tableau, pour ouvrir la fiche. Agrandir le terminal réaffiche le panneau si son activation a été conservée. Les raccourcis Alt+↑/↓ (ligne par ligne) et Ctrl+Page précédente/suivante (par page) défilent uniquement lorsque le panneau est visible, sans modifier le texte recherché ni l'hôte sélectionné. Alt+↑/↓ reste disponible si le terminal réserve Ctrl+Page précédente/suivante à ses onglets.

Les détails affichent les valeurs effectives résolues par `ssh -G` : utilisateur, port, toutes les identités, ProxyJump, ProxyCommand et autres directives, avec le fichier et la ligne source de la déclaration et le diagnostic de connectivité disponible. Pendant la résolution ou en cas d’erreur, les valeurs déclarées sont affichées avec cet état explicitement indiqué. Le formulaire édite toujours la déclaration. Les longues valeurs sont repliées et restent entièrement accessibles par défilement, y compris dans la fiche **i**.

Les favoris portent un repère `*` et précèdent le tri par nom ou par dernière connexion, sans perdre la sélection lors d’un épinglage. Le filtre **Favoris seuls** se combine avec la recherche et la visibilité des hôtes cachés. Les notes apparaissent dans **Détails** et la fiche **Info**. Une erreur de sauvegarde conserve le brouillon ouvert ; un échec d’épinglage conserve l’état précédent.

Favoris et notes sont enregistrés uniquement dans `~/.config/ssho/metadata.json` (ou le répertoire applicatif XDG/APPDATA), avec publication atomique et permissions privées à la création. Chaque entrée est indexée par le chemin canonique du fichier SSH racine puis par le nom d’alias : deux configurations restent indépendantes, et les chemins symboliques existants menant à la même racine partagent leurs données. Une racine encore absente utilise son chemin absolu avec le parent canonique, stable après création. Ces actions ne modifient aucun fichier SSH ni le dossier `sshm/` ; les champs JSON inconnus sont conservés.

Lors d’un renommage dans le formulaire, les alias conservés gardent leurs données ; les alias supprimés et ajoutés sont associés dans leur ordre de déclaration. Supprimer seulement un alias ne transfère rien vers un alias conservé. Une destination possédant déjà des métadonnées bloque le transfert pour préserver les deux notes. La sauvegarde SSH ayant déjà réussi, l’interface affiche l’hôte sauvegardé et un avertissement distinct ; les anciennes métadonnées restent disponibles pour récupération. Les formulaires CLI autonomes terminent avec une erreur si ce transfert échoue après la sauvegarde SSH.


## Complétion

La complétion doit être chargée dans le shell. Avec le binaire
compilé localement, depuis la racine du projet :

```bash
source <(./target/release/ssho completion bash)
./target/release/ssho BS<Tab>
```

Pour la charger à chaque nouveau terminal Bash, ajouter à `~/.bashrc` la ligne
suivante en remplaçant le chemin par le chemin absolu du binaire :

```bash
source <(/absolute/path/to/ssho/target/release/ssho completion bash)
```

Une fois `ssho` installé dans le `PATH`, utiliser :

```sh
# Bash
source <(ssho completion bash)

# Zsh
source <(ssho completion zsh)

# Fish
ssho completion fish | source
```

```powershell
# PowerShell
ssho completion powershell | Out-String | Invoke-Expression
```

## Vérification

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release

# Parcours dans un pseudo-terminal, Linux/macOS avec Python 3
python3 tests/pty_smoke.py target/release/ssho
# Parcours ciblé du filtre par fichier
python3 tests/pty_smoke.py target/release/ssho --source-filter-only
# FileTags, héritage et mutation de destination
python3 tests/pty_smoke.py target/release/ssho --file-tags-only
# Parcours ciblé du lanceur et de son retour au terminal
python3 tests/pty_smoke.py target/release/ssho --launcher-only
# Parcours ciblé alias regroupés, choix de cible et préférences
python3 tests/pty_smoke.py target/release/ssho --aliases-only
# Groupes, sécurité des en-têtes, recherche, préférences et petit terminal
python3 tests/pty_smoke.py target/release/ssho --groups-only

# Parcours ergonomiques UX (clavier, focus, retour) dans un dossier de fixture neuf
python3 tests/pty_ux.py target/release/ssho "$(mktemp -d)/ux"

# Comparaison facultative avec un exécutable SSHM
python3 tests/reference_parity.py /path/to/sshm target/release/ssho
```

Tous les tests utilisent des dossiers temporaires. Les parcours de connexion et de tunnel utilisent un faux client SSH ; ils ne se connectent pas aux serveurs personnels de l'utilisateur.

## Versions et plateformes

La vérification des mises à jour interroge, en arrière-plan, le dépôt de releases fixé à la compilation par `SSHO_REPOSITORY=ArnaudFra/ssho` ou `SSHO_RELEASES_URL=https://…` ; sans l'une de ces variables, aucune requête n'est faite. Les préférences et `--no-update-check` désactivent la vérification.

Le programme et les parcours terminal sont testés sous Linux. Les chemins et branches spécifiques macOS/Windows existent mais ne sont pas testés.

## Licence

MIT. Les premières versions adaptent la logique et l'organisation de [SSHM](https://github.com/Gu1llaum-3/sshm) ; la mention de copyright d'origine de Guillaume Archambault est conservée dans [LICENSE](LICENSE).

## Recherche avancée dans l’interface

La recherche de la TUI accepte jusqu’à 1 024 caractères. Les termes sont combinés par **ET**, implicitement avec des espaces ou explicitement avec `AND` / `ET` (sans casse) : `tag:prod user:deploy`, `tag:prod AND user:deploy` et `tag:prod ET user:deploy` sont équivalents. Une conjonction seule, au début, à la fin ou répétée (`alpha AND ET beta`) produit une erreur qui demande un terme de chaque côté. Les mots `"AND"` et `"ET"` entre guillemets et les valeurs telles que `tag:and` restent des textes littéraux. Aucune syntaxe OU/OR, négation ou parenthèses n’est interprétée.

- Un terme simple cherche une sous-chaîne dans le nom, l’adresse, l’utilisateur, les tags ou la note personnelle. Chaque terme peut correspondre à un champ différent.
- `tag:texte`, `user:texte` et `file:texte` limitent cette recherche au tag, à l’utilisateur ou au chemin du fichier source déclaré. `file:` ne cherche pas dans le contenu des fichiers.
- `is:online`, `is:offline`, `is:unknown`, `is:connecting`, `is:favorite` filtrent les états et le favori local. Plusieurs `is:` sont aussi combinés par ET ; les états incompatibles ne donnent donc aucun résultat.
- Les guillemets doubles groupent une phrase exacte, par exemple `"release café"`, `tag:"équipe infra"` ou `file:"Team Hosts"`. Ils peuvent entourer tout un terme ; les guillemets simples et les barres obliques inverses sont littéraux (aucun échappement). Un préfixe inconnu, comme `ticket:42`, reste un terme littéral.
- La casse est ignorée par conversion Unicode en minuscules, sans supprimer les accents ni normaliser les formes Unicode. Les champs et états sont également sans casse. Une requête vide affiche tous les hôtes autorisés par les autres filtres.
- Une valeur vide (`tag:`, `""`), un guillemet non fermé ou un état inconnu (`is:maybe`) affiche une erreur de recherche et aucun résultat. Corriger la saisie rétablit immédiatement la liste ; les erreurs d’opération restent conservées séparément.

L’adresse et l’utilisateur proviennent du cache `ssh -G` dès qu’il est résolu, avec repli sur les valeurs déclarées pendant l’attente ou après une erreur. Les résultats se mettent à jour à l’arrivée des résolutions et des sondes. `Unknown` est l’état initial ou celui d’un hôte non sondé ; `Connecting` indique une sonde en cours. Le titre affiche toujours en clair l’état de l’hôte sélectionné, y compris après un retour SSH et dans un terminal compact. La sélection conservée est identifiée par nom et fichier ; une arrivée qui ne change pas les membres de la liste ne relance pas son tri.

Depuis le tableau, `c` ouvre les fichiers SSH connus, y compris les fichiers vides, avec une entrée « Tous les fichiers » ; `C` retire ce filtre. Les mêmes actions figurent dans F2, également depuis une recherche sans résultat. Le chemin exact sélectionné apparaît dans une bannière persistante et dans l’aide. Ce filtre se compose avec la requête, les favoris et `H` : même un fichier sans résultat conserve son filtre après recherche, bascule `H` ou F5. Seuls une suppression ou un déplacement du dernier hôte du fichier le retirent automatiquement, en comptant aussi les hôtes masqués et hors recherche. La requête, son curseur et le focus restent conservés pendant le choix du fichier. Deux fichiers de même nom restent distincts par leur chemin complet. Pendant la saisie, `c` et `C` restent du texte.

Exemples : `tag:prod AND user:deploy is:online`, `"maintenance prévue" ET is:favorite`, `file:"Team Hosts" is:unknown`. Les filtres d’hôtes masqués et « Favoris seuls » s’appliquent en plus de la requête. Les correspondances du tableau, du panneau de détails et de la fiche sont soulignées et mises en évidence sans découper les caractères Unicode ; les marqueurs d’état/favori restent distincts. La requête reste présente après SSH et F5. Après ajout, édition ou déplacement dans le fichier actif (ou sans filtre de fichier), une expression compatible avec l’hôte enregistré est conservée ; une expression incompatible est effacée pour montrer cet hôte. Les valeurs résolues précédentes servent uniquement à une vérification provisoire. Même une modification explicite d’utilisateur/adresse attend le résultat natif avant d’effacer une recherche compatible avec ces valeurs : une directive globale ou un bloc `Host *` antérieur peut conserver la priorité. Ce contexte provisoire est conservé entre plusieurs enregistrements avant réception du résultat, sans être publié dans le cache résolu. Un enregistrement hors du fichier filtré conserve ce filtre et reste masqué. Dans le fichier actif, l’hôte enregistré reste visible pendant la nouvelle résolution ; son résultat frais confirme la recherche ou l’efface si l’héritage a changé (notamment après déplacement). Une erreur de résolution tranche avec les valeurs déclarées, et toute nouvelle saisie de recherche annule cette attente.

Cette syntaxe concerne uniquement l’interface interactive. La commande CLI `ssho search` conserve sa recherche historique et ses formats de sortie.

Le collage encadré (*bracketed paste*) est activé pendant l’interface et désactivé pour SSH et à la sortie. Dans la recherche et les champs simples, les retours à la ligne et tabulations deviennent des espaces ; dans les notes, les retours à la ligne sont conservés (CRLF/CR normalisés en LF). Les autres caractères de contrôle sont ignorés. Le collage modifie uniquement le champ actif, sans exécuter de raccourci ni valider un formulaire ; dans le tableau, les palettes et les confirmations, il est ignoré. Ce comportement nécessite un terminal qui envoie les séquences de collage encadré.
