# Installation de ssho

Ce guide décrit la compilation depuis les sources, l'installateur `install.sh`,
le format des archives et le workflow de release. Les plateformes prises en
charge sont listées dans [README](../README.md), « Versions et plateformes ».

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
