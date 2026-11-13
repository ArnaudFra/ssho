# Compilation et installation avancée

**English:** this guide is also available in English, see [INSTALLATION.md](INSTALLATION.md).

Ce guide décrit la compilation depuis les sources, les archives de release, le
workflow de release, les plateformes prises en charge et les vérifications de
développement. Pour l'usage courant, voir le [README](../README_fr.md) et le
[guide d'utilisation](GUIDE_fr.md).

## Compiler depuis les sources

Rust 1.88 minimum, compilateur C, outils de compilation et Perl sont nécessaires
(OpenSSL et zlib embarqués). Le client OpenSSH `ssh` reste nécessaire à
l'utilisation. Cette méthode compile pour la machine locale :

```sh
cargo build --release --locked
cargo install --path . --locked
```

Cargo installe habituellement dans `$HOME/.cargo/bin`. Pour retirer cette
installation : `cargo uninstall ssho`.

## Archives de release

Chaque release contient une archive par plateforme et un fichier
`checksums.txt` :

| Archive | Plateforme |
| --- | --- |
| `ssho_Linux_x86_64.tar.gz` | GNU/Linux x86_64, glibc 2.39 ou plus récente |
| `ssho_Linux_arm64.tar.gz` | GNU/Linux ARM64, glibc 2.39 ou plus récente |
| `ssho_Linux_i386.tar.gz` | GNU/Linux x86, 32 bits |
| `ssho_Linux_armv7.tar.gz` | GNU/Linux ARMv7, 32 bits |
| `ssho_Linux_armv6.tar.gz` | GNU/Linux ARMv6, 32 bits |
| `ssho_Darwin_x86_64.tar.gz` | macOS 15 ou plus récent, Intel |
| `ssho_Darwin_arm64.tar.gz` | macOS 15 ou plus récent, Apple Silicon |
| `ssho_Windows_x86_64.zip` | Windows 10 ou plus récent, x86_64 |
| `ssho_Windows_i386.zip` | Windows 10 ou plus récent, x86 32 bits |

Une archive contient le binaire `ssho` (mode 0755 ; `ssho.exe` dans les archives
Windows), `LICENSE` et `README.md`, sans répertoire englobant. Pour vérifier un téléchargement et l'installer sans
aucun privilège :

```sh
sha256sum --check --ignore-missing checksums.txt
tar -xzf ssho_Linux_x86_64.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

Sous macOS, la vérification est `shasum -a 256 --check --ignore-missing checksums.txt`.
Retirer le programme revient à supprimer ce fichier : les fichiers SSH, les clés
et `~/.config/ssho` restent intacts.

Les archives Linux ne sont pas des binaires musl universels, et les binaires
macOS ne sont ni signés ni notarisés ; les binaires Windows ne sont pas signés
non plus. Il n'y a pas d'archive pour Android/Termux, ni de paquet Homebrew ou Nix.

## Workflow de release

`.github/workflows/release.yml` fabrique les archives avec Rust 1.88.0 :

| Cible | Compilée sur | Tests |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | tests Rust, parcours en pseudo-terminal |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | tests Rust, parcours en pseudo-terminal |
| `x86_64-apple-darwin` | `macos-15-intel` | tests Rust |
| `aarch64-apple-darwin` | `macos-15` | tests Rust |
| `i686-unknown-linux-gnu` | cross | aucun |
| `armv7-unknown-linux-gnueabihf` | cross | aucun |
| `arm-unknown-linux-gnueabihf` | cross | aucun |
| `x86_64-pc-windows-msvc` | `windows-2025` | exécution du binaire archivé |
| `i686-pc-windows-msvc` | `windows-2025` | exécution du binaire archivé |

Les trois cibles 32 bits sont compilées en croisé dans un conteneur avec
[cross](https://github.com/cross-rs/cross) et ne peuvent pas exécuter leurs
tests sur la machine de compilation. Les cibles Windows sont compilées
nativement mais ne lancent pas la suite de tests, dont l'essentiel pilote des
outils Unix. Si l'une de ces cinq cibles ne compile pas, la release est publiée
sans son archive.

Le workflow démarre lorsqu'un tag de version est poussé, et publie lui-même la
release :

```sh
git tag v1.0.0
git push origin v1.0.0
```

Le tag doit être `v` suivi de la version de `Cargo.toml` (`v1.0.0`,
`v1.1.0-rc.1`) ; tout autre tag est ignoré, et un tag différent de `Cargo.toml`
échoue avant toute compilation. Les quatre compilations natives doivent réussir,
avec les tests Rust, les parcours en pseudo-terminal sous Linux et une exécution
du binaire archivé. Seul le job `publish` reçoit `contents: write`. Une release
existante n'est pas écrasée. Une version avec suffixe (`-rc.1`) est publiée
comme prerelease et n'est pas promue `latest`.

Lancé à la main depuis l'onglet Actions, le workflow fabrique les archives sans
rien publier : c'est le moyen d'essayer une cible.

Les notes de la release sont écrites par `scripts/release-notes.py` à partir des
commits depuis le tag précédent : fonctionnalités (`feat`), corrections (`fix`)
et autres. Les commits qui ne changent pas le programme (`docs`, `test`, `ci`,
`chore`, `build`) sont laissés de côté. Les actions checkout/upload/download
sont épinglées par SHA complet.

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

**Intégration continue.** `.github/workflows/ci.yml` lance ces vérifications sur les pull requests, ou à la demande depuis l'onglet Actions, sans rien publier : un contrôle rapide sous Linux (formatage, Clippy sans avertissement, tests Rust), les tests avec Rust 1.88 (version minimale déclarée), la syntaxe des scripts, puis la compilation et les tests sur les quatre cibles annoncées (Linux et macOS, x86_64 et ARM64, chacune sur son exécuteur natif). Les parcours en pseudo-terminal y tournent sous Linux seulement ; ils n'ont pas été adaptés à macOS. Le workflow `release.yml` reste le seul à produire des archives.

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
| Linux x86_64 | Plateforme de développement : tests Rust, parcours en pseudo-terminal, complétion Bash. |
| Linux ARM64, macOS Intel, macOS ARM64 | Compilées et testées par `ci.yml` ; archives produites par `release.yml`. |
| Linux 32 bits (i386, ARMv7, ARMv6) | Compilées en croisé par `release.yml` ; non couvertes par les tests. |
| Windows (x86_64, i386) | Compilées par `release.yml` ; non couvertes par les tests. Les commandes sur plusieurs hôtes, les tunnels en arrière-plan et le déploiement de clés demandent Unix et sont refusés avec ce motif. |

Le workflow de release compile l'adresse du dépôt qui publie (`SSHO_REPOSITORY`), pour qu'un binaire publié cherche ses mises à jour dans sa propre distribution.

**Outils natifs.** `ssh` est indispensable. `sftp`, `ssh-keygen`, `ssh-copy-id`, `ssh-add` et `ping` ne servent chacun qu'à une fonction : s'ils manquent, seule cette fonction échoue, avec la raison. L'écran Préférences (`o`), section **Outils natifs**, indique pour chacun s'il est trouvé dans le `PATH` (sans le lancer), à quoi il sert et quoi faire s'il manque.
