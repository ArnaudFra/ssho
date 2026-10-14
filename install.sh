#!/bin/sh
# Standalone user installer. Never source archive metadata or execute the payload.
set -eu
LC_ALL=C
export LC_ALL
unset TAR_OPTIONS
umask 077
fail() { printf 'ssho: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'EOF'
Usage: sh install.sh --binary PATH [destination] [--replace]
       sh install.sh --archive PATH [--checksum PATH] [--version VERSION] [--target TARGET] [destination] [--replace]
       sh install.sh [--repo OWNER/REPO] [--version VERSION|latest] [--target TARGET] [destination] [--replace]
       sh install.sh --uninstall [destination]
Destination: --bin-dir DIR > --prefix DIR (adds /bin) > SSHO_INSTALL_DIR > $HOME/.local/bin
Downloads come from ArnaudFra/ssho unless --repo or SSHO_REPOSITORY names another repository.
Existing files require --replace. Symlinks and non-files are always refused.
See docs/INSTALLATION.md for supported targets, trust and concurrency limits.
EOF
}
version_valid() { printf '%s\n' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$'; }
target_valid() {
    case "$1" in x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin) ;; *) fail "unsupported target: $1";; esac
}
absolute() { case "$1" in /*) printf '%s\n' "$1";; *) printf '%s/%s\n' "$PWD" "$1";; esac; }
# Refuse symbolic links in every destination component, and ambiguous parent paths.
check_directory_path() {
    rest=$1
    checked=
    while [ -n "$rest" ]; do
        part=${rest%%/*}
        if [ "$rest" = "$part" ]; then rest=; else rest=${rest#*/}; fi
        case "$part" in ''|.) continue;; ..) fail 'destination must not contain ..';; esac
        checked=$checked/$part
        [ ! -L "$checked" ] || fail "symlink destination component: $checked"
        if [ -e "$checked" ]; then [ -d "$checked" ] || fail "not a directory: $checked"; fi
    done
}
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then hash_output=$(sha256sum < "$1") || fail 'SHA256 tool failed'
    elif command -v shasum >/dev/null 2>&1; then hash_output=$(shasum -a 256 < "$1") || fail 'SHA256 tool failed'
    else fail 'sha256sum or shasum is required'; fi
    hash_value=${hash_output%% *}
    printf '%s\n' "$hash_value" | grep -Eq '^[0-9a-f]{64}$' || fail 'invalid SHA256 tool output'
    printf '%s\n' "$hash_value"
}
source_count=0
binary='' archive='' checksum='' bin_dir='' prefix='' target='' version='' repo=${SSHO_REPOSITORY:-ArnaudFra/ssho}
replace=false uninstall=false
while [ "$#" -gt 0 ]; do
    option=$1; shift
    case "$option" in
        --help|-h) usage; exit 0;;
        --replace) replace=true; continue;;
        --uninstall) uninstall=true; continue;;
        --binary|--archive|--checksum|--bin-dir|--prefix|--target|--version|--repo)
            [ "$#" -gt 0 ] && [ -n "$1" ] || fail "missing value for $option"
            value=$1; shift;;
        *) fail "unknown option: $option";;
    esac
    case "$option" in
        --binary) binary=$value; source_count=$((source_count + 1));;
        --archive) archive=$value; source_count=$((source_count + 1));;
        --checksum) checksum=$value;; --bin-dir) bin_dir=$value;; --prefix) prefix=$value;;
        --target) target=$value;; --version) version=$value;; --repo) repo=$value;;
    esac
done
[ -z "$version" ] || [ "$version" = latest ] || version_valid "$version" || fail 'invalid version (expected e.g. 0.1.0)'
if [ -z "$bin_dir" ]; then
    if [ -n "$prefix" ]; then bin_dir=$prefix/bin
    elif [ -n "${SSHO_INSTALL_DIR:-}" ]; then bin_dir=$SSHO_INSTALL_DIR
    else [ -n "${HOME:-}" ] || fail 'HOME is unset; use --bin-dir'; bin_dir=$HOME/.local/bin; fi
fi
case "$bin_dir" in *'
'*) fail 'destination must not contain a newline';; esac
bin_dir=$(absolute "$bin_dir")
check_directory_path "$bin_dir"
# A private workspace holds verified input; only the second stage is on the target filesystem.
workspace='' stage='' lock=''
cleanup() {
    [ -z "$stage" ] || rm -rf "$stage"
    [ -z "$workspace" ] || rm -rf "$workspace"
    [ -z "$lock" ] || rmdir "$lock"
    :
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
if "$uninstall"; then
    if [ "$source_count" -ne 0 ] || [ -n "$checksum$version$target" ] || "$replace"; then
        fail '--uninstall accepts destination options only'
    fi
    [ -d "$bin_dir" ] || { printf 'Nothing installed in %s\n' "$bin_dir"; exit 0; }
else
    [ "$source_count" -le 1 ] || fail 'choose exactly one of --binary or --archive'
    if [ -n "$binary" ]; then
        [ -z "$checksum" ] || fail '--checksum requires an archive'
        binary=$(absolute "$binary")
        [ "${binary##*/}" = ssho ] || fail 'binary must be named ssho (Rust build)'
        [ -f "$binary" ] && [ ! -L "$binary" ] && [ -x "$binary" ] && [ -s "$binary" ] || fail 'binary must be a nonempty executable regular file, not a symlink'
    else
        if [ -z "$target" ]; then
            case "$(uname -s):$(uname -m)" in
                Linux:x86_64) target=x86_64-unknown-linux-gnu;; Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-gnu;;
                Darwin:x86_64) target=x86_64-apple-darwin;; Darwin:arm64) target=aarch64-apple-darwin;;
                *) fail 'unsupported host; choose a supported --target explicitly';;
            esac
        fi
        target_valid "$target"
        asset=ssho-$target.tar.gz
        workspace=$(mktemp -d "${TMPDIR:-/tmp}/sshm-install.XXXXXX")
        if [ -z "$archive" ]; then
            printf '%s\n' "$repo" | grep -Eq '^[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*$' || fail 'invalid repository (expected OWNER/REPO)'
            [ -z "$checksum" ] || fail '--checksum is only for local archives'
            if [ -z "$version" ] || [ "$version" = latest ]; then release=latest/download
            else release=download/v$version; fi
            command -v curl >/dev/null 2>&1 || fail 'curl is required for downloading'
            archive=$workspace/$asset
            checksum=$archive.sha256
            curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --output "$archive" "https://github.com/$repo/releases/$release/$asset"
            curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --output "$checksum" "https://github.com/$repo/releases/$release/$asset.sha256"
        else
            archive=$(absolute "$archive")
            [ "${archive##*/}" = "$asset" ] || fail "archive must be named $asset"
            checksum=${checksum:-$archive.sha256}
            checksum=$(absolute "$checksum")
        fi
        [ -f "$archive" ] && [ ! -L "$archive" ] || fail 'archive must be a regular file, not a symlink'
        [ -f "$checksum" ] && [ ! -L "$checksum" ] || fail 'checksum file is required and must not be a symlink'
        # Freeze both inputs before inspection; never use a checksum as shell input.
        cp "$archive" "$workspace/input.tar.gz"
        cp "$checksum" "$workspace/input.sha256"
        digest=$(sha256 "$workspace/input.tar.gz")
        printf '%s  %s\n' "$digest" "$asset" > "$workspace/expected.sha256"
        cmp -s "$workspace/expected.sha256" "$workspace/input.sha256" || fail 'SHA256 checksum mismatch or invalid checksum format'
        tar -tzf "$workspace/input.tar.gz" > "$workspace/members"
        printf '%s\n' INSTALLATION.md LICENSE MANIFEST ssho > "$workspace/expected-members"
        sort "$workspace/members" > "$workspace/sorted-members"
        cmp -s "$workspace/expected-members" "$workspace/sorted-members" || fail 'archive must contain exactly the four protocol members'
        tar -tvzf "$workspace/input.tar.gz" > "$workspace/types"
        awk 'substr($0,1,1) != "-" {bad=1} END {exit (bad || NR != 4)}' "$workspace/types" || fail 'archive members must all be regular files'
        tar -xzOf "$workspace/input.tar.gz" MANIFEST > "$workspace/manifest"
        manifest_version=$(sed -n '2s/^version=//p' "$workspace/manifest")
        version_valid "$manifest_version" || fail 'invalid manifest version'
        printf 'format=1\nversion=%s\ntarget=%s\n' "$manifest_version" "$target" > "$workspace/expected-manifest"
        cmp -s "$workspace/manifest" "$workspace/expected-manifest" || fail 'invalid manifest format or target'
        [ -z "$version" ] || [ "$version" = latest ] || [ "$version" = "$manifest_version" ] || fail 'archive version does not match requested version'
        tar -xzOf "$workspace/input.tar.gz" ssho > "$workspace/ssho"
        [ -s "$workspace/ssho" ] || fail 'archive binary is empty'
        binary=$workspace/ssho
    fi
    mkdir -p "$bin_dir"
fi
check_directory_path "$bin_dir"
# Coordinates these scripts; the user must not concurrently mutate this directory externally.
mkdir "$bin_dir/.ssho-install-lock" 2>/dev/null || fail "installation lock unavailable in $bin_dir (active operation or stale lock)"
lock=$bin_dir/.ssho-install-lock
check_directory_path "$bin_dir"
destination=$bin_dir/ssho
[ ! -L "$destination" ] || fail "refusing symlink: $destination"
if [ -e "$destination" ]; then [ -f "$destination" ] || fail "refusing non-file: $destination"; fi
if "$uninstall"; then
    if [ -f "$destination" ]; then rm "$destination"; printf 'Removed %s; user data preserved.\n' "$destination"
    else printf 'Nothing installed in %s\n' "$bin_dir"; fi
    exit 0
fi
if [ -e "$destination" ] && ! "$replace"; then fail "already exists: $destination (use --replace explicitly)"; fi
stage=$(mktemp -d "$bin_dir/.sshm-install.XXXXXX")
cp "$binary" "$stage/ssho"
chmod 755 "$stage/ssho"
if "$replace"; then mv -f "$stage/ssho" "$destination"
else ln "$stage/ssho" "$destination"; fi
printf 'Installed %s\n' "$destination"
found=$(command -v ssho 2>/dev/null || :)
if [ -n "$found" ] && [ "$found" != "$destination" ]; then
    printf 'Warning: PATH resolves ssho to %s, which may mask this installation.\n' "$found" >&2
elif [ -z "$found" ]; then
    printf 'Add %s to PATH in your shell configuration to run ssho by name.\n' "$bin_dir" >&2
fi
