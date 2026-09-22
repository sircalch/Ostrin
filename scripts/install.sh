#!/usr/bin/env sh
set -eu

repository="${OSTRIN_REPOSITORY:-sircalch/Ostrin}"
version="${OSTRIN_VERSION:-latest}"
install_dir="${OSTRIN_INSTALL_DIR:-${HOME:-}/.local/bin}"

usage() {
    cat <<'USAGE'
Install the ostrinc compiler from a published Ostrin GitHub release.

Usage: install.sh [--version VERSION] [--install-dir DIRECTORY] [--repository OWNER/REPO]

Environment overrides: OSTRIN_VERSION, OSTRIN_INSTALL_DIR, OSTRIN_REPOSITORY
USAGE
}

fail() {
    printf 'ostrinc installer: %s\n' "$1" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || fail "--version needs a value"
            version="$2"
            shift 2
            ;;
        --install-dir)
            [ "$#" -ge 2 ] || fail "--install-dir needs a value"
            install_dir="$2"
            shift 2
            ;;
        --repository)
            [ "$#" -ge 2 ] || fail "--repository needs a value"
            repository="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            fail "unknown option '$1' (use --help for usage)"
            ;;
    esac
done

case "$repository" in
    */*/*|"") fail "repository must look like OWNER/REPO" ;;
    */*) ;;
    *) fail "repository must look like OWNER/REPO" ;;
esac

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"
if command -v sha256sum >/dev/null 2>&1; then
    checksum_tool="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    checksum_tool="shasum"
else
    fail "sha256sum or shasum is required to verify the release"
fi

system="$(uname -s)"
machine="$(uname -m)"
case "$system:$machine" in
    Linux:x86_64|Linux:amd64)
        target="x86_64-unknown-linux-gnu"
        ;;
    Darwin:arm64|Darwin:aarch64)
        target="aarch64-apple-darwin"
        ;;
    *)
        fail "no published Ostrin archive is available for $system/$machine"
        ;;
esac

if [ "$version" = "latest" ]; then
    api_url="https://api.github.com/repos/${repository}/releases/latest"
    version="$(curl -fsSL --retry 3 -H 'Accept: application/vnd.github+json' "$api_url" \
        | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
        | head -n 1)" || fail "could not resolve the latest published release"
    [ -n "$version" ] || fail "the repository has no published release"
fi

case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
esac
case "$tag" in
    v[0-9A-Za-z._-]*) ;;
    *) fail "version must be a release tag such as v0.1.0" ;;
esac

archive="ostrinc-${tag}-${target}.tar.gz"
base_url="https://github.com/${repository}/releases/download/${tag}"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/ostrinc-install.XXXXXX")"
cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT INT TERM

curl -fsSL --retry 3 -o "$work_dir/$archive" "$base_url/$archive" \
    || fail "could not download $archive; confirm that release $tag exists"
curl -fsSL --retry 3 -o "$work_dir/$archive.sha256" "$base_url/$archive.sha256" \
    || fail "could not download the checksum for $archive"

cd "$work_dir"
if [ "$checksum_tool" = "sha256sum" ]; then
    sha256sum --check "$archive.sha256" || fail "checksum verification failed for $archive"
else
    shasum -a 256 --check "$archive.sha256" || fail "checksum verification failed for $archive"
fi

tar -xzf "$archive" || fail "could not extract $archive"
binary="$work_dir/ostrinc-${tag}-${target}/ostrinc"
[ -f "$binary" ] || fail "release archive did not contain ostrinc"

mkdir -p "$install_dir"
staged="$install_dir/.ostrinc.$$"
cp "$binary" "$staged"
chmod 0755 "$staged"
mv -f "$staged" "$install_dir/ostrinc"

expected_version="ostrinc ${tag#v}"
actual_version="$("$install_dir/ostrinc" --version 2>/dev/null || true)"
[ "$actual_version" = "$expected_version" ] || fail "installed compiler reported '$actual_version', expected '$expected_version'"

printf 'Installed %s to %s/ostrinc (checksum verified).\n' "$actual_version" "$install_dir"
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *) printf 'Add %s to PATH to use ostrinc from new shells.\n' "$install_dir" ;;
esac
