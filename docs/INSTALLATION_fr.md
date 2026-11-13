# Compilation et installation avancée

**English:** this guide is also available in English, see [INSTALLATION.md](INSTALLATION.md).

Ce guide décrit la compilation depuis les sources, l'installateur `install.sh`,
le format des archives, le workflow de release, les plateformes prises en charge
et les vérifications de développement. Pour l'usage courant, voir le
[README](../README_fr.md) et le [guide d'utilisation](GUIDE_fr.md).

## Compiler depuis les sources

Rust 1.88 minimum, compilateur C, outils de compilation et Perl sont nécessaires
(OpenSSL et zlib embarqués). Le client OpenSSH `ssh` reste nécessaire à
l'utilisation. Cette méthode compile pour la machine locale :

```sh
cargo build --release --locked
cargo install --path . --locked
```

Cargo installe habituellement dans `$HOME/.cargo/bin`. Pour retirer cette
installation Cargo : `cargo uninstall ssho`. Les commandes ci-dessous
concernent l'installateur d'archives/local, dont le répertoire par défaut est
**`$HOME/.local/bin`**.

## Installer un binaire local ou une archive

L'installateur autonome est `install.sh`. Il n'exécute jamais le binaire
pendant l'installation.

```sh
sh install.sh --binary ./target/release/ssho
sh install.sh --binary ./target/release/ssho --bin-dir "$HOME/tools/bin"
sh install.sh --archive ./ssho-x86_64-unknown-linux-gnu.tar.gz --version 0.1.0
```

Une archive locale doit garder son nom de protocole et être accompagnée de
`ssho-TARGET.tar.gz.sha256`, ou d'un fichier choisi par `--checksum CHEMIN`.
Le checksum, le format, les membres, la version et la cible sont vérifiés avant
publication du binaire. `--version` est facultatif pour une archive locale ;
s'il est fourni, il doit correspondre au manifeste. `--target` sélectionne une
cible explicite ; sinon le couple `uname -s`/`uname -m` détermine la cible native.
Un choix explicite ne rend pas un binaire d'une autre architecture exécutable.
`--binary` accepte uniquement un fichier régulier exécutable non vide nommé
`ssho`, sans lien symbolique : sa provenance et sa compatibilité restent à
vérifier par l'utilisateur, aucun checksum de distribution n'est disponible
pour ce mode.

La destination suit cet ordre, indépendamment de l'ordre des arguments :

1. `--bin-dir DIR` ;
2. `--prefix DIR`, qui ajoute `/bin` ;
3. variable non vide `SSHO_INSTALL_DIR` ;
4. `$HOME/.local/bin`.

Les espaces et les caractères shell sont littéraux lorsqu'ils sont entre
quotes. Les chemins relatifs partent du répertoire courant. Les composants
`..`, les nouvelles lignes et tout composant symbolique de la destination sont
refusés ; choisir son chemin réel (utile si le HOME est lui-même un lien).
L'installateur refuse un fichier existant sauf avec **`--replace`**. Même avec
cette option, un lien symbolique, un dossier ou un fichier spécial est refusé.
Il ne demande jamais `sudo`, ne change aucun profil et ne modifie pas le PATH.
Une destination non accessible en écriture entraîne une erreur.

Après installation, un message indique si un autre `ssho` trouvé dans le
PATH peut masquer celui installé. Vérifier `command -v ssho`, utiliser le
chemin complet ou ajouter manuellement le répertoire voulu au PATH. Un shell
peut conserver une ancienne résolution en cache (`hash -r` sous bash).

## Télécharger une release

Les releases sont téléchargées depuis `ArnaudFra/ssho` par défaut. `--repo OWNER/REPO`
ou `SSHO_REPOSITORY` choisit un autre dépôt, un fork par exemple :

```sh
sh install.sh --version 0.1.0
sh install.sh --version latest --replace
sh install.sh --repo OWNER/REPO --version 0.1.0
```

Sans `--version`, le téléchargement choisit `latest`. Pour une version précise,
le tag est `vVERSION`, sans doubler le `v`. Les versions acceptées ont la forme
`MAJOR.MINOR.PATCH`, avec éventuellement un suffixe `-prerelease` constitué de
lettres/chiffres, points et tirets ; pas de métadonnées `+build`. La release
contient l'installateur à télécharger/lire séparément, sans pipeline `curl | sh`.
Les URL de données sont :

```text
https://github.com/ArnaudFra/ssho/releases/download/vVERSION/ssho-TARGET.tar.gz
https://github.com/ArnaudFra/ssho/releases/latest/download/ssho-TARGET.tar.gz
```

Chaque URL possède son compagnon `.sha256`. `curl` impose HTTPS y compris
après redirection et refuse les erreurs HTTP. Le checksum détecte une corruption
ou un décalage entre deux téléchargements de `latest` ; il n'est pas une signature
indépendante du dépôt. Utiliser un dépôt et un installateur de confiance. Les
archives mal formées sont refusées, sans extraction de leurs chemins dans le
système utilisateur. Les quatre membres doivent être des fichiers réguliers
uniques ; seul `MANIFEST` puis `ssho` sont lus par extraction vers stdout.

## Désinstaller en conservant les données

```sh
sh install.sh --uninstall
sh install.sh --uninstall --bin-dir "$HOME/tools/bin"
```

La même priorité de destination s'applique. Seul le fichier régulier
`DIR/ssho` est retiré ; l'installateur ne recherche ni ne supprime d'autres
copies. Un binaire absent est un succès sans modification. Les répertoires SSH,
clés, profils, `~/.config/ssho`, historique et sauvegardes restent intacts. Un lien symbolique ou un dossier au nom de `ssho` est refusé.

## Préparer une archive

Le packager prend une version explicite égale à celle de `Cargo.toml`, un binaire
Cargo de confiance nommé `ssho` et une cible explicite ; il n'exécute pas le
binaire, y compris en compilation croisée. `--tag`, facultatif, doit être
exactement `vVERSION`. Il ne prouve pas l'architecture du fichier fourni : le
workflow construit chaque cible nativement et vérifie ensuite le vrai binaire.

```sh
sh scripts/package-release.sh \
  --binary ./target/release/ssho \
  --target x86_64-unknown-linux-gnu --version 0.1.0 --tag v0.1.0 \
  --output-dir ./dist/v0.1.0/x86_64-unknown-linux-gnu
```

`--output-dir` doit être nouveau ; aucune archive existante n'est écrasée. Le
répertoire complet est préparé à côté, puis publié par renommage. Il contient
exactement deux fichiers, dont les noms sont stables entre versions :

```text
ssho-TARGET.tar.gz
ssho-TARGET.tar.gz.sha256
```

Le checksum est une ligne ASCII terminée par LF : **64 chiffres hexadécimaux
minuscules**, **deux espaces**, **nom de l'archive**. L'archive contient exactement
`ssho` (mode 0755), `LICENSE`, `INSTALLATION.md` et `MANIFEST` (mode 0644), sans
répertoire englobant. Le manifeste est exactement, avec un LF final :

```text
format=1
version=0.1.0
target=x86_64-unknown-linux-gnu
```

Les scripts nécessitent un shell Unix, `tar` avec gzip et sortie `-O`, `mktemp`,
les outils Unix usuels (`cp`, `chmod`, `ln`, `mv`, `cmp`, `sort`, `sed`, `awk`,
`grep`, etc.) et `sha256sum` ou `shasum -a 256`. `curl` est requis uniquement pour
le téléchargement. Python 3.11+ n'est nécessaire qu'aux tests de développement (lecture TOML via la bibliothèque standard).

Installation/remplacement préparent un fichier privé sur le même système de
fichiers puis le publient par lien dur sans écrasement, ou renommage avec
`--replace`. Installation et désinstallation utilisent le verrou commun
`DIR/.ssho-install-lock` ; packaging utilise `OUTPUT.lock`. Un verrou occupé
provoque un refus. Après une interruption non récupérable, supprimer seulement
le verrou vide devenu obsolète, après avoir vérifié qu'aucun script ne tourne.
Les temporaires privés sont nettoyés aux sorties normales et aux signaux
récupérables ; `SIGKILL` ou une panne peut laisser un temporaire à inspecter.
Les scripts ne nettoient jamais un chemin publié après une erreur.

Cette coordination couvre les scripts coopérants dans des répertoires contrôlés
par l'utilisateur. Ne pas modifier ces chemins depuis un autre processus pendant
l'opération ; elle ne protège pas contre un acteur local qui change simultanément
les répertoires ou fichiers. Le système de fichiers doit accepter les liens durs
pour une installation sans remplacement.

## Workflow et plateformes

`.github/workflows/release.yml` fabrique les archives sur les runners natifs
suivants, avec Rust 1.88.0, sans action tierce de configuration Rust :

| Cible | Runner | Limite de compatibilité visée |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | GNU/Linux, base Ubuntu 24.04 / glibc 2.39 |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | GNU/Linux ARM64, même base |
| `x86_64-apple-darwin` | `macos-15-intel` | macOS 15+, Intel |
| `aarch64-apple-darwin` | `macos-15` | macOS 15+, Apple Silicon |

Les archives Linux ne sont pas des binaires musl universels. Le workflow fixe
`MACOSX_DEPLOYMENT_TARGET=15.0` ; les binaires macOS ne sont ni signés ni
notarisés. Un paquet local hérite de la libc de sa propre machine de
compilation, pas de la base CI. Il n'y a pas d'archive pour Android/Termux,
Windows ou ARMv7, ni de paquet NixOS, Homebrew ou Nix.

Un push de tag `v*` fabrique et conserve les artifacts CI, **sans publier**.
`workflow_dispatch` fabrique également les artifacts ; `publish` vaut `false`
par défaut. Pour publier, un mainteneur doit choisir un tag existant `vVERSION`
et mettre explicitement `publish=true`. Une branche avec `publish=true`, ou un
tag différent de `Cargo.toml`, est refusé. Les quatre builds doivent réussir,
avec tests de scripts, packaging et installation/version/désinstallation dans
un HOME temporaire. Seul le job `publish` reçoit `contents: write` ; il revérifie
les checksums et manifestes avant `gh release create --verify-tag`. Une release
existante n'est pas écrasée ; un échec distant peut laisser un brouillon à
inspecter. Une prerelease n'est pas promue `latest`.

La release comprend les huit fichiers d'archives/checksums, `install.sh`,
`LICENSE` et ce guide. Les actions checkout/upload/download sont épinglées par
SHA complet.

Références : [runners GitHub](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[permissions et déclencheurs](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax),
[checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1),
[upload-artifact v7.0.2](https://github.com/actions/upload-artifact/releases/tag/v7.0.2),
[download-artifact v8.0.2](https://github.com/actions/download-artifact/releases/tag/v8.0.2),
[outils du runner Ubuntu 24.04](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md),
[création d'une release via gh](https://cli.github.com/manual/gh_release_create).

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

## Enregistrer la démo

L'animation du README est enregistrée à partir de données inventées : hôtes
d'exemple sur des adresses de documentation, faux `ssh` et faux `ping`. Le script
pilote le programme dans un pseudo-terminal et écrit un enregistrement asciinema,
que [agg](https://github.com/asciinema/agg) convertit en GIF :

```sh
python3 scripts/record-demo.py target/release/ssho demo.cast
agg --font-size 14 --last-frame-duration 3 demo.cast docs/demo.gif
```

## Versions et plateformes

La vérification des mises à jour interroge, en arrière-plan, le dépôt de releases fixé à la compilation par `SSHO_REPOSITORY=ArnaudFra/ssho` ou `SSHO_RELEASES_URL=https://…` ; sans l'une de ces variables, aucune requête n'est faite. Les préférences et `--no-update-check` désactivent la vérification.

**Plateformes.**

| Plateforme | Prise en charge |
| --- | --- |
| Linux x86_64 | Plateforme de développement : tests Rust, parcours en pseudo-terminal, tests d'installation (`tests/distribution.py`), complétion Bash. |
| Linux ARM64, macOS Intel, macOS ARM64 | Compilées et testées par `ci.yml` ; archives produites par `release.yml`. |
| Windows | Pas d'archive. Des branches de code existent (chemins, `%APPDATA%`) mais ne sont ni compilées ni testées. |

Le workflow de release compile l'adresse du dépôt qui publie (`SSHO_REPOSITORY`), pour qu'un binaire publié cherche ses mises à jour dans sa propre distribution.

**Outils natifs.** `ssh` est indispensable. `sftp`, `ssh-keygen`, `ssh-copy-id`, `ssh-add` et `ping` ne servent chacun qu'à une fonction : s'ils manquent, seule cette fonction échoue, avec la raison. L'écran Préférences (`o`), section **Outils natifs**, indique pour chacun s'il est trouvé dans le `PATH` (sans le lancer), à quoi il sert et quoi faire s'il manque.
