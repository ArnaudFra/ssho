# ssho

**ssho — SSH Organizer** : un gestionnaire d'hôtes SSH pour le terminal, écrit en Rust.
Le projet reprend le principe de [SSHM](https://github.com/Gu1llaum-3/sshm) (Go, licence MIT).

## Compilation

```sh
cargo build --release
./target/release/ssho --help
```

## Utilisation

```sh
ssho prod-server                 # connexion SSH
ssho prod-server uptime          # commande distante
ssho search --tags work          # recherche par nom, adresse ou tag
ssho info prod-server --pretty   # description JSON d'un hôte
```

Les hôtes restent dans `~/.ssh/config` et ses fichiers `Include` ; l'option `-c`
choisit une autre configuration. Les préférences et l'historique sont dans
`~/.config/ssho/`. Au premier usage, un dossier `~/.config/sshm/` existant est
copié sans être modifié.
