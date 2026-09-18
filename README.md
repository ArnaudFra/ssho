# ssho

**ssho** (SSH Organizer) est un gestionnaire de connexions SSH pour le terminal, écrit en Rust. Les hôtes restent dans `~/.ssh/config` et ses fichiers `Include`.

Le projet s'est fortement inspiré de [SSHM](https://github.com/Gu1llaum-3/sshm) à ses débuts : j'utilise cet outil au quotidien, mais il lui manquait de nombreuses fonctionnalités, ce qui m'a poussé à en écrire une version en Rust et à la faire évoluer. Les données d'une installation [SSHM](https://github.com/Gu1llaum-3/sshm) existante sont reprises au premier lancement.

Le paquet et l'exécutable s'appellent **`ssho`**.

## Compilation et lancement

Prérequis de compilation : Rust 1.88 ou plus récent, un compilateur C, les outils de compilation usuels et Perl pour OpenSSL embarqué. Les bibliothèques de cryptographie et de compression du contrôle de connectivité sont compilées avec le programme. Le client `ssh` d'OpenSSH doit être accessible dans le `PATH` pour les connexions et les tunnels.

```sh
cargo build --release
./target/release/ssho
```

Installation facultative dans le répertoire Cargo de l'utilisateur :

```sh
cargo install --path . --locked
ssho
```

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
  "key_bindings": {
    "quit_keys": ["q", "ctrl+c"],
    "disable_esc_quit": false
  }
}
```

## Utilisation

```sh
ssho                            # Interface terminal
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

`info` conserve le schéma JSON `sshm.info.v1`, les valeurs nulles et les codes de sortie : 0 pour un hôte trouvé, 2 pour un hôte absent, 1 pour une erreur de configuration. La recherche CLI propose les formats `table`, `json` et `simple`.

L'historique enregistre les tentatives de connexion, avec date, compteur et dernier tunnel utilisé. Les modifications SSH sont précédées d'une sauvegarde dans `ssho/backups/`. Les blocs sans rapport, commentaires, options répétées et clés multiples sont conservés lors de l'édition. Les déplacements préparent les deux écritures et restaurent la destination si l'écriture de la source échoue.

## Navigation au clavier

L'écran principal utilise un titre compact et toute la hauteur disponible pour la liste. **Name** reste visible ; **Hostname**, **Tags**, puis **Last Login** s'ajoutent selon la largeur. Les textes trop longs se terminent par une ellipse `…`, mesurée en colonnes Unicode. Sous le tableau figurent le nombre d'hôtes filtrés/total (y compris les hôtes masqués), la position sélectionnée et la plage de lignes affichée. Les formulaires reprennent les onglets **General / Advanced** et leurs groupes de champs.

| Contexte | Touches |
| --- | --- |
| Recherche | Saisir pour filtrer ; ↑/↓ ou Ctrl+P/Ctrl+N pour sélectionner ; Entrée pour se connecter |
| Palette d'actions | F2 depuis la recherche ou le tableau ; ↑/↓ puis Entrée ; Échap annule sans changer la recherche ni le focus |
| Changer le focus | Tab ou Shift+Tab ; `/` pour revenir à la recherche depuis le tableau |
| Effacer ou quitter | Échap efface d'abord une recherche non vide, puis quitte le focus de recherche ; depuis le tableau, les préférences contrôlent la sortie |
| Tableau | ↑/↓ ou j/k ; a ajouter, e modifier, d supprimer avec confirmation, m déplacer |
| Défilement du tableau | Page précédente/suivante ; Début/Fin pour le premier/dernier hôte |
| Informations et aide | i pour la fiche de l'hôte, h pour l'aide |
| Détails à côté de la liste | F4 depuis la recherche ou le tableau ; Alt+↑/↓ pour défiler ligne par ligne, Ctrl+Page précédente/suivante par page |
| Connectivité et visibilité | p pour relancer les contrôles, H pour afficher les hôtes portant le tag `hidden` |
| Tri | s alterne nom/date, n trie par nom, r par dernière connexion |
| Tunnel | f ; ←/→ pour Local, Remote ou Dynamic, puis ports et adresses |
| Formulaires | Tab/Shift+Tab pour naviguer, Ctrl+J/Ctrl+K pour changer d'onglet, Ctrl+S pour enregistrer/se connecter, Entrée sur le dernier champ pour valider, Échap ou Ctrl+C pour annuler |
| Édition de texte | Ctrl+A/E ou Début/Fin pour début/fin de ligne ; ←/→ ou Ctrl+B/F pour un caractère ; Ctrl+←/→ ou Alt+B/F pour un mot ; Retour arrière/Ctrl+H efface avant, Suppr/Ctrl+D après ; Ctrl+U efface avant le curseur, Ctrl+K après (hors formulaire hôte où il change d’onglet), Ctrl+W ou Alt+Retour arrière efface le mot précédent |
| Abandon des modifications | Entrée confirme l’abandon ; Échap ou Ctrl+C reprend exactement le formulaire |
| Alias en édition | Alt+A ajoute un nom, Alt+D supprime le nom actif lorsqu'il en reste plusieurs |

Sur un terminal court, les champs occupent deux lignes (libellé et saisie). Chaque erreur apparaît sous son champ et la validation place le focus sur la première erreur, dans le bon onglet. Le curseur natif reste visible pendant la saisie, y compris avec caractères larges, accents combinés et emoji. Une annulation quitte immédiatement un formulaire inchangé ; seules des valeurs, des alias ou un type de tunnel modifiés demandent confirmation. Revenir aux valeurs initiales enlève cette confirmation. Échap dans la confirmation reprend le formulaire avec son texte, son curseur et sa navigation ; Entrée abandonne et revient à la vue parente.

Les hôtes masqués restent accessibles en connexion directe. La recherche de la TUI combine plusieurs mots sur le nom, l'adresse, l'utilisateur et les tags. La recherche CLI conserve ses propres filtres de champs.

La palette propose **Ajouter**, **Modifier**, **Tunnel**, **Tester**, **Aide** et **Détails**. Modifier, Tunnel et Tester sont indisponibles sans hôte sélectionné ; Ajouter, Aide et Détails restent accessibles avec une liste vide. Tester réutilise le contrôle de connectivité de tous les hôtes, comme `p`. La barre sous le tableau adapte ses rappels à la largeur ; sur un écran étroit, **F2**, puis **Aide**, donne accès aux raccourcis détaillés. Les lettres saisies dans la recherche restent du texte.

Le tri, la visibilité et le rechargement conservent l'hôte sélectionné par son nom et son fichier source. Un ajout, un renommage ou un déplacement sélectionne l'hôte résultant ; si la recherche précédente le masque, elle est effacée, et les hôtes cachés sont affichés si nécessaire. Une édition ouverte depuis la fiche revient à cette fiche actualisée après sauvegarde, ou à la fiche initiale après annulation. Annuler un ajout ouvert depuis le sélecteur de fichiers revient à ce sélecteur. Les formulaires lancés directement par la CLI quittent après sauvegarde ou après annulation de leur vue initiale.

Le panneau **Détails** est désactivé au lancement et s'ouvre à droite lorsque le terminal atteint **110 colonnes**. Il suit l'hôte sélectionné sans réduire le nombre de lignes du tableau ; les colonnes du tableau s'adaptent à la place restante. En dessous de 110 colonnes, la liste reprend toute la largeur et un rappel indique **Tab, puis i** depuis la recherche, ou **i** depuis le tableau, pour ouvrir la fiche. Agrandir le terminal réaffiche le panneau si son activation a été conservée. Les raccourcis Alt+↑/↓ (ligne par ligne) et Ctrl+Page précédente/suivante (par page) défilent uniquement lorsque le panneau est visible, sans modifier le texte recherché ni l'hôte sélectionné. Alt+↑/↓ reste disponible si le terminal réserve Ctrl+Page précédente/suivante à ses onglets.

Les détails affichent les valeurs du bloc SSH chargé : utilisateur, port, toutes les identités, ProxyJump, ProxyCommand, fichier et ligne source, options et diagnostic de connectivité disponible. Les longues valeurs sont repliées et restent entièrement accessibles par défilement, y compris dans la fiche **i**. Ces valeurs ne représentent pas encore une résolution de la configuration SSH effective.

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

# Comparaison facultative avec un exécutable SSHM
python3 tests/reference_parity.py /path/to/sshm target/release/ssho
```

Tous les tests utilisent des dossiers temporaires. Les parcours de connexion et de tunnel utilisent un faux client SSH ; ils ne se connectent pas aux serveurs personnels de l'utilisateur.

## Versions et plateformes

La vérification des mises à jour interroge, en arrière-plan, le dépôt de releases fixé à la compilation par `SSHO_REPOSITORY=ArnaudFra/ssho` ou `SSHO_RELEASES_URL=https://…` ; sans l'une de ces variables, aucune requête n'est faite. Les préférences et `--no-update-check` désactivent la vérification.

Le programme et les parcours terminal sont testés sous Linux. Les chemins et branches spécifiques macOS/Windows existent mais ne sont pas testés.

## Licence

MIT. Les premières versions adaptent la logique et l'organisation de [SSHM](https://github.com/Gu1llaum-3/sshm) ; la mention de copyright d'origine de Guillaume Archambault est conservée dans [LICENSE](LICENSE).
