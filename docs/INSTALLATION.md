# Build and advanced installation

**Français :** ce guide existe aussi en français, voir [INSTALLATION_fr.md](INSTALLATION_fr.md).

This guide covers building from source, the `install.sh` installer, the archive
format, the release workflow, the supported platforms and the development checks.
For everyday use, see the [README](../README.md) and the [user guide](GUIDE.md).

## Building from source

Rust 1.88 or later, a C compiler, build tools and Perl are required (OpenSSL and
zlib are vendored). The OpenSSH `ssh` client is still needed at run time. This
method builds for the local machine:

```sh
cargo build --release --locked
cargo install --path . --locked
```

Cargo usually installs into `$HOME/.cargo/bin`. To remove that Cargo
installation: `cargo uninstall ssho`. The commands below are about the
archive/local installer, whose default directory is **`$HOME/.local/bin`**.

## Installing a local binary or an archive

The standalone installer is `install.sh`. It never runs the binary during
installation.

```sh
sh install.sh --binary ./target/release/ssho
sh install.sh --binary ./target/release/ssho --bin-dir "$HOME/tools/bin"
sh install.sh --archive ./ssho-x86_64-unknown-linux-gnu.tar.gz --version 0.1.0
```

A local archive must keep its protocol name and come with
`ssho-TARGET.tar.gz.sha256`, or with a file chosen by `--checksum PATH`.
The checksum, the format, the members, the version and the target are checked
before the binary is published. `--version` is optional for a local archive;
when given, it must match the manifest. `--target` selects an explicit target;
otherwise the `uname -s`/`uname -m` pair determines the native target.
An explicit choice does not make a binary of another architecture runnable.
`--binary` only accepts a non-empty regular executable file named `ssho`, not a
symbolic link: its origin and compatibility are left to the user, since no
distribution checksum is available for this mode.

The destination follows this order, whatever the order of the arguments:

1. `--bin-dir DIR`;
2. `--prefix DIR`, which appends `/bin`;
3. a non-empty `SSHO_INSTALL_DIR` variable;
4. `$HOME/.local/bin`.

Spaces and shell characters are literal when quoted. Relative paths start from
the current directory. `..` components, newlines and any symbolic component of
the destination are refused; choose its real path (useful when HOME itself is a
link). The installer refuses an existing file unless **`--replace`** is given.
Even with this option, a symbolic link, a directory or a special file is
refused. It never asks for `sudo`, changes no profile and does not modify the
PATH. A destination that is not writable is an error.

After installation, a message says whether another `ssho` found in the PATH may
shadow the installed one. Check `command -v ssho`, use the full path or add the
wanted directory to the PATH by hand. A shell may keep an old lookup in its
cache (`hash -r` in bash).

## Downloading a release

Releases are downloaded from `ArnaudFra/ssho` by default. `--repo OWNER/REPO` or
`SSHO_REPOSITORY` selects another repository, a fork for example:

```sh
sh install.sh --version 0.1.0
sh install.sh --version latest --replace
sh install.sh --repo OWNER/REPO --version 0.1.0
```

Without `--version`, the download picks `latest`. For a given version, the tag
is `vVERSION`, without doubling the `v`. Accepted versions have the form
`MAJOR.MINOR.PATCH`, optionally followed by a `-prerelease` suffix made of
letters/digits, dots and dashes; no `+build` metadata. The release contains the
installer, to be downloaded and read separately, with no `curl | sh` pipeline.
The data URLs are:

```text
https://github.com/ArnaudFra/ssho/releases/download/vVERSION/ssho-TARGET.tar.gz
https://github.com/ArnaudFra/ssho/releases/latest/download/ssho-TARGET.tar.gz
```

Each URL has its `.sha256` companion. `curl` enforces HTTPS, including after a
redirect, and refuses HTTP errors. The checksum detects a corruption or a
mismatch between two downloads of `latest`; it is not a signature independent
from the repository. Use a repository and an installer you trust. Malformed
archives are refused, and their paths are never extracted into the user's
system. The four members must be unique regular files; only `MANIFEST` then
`ssho` are read, by extraction to stdout.

## Uninstalling while keeping the data

```sh
sh install.sh --uninstall
sh install.sh --uninstall --bin-dir "$HOME/tools/bin"
```

The same destination priority applies. Only the regular file `DIR/ssho` is
removed; the installer neither looks for nor deletes other copies. A missing
binary is a success with no change. SSH directories, keys, profiles,
`~/.config/ssho`, history and backups are left untouched. A symbolic link or a
directory named `ssho` is refused.

## Preparing an archive

The packager takes an explicit version equal to the one in `Cargo.toml`, a
trusted Cargo binary named `ssho` and an explicit target; it does not run the
binary, cross-compiled or not. `--tag`, optional, must be exactly `vVERSION`.
It does not prove the architecture of the given file: the workflow builds each
target natively and then checks the real binary.

```sh
sh scripts/package-release.sh \
  --binary ./target/release/ssho \
  --target x86_64-unknown-linux-gnu --version 0.1.0 --tag v0.1.0 \
  --output-dir ./dist/v0.1.0/x86_64-unknown-linux-gnu
```

`--output-dir` must be new; no existing archive is overwritten. The whole
directory is prepared next to it, then published by a rename. It contains
exactly two files, whose names are stable across versions:

```text
ssho-TARGET.tar.gz
ssho-TARGET.tar.gz.sha256
```

The checksum is one ASCII line ending with LF: **64 lowercase hexadecimal
digits**, **two spaces**, **archive name**. The archive contains exactly `ssho`
(mode 0755), `LICENSE`, `INSTALLATION.md` and `MANIFEST` (mode 0644), with no
enclosing directory. The manifest is exactly, with a final LF:

```text
format=1
version=0.1.0
target=x86_64-unknown-linux-gnu
```

The scripts need a Unix shell, `tar` with gzip and `-O` output, `mktemp`, the
usual Unix tools (`cp`, `chmod`, `ln`, `mv`, `cmp`, `sort`, `sed`, `awk`,
`grep`, etc.) and `sha256sum` or `shasum -a 256`. `curl` is only required for
downloads. Python 3.11+ is only needed by the development tests (TOML parsing
from the standard library).

Installation and replacement prepare a private file on the same file system,
then publish it with a hard link that never overwrites, or with a rename when
`--replace` is given. Installation and uninstallation share the lock
`DIR/.ssho-install-lock`; packaging uses `OUTPUT.lock`. A held lock is a
refusal. After an interruption that could not be recovered, remove only the
empty stale lock, after checking that no script is running. Private temporary
files are cleaned up on normal exits and on catchable signals; `SIGKILL` or a
crash may leave a temporary file to inspect. The scripts never clean up a
published path after an error.

This coordination covers cooperating scripts in directories controlled by the
user. Do not modify these paths from another process during the operation; it
does not protect against a local actor changing the directories or files at the
same time. The file system must support hard links for an installation without
replacement.

## Workflow and platforms

`.github/workflows/release.yml` builds the archives on the following native
runners, with Rust 1.88.0 and no third-party Rust setup action:

| Target | Runner | Intended compatibility |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | GNU/Linux, Ubuntu 24.04 base / glibc 2.39 |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | GNU/Linux ARM64, same base |
| `x86_64-apple-darwin` | `macos-15-intel` | macOS 15+, Intel |
| `aarch64-apple-darwin` | `macos-15` | macOS 15+, Apple Silicon |

The Linux archives are not universal musl binaries. The workflow sets
`MACOSX_DEPLOYMENT_TARGET=15.0`; the macOS binaries are neither signed nor
notarised. A local package inherits the libc of its own build machine, not the
CI base. There is no archive for Android/Termux, Windows or ARMv7, and no
NixOS, Homebrew or Nix package.

Pushing a `v*` tag builds and keeps the CI artifacts, **without publishing**.
`workflow_dispatch` builds the artifacts too; `publish` is `false` by default.
To publish, a maintainer must choose an existing `vVERSION` tag and explicitly
set `publish=true`. A branch with `publish=true`, or a tag that differs from
`Cargo.toml`, is refused. The four builds must succeed, with the script tests,
packaging and installation/version/uninstallation in a temporary HOME. Only the
`publish` job receives `contents: write`; it checks the checksums and manifests
again before `gh release create --verify-tag`. An existing release is not
overwritten; a remote failure may leave a draft to inspect. A prerelease is not
promoted to `latest`.

The release holds the eight archive/checksum files, `install.sh`, `LICENSE` and
this guide. The checkout/upload/download actions are pinned by full SHA.

References: [GitHub runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[permissions and triggers](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax),
[checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1),
[upload-artifact v7.0.2](https://github.com/actions/upload-artifact/releases/tag/v7.0.2),
[download-artifact v8.0.2](https://github.com/actions/download-artifact/releases/tag/v8.0.2),
[Ubuntu 24.04 runner tools](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md),
[creating a release with gh](https://cli.github.com/manual/gh_release_create).

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release

# All the pseudo-terminal (PTY) flows in one go (Linux, Python 3): each
# scenario gets a fresh, short fixture folder, removed at the end.
sh scripts/pty-checks.sh target/release/ssho

# Or one by one. Flows in a pseudo-terminal, Linux/macOS with Python 3
python3 tests/pty_smoke.py target/release/ssho
# Targeted flow for the file filter
python3 tests/pty_smoke.py target/release/ssho --source-filter-only
# FileTags, inheritance and destination mutation
python3 tests/pty_smoke.py target/release/ssho --file-tags-only
# Targeted flow for the launcher and its return to the terminal
python3 tests/pty_smoke.py target/release/ssho --launcher-only
# Targeted flow for grouped aliases, target choice and preferences
python3 tests/pty_smoke.py target/release/ssho --aliases-only
# Groups, header safety, search, preferences and small terminal
python3 tests/pty_smoke.py target/release/ssho --groups-only

# Ergonomic UX flows (keyboard, focus, back) in a fresh fixture folder.
# This script reads the interface in French; the other PTY scripts read it in English.
python3 tests/pty_ux.py target/release/ssho "$(mktemp -d)/ux"

# Optional comparison with an SSHM executable
python3 tests/reference_parity.py /path/to/sshm target/release/ssho
```

**Continuous integration.** `.github/workflows/ci.yml` runs these checks on every push and pull request, without publishing anything: a quick check on Linux (formatting, Clippy with no warnings, Rust tests), the tests with Rust 1.88 (the declared minimum version), the installation and packaging scripts (`sh -n`, `tests/distribution.py`), then the build and the tests on the four announced targets (Linux and macOS, x86_64 and ARM64, each on its native runner). The pseudo-terminal flows run there on Linux only; they have not been adapted to macOS. The `release.yml` workflow remains the only one that produces archives.

All tests use temporary folders. The connection and tunnel flows use a fake SSH client; they do not connect to the user's personal servers.

## Recording the demo

The animation of the README is recorded from made-up data: example hosts on
documentation addresses, a fake `ssh` and a fake `ping`. The script drives the
program in a pseudo-terminal and writes an asciinema cast, which
[agg](https://github.com/asciinema/agg) turns into a GIF:

```sh
python3 scripts/record-demo.py target/release/ssho demo.cast
agg --font-size 14 --last-frame-duration 3 demo.cast docs/demo.gif
```

## Versions and platforms

The update check queries, in the background, the release repository set at build time with `SSHO_REPOSITORY=ArnaudFra/ssho` or `SSHO_RELEASES_URL=https://…`; without one of these variables, no request is made. The preferences and `--no-update-check` disable the check.

**Platforms.**

| Platform | Support |
| --- | --- |
| Linux x86_64 | Development platform: Rust tests, pseudo-terminal flows, installation tests (`tests/distribution.py`), Bash completion. |
| Linux ARM64, macOS Intel, macOS ARM64 | Built and tested by `ci.yml`; archives produced by `release.yml`. |
| Windows | No archive. Code branches exist (paths, `%APPDATA%`) but are neither built nor tested. |

The release workflow compiles in the address of the publishing repository (`SSHO_REPOSITORY`), so that a published binary looks for its updates in its own distribution.

**Native tools.** `ssh` is essential. `sftp`, `ssh-keygen`, `ssh-copy-id`, `ssh-add` and `ping` each serve a single feature: if one is missing, only that feature fails, with the reason. The Preferences screen (`o`), **Native tools** section, indicates for each one whether it is found in the `PATH` (without running it), what it is used for and what to do if it is missing.
