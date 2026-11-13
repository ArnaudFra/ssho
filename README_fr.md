# ssho - SSH Organizer

[![Rust](https://img.shields.io/badge/Rust-1.88+-CE412B?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/ArnaudFra/ssho?style=for-the-badge)](https://github.com/ArnaudFra/ssho/releases)
[![License](https://img.shields.io/github/license/ArnaudFra/ssho?style=for-the-badge)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey?style=for-the-badge)](https://github.com/ArnaudFra/ssho/releases)

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
- **S'adapte à votre installation.** Écran de préférences, lanceur SSH personnalisé (un wrapper autour de `ssh`), autre configuration avec `-c`, rechargement quand les fichiers changent sur le disque, mise en page qui suit la taille du terminal, avis de mise à jour. Les données d'une installation SSHM existante sont reprises au premier lancement.
- **Vos données voyagent.** Export et import des favoris, notes, profils de tunnels, vues, commandes et préférences ; jamais de clés ni de mots de passe.
- **Anglais et français**, avec un mode sans couleurs et des symboles ASCII.
- **Ligne de commande.** Connexion directe, recherche, informations d'un hôte en JSON, complétion pour Bash, Zsh, Fish et PowerShell.

## Installation

Des archives prêtes à l'emploi sont jointes à chaque [release](https://github.com/ArnaudFra/ssho/releases) ; la liste est dans [Releases](#releases). Le client OpenSSH (`ssh`) et `~/.local/bin` doivent être dans votre `PATH`. Pour une distribution plus ancienne ou une autre plateforme, voir [Compilation et installation avancée](docs/INSTALLATION_fr.md).

### Linux

x86_64, glibc 2.39 ou plus récente (Ubuntu 24.04, Debian 13, Fedora 40...) :

```sh
curl -fsSLO https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_x86_64.tar.gz
tar -xzf ssho_Linux_x86_64.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

Sur ARM64, prendre `ssho_Linux_arm64.tar.gz`.

### macOS

macOS 15 ou plus récent. Apple Silicon :

```sh
curl -fsSLO https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Darwin_arm64.tar.gz
tar -xzf ssho_Darwin_arm64.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

Sur un Mac Intel, prendre `ssho_Darwin_x86_64.tar.gz`. Le binaire n'est pas signé : si l'archive a été téléchargée avec un navigateur, retirer la quarantaine avec `xattr -d com.apple.quarantine ~/.local/bin/ssho`.

### Windows

Windows 10 ou plus récent, avec son client OpenSSH intégré. Télécharger [ssho_Windows_x86_64.zip](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Windows_x86_64.zip), en extraire `ssho.exe` et le placer dans un dossier du `PATH`.

Les commandes sur plusieurs hôtes, les tunnels en arrière-plan et le déploiement de clés demandent Unix : sous Windows, ils sont disponibles par [WSL](https://learn.microsoft.com/windows/wsl/) avec l'archive Linux.

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

## Releases

Une release est construite pour chaque tag de version :

| Plateforme | Architecture | Téléchargement |
| --- | --- | --- |
| Linux | x86_64 | [ssho_Linux_x86_64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_x86_64.tar.gz) |
| Linux | ARM64 | [ssho_Linux_arm64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_arm64.tar.gz) |
| Linux | ARMv7 (32 bits) | [ssho_Linux_armv7.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_armv7.tar.gz) |
| Linux | ARMv6 (32 bits) | [ssho_Linux_armv6.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_armv6.tar.gz) |
| Linux | i386 | [ssho_Linux_i386.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_i386.tar.gz) |
| macOS | Apple Silicon | [ssho_Darwin_arm64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Darwin_arm64.tar.gz) |
| macOS | Intel | [ssho_Darwin_x86_64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Darwin_x86_64.tar.gz) |
| Windows | x86_64 | [ssho_Windows_x86_64.zip](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Windows_x86_64.zip) |
| Windows | i386 | [ssho_Windows_i386.zip](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Windows_i386.zip) |

Chaque archive contient le binaire `ssho`, la licence et le README. `checksums.txt` donne le SHA-256 des archives.

## Contribuer

Les contributions sont les bienvenues ! N'hésitez pas à proposer une pull request. Pour un changement important, ouvrez d'abord une issue pour en discuter.

1. **Forker** le dépôt
2. **Créer** une branche (`git checkout -b feature/ma-fonction`)
3. **Commiter** vos changements
4. **Pousser** la branche (`git push origin feature/ma-fonction`)
5. **Ouvrir** une pull request

Le formatage, Clippy et les tests tournent sur chaque pull request ; [Compilation et installation avancée](docs/INSTALLATION_fr.md) explique comment les lancer en local.

## Licence

Ce projet est sous licence MIT - voir le fichier [LICENSE](LICENSE).

## Remerciements

- [SSHM](https://github.com/Gu1llaum-3/sshm) de Guillaume Archambault. ssho s'en est fortement inspiré à ses débuts : j'utilise cet outil au quotidien, mais il lui manquait de nombreuses fonctionnalités, ce qui m'a poussé à en écrire une version en Rust et à la faire évoluer. Les premières versions adaptent sa logique et son organisation, et sa mention de copyright est conservée dans [LICENSE](LICENSE).
- [Ratatui](https://ratatui.rs/) et [Crossterm](https://github.com/crossterm-rs/crossterm) pour les bibliothèques d'interface terminal
- [OpenSSH](https://www.openssh.com/), qui fait le vrai travail
- La communauté Rust pour ses outils

---

<div align="center">

**Fait avec ❤️ par [ArnaudFra](https://github.com/ArnaudFra)**

</div>
