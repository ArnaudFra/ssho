# ssho - SSH Organizer

[![Rust](https://img.shields.io/badge/Rust-1.88+-CE412B?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/ArnaudFra/ssho?style=for-the-badge)](https://github.com/ArnaudFra/ssho/releases)
[![License](https://img.shields.io/github/license/ArnaudFra/ssho?style=for-the-badge)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey?style=for-the-badge)](https://github.com/ArnaudFra/ssho/releases)

> **An SSH connection manager for your terminal**

**Français :** ce document existe aussi en français, voir [README_fr.md](README_fr.md).

**ssho** is written in Rust. It reads the hosts of your `~/.ssh/config`, lets you search, organise and edit them, and connects with the regular OpenSSH client.

<p align="center">
    <a href="docs/demo.gif"><img src="docs/demo.gif" alt="ssho in a terminal" width="800" /></a>
</p>

## Features

- **Your SSH files stay the source of truth.** Hosts are read from `~/.ssh/config` and its `Include` files. Edits keep comments and formatting, are shown as a diff before anything is written, and every write is backed up and can be restored.
- **Find a host quickly.** Search by name, address, user or tag, with filters such as `tag:prod`, `user:deploy`, `file:` or `is:online`; favorites, hidden hosts, sort by name or last connection.
- **Organise.** Tags per host or inherited from a whole file, groups by file or tag, grouped aliases, configurable columns, saved views, personal notes kept outside the SSH files.
- **See what OpenSSH will really do.** The details panel shows the effective configuration resolved by `ssh -G`, next to the declaration and its file and line.
- **Know what is up.** Availability checks in the background, through SSH or ICMP.
- **Connection history.** Last connection of each host and how it ended; the list can be sorted by it.
- **Manage hosts from forms.** Add, edit, duplicate, move between files, delete.
- **Tunnels.** Local, remote and dynamic forwarding, named profiles, tunnels kept running in the background.
- **Several hosts at once.** Run a command on marked hosts and read each result; keep frequent commands with typed parameters.
- **Keys and files.** OpenSSH key assistant (create, add to the agent, deploy) and SFTP sessions, with the native tools.
- **Keyboard first.** Every action is in a searchable palette (F2), shortcuts can be changed, help follows the current screen.
- **Fits your setup.** Preferences screen, custom SSH launcher (a wrapper around `ssh`), another configuration with `-c`, reload when the files change on disk, layout that adapts to the size of the terminal, update notice. The data of an existing SSHM installation is picked up on first launch.
- **Your data travels.** Export and import of favorites, notes, tunnel profiles, views, commands and preferences; never keys or passwords.
- **English and French**, with a no-colour mode and ASCII symbols.
- **Command line.** Direct connection, search, host information as JSON, completion for Bash, Zsh, Fish and PowerShell.

## Installation

Prebuilt archives are attached to each [release](https://github.com/ArnaudFra/ssho/releases); the list is in [Releases](#releases). The OpenSSH client (`ssh`) and `~/.local/bin` must be in your `PATH`. For an older distribution or another platform, see [Build and advanced installation](docs/INSTALLATION.md).

### Linux

x86_64, glibc 2.39 or later (Ubuntu 24.04, Debian 13, Fedora 40...):

```sh
curl -fsSLO https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_x86_64.tar.gz
tar -xzf ssho_Linux_x86_64.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

On ARM64, use `ssho_Linux_arm64.tar.gz`.

### macOS

macOS 15 or later. Apple Silicon:

```sh
curl -fsSLO https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Darwin_arm64.tar.gz
tar -xzf ssho_Darwin_arm64.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

On an Intel Mac, use `ssho_Darwin_x86_64.tar.gz`. The binary is not signed: if the archive was downloaded with a browser, remove the quarantine with `xattr -d com.apple.quarantine ~/.local/bin/ssho`.

### Windows

Windows 10 or later, with its built-in OpenSSH client. Download [ssho_Windows_x86_64.zip](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Windows_x86_64.zip), extract `ssho.exe` and put it in a folder of your `PATH`.

Running a command on several hosts, background tunnels and key deployment need Unix: on Windows they are available through [WSL](https://learn.microsoft.com/windows/wsl/) with the Linux archive.

## Usage

```sh
ssho                          # interactive list
ssho production               # connect directly
ssho production uptime        # run a remote command
ssho --query 'tag:prod'       # open the list on a search
ssho search web --format json
ssho info production --pretty
ssho add [hostname]
ssho edit production
```

**Navigation**

| Key | Action |
| --- | --- |
| ↑ / ↓ | Move in the list |
| PageUp / PageDown | Move one page |
| `/` | Search |
| F2 | Palette of every action |
| F5 | Reload the SSH files |
| `?` | Help for the current screen |
| `q` | Quit |

**Connection**

| Key | Action |
| --- | --- |
| Enter | Connect to the selected host |
| `S` | Open an SFTP session |
| `f` | Create a tunnel |
| F6 | Tunnel profiles |
| F7 | Active tunnels |
| `p` | Test availability |

**Hosts**

| Key | Action |
| --- | --- |
| `a` | Add a host |
| `e` | Edit |
| `D` | Duplicate |
| `m` | Move to another file |
| `d` | Delete |
| `i` | Host sheet |
| F4 | Details panel |
| `*` | Favorite |
| `N` | Personal note |

**Organisation and tools**

| Key | Action |
| --- | --- |
| `g` | Groups |
| `c` | Filter by file |
| `v` | Columns |
| `V` | Saved views |
| `x` | Mark a host for a batch |
| `B` | Run a command on the marked hosts |
| `R` | Saved commands |
| `K` | OpenSSH keys |
| `o` | Preferences |

Shell completion is loaded with `source <(ssho completion bash)` (also `zsh`, `fish` and `powershell`).

## Documentation

- [User guide](docs/GUIDE.md): configuration, every screen and shortcut, search syntax, completion.
- [Build and advanced installation](docs/INSTALLATION.md): building from source, installer options, archives, platforms, tests.

## Releases

A release is built for each version tag:

| Platform | Architecture | Download |
| --- | --- | --- |
| Linux | x86_64 | [ssho_Linux_x86_64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_x86_64.tar.gz) |
| Linux | ARM64 | [ssho_Linux_arm64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_arm64.tar.gz) |
| Linux | ARMv7 (32-bit) | [ssho_Linux_armv7.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_armv7.tar.gz) |
| Linux | ARMv6 (32-bit) | [ssho_Linux_armv6.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_armv6.tar.gz) |
| Linux | i386 | [ssho_Linux_i386.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Linux_i386.tar.gz) |
| macOS | Apple Silicon | [ssho_Darwin_arm64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Darwin_arm64.tar.gz) |
| macOS | Intel | [ssho_Darwin_x86_64.tar.gz](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Darwin_x86_64.tar.gz) |
| Windows | x86_64 | [ssho_Windows_x86_64.zip](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Windows_x86_64.zip) |
| Windows | i386 | [ssho_Windows_i386.zip](https://github.com/ArnaudFra/ssho/releases/latest/download/ssho_Windows_i386.zip) |

Each archive holds the `ssho` binary, the license and this README. `checksums.txt` lists the SHA-256 of the archives.

## Contributing

Contributions are welcome! Please feel free to submit a pull request. For major changes, please open an issue first to discuss what you would like to change.

1. **Fork** the repository
2. **Create** a feature branch (`git checkout -b feature/my-feature`)
3. **Commit** your changes
4. **Push** to the branch (`git push origin feature/my-feature`)
5. **Open** a pull request

Formatting, Clippy and the tests run on every pull request; [Build and advanced installation](docs/INSTALLATION.md) explains how to run them locally.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [SSHM](https://github.com/Gu1llaum-3/sshm) by Guillaume Archambault. ssho was heavily inspired by it in its early days: I use that tool every day, but it lacked many features, which pushed me to write a Rust version and to keep extending it. The first versions adapt its logic and organisation, and its copyright notice is kept in [LICENSE](LICENSE).
- [Ratatui](https://ratatui.rs/) and [Crossterm](https://github.com/crossterm-rs/crossterm) for the terminal interface libraries
- [OpenSSH](https://www.openssh.com/), which does the real work
- The Rust community for building such good tools

---

<div align="center">

**Made with ❤️ by [ArnaudFra](https://github.com/ArnaudFra)**

</div>
