# ssho - SSH Organizer

[![Rust](https://img.shields.io/badge/Rust-1.88+-CE412B?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/ArnaudFra/ssho?style=for-the-badge)](https://github.com/ArnaudFra/ssho/releases)
[![License](https://img.shields.io/github/license/ArnaudFra/ssho?style=for-the-badge)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-lightgrey?style=for-the-badge)](https://github.com/ArnaudFra/ssho/releases)

> **Un gestionnaire de connexions SSH pour votre terminal**

**English:** this document is also available in English, see [README.md](README.md).

**ssho** est écrit en Rust. Il lit les hôtes de votre `~/.ssh/config`, permet de les chercher, de les organiser et de les modifier, et se connecte avec le client OpenSSH habituel.

<p align="center">
    <a href="docs/demo.gif"><img src="docs/demo.gif" alt="ssho dans un terminal" width="800" /></a>
</p>

## Fonctionnalités

- **Vos fichiers SSH restent la référence.** Les hôtes sont lus dans `~/.ssh/config` et ses fichiers `Include`. Les modifications conservent commentaires et mise en forme, sont montrées sous forme de diff avant toute écriture, et chaque écriture est précédée d'une sauvegarde que l'on peut restaurer.
- **Retrouver un hôte rapidement.** Recherche par nom, adresse, utilisateur ou tag, avec des filtres comme `tag:prod`, `user:deploy`, `file:` ou `is:online` ; favoris, hôtes masqués, tri par nom ou par dernière connexion.
- **Organiser.** Tags par hôte ou hérités d'un fichier entier, groupes par fichier ou par tag, alias regroupés, colonnes configurables, vues enregistrées, notes personnelles conservées hors des fichiers SSH.
- **Voir ce qu'OpenSSH fera vraiment.** Le panneau de détails affiche la configuration effective résolue par `ssh -G`, à côté de la déclaration, de son fichier et de sa ligne.
- **Savoir ce qui répond.** Contrôles de disponibilité en arrière-plan, par SSH ou par ICMP.
- **Historique des connexions.** Dernière connexion de chaque hôte et son résultat ; la liste peut être triée dessus.
- **Gérer les hôtes par formulaire.** Ajouter, modifier, dupliquer, déplacer d'un fichier à l'autre, supprimer.
- **Tunnels.** Redirections locales, distantes et dynamiques, profils nommés, tunnels gardés en arrière-plan.
- **Plusieurs hôtes à la fois.** Lancer une commande sur les hôtes marqués et lire chaque résultat ; garder les commandes fréquentes avec des paramètres typés.
- **Clés et fichiers.** Assistant de clés OpenSSH (création, ajout à l'agent, déploiement) et sessions SFTP, avec les outils natifs.
- **Tout au clavier.** Chaque action est dans une palette recherchable (F2), les raccourcis se modifient, l'aide suit l'écran en cours.
- **S'adapte à votre installation.** Écran de préférences, lanceur SSH personnalisé (un wrapper autour de `ssh`), autre configuration avec `-c`, rechargement quand les fichiers changent sur le disque, mise en page qui suit la taille du terminal, avis de mise à jour.
- **Vos données voyagent.** Export et import des favoris, notes, profils de tunnels, vues, commandes et préférences ; jamais de clés ni de mots de passe.
- **Anglais et français**, avec un mode sans couleurs et des symboles ASCII.
- **Ligne de commande.** Connexion directe, recherche, informations d'un hôte en JSON, complétion pour Bash, Zsh, Fish et PowerShell.

ssho s'est fortement inspiré de [SSHM](https://github.com/Gu1llaum-3/sshm) à ses débuts : j'utilise cet outil au quotidien, mais il lui manquait de nombreuses fonctionnalités, ce qui m'a poussé à en écrire une version en Rust et à la faire évoluer. Les données d'une installation SSHM existante sont reprises au premier lancement.

## Installation

Des archives prêtes à l'emploi sont jointes à chaque [release](https://github.com/ArnaudFra/ssho/releases). Le client OpenSSH (`ssh`) et `~/.local/bin` doivent être dans votre `PATH`. Pour une distribution plus ancienne, une autre plateforme, les checksums ou le script d'installation, voir [Compilation et installation avancée](docs/INSTALLATION_fr.md).

### Linux

x86_64, glibc 2.39 ou plus récente (Ubuntu 24.04, Debian 13, Fedora 40...) :

```sh
curl -fsSLO https://github.com/ArnaudFra/ssho/releases/latest/download/ssho-x86_64-unknown-linux-gnu.tar.gz
tar -xzf ssho-x86_64-unknown-linux-gnu.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

Sur ARM64, prendre `ssho-aarch64-unknown-linux-gnu.tar.gz`.

### macOS

macOS 15 ou plus récent. Apple Silicon :

```sh
curl -fsSLO https://github.com/ArnaudFra/ssho/releases/latest/download/ssho-aarch64-apple-darwin.tar.gz
tar -xzf ssho-aarch64-apple-darwin.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

Sur un Mac Intel, prendre `ssho-x86_64-apple-darwin.tar.gz`. Le binaire n'est pas signé : si l'archive a été téléchargée avec un navigateur, retirer la quarantaine avec `xattr -d com.apple.quarantine ~/.local/bin/ssho`.

### Windows

Il n'y a pas de version Windows. Utiliser [WSL](https://learn.microsoft.com/windows/wsl/) et suivre les instructions Linux.

## Utilisation

```sh
ssho                          # liste interactive
ssho production               # connexion directe
ssho production uptime        # commande distante
ssho --query 'tag:prod'       # ouvrir la liste sur une recherche
ssho search web --format json
ssho info production --pretty
ssho add [hostname]
ssho edit production
```

**Navigation**

| Touche | Action |
| --- | --- |
| ↑ / ↓ | Se déplacer dans la liste |
| Page précédente / suivante | Avancer d'une page |
| `/` | Rechercher |
| F2 | Palette de toutes les actions |
| F5 | Recharger les fichiers SSH |
| `?` | Aide de l'écran en cours |
| `q` | Quitter |

**Connexion**

| Touche | Action |
| --- | --- |
| Entrée | Se connecter à l'hôte sélectionné |
| `S` | Ouvrir une session SFTP |
| `f` | Créer un tunnel |
| F6 | Profils de tunnels |
| F7 | Tunnels actifs |
| `p` | Tester la disponibilité |

**Hôtes**

| Touche | Action |
| --- | --- |
| `a` | Ajouter un hôte |
| `e` | Modifier |
| `D` | Dupliquer |
| `m` | Déplacer vers un autre fichier |
| `d` | Supprimer |
| `i` | Fiche de l'hôte |
| F4 | Panneau de détails |
| `*` | Favori |
| `N` | Note personnelle |

**Organisation et outils**

| Touche | Action |
| --- | --- |
| `g` | Groupes |
| `c` | Filtre par fichier |
| `v` | Colonnes |
| `V` | Vues enregistrées |
| `x` | Marquer un hôte pour un lot |
| `B` | Lancer une commande sur les hôtes marqués |
| `R` | Commandes enregistrées |
| `K` | Clés OpenSSH |
| `o` | Préférences |

La complétion se charge avec `source <(ssho completion bash)` (ou `zsh`, `fish`, `powershell`).

## Documentation

- [Guide d'utilisation](docs/GUIDE_fr.md) : configuration, tous les écrans et raccourcis, syntaxe de recherche, complétion.
- [Compilation et installation avancée](docs/INSTALLATION_fr.md) : compilation depuis les sources, options de l'installateur, archives, plateformes, tests.

## Licence

MIT. Les premières versions adaptent la logique et l'organisation de [SSHM](https://github.com/Gu1llaum-3/sshm) ; la mention de copyright d'origine de Guillaume Archambault est conservée dans [LICENSE](LICENSE).
