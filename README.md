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
  "language": "en",
  "colors": true,
  "ascii": false,
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

**Diagnostics bornés.** Chaque opération native de diagnostic — `ssh -G`, contrôle via proxy, `ping` — est limitée à 5 secondes et à 256 Kio par flux de sortie ; au plus 8 tournent en même temps, donc 16 fichiers de capture, anonymes et libérés par le système dès la fin de l'opération, quelle qu'en soit l'issue. Une sortie qui ne cesse de grossir est arrêtée dès qu'elle est remarquée (contrôle toutes les 10 ms), avec l'erreur « output limit (256 KiB per stream) reached: the process was stopped », sans attendre l'échéance. Une sortie volumineuse d'une commande terminée est tronquée avec la marque visible `[output truncated at 256 KiB]`, et un message d'erreur ne garde que la fin utile d'un long diagnostic, précédée de `[beginning truncated]`. Recharger, changer de méthode ou quitter **annule** les diagnostics SSH en cours comme les diagnostics ICMP : leur groupe de processus est arrêté, un ancien diagnostic ne va plus à son terme, et la fermeture attend la fin de ce nettoyage — un instant, jamais l'échéance. La limite stricte du noyau sur la taille des fichiers reste réservée à `ping`, outil entièrement possédé : un auxiliaire de proxy ou une commande `Match exec` peut légitimement écrire dans son propre journal. Le contrôle SSH direct (poignée de main sans processus) n'est pas interrompu par une annulation ; il garde ses délais de 5 s de connexion et 2 s d'échange.

ICMP vérifie la disponibilité réseau du **Hostname effectif obtenu par `ssh -G`, depuis cette machine** ; il ne traverse ni ProxyJump ni ProxyCommand. Un résultat positif ne démontre ni disponibilité du service SSH ni authentification réussie. La durée affichée est le temps du diagnostic (résolution comprise), **pas un RTT**. Le budget ICMP est de cinq secondes par alias, résolution et ping compris, avec huit workers fixes. L’absence de réponse déclarée par l’outil donne Offline ; outil absent, permission refusée, délai externe dépassé ou erreur donnent Unknown avec cause. Aucun sudo ni changement de capabilities. Sous Unix, stdout/stderr sont limités à 256 Kio par flux, et les processus possédés sont nettoyés après fin, expiration, changement de génération ou fermeture. La fermeture attend cette récolte ICMP ; une attente noyau ne constitue pas une garantie absolue de temps réel.

Linux utilise le contrat `ping` iputils (IPv4/IPv6), macOS `ping` ou `ping6` pour une adresse IPv6 littérale, FreeBSD le `ping` unifié. Les arguments et codes de retour des trois plateformes sont couverts par les tests. Sur macOS, un nom DNS passe par `ping` IPv4 ; `ping6` n’offre pas les mêmes options de délai et une expiration externe reste Unknown. Windows et les autres plateformes donnent Unknown : aucun succès n’est déduit du seul code 0 de `ping.exe`.

## Tags hérités par fichier

Une directive d’en-tête `# FileTags: work, lab` attribue ces tags à tous les hôtes déclarés dans ce fichier. Elle accepte la casse variable du mot `FileTags` et les espaces autour de `#`, du nom et de `:`. Les répétitions s’additionnent dans l’ordre, sans doublons exacts ; `Work` et `work` restent distincts. L’en-tête se termine au premier `Host`, `Include` ou `Match`, même sans argument. Les fichiers inclus appliquent leur propre en-tête, sans recevoir les tags du parent.

Le tableau et la fiche affichent `%work` pour un tag hérité et `#prod` pour un tag propre. La recherche TUI (mots et `tag:`), la recherche CLI et la visibilité utilisent leur union. Le tag hérité `hidden` masque aussi l’hôte ; `H` permet de le montrer dans la TUI, tandis que la complétion de `info` permet toujours de le retrouver explicitement. Ces règles se composent avec le filtre de fichier et les favoris.

Le formulaire édite uniquement les tags propres ; l’héritage de la destination apparaît séparément en lecture seule, y compris pour un fichier vide. Éditer, copier ou déplacer ne matérialise jamais les tags hérités dans `# Tags:`. Après copie ou déplacement, l’hôte reçoit l’en-tête de sa destination. Les en-têtes restent dans leur fichier lors d’une suppression ; les directives tardives sont conservées sous forme de commentaires `# Inactive FileTags: ...` lorsqu’un bloc est retiré, afin qu’elles ne deviennent jamais actives par accident. L’aperçu montre exactement ces changements avant confirmation.

Les directives tardives sont ignorées et signalées avec fichier et ligne : une seule fois sur stderr par commande CLI, dans des lignes persistantes de la TUI. F5 et les sauvegardes renouvellent ces diagnostics ; un échec de rechargement conserve le dernier état. Les avertissements complets sont consultables dans l’aide `h` en faisant défiler, même dans un terminal étroit.

En JSON, `tags` contient l’union sans préfixes graphiques. Lorsqu’un hôte hérite de tags, les clés additives `host_tags` et `file_tags` indiquent la provenance, dans chaque résultat de recherche et dans `info.result` ; `sshm.info.v1` est conservé. Sans héritage, les champs et valeurs historiques sont inchangés, notamment les valeurs nulles de `info`. Avec une directive FileTags reconnue dans le graphe chargé (même vide ou ignorée), une recherche JSON vide renvoie `[]` ; sans FileTags, les anciens messages textuels de recherche vide restent conservés. Les codes de sortie historiques restent inchangés : 1 si aucun hôte n’est déclaré, 0 si le filtre ne trouve rien.

## Navigation au clavier

### SFTP natif

`S` sur un hôte, ou F2 → **SFTP: open a session** (*SFTP : ouvrir une session*), ouvre le client `sftp` d'OpenSSH dans le terminal pour **l'alias exact sélectionné**, avec la configuration SSH de la session (`sftp -F <configuration> -- <alias>`). C'est le client natif en ligne de commande, pas un gestionnaire de fichiers à deux panneaux : `help` y liste ses commandes, `bye` ou Ctrl+D revient à la liste, retrouvée telle qu'elle était (sélection, recherche, filtres).

- Avant de céder le terminal, ssho y écrit une ligne annonçant le client natif, l'alias et la configuration ; au retour, la liste indique comment il s'est terminé (« SFTP a fini avec le code 0 : beta (client natif) », ou l'erreur de lancement si `sftp` est absent).
- **Lanceur personnalisé** : `ssh_command` ne s'applique pas à SFTP, `sftp` démarrant son propre `ssh`. Si un lanceur est configuré, c'est dit avant et après la session.
- L'alias doit être déclaré dans la configuration ; il est passé après `--` comme un seul argument et ne peut pas être lu comme une option. Aucun chemin ni commande n'est ajouté par ssho.
- Une session SFTP n'est pas comptée comme une connexion dans l'historique. Pendant un lot en cours, l'action est refusée comme les autres outils au premier plan.
- Sur un groupe ou sans hôte sélectionné, la touche explique pourquoi elle ne fait rien.

### Exporter et importer les données de l'application

F2 → **Export / import data** (*Exporter / importer les données*) sert à emporter ses données d'une machine à l'autre, ou à en garder une copie lisible.

**Périmètre.** Un export contient, au choix (cases à cocher, Espace) : favoris et notes, profils de tunnels et vues enregistrées **de la configuration SSH en cours**, commandes enregistrées, et préférences portables (langue, affichage, liste, clavier). Le nombre d'entrées de chaque section est affiché avant l'écriture, et une section vide n'est pas cochée. **Ne sont jamais inclus** : clés privées, mots de passe, fichiers SSH, historique des connexions, sauvegardes, ni le lanceur SSH (un chemin local). Notes et commandes pouvant contenir des informations privées, le fichier est créé lisible par vous seul (`0600`) ; un fichier existant n'est jamais remplacé. `~/` désigne le dossier personnel.

**Format.** Fichier JSON versionné (`"format": "ssho.export"`, `"version": 1`), avec la date, la version de l'application et la configuration d'origine. Un fichier d'un autre format, d'une version inconnue ou contenant une section inconnue est refusé avec la raison. Le fichier source d'une vue est écrit relativement au dossier de la configuration quand il s'y trouve, et recherché de la même façon à l'import.

**Import.** L'import se fait toujours dans la configuration en cours — l'association est « cette configuration + alias », jamais un nom d'alias seul entre deux fichiers. Lire un fichier ne modifie rien ; l'aperçu indique d'où il vient et où il sera importé, puis pour chaque section : entrées nouvelles, identiques, différentes, « pour un alias inconnu ici » et « seulement ici ». Trois choix explicites :

- **Entrées présentes des deux côtés** (←/→) : *Ajouter seulement ce qui est nouveau* (défaut : rien de ce qui existe ici n'est modifié), *Ajouter, et remplacer ce qui diffère*, ou *Remplacer entièrement les sections choisies* (les entrées qui n'existent qu'ici sont supprimées).
- **Alias inconnus ici** : les laisser de côté (défaut) ou les importer quand même.
- les sections à importer (Espace).

Une phrase récapitule ce que l'import ajouterait, remplacerait, supprimerait et laisserait de côté ; elle se met à jour à chaque choix. Une vue dont le fichier source n'existe pas dans cette configuration est signalée (son application demandera une mise à jour). Le bouton **Importer** demande confirmation, focus sur « Revenir ». Avant toute écriture, le résultat est d'abord relu par les chargeurs de l'application dans un dossier à part — un fichier invalide ne change rien — puis les fichiers actuels sont copiés dans `ssho/data-backups/<date>/`, et seulement alors remplacés. La liste, les vues, le catalogue et les préférences sont rechargés aussitôt.

### Commandes enregistrées

Le catalogue garde des commandes personnelles sous un nom, avec une description, la portée prévue (un hôte ou un lot, un hôte seulement, un lot seulement) et les paramètres demandés à chaque utilisation. `R` depuis le tableau, ou F2 → **Saved commands** (*Commandes enregistrées*), l'ouvre ; six actions nommées : **Utiliser sur cet hôte**, **Utiliser sur le lot**, **Nouvelle**, **Modifier**, **Supprimer**, **Fermer**.

Une commande s'écrit sur une ligne, avec ses paramètres entre accolades :

| Écriture | Paramètre demandé |
| --- | --- |
| `{file}` | texte, sans valeur par défaut |
| `{file=/var/log/syslog}` | texte, avec une valeur par défaut |
| `{lines:number=50}` | nombre entier |
| `{level:choice=info\|warn\|error}` | une valeur parmi celles listées (←/→) |
| `{{` et `}}` | accolades littérales, par exemple dans `awk '{{print $1}}'` |

Exemple : `tail -n {lines:number=50} {file=/var/log/syslog}`. Un même nom réutilisé (`cp {f} {f}.bak`) n'est demandé qu'une fois. Le formulaire affiche, pendant la frappe, les paramètres reconnus ou la raison d'un refus (accolade non fermée, type inconnu, valeur par défaut qui n'est pas un nombre…). Entrée passe au champ suivant et n'enregistre jamais ; le bouton Enregistrer ou Ctrl+S enregistre.

**Substitution.** Ce n'est pas un moteur de script : chaque valeur est contrôlée selon son type, puis insérée comme **un seul mot protégé** pour un shell POSIX (entre apostrophes, les apostrophes de la valeur étant échappées). Une valeur ne peut donc ni ajouter une commande, ni une redirection, ni une substitution : taper `x; id` dans un texte donne `'x; id'`, et dans un nombre un refus expliqué. En conséquence, **ne mettez pas de guillemets autour d'un paramètre** dans la commande. Sauts de ligne et caractères de contrôle sont refusés. Limite : la protection suppose un shell distant de type POSIX.

**Utilisation.** « Utiliser » demande les valeurs (pré-remplies par les défauts), montre la « Commande à vérifier » recalculée à chaque frappe, puis **remet la commande à la préparation de lot** (voir Lots) : une seule cible explicite pour « sur cet hôte » — les marques de la liste ne sont pas touchées — ou les cibles déjà marquées pour « sur le lot ». La vérification des cibles et de la commande, puis le lancement explicite, restent ceux du lot : rien ne s'exécute depuis le catalogue, et aucune commande n'est lancée à l'ouverture d'un hôte. Sans cible marquée, « sur le lot » l'explique et ne choisit jamais de cible à votre place ; une commande prévue pour un hôte seulement est refusée pour un lot, et inversement, avec la raison.

**Secrets.** Il n'existe pas de paramètre « secret » : une valeur finirait dans la ligne de commande et dans l'historique des lots. Les commandes sont enregistrées en clair dans `ssho/commands.json` (catalogue commun à toutes les configurations SSH) ; n'y écrivez jamais de mot de passe. Les valeurs saisies à l'utilisation ne sont pas enregistrées dans le catalogue.

### Vues enregistrées

Une vue garde sous un nom le contexte de la liste : la recherche, le filtre exact par fichier, « favoris seuls », la visibilité des hôtes masqués, le tri (nom ou connexion récente), le regroupement, les alias regroupés et les colonnes. Exemples : « Production », « À maintenir », « Bases de données ». Les marques de lot et l'état de disponibilité n'en font jamais partie : ils décrivent un instant, pas une vue.

`V` depuis le tableau, ou F2 → **Saved views** (*Vues enregistrées*), ouvre l'écran des vues : la vue en cours, la liste des vues, les critères de celle qui est sélectionnée, puis six actions nommées atteintes par Tab ou ←/→ et lancées par Entrée.

- **Appliquer** règle la liste d'après la vue et revient à la liste (« ✓ Vue « Production » appliquée »). Le regroupement et les colonnes sont enregistrés dans `config.json`, comme un choix fait à la main.
- **Enregistrer sous…** crée une vue à partir de la liste actuelle : les critères qui seront gardés sont affichés avant la saisie du nom. Entrée dans le nom mène au bouton Enregistrer et n'écrit rien ; Ctrl+S enregistre. Un nom déjà pris est refusé et expliqué.
- **Mettre à jour** remplace les critères de la vue sélectionnée par la liste actuelle ; **Renommer** ne change que le nom ; **Supprimer** demande confirmation, focus sur « Revenir », et ne touche ni à la liste ni aux fichiers SSH.

Après application, le bandeau de la liste affiche « Vue : Production ». Dès que la recherche, un filtre, le tri, le regroupement ou les colonnes changent, il affiche « Vue : Production (modifiée) » : la vue enregistrée n'est jamais écrasée sans « Mettre à jour ». « Effacer tous les filtres » quitte la vue. Si le fichier source d'une vue ne fait plus partie de la configuration SSH, l'application est refusée avec une explication et rien n'est appliqué : le filtre n'est jamais élargi en silence à tous les fichiers.

Les vues sont conservées dans `ssho/views.json`, séparément pour chaque fichier de configuration SSH (comme les favoris et les notes). Noms de 40 caractères au plus.

### Préférences, données et premier lancement

`o` depuis le tableau, ou F2 → **Preferences** (*Préférences*), ouvre un écran unique pour les options de l'application, en cinq sections :

- **Apparence** : langue, couleurs, symboles ASCII.
- **Liste des hôtes** : recherche au démarrage, groupes, alias regroupés, colonnes (Entrée ouvre leur éditeur, qui enregistre de lui-même).
- **Connexion et diagnostics** : méthode de disponibilité (SSH / ICMP local), sondes au démarrage, avis de mise à jour, lanceur SSH (lecture seule : tableau d'arguments littéraux de `ssh_command`, qui ne lance que les connexions ordinaires — tunnels, lots, `ssh -G` et sondes utilisent toujours `ssh`).
- **Clavier** : touche des actions rapides, Échap quittant ou non à la liste, raccourcis.
- **Données (lecture seule)** : dossier de l'application, `config.json` (préférences), `metadata.json` (favoris et notes), `sshm_history.json` (historique), `tunnel_profiles.json` (profils de tunnels), `views.json` (vues enregistrées), `commands.json` (commandes enregistrées), `backups/` (sauvegardes), `data-backups/` (copies avant import) et la configuration SSH lue par la session.

Chaque ligne montre la valeur en cours ; sous la liste, l'option qui a le focus est expliquée (son effet, puis « prend effet dès l'application » ou « au prochain démarrage ») et une valeur longue, comme un chemin, y est répétée en entier. ↑↓ choisit une option ; Espace, Entrée ou ←/→ change sa valeur **dans le brouillon** : la ligne porte « ● modifié » et rien n'est écrit. Tab atteint les boutons **Appliquer**, **Annuler** et **Valeurs par défaut** ; Ctrl+S applique aussi. « Valeurs par défaut » remplit seulement le brouillon (les options sans ligne propre, colonnes et lanceur, sont conservées) et place le focus sur Appliquer. Échap ou Annuler sur un brouillon modifié demande confirmation, focus sur « Continuer l'édition ». Appliquer relit `config.json`, n'y remplace que les options de cet écran — une option modifiée ailleurs entre-temps n'est pas écrasée — puis confirme « ✓ Préférences enregistrées ».

**Raccourcis.** La ligne Raccourcis ouvre un éditeur : une ligne par action de la liste, avec ses touches et leur origine (défaut, personnalisé, F2 seul). Entrée ou **Changer la touche** attend la prochaine touche, qui devient le raccourci de l'action ; **Aucune touche** laisse l'action à F2 ; **Défaut** retire la personnalisation ; **Terminer** (ou Ctrl+S) rend les raccourcis au brouillon des préférences, qui les écrit avec le reste. Une touche déjà prise, réservée à la navigation ou gardée par les touches de sortie est refusée avec la raison donnée par la table de raccourcis (« d sert déjà à « Supprimer » »). Les touches de sortie (`quit_keys`) autres qu'Échap se règlent encore dans `config.json`. Une configuration existante qui attribuait `o` à une autre action voit ce raccourci refusé et expliqué, `o` ouvrant désormais les préférences.

**Premier lancement.** Quand le dossier `ssho/` vient d'être créé par copie de `sshm/`, la liste l'annonce une fois (« Application data copied from the former sshm folder — !: details ») et l'écran Messages (`!`) garde pour la session la phrase complète, avec les deux chemins : le dossier d'origine est inchangé, seule la copie est utilisée. Aucun avis si le dossier existait déjà ou s'il n'y avait rien à copier ; la règle de copie elle-même est inchangée.

**Liste vide.** Trois situations sont distinguées, chacune avec la touche utile tirée des raccourcis en vigueur : configuration sans hôte (« No host in *fichier*. a: add a host »), aucun résultat pour les filtres (« … clear the search • F2: Clear all filters ») et configuration illisible (« … !: see the error • F5: reload »).

### Langue, couleurs, symboles et petits terminaux

**Langue.** L'interface existe en anglais et en français. L'anglais est la langue par défaut ; elle se change dans l'application depuis les Préférences (`o`) ou directement par F2 → **Language** (en français : **Langue de l'interface**), action que la recherche de la palette trouve dans les deux langues (`language`, `langue`). Le sélecteur porte un titre lisible dans les deux langues, marque la langue en cours par `✓`, enregistre le choix dans `ssho/config.json` (`"language": "en"` ou `"fr"`) et l'applique aussitôt, sans redémarrage. Une valeur absente, `null` ou inconnue donne l'anglais et ne bloque jamais le lancement. Les noms de touches suivent la langue (Enter / Entrée, Esc / Échap, Space / Espace). Restent en anglais dans les deux langues : les termes et directives OpenSSH (`ProxyJump`, `RequestTTY`, `Local (-L)`…), la syntaxe de recherche (`tag:`, `is:online`, `AND`), l'aide en ligne de commande (`ssho --help`) et le détail technique des erreurs renvoyées par le système ou par OpenSSH.

Ce guide cite les libellés tantôt en français, tantôt en anglais selon les sections ; l'application, elle, n'affiche qu'une langue à la fois. Les correspondances sont directes (Filtres / Filters, Enregistrer / Save, Lancer / Launch, Revenir / Go back…).

**Sans couleurs.** F2 → **Display: colours** (*Affichage : couleurs*) coupe ou rétablit les couleurs (`"colors": false` dans `config.json`). La variable d'environnement `NO_COLOR` (toute valeur non vide, convention [no-color.org](https://no-color.org)) les coupe aussi, et passe avant la préférence : l'action l'explique au lieu de basculer pour rien. Sans couleurs, aucune séquence de couleur n'est envoyée au terminal ; la sélection reste visible (vidéo inverse et `▶`), les valeurs indicatives sont atténuées, et chaque état garde son texte ou son signe (`Error:`, `+ Online`, `● Modifiée`…). Le contenu des écrans est identique caractère pour caractère.

**Symboles ASCII.** F2 → **Display: ASCII symbols** (*Affichage : symboles ASCII*, `"ascii": true`) remplace cadres et signes par des caractères ASCII pour les polices ou terminaux qui ne les affichent pas : `+`, `-`, `|` pour les cadres, `>` pour `▶`, `o` pour `●`, `+` pour `✓`, `=` pour `≡`, `v` pour `▼`, `^v<>` pour les flèches, `|` pour `•`. Les lettres accentuées du texte et les données de l'utilisateur (noms d'hôtes, notes) ne sont pas réécrites.

**Largeurs.** La légende de la liste choisit sa version selon la largeur, puis renonce à une ligne qui serait coupée plutôt que de l'afficher sans sa fin : une langue plus longue ne perd jamais la fin d'une ligne. En français, le rappel de la séquence rapide (`Ctrl+G : rapide`) apparaît quand une ligne a la place pour lui (par exemple à 100 ou à 140 colonnes, pas à 120) ; l'aide et F2 l'indiquent toujours. L'aide occupe jusqu'à 126 colonnes.

**Terminal trop petit.** En dessous de 24 × 6, la liste des hôtes annonce « Terminal too small (L×H) » / « Terminal trop petit » avec la taille minimale, au lieu d'un tableau tronqué. Formulaires, sélecteurs et confirmations gardent leur disposition compacte, avec le contrôle actif et le curseur ; Échap y revient toujours d'un niveau.

Non pris en charge : lecteurs d'écran et souris. Sur un thème clair, l'affichage sans couleurs sert de repli.

### Rechercher, filtrer, naviguer

L'accueil distingue trois états, et un seul élément a le focus (son cadre porte un `●` en plus de la couleur) :

- **Saisie** : `/`, Ctrl+F ou Tab ouvrent la zone « ● Recherche », avec le curseur natif. Chaque caractère et chaque geste d'édition y est du texte ; Entrée connecte l'hôte sélectionné. Le bas du cadre, à gauche, affiche le décompte, par exemple « 12 hôtes sur 240 ».
- **Navigation** : Tab ou Échap rendent le focus au tableau « ● Hôtes ». La zone de saisie disparaît entièrement : aucun champ inactif ne reste à l'écran, et le tableau gagne deux lignes. La requête et la position de son curseur sont conservées pour la prochaine édition.
- **Filtres actifs** : un bandeau unique « Filtres : … » récapitule tout ce qui restreint la liste — texte de la recherche, « Favoris seuls », « Masqués visibles », regroupement, et sur sa propre ligne « Fichier : chemin exact ». Il rappelle les touches utiles (`/` modifier, Retour arrière effacer, `C`/F2 tous les fichiers) et le décompte d'hôtes. Sans filtre, il se réduit à « / Rechercher ». Sur un terminal court, tout tient sur une ligne, le fichier en tête.

**Échap ne vide plus la recherche** : il ferme seulement la saisie, le filtre reste appliqué et affiché. *Changement par rapport aux versions précédentes, où le premier Échap effaçait la requête.* Effacer est une action explicite : **Retour arrière depuis le tableau** (« Effacer la recherche »), Ctrl+U dans la saisie, ou F2 → « Effacer tous les filtres », qui retire aussi le filtre par fichier, les favoris seuls et l'affichage des masqués, sans toucher au regroupement.

Le décompte « n hôtes sur total » compte les hôtes retenus par les filtres, jamais les lignes de groupe ; la position de sélection et la plage de lignes restent sous le tableau. Une erreur de syntaxe de recherche s'affiche juste au-dessus de la zone, et la liste reste telle quelle.

### Créer, modifier, dupliquer un hôte

Le formulaire d'hôte tient sur **une page**, sans onglets, et son titre nomme le mode : « Hôtes > Ajouter un hôte », « Modifier alpha », « Dupliquer alpha ».

- **Fichier cible toujours visible**, en tête. Pour un ajout ou une copie, le bouton **Changer** ouvre le sélecteur de fichiers puis revient au même brouillon : valeurs, curseurs et focus sont conservés, et les tags hérités suivent le nouveau fichier. Un bloc existant se déplace par l'action Déplacer (`m`), qui affiche sa source et sa destination.
- **L'essentiel d'abord** : Alias, Adresse, Utilisateur, Port, Clé SSH (avec **Choisir**, équivalent de F9). Trois sections dépliables suivent : **Alias supplémentaires**, **Options avancées** (ProxyJump, ProxyCommand, options SSH, commande distante, RequestTTY) et **Tags et note**. Elles sont fermées pour un ajout ; à l'édition ou à la copie, une section qui contient déjà des valeurs est ouverte, et son titre indique ce qu'elle contient.
- **Un seul ordre de parcours** : Tab et ↓ avancent, Maj+Tab et ↑ reculent, à travers les champs, les titres de section et les boutons, en boucle. Tab ne déplie jamais une section ; Entrée ou Espace sur son titre la déplie ou la replie.
- **Entrée ne soumet plus** : dans un champ elle passe au champ suivant, et après le dernier elle place le focus sur **Prévisualiser les modifications**. Seul ce bouton, ou Ctrl+S depuis n'importe où, ouvre l'aperçu ; l'écriture demande ensuite la confirmation de l'aperçu. *Auparavant Entrée sur le dernier champ de l'onglet Avancé lançait l'enregistrement.*
- **Un champ vide n'écrit rien, et le dit** au lieu d'afficher un exemple grisé ressemblant à une valeur : « requis », « selon OpenSSH », « 22 — défaut OpenSSH, non écrit ». L'exemple du champ actif s'affiche sous le formulaire (« ex. : … »). *Changement : un nouvel hôte créé sans utilisateur n'écrit plus `User <utilisateur local>` ; OpenSSH utilise alors de lui-même le nom local, ou la valeur d'un bloc `Host *`. Ce que vous saisissez est ce qui est écrit.*
- **Alias supplémentaires** : commandes visibles **+ Ajouter un alias** et **Retirer** (Alt+A et Alt+D restent des accélérateurs). Un nouvel hôte peut désormais déclarer plusieurs alias dès sa création. Ils partagent le même bloc ; favoris, notes et connexions visent un alias précis.
- **Note personnelle** : à l'édition, la section Tags et note donne accès à la note, conservée dans les données de `ssho` et non dans le fichier SSH ; son éditeur revient au brouillon.
- **Erreurs** : un résumé en tête, le message sous le champ concerné, le focus sur ce champ, et seule la section de la première erreur est dépliée. Les autres valeurs sont conservées.
- **Édition de texte** : Ctrl+K efface de nouveau jusqu'à la fin de la ligne ; Ctrl+J/Ctrl+K ne changent plus d'onglet.

**Grande largeur** (plein écran Full HD, 4K, 32:9) : le formulaire ne dépasse pas 96 colonnes et chaque bouton suit immédiatement sa valeur (« Choisir » après la clé, « Retirer » après un alias, « Modifier » après la note, « Changer » après le chemin). Aucun bouton n'est repoussé au bord droit. De même, le tableau d'hôtes, celui des tunnels actifs et celui des profils ne s'étalent plus au-delà de 160 colonnes, et le décompte « n hôtes sur total » suit le bandeau des filtres au lieu d'être aligné à droite.

Sur un terminal de moins de 44 colonnes, le libellé passe au-dessus de sa saisie. Le curseur natif reste visible dans le champ actif ; un bouton ou un titre de section ayant le focus n'a pas de curseur et ne reçoit ni frappe ni collage.

### Aide, messages et travaux en cours

- **Aide contextuelle** (`?`, F1, ou « aide » dans F2 depuis une fiche ou un formulaire) : elle commence par dire à quoi sert l'écran d'où elle a été ouverte (liste, fiche, formulaire d'hôte, tunnel, note), puis liste les actions avec les raccourcis réellement actifs. `/` y lance une **recherche** : seules les lignes correspondantes restent, casse et accents ignorés ; pendant la saisie les lettres sont du texte, Échap efface la recherche avant de fermer l'aide.
- **Niveaux de message en toutes lettres** : `Error:`, `Warning:`, et une confirmation brève après chaque écriture (« ✓ Saved alpha », « ✓ Deleted beta », « ✓ Moved … to … ») qui disparaît à la touche suivante. Un avertissement n'est plus présenté comme une erreur.
- **Message lisible en entier** : une erreur trop longue pour sa ligne se termine par « (!: full text) ». `!` (ou F2 → « Messages et dernière erreur ») ouvre un écran qui affiche en entier, avec retour à la ligne et défilement, la dernière erreur, les avertissements en cours (raccourcis refusés, FileTags ignorés) et le dernier résultat SSH. Lire un message ne l'efface pas. Dans l'éditeur de colonnes et les sélecteurs, une erreur occupe désormais le nombre de lignes dont elle a besoin.
- **Travaux en cours** : tant qu'un tunnel possédé, un lot ou une sonde de disponibilité tourne, une ligne « Jobs — tunnels: 2 active (F7: follow or stop) • batch: 3/10 done (B in the list: follow or cancel) • availability: 5 probe(s) running » reste affichée en bas de **tous** les écrans. Une sonde en cours n'y est confondue ni avec une mesure passée ni avec une session SSH.

### Colonnes de la liste

L'éditeur de colonnes (`v`) montre, sous la liste à cocher, le **résultat à la largeur courante** : « Shown at this width: Name | Hostname | Tags » dans l'ordre final, puis « Active but short of room: … » pour les colonnes activées mais masquées faute de place. Une colonne désactivée (`[ ]`, « off ») ne se confond donc plus avec une colonne manquant de place (« hidden: width »). `Name` reste obligatoire et première (« fixed »).

Les actions sont nommées et atteintes par Tab : **Move up**, **Move down**, **Reset**, **Apply**, **Cancel**. Déplacer une colonne est ainsi une action distincte du choix de la ligne, sans avoir à mémoriser Maj+↑/↓ (toujours disponible, comme Espace, `r`, Entrée et Échap). Le déplacement ne boucle pas, et tout reste un brouillon jusqu'à Apply.

### Aperçu des modifications, suppression et restauration

Avant toute écriture dans un fichier SSH, l'aperçu affiche d'abord un **résumé en clair** : l'opération (`ADD`, `EDIT`, `COPY`, `DELETE`, `MOVE`, `RESTORE`) et l'hôte concerné, le ou les fichiers (source → destination pour un déplacement), et ce qu'il advient des données personnelles — par exemple « Favorite, note and tunnel profiles follow a renamed alias », ou qu'elles restent en place après une suppression. Une suppression nomme donc son objet sans dépendre de la couleur du diff.

- Le **diff a le focus pour la lecture**. *Entrée n'y confirme plus rien* : un message le rappelle et aucun octet n'est écrit. Tab atteint le bouton nommé selon l'opération — **Save**, **Copy**, **Delete**, **Move** ou **Restore** — puis **Back**. Entrée répétée depuis le formulaire ne peut donc plus valider deux étapes d'un coup.
- **Ctrl+S** est la confirmation explicite, affichée en pied de page : Ctrl+S ouvre l'aperçu depuis le formulaire, Ctrl+S le confirme.
- Échap ou **Back** reviennent au brouillon, intact.
- **Sauvegardes (F8)** : taper filtre la liste par date, fichier ou type de modification ; Entrée ouvre l'aperçu de restauration, qui suit les mêmes règles. Une provenance inconnue reste signalée et non restaurable.

### Clés : actions locales et distantes séparées

L'écran des clés annonce son contexte. Ouvert par `K`, il **gère les clés** de l'alias sélectionné ; ouvert par F9 ou le bouton Choisir d'un formulaire, il sert seulement à **choisir une clé pour le brouillon** (« only its IdentityFile is filled, nothing is deployed ») et ne propose ni association ni déploiement.

- **Actions nommées**, regroupées par portée : `Local:` **Create key**, **Add to agent**, **Associate with this host** • `Remote server:` **Deploy public key**. ←/→ ou Tab choisit, Entrée lance ; `n`, `a`, `t`, `d` restent des accès directs. Dans le sélecteur d'un brouillon : **Use this key**, Create key, Add to agent.
- **Génération** : chemin et commentaire, puis les boutons **Continue in terminal** et **Back**. *Entrée dans un champ ne génère plus : elle passe au champ suivant puis au bouton.* L'écran annonce que `ssh-keygen` demandera la phrase secrète dans le terminal avant le retour à l'application.
- **Déploiement** : la revue indique en tête qu'elle agit sur le serveur distant, l'alias exact et l'empreinte publique, puis propose **Back** (focus initial) et **Deploy to alias**. *Entrée seule ne déploie plus.* Le gel de la cible et la revalidation de la clé publique sont inchangés.
- Les phrases secrètes restent entièrement confiées aux outils natifs ; aucun coffre n'est ajouté.

### Lots : un parcours en étapes

Une ligne en tête situe le parcours : **Targets → Command → Review → Results**, l'étape en cours entre crochets.

- **Aucune cible marquée** : `B` ouvre une page qui l'explique et nomme deux issues, **Back to the list** (focus initial) et **Add the n host(s) shown by the filter**. Le filtre n'est jamais transformé en cibles sans ce choix, et aucune commande ne peut être saisie avant d'avoir une cible.
- **Préparation** : Tab parcourt la commande, le parallélisme, le délai, la liste des cibles puis les boutons **Review** et **Back**. Les flèches agissent dans le contrôle actif : elles ne déplacent la sélection des cibles que lorsque la liste a le focus (son cadre l'annonce), sinon elles passent au contrôle voisin. Entrée avance d'un champ au suivant et s'arrête sur Review.
- **Vérification** : commande complète, parallélisme, délai et cibles figées ; boutons **Back to edit** (focus initial) et **Launch on n target(s)**. *Entrée ne lance plus le lot : elle valide le choix marqué, donc revient à l'édition tant que Launch n'a pas été choisi avec Tab. Ctrl+L, affiché en pied de page, reste la confirmation explicite.*
- **Résultats** : deux onglets nommés **Output** et **Errors** (Tab), l'onglet ouvert marqué `▶`, troncature signalée. Revenir ne stoppe pas le lot ; `s` l'annule. Tant qu'un lot tourne, la ligne « Jobs » le rappelle en bas de tous les écrans.

### Tunnels : formulaire, profils et suivi

- **Sens du transfert en clair** : le type se choisit parmi trois options nommées — Local (-L), Remote (-R), Dynamic (-D) — et une phrase calculée dit le trajet avec vos valeurs, par exemple « Listen on this machine, port 5433 → reach db.internal:5432 from the SSH server ». Pour un transfert distant la phrase s'inverse. Les libellés distinguent le port d'écoute de la destination.
- **Mode de lancement visible** : « Shell session » ou « Tunnel only », avec l'explication de leur cycle de vie (le tunnel s'arrête en quittant le shell, ou tourne en arrière-plan et se suit dans F7). Ctrl+T reste un raccourci.
- **Boutons distincts** **Launch**, **Save profile** et **Cancel**, atteints par Tab. Le nom du profil n'est requis que pour enregistrer. Entrée avance de champ en champ et s'arrête au dernier : la répéter n'enregistre ni ne lance rien. Ctrl+S et Ctrl+L restent disponibles.
- **Profils (F6)** : sous la liste, le trajet du profil sélectionné en toutes lettres et les actions **Open**, **Launch**, **Duplicate**, **Delete**, **New** (←/→ pour choisir, Entrée pour lancer l'action). Dupliquer ouvre un brouillon non enregistré nommé `nom-copy`. Sans profil, seul New agit.
- **Tunnels actifs (F7)** : `s` n'arrête plus directement ; une confirmation nomme le tunnel (identifiant, hôte, ports) et garde le focus sur **Revenir**. *Auparavant `s` arrêtait immédiatement.* Revenir de l'écran n'arrête rien.

### Notes personnelles

L'éditeur de note (`N`, ou le bouton Note d'une fiche ou du formulaire) indique à quoi la note se rattache et où elle vit : l'alias, le fichier SSH de l'hôte, puis le fichier `metadata.json` des données de `ssho`, avec le rappel qu'une note n'est jamais écrite dans le fichier SSH et ne se confond pas avec un tag. L'état **Enregistrée** ou **● Modifiée** est affiché en tête.

- Entrée insère une nouvelle ligne ; Tab atteint les boutons **Enregistrer** et **Annuler** (←/→ passe de l'un à l'autre, Entrée ou Espace l'active) ; Ctrl+S enregistre depuis le texte. Un bouton ayant le focus ne reçoit ni frappe ni collage.
- Annuler une note modifiée propose toujours de continuer l'édition.
- Dans la liste, un hôte qui porte une note affiche le signe discret `≡` entre sa disponibilité et son favori ; le texte de la note reste dans la fiche et le volet F4, sans colonne dédiée.

### Fiches d'hôte et de groupe

La fiche d'un hôte (`i`) est organisée en cinq sections, les mêmes que le volet F4 : **Résumé** (alias, adresse, utilisateur, port, alias du bloc, tags, note), **Configuration effective** (résolution `ssh -G`, clés, proxys), **Déclaration et source** (fichier et ligne, cible déclarée, options), **Disponibilité mesurée** (méthode SSH ou ICMP, état, durée, cause) et **Historique des connexions**. Une tentative, une fin avec code 0, un état de disponibilité et une authentification restent des informations distinctes, chacune sur sa ligne.

- **Lire ne modifie jamais** : Entrée seule ne fait rien et le rappelle. *Auparavant Entrée ouvrait l'édition.* Les actions sont nommées sous la fiche — **Connexion**, **Modifier**, **Note**, **Copier**, **Fermer** — et Tab ou ←/→ y placent le focus avant qu'Entrée ne les lance ; `e` reste un accès direct à l'édition, F2 donne les autres actions.
- **Copier** ouvre un choix d'une seule valeur (alias, commande `ssh alias`, adresse, `utilisateur@adresse`, fichier source). La copie est demandée au terminal par la séquence OSC 52, puis la valeur reste affichée seule sur sa ligne : si le terminal ou le multiplexeur n'honore pas la demande (tmux exige `set -g set-clipboard on`), elle se sélectionne à la main. Rien d'autre n'est exporté.
- La **fiche d'un groupe** indique son critère : hôtes d'un fichier, ou hôtes de ce fichier partageant un tag principal.

### Signes de la liste, groupes et sélection multiple

Chaque signe de la colonne Name a sa zone, pour qu'un favori ne ressemble jamais à une cible de lot :

| Zone | Signe | Sens |
| --- | --- | --- |
| Cible de lot | `[x]` / `[ ]` | L'alias est marqué pour un lot. Cette zone n'existe que lorsqu'au moins une cible est marquée. |
| Disponibilité | `+` `!` `~` `?` | En ligne, hors ligne, mesure en cours, inconnu. |
| Note | `≡` | L'hôte porte une note personnelle. |
| Favori | `*` | Hôte épinglé. |
| Groupe | `▼` / `▶` | En-tête ouvert ou replié ; « (4 • 2 cibles) » donne ses hôtes et ses cibles marquées, même replié. |
| Alias | `principal→actif`, `+2 alias` | Bloc regroupé : alias utilisé pour la connexion, et nombre d'autres alias du bloc (F3 pour choisir). |

Une **légende** des signes présents s'affiche au-dessus du pied de page lorsque le terminal a au moins 20 lignes ; l'aide (`?`) la donne toujours.

- **Sélection multiple visible** : dès qu'une cible est marquée, la ligne « Lot : 4 cibles marquées, dont 1 hors du filtre • B : voir / préparer • F2 : tout retirer » reste affichée. Les marques sont des cibles figées : une recherche, un filtre ou un groupe replié ne les retirent pas et ne faussent pas le compte.
- **Entrée** connecte sur un hôte et ouvre ou replie sur un en-tête de groupe ; le pied de page l'indique selon la ligne sélectionnée.
- **Un en-tête de groupe n'est jamais la cible silencieuse d'une action** : `x`, `e`, `d`, `m`, `N`… sur un en-tête affichent « … — Sélectionnez un hôte, pas un groupe » au lieu de ne rien faire. Pour cibler tout un groupe, F2 → « Lot : cibler les résultats du filtre » après avoir filtré.

### Sélecteurs communs

Le choix d'un fichier de destination, du filtre par fichier source, d'un alias exact, du regroupement et de la méthode de disponibilité utilisent un même écran :

- un **titre** et une ligne disant ce que l'on choisit ; la **valeur en vigueur** porte une coche `✓` et la mention « (actuel) », distinctes du curseur `▶` ;
- ↑/↓ **bouclent**, Début/Fin vont aux extrémités, **Entrée** applique, **Échap ou Ctrl+C** annulent sans rien changer ;
- une **zone de détail** sous la liste décrit le choix survolé : chemin complet et nombre d'hôtes d'un fichier (« aucun hôte : fichier vide… »), fichier et portée d'un alias, ce que mesure SSH ou ICMP, effet d'un mode de regroupement ;
- les **chemins** s'affichent en entier tant que la largeur le permet ; sinon sous la plus courte fin qui les distingue (`…/a/hosts` et `…/b/hosts`), le chemin complet restant dans la zone de détail. « Tous les fichiers » est un choix à part entière ;
- dans les sélecteurs de fichiers et d'alias, **taper réduit la liste** (casse et accents ignorés, sur le chemin complet). Réduire ne choisit rien : la coche reste sur la valeur en vigueur jusqu'à Entrée. *Dans ces trois sélecteurs les lettres sont donc du texte de recherche : `j`, `k` et `q` n'y naviguent plus et n'y ferment plus ; ils gardent ce rôle dans les sélecteurs de regroupement et de méthode, qui n'ont pas de recherche.*

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

Les touches s'écrivent `a`, `D` (la casse compte pour un caractère), `f6`, `ctrl+f`, `alt+x`, `enter`. Une préférence en conflit n'est jamais appliquée en silence : elle est ignorée, un bandeau « Raccourcis : n préférence(s) refusée(s) » apparaît dans la liste et l'aide en donne le motif (« d sert déjà à « Supprimer » »). La palette F2, le pied de page et l'aide affichent toujours les touches réellement actives. Ces préférences se modifient dans le fichier JSON ou avec l'éditeur de raccourcis de l'écran Préférences.

Identifiants d'action : `host.connect`, `host.add`, `host.edit`, `host.duplicate`, `host.move`, `host.delete`, `host.info`, `host.details`, `host.favorite`, `host.note`, `host.alias`, `tunnel.form`, `tunnel.profiles`, `tunnel.active`, `availability.test`, `availability.method`, `search.focus`, `search.clear`, `filter.clear`, `filter.favorites`, `filter.hidden`, `filter.source`, `filter.source.clear`, `view.aliases`, `view.groups`, `view.columns`, `sort.toggle`, `sort.name`, `sort.recent`, `batch.mark`, `batch.select_visible`, `batch.clear`, `batch.open`, `batch.results`, `keys.open`, `app.backups`, `app.reload`, `app.palette`, `app.help`. La sortie se règle par `quit_keys`.

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
| Palette d'actions | F2 depuis la recherche, le tableau, une fiche ou un formulaire ; taper pour chercher, ↑/↓ en boucle, Entrée lance ; Échap annule sans changer la recherche ni le focus |
| Changer le focus | Tab ou Shift+Tab ; `/` pour revenir à la recherche depuis le tableau |
| Effacer ou revenir | Échap ferme la saisie sans effacer le filtre ; Retour arrière depuis le tableau ou Ctrl+U dans la saisie effacent la recherche ; depuis le tableau, Échap ne quitte pas (`q` ou Ctrl+C, selon `quit_keys`) |
| Tableau | ↑/↓ ou j/k ; a ajouter, e modifier, d supprimer avec confirmation, m déplacer, D dupliquer |
| Défilement du tableau | Page précédente/suivante ; Début/Fin pour le premier/dernier hôte |
| Informations et aide | i pour la fiche de l'hôte, h pour l'aide |
| Détails à côté de la liste | F4 depuis la recherche ou le tableau ; Alt+↑/↓ pour défiler ligne par ligne, Ctrl+Page précédente/suivante par page |
| Connectivité et visibilité | P/F2 choisit SSH ou ICMP local ; p relance les contrôles, H pour afficher les hôtes portant le tag `hidden` |
| Favoris et notes | `*` épingle/désépingle, `N` ouvre la note personnelle ; F2 → Favoris seuls active/désactive le filtre |
| Note personnelle | Ctrl+S enregistre, Entrée ajoute une ligne, ↑/↓ et Début/Fin déplacent le curseur ; Échap/Ctrl+C annule avec confirmation si modifiée |
| Tri | s alterne nom/date, n trie par nom, r par dernière connexion |
| Tunnel | f ; ←/→ pour Local, Remote ou Dynamic, puis ports et adresses |
| Formulaire hôte | Tab/↓ et Maj+Tab/↑ parcourent champs, sections et boutons ; Entrée : champ suivant ou activer ; Entrée/Espace sur un titre : déplier ; Ctrl+S : prévisualiser ; Échap ou Ctrl+C : annuler |
| Assistant de clés | K depuis le tableau ou F2 → Clés OpenSSH ; F9 depuis un formulaire remplit seulement IdentityFile ; ↑/↓, Page précédente/suivante, Début/Fin naviguent ; F4 affiche les détails/erreurs complets |
| Profils de tunnels | F6 ou F2 → Profils de tunnels ; a crée, Entrée/e/r édite ou renomme, d supprime avec confirmation, Ctrl+L lance |
| Sauvegardes | F8 ou F2 → Sauvegardes ; taper pour chercher ; Entrée ouvre le diff ; Tab puis Entrée sur Restore, ou Ctrl+S, confirme ; Échap revient |
| Aperçu SSH | Ajout/édition/copie/suppression/déplacement : diff par fichier ; Entrée applique les octets montrés, Échap retrouve la saisie |
| Commandes multi-hôtes (Unix) | x marque l’alias actif ; B prépare le lot ; F2 sélectionne explicitement les résultats du filtre, efface les marques ou ouvre les derniers résultats |
| Préparation du lot | Tab : commande, concurrence, délai, cibles, boutons ; ↑/↓ : cible si la liste a le focus, sinon contrôle voisin ; Entrée : champ suivant puis Review ; Ctrl+L : vérification, puis Ctrl+L ou le bouton Launch pour lancer ; Échap : retour |
| Résultats du lot | ↑/↓ : hôte ; Tab : Output / Errors ; Page précédente/suivante : sortie ; s : annuler le lot ; q, Échap ou Ctrl+C : retour, le lot continue |
| Tunnels actifs | F7 ou F2 → Tunnels actifs ; s demande confirmation puis arrête le processus sélectionné ; q, Échap ou Ctrl+C reviennent sans rien arrêter |
| Formulaire tunnel | Ctrl+T choisit session shell ou tunnel seul ; Ctrl+S enregistre le profil nommé sans SSH ; Ctrl+L lance le forwarding courant ; Entrée avance sans lancer ; Échap/Ctrl+C annule |
| Édition de texte | Ctrl+A/E ou Début/Fin pour début/fin de ligne ; ←/→ ou Ctrl+B/F pour un caractère ; Ctrl+←/→ ou Alt+B/F pour un mot ; Retour arrière/Ctrl+H efface avant, Suppr/Ctrl+D après ; Ctrl+U efface avant le curseur, Ctrl+K après, Ctrl+W ou Alt+Retour arrière efface le mot précédent |
| Abandon des modifications | Tab ou ←/→ choisit entre Continuer l'édition (focus initial) et Abandonner ; Entrée valide le choix marqué ; Échap ou Ctrl+C reprend exactement le formulaire |
| Alias supplémentaires | Boutons « + Ajouter un alias » et « Retirer » dans la section ; Alt+A ajoute, Alt+D retire l'alias supplémentaire actif |

Chaque erreur apparaît sous son champ et la validation place le focus sur la première erreur, en dépliant sa section. Le curseur natif reste visible pendant la saisie, y compris avec caractères larges, accents combinés et emoji. Une annulation quitte immédiatement un formulaire inchangé ; seules des valeurs, des alias ou un type de tunnel modifiés demandent confirmation. Revenir aux valeurs initiales enlève cette confirmation. Dans la confirmation, Échap ou Entrée sur le choix marqué « Continuer l'édition » reprennent le formulaire avec son texte, son curseur et sa navigation ; « Abandonner », choisi avec Tab puis Entrée, revient à la vue parente.

Les hôtes masqués restent accessibles en connexion directe. La recherche de la TUI combine plusieurs mots sur le nom, l'adresse, l'utilisateur et les tags. La recherche CLI conserve ses propres filtres de champs.

**Palette F2.** F2 ouvre une palette recherchable de toutes les actions, depuis la recherche comme depuis le tableau :

- **Chercher** : taper directement des mots du nom, d'un synonyme ou de la catégorie (`tunnel`, `copier`, `methode`, `cles`). La recherche ignore la casse et les accents ; tous les mots doivent correspondre. Dans la palette, les lettres ne sont que du texte de recherche : `d` ou `q` ne suppriment ni ne quittent. Une correspondance unique n'est jamais lancée d'elle-même.
- **Choisir** : ↑/↓ (ou Ctrl+P/Ctrl+N) **bouclent** de la première à la dernière action affichée et inversement, sans s'arrêter sur les titres de catégorie ; Page précédente/suivante et, recherche vide, Début/Fin. **Entrée** lance uniquement l'action sélectionnée. Échap, Ctrl+C ou F2 referment la palette sans rien changer à la recherche, au focus ni à la sélection de la liste.
- **Colonnes Action / Raccourci / État** : un libellé ne contient jamais de touche ; la colonne Raccourci affiche la touche réellement active (`—` si l'action n'existe que dans F2) ; État donne la valeur courante (« activé », méthode SSH/ICMP, mode de groupes, fichier filtré, tunnels actifs, cibles marquées). Les libellés restent entiers à 80 colonnes.
- **Catégories** : Connexion, Hôte, Organisation, Outils, Application. Une ligne sous la liste décrit l'action sélectionnée.
- **Indisponibilité motivée** : une action impossible reste listée en gris avec son motif (« Sélectionnez un hôte », « Sélectionnez un hôte, pas un groupe », « Aucun filtre de fichier actif », « Aucune cible marquée », « Aucun lot lancé »). Entrée sur cette ligne l'explique et ne lance rien.
- **Palette contextuelle** : F2 fonctionne aussi depuis une fiche d'hôte (actions sur cet hôte) et depuis un formulaire d'hôte, de tunnel ou une note. Dans ce dernier cas le brouillon reste intact sous la palette, qui ne propose que les écrans d'où l'on revient au brouillon (Tunnels actifs, Aide).

Sans hôte sélectionné, Ajouter, Aide, Recharger, Tunnels actifs, Sauvegardes, les filtres, les groupes et les actions de lot (sauf marquer un alias) restent disponibles ; « Favoris seuls » permet donc de sortir d'un filtre sans résultats. « Tester la disponibilité » réutilise le contrôle de tous les hôtes, comme `p`. La barre sous le tableau adapte ses rappels à la largeur ; sur un écran étroit, F2 puis « aide » donne accès aux raccourcis détaillés.

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

**Profils de tunnels** : F6 ouvre les profils de l’alias sélectionné, même depuis la recherche ; la palette F2 propose aussi « Profils de tunnels ». Plusieurs profils local (`-L`), distant (`-R`) et SOCKS (`-D`) peuvent coexister. Le nom accepte jusqu’à 100 caractères, sans caractères de contrôle ni espaces de début/fin ; les noms sont sensibles à la casse. Modifier le champ nom puis Ctrl+S renomme le profil ouvert ; un nom déjà occupé est refusé sans écrasement. Ctrl+S reste dans le formulaire, indique « Profile saved » et ne lance aucun SSH. Ctrl+L lance les valeurs actuelles selon le mode choisi, y compris un brouillon non enregistré ou modifié ; le lancement ne sauvegarde pas implicitement un profil. Entrée sur le dernier champ ne lance rien. `d` demande confirmation avant suppression. Échap annule sans écriture, avec confirmation seulement si le formulaire a changé.

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

# Tous les parcours en pseudo-terminal d'un coup (Linux, Python 3) : chaque
# scénario reçoit un dossier de fixture neuf et court, supprimé à la fin.
sh scripts/pty-checks.sh target/release/ssho

# Ou un par un. Parcours dans un pseudo-terminal, Linux/macOS avec Python 3
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

# Parcours ergonomiques UX (clavier, focus, retour) dans un dossier de fixture neuf.
# Ce script lit l'interface en français ; les autres scripts PTY la lisent en anglais.
python3 tests/pty_ux.py target/release/ssho "$(mktemp -d)/ux"

# Comparaison facultative avec un exécutable SSHM
python3 tests/reference_parity.py /path/to/sshm target/release/ssho
```

**Intégration continue.** `.github/workflows/ci.yml` lance ces vérifications à chaque push et pull request, sans rien publier : un contrôle rapide sous Linux (formatage, Clippy sans avertissement, tests Rust), les tests avec Rust 1.88 (version minimale déclarée), les scripts d'installation et de paquetage (`sh -n`, `tests/distribution.py`), puis la compilation et les tests sur les quatre cibles annoncées (Linux et macOS, x86_64 et ARM64, chacune sur son exécuteur natif). Les parcours en pseudo-terminal y tournent sous Linux seulement ; ils n'ont pas été adaptés à macOS. Le workflow `release.yml` reste le seul à produire des archives.

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

Depuis le tableau, `c` ouvre les fichiers SSH connus, y compris les fichiers vides, avec une entrée « Tous les fichiers » ; `C` retire ce filtre. Les mêmes actions figurent dans F2, également depuis une recherche sans résultat. Le chemin exact sélectionné apparaît dans le bandeau « Filtres » (ligne « Fichier : … ») et dans l’aide. Ce filtre se compose avec la requête, les favoris et `H` : même un fichier sans résultat conserve son filtre après recherche, bascule `H` ou F5. Seuls une suppression ou un déplacement du dernier hôte du fichier le retirent automatiquement, en comptant aussi les hôtes masqués et hors recherche. La requête, son curseur et le focus restent conservés pendant le choix du fichier. Deux fichiers de même nom restent distincts par leur chemin complet. Pendant la saisie, `c` et `C` restent du texte.

Exemples : `tag:prod AND user:deploy is:online`, `"maintenance prévue" ET is:favorite`, `file:"Team Hosts" is:unknown`. Les filtres d’hôtes masqués et « Favoris seuls » s’appliquent en plus de la requête. Les correspondances du tableau, du panneau de détails et de la fiche sont soulignées et mises en évidence sans découper les caractères Unicode ; les marqueurs d’état/favori restent distincts. La requête reste présente après SSH et F5. Après ajout, édition ou déplacement dans le fichier actif (ou sans filtre de fichier), une expression compatible avec l’hôte enregistré est conservée ; une expression incompatible est effacée pour montrer cet hôte. Les valeurs résolues précédentes servent uniquement à une vérification provisoire. Même une modification explicite d’utilisateur/adresse attend le résultat natif avant d’effacer une recherche compatible avec ces valeurs : une directive globale ou un bloc `Host *` antérieur peut conserver la priorité. Ce contexte provisoire est conservé entre plusieurs enregistrements avant réception du résultat, sans être publié dans le cache résolu. Un enregistrement hors du fichier filtré conserve ce filtre et reste masqué. Dans le fichier actif, l’hôte enregistré reste visible pendant la nouvelle résolution ; son résultat frais confirme la recherche ou l’efface si l’héritage a changé (notamment après déplacement). Une erreur de résolution tranche avec les valeurs déclarées, et toute nouvelle saisie de recherche annule cette attente.

Cette syntaxe concerne uniquement l’interface interactive. La commande CLI `ssho search` conserve sa recherche historique et ses formats de sortie.

Le collage encadré (*bracketed paste*) est activé pendant l’interface et désactivé pour SSH et à la sortie. Dans la recherche et les champs simples, les retours à la ligne et tabulations deviennent des espaces ; dans les notes, les retours à la ligne sont conservés (CRLF/CR normalisés en LF). Les autres caractères de contrôle sont ignorés. Le collage modifie uniquement le champ actif, sans exécuter de raccourci ni valider un formulaire ; dans le tableau, les palettes et les confirmations, il est ignoré. Ce comportement nécessite un terminal qui envoie les séquences de collage encadré.
