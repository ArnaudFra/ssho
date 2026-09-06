# ssho

**ssho — SSH Organizer** : un gestionnaire d'hôtes SSH pour le terminal, écrit en Rust.
Le projet reprend le principe de [SSHM](https://github.com/Gu1llaum-3/sshm) (Go, licence MIT).

Pour l'instant, le programme lit `~/.ssh/config`, suit les directives `Include`
et affiche les hôtes trouvés :

```sh
cargo run -- ~/.ssh/config
```
