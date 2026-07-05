#!/bin/sh

set -eu

repository="shayyz-code/rmbg-cli"
install_dir="${RMBG_INSTALL_DIR:-$HOME/.local/bin}"
version="${RMBG_VERSION:-latest}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  Linux-x86_64|Linux-amd64) target="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64|Linux-arm64) target="aarch64-unknown-linux-gnu" ;;
  *)
    echo "rmbg installer: unsupported platform $(uname -s)/$(uname -m)" >&2
    exit 1
    ;;
esac

if [ "$(uname -s)" = "Linux" ] && ! getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
  echo "rmbg installer: Linux musl is not supported; glibc is required" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "rmbg installer: curl is required" >&2
  exit 1
fi

if [ "$version" = "latest" ]; then
  base_url="https://github.com/$repository/releases/latest/download"
else
  version="${version#v}"
  base_url="https://github.com/$repository/releases/download/v$version"
fi

archive="rmbg-$target.tar.gz"
temporary="$(mktemp -d 2>/dev/null || mktemp -d -t rmbg)"
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

curl --proto '=https' --tlsv1.2 -fLsS "$base_url/$archive" -o "$temporary/$archive"
curl --proto '=https' --tlsv1.2 -fLsS "$base_url/SHA256SUMS" -o "$temporary/SHA256SUMS"

expected="$(awk -v name="$archive" '$2 == name { print $1 }' "$temporary/SHA256SUMS")"
if [ -z "$expected" ]; then
  echo "rmbg installer: checksum for $archive is missing" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$temporary/$archive" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$temporary/$archive" | awk '{ print $1 }')"
else
  echo "rmbg installer: sha256sum or shasum is required" >&2
  exit 1
fi

if [ "$actual" != "$expected" ]; then
  echo "rmbg installer: checksum verification failed" >&2
  exit 1
fi

mkdir -p "$install_dir"
tar -xzf "$temporary/$archive" -C "$temporary"
install -m 755 "$temporary/rmbg" "$install_dir/rmbg"
install -m 755 "$temporary/uv" "$install_dir/uv"
install -m 644 "$temporary/THIRD_PARTY_NOTICES.md" "$install_dir/rmbg-THIRD-PARTY-NOTICES.md"

echo "rmbg installed at $install_dir/rmbg"
echo "bundled uv installed at $install_dir/uv"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) echo "Add $install_dir to PATH before using rmbg." ;;
esac
echo "Run 'rmbg setup' to install the local model runtime."
