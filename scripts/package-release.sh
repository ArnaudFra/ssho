#!/bin/sh
# Package an explicitly selected Cargo-built binary without executing it.
set -eu
LC_ALL=C
COPYFILE_DISABLE=1
export LC_ALL COPYFILE_DISABLE
unset TAR_OPTIONS
umask 077
fail() { printf 'package-release: %s\n' "$*" >&2; exit 1; }
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
binary='' target='' version='' output='' tag=''
while [ "$#" -gt 0 ]; do
    option=$1; shift
    case "$option" in
        --help|-h) printf '%s\n' 'Usage: sh scripts/package-release.sh --binary PATH --target TRIPLE --version VERSION --output-dir NEW_DIR [--tag vVERSION]'; exit 0;;
        --binary|--target|--version|--output-dir|--tag)
            [ "$#" -gt 0 ] && [ -n "$1" ] || fail "missing value for $option"
            value=$1; shift;;
        *) fail "unknown option: $option";;
    esac
    case "$option" in --binary) binary=$value;; --target) target=$value;; --version) version=$value;; --output-dir) output=$value;; --tag) tag=$value;; esac
done
[ -n "$binary" ] && [ -n "$target" ] && [ -n "$version" ] && [ -n "$output" ] || fail 'binary, target, version and output-dir are required'
printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$' || fail 'invalid version'
manifest_version=$(sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)"$/\1/p' "$root/Cargo.toml")
[ "$version" = "$manifest_version" ] || fail "version must match Cargo.toml ($manifest_version)"
[ -z "$tag" ] || [ "$tag" = "v$version" ] || fail 'tag must be vVERSION and match Cargo.toml'
case "$target" in x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin) ;; *) fail "unsupported target: $target";; esac
case "$binary" in /*) ;; *) binary=$PWD/$binary;; esac
[ "${binary##*/}" = ssho ] || fail 'binary must be named ssho; Go assets are not supported'
[ -f "$binary" ] && [ ! -L "$binary" ] && [ -x "$binary" ] && [ -s "$binary" ] || fail 'binary must be a nonempty executable regular file, not a symlink'
case "$output" in /*) ;; *) output=$PWD/$output;; esac
# Normalize trailing separators, then reject links/parent traversal before mkdir.
while [ "${output%/}" != "$output" ]; do output=${output%/}; done
[ -n "$output" ] || fail 'output must be a new directory'
case "$output" in *'
'*) fail 'output must not contain a newline';; esac
rest=$output
checked=
while [ -n "$rest" ]; do
    part=${rest%%/*}
    if [ "$rest" = "$part" ]; then rest=; else rest=${rest#*/}; fi
    case "$part" in ''|.) continue;; ..) fail 'output must not contain ..';; esac
    checked=$checked/$part
    [ ! -L "$checked" ] || fail "symlink output component: $checked"
    if [ -e "$checked" ]; then [ -d "$checked" ] || fail "not a directory: $checked"; fi
done
# Publish a complete directory in one rename. Its parent must be controlled by the caller.
[ ! -e "$output" ] && [ ! -L "$output" ] || fail "output already exists: $output"
parent=$(dirname -- "$output")
mkdir -p "$parent"
lock=$output.lock
mkdir "$lock" 2>/dev/null || fail 'output lock unavailable'
stage=
cleanup() { [ -z "$stage" ] || rm -rf "$stage"; rmdir "$lock"; }
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
[ ! -e "$output" ] && [ ! -L "$output" ] || fail "output already exists: $output"
stage=$(mktemp -d "$parent/.sshm-package.XXXXXX")
mkdir "$stage/content" "$stage/release"
cp "$binary" "$stage/content/ssho"
chmod 755 "$stage/content/ssho"
cp "$root/LICENSE" "$stage/content/LICENSE"
cp "$root/docs/INSTALLATION.md" "$stage/content/INSTALLATION.md"
printf 'format=1\nversion=%s\ntarget=%s\n' "$version" "$target" > "$stage/content/MANIFEST"
chmod 644 "$stage/content/LICENSE" "$stage/content/INSTALLATION.md" "$stage/content/MANIFEST"
asset=ssho-$target.tar.gz
tar -czf "$stage/release/$asset" -C "$stage/content" ssho LICENSE INSTALLATION.md MANIFEST
if command -v sha256sum >/dev/null 2>&1; then hash_output=$(sha256sum < "$stage/release/$asset") || fail 'SHA256 tool failed'
elif command -v shasum >/dev/null 2>&1; then hash_output=$(shasum -a 256 < "$stage/release/$asset") || fail 'SHA256 tool failed'
else fail 'sha256sum or shasum is required'; fi
digest=${hash_output%% *}
printf '%s\n' "$digest" | grep -Eq '^[0-9a-f]{64}$' || fail 'invalid SHA256 tool output'
printf '%s  %s\n' "$digest" "$asset" > "$stage/release/$asset.sha256"
chmod 755 "$stage/release"
chmod 644 "$stage/release/$asset" "$stage/release/$asset.sha256"
mv "$stage/release" "$output"
printf '%s\n' "$output/$asset" "$output/$asset.sha256"
