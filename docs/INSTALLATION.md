# Build and advanced installation

**Français :** ce guide existe aussi en français, voir [INSTALLATION_fr.md](INSTALLATION_fr.md).

This guide covers building from source, the release archives, the release
workflow, the supported platforms and the development checks. For everyday use,
see the [README](../README.md) and the [user guide](GUIDE.md).

## Building from source

Rust 1.88 or later, a C compiler, build tools and Perl are required (OpenSSL and
zlib are vendored). The OpenSSH `ssh` client is still needed at run time. This
method builds for the local machine:

```sh
cargo build --release --locked
cargo install --path . --locked
```

Cargo usually installs into `$HOME/.cargo/bin`. To remove that installation:
`cargo uninstall ssho`.

## Release archives

Each release holds one archive per platform and a `checksums.txt` file:

| Archive | Platform |
| --- | --- |
| `ssho_Linux_x86_64.tar.gz` | GNU/Linux x86_64, glibc 2.39+ |
| `ssho_Linux_arm64.tar.gz` | GNU/Linux ARM64, glibc 2.39+ |
| `ssho_Linux_i386.tar.gz` | GNU/Linux x86, 32-bit |
| `ssho_Linux_armv7.tar.gz` | GNU/Linux ARMv7, 32-bit |
| `ssho_Linux_armv6.tar.gz` | GNU/Linux ARMv6, 32-bit |
| `ssho_Darwin_x86_64.tar.gz` | macOS 15 or later, Intel |
| `ssho_Darwin_arm64.tar.gz` | macOS 15 or later, Apple Silicon |
| `ssho_Windows_x86_64.zip` | Windows 10 or later, x86_64 |
| `ssho_Windows_i386.zip` | Windows 10 or later, x86 32-bit |

An archive contains the `ssho` binary (mode 0755; `ssho.exe` in the Windows
archives), `LICENSE` and `README.md`, with no enclosing directory. To check a download and install it without any
privilege:

```sh
sha256sum --check --ignore-missing checksums.txt
tar -xzf ssho_Linux_x86_64.tar.gz ssho
mkdir -p ~/.local/bin && mv ssho ~/.local/bin/
```

On macOS the check is `shasum -a 256 --check --ignore-missing checksums.txt`.
Removing the program is deleting that file: SSH files, keys and
`~/.config/ssho` are left untouched.

The Linux archives are not universal musl binaries, and the macOS binaries are
neither signed nor notarised; the Windows binaries are not signed either. There
is no archive for Android/Termux, and no Homebrew or Nix package.

## Release workflow

`.github/workflows/release.yml` builds the archives with Rust 1.88.0:

| Target | Built on | Tests |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | Rust tests, pseudo-terminal flows |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | Rust tests, pseudo-terminal flows |
| `x86_64-apple-darwin` | `macos-15-intel` | Rust tests |
| `aarch64-apple-darwin` | `macos-15` | Rust tests |
| `i686-unknown-linux-gnu` | cross | none |
| `armv7-unknown-linux-gnueabihf` | cross | none |
| `arm-unknown-linux-gnueabihf` | cross | none |
| `x86_64-pc-windows-msvc` | `windows-2025` | run of the archived binary |
| `i686-pc-windows-msvc` | `windows-2025` | run of the archived binary |

The three 32-bit targets are cross-compiled in a container with
[cross](https://github.com/cross-rs/cross) and cannot run their tests on the
build machine. The Windows targets are built natively but do not run the test
suite, most of which drives Unix tools. When one of these five fails to build,
the release is published without its archive.

The workflow starts when a version tag is pushed, and publishes the release by
itself:

```sh
git tag v1.0.0
git push origin v1.0.0
```

The tag must be `v` followed by the version of `Cargo.toml` (`v1.0.0`,
`v1.1.0-rc.1`); any other tag is ignored, and a tag that differs from
`Cargo.toml` fails before anything is built. The four native builds must
succeed, with the Rust tests, the pseudo-terminal flows on Linux and a run of
the archived binary. Only the `publish` job receives `contents: write`. An
existing release is not overwritten. A version with a suffix (`-rc.1`) is
published as a prerelease and is not promoted to `latest`.

Started by hand from the Actions tab, the workflow builds the archives without
publishing anything: this is the way to try a target.

The notes of the release are written by `scripts/release-notes.py` from the
commits since the previous tag: features (`feat`), bug fixes (`fix`) and
others. Commits that do not change the program (`docs`, `test`, `ci`, `chore`,
`build`) are left out. The checkout/upload/download actions are pinned by full
SHA.

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

**Continuous integration.** `.github/workflows/ci.yml` runs these checks on pull requests, or when started by hand from the Actions tab, without publishing anything: a quick check on Linux (formatting, Clippy with no warnings, Rust tests), the tests with Rust 1.88 (the declared minimum version), the syntax of the helper scripts, then the build and the tests on the four announced targets (Linux and macOS, x86_64 and ARM64, each on its native runner). The pseudo-terminal flows run there on Linux only; they have not been adapted to macOS. The `release.yml` workflow remains the only one that produces archives.

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
| Linux x86_64 | Development platform: Rust tests, pseudo-terminal flows, Bash completion. |
| Linux ARM64, macOS Intel, macOS ARM64 | Built and tested by `ci.yml`; archives produced by `release.yml`. |
| Linux 32-bit (i386, ARMv7, ARMv6) | Cross-compiled by `release.yml`; not covered by the tests. |
| Windows (x86_64, i386) | Built by `release.yml`; not covered by the tests. Running a command on several hosts, background tunnels and key deployment need Unix and are refused with that reason. |

The release workflow compiles in the address of the publishing repository (`SSHO_REPOSITORY`), so that a published binary looks for its updates in its own distribution.

**Native tools.** `ssh` is essential. `sftp`, `ssh-keygen`, `ssh-copy-id`, `ssh-add` and `ping` each serve a single feature: if one is missing, only that feature fails, with the reason. The Preferences screen (`o`), **Native tools** section, indicates for each one whether it is found in the `PATH` (without running it), what it is used for and what to do if it is missing.
