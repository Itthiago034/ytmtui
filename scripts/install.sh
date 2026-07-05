#!/usr/bin/env bash
# Installs a prebuilt ytmtui release binary for this machine's OS/arch.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Itthiago034/ytmtui/master/scripts/install.sh | bash
#   ./install.sh [tag]            # install a specific release, e.g. ./install.sh v0.2.0
#   YTMTUI_INSTALL_DIR=/usr/local/bin ./install.sh
set -euo pipefail

REPO="Itthiago034/ytmtui"
BIN_NAME="ytmtui"
INSTALL_DIR="${YTMTUI_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"
case "$os-$arch" in
  Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
  Darwin-arm64 | Darwin-aarch64) target="aarch64-apple-darwin" ;;
  *)
    echo "Error: no prebuilt binary for $os/$arch." >&2
    echo "Supported: Linux x86_64, macOS Apple Silicon (arm64)." >&2
    echo "Build from source instead: cargo install --path ." >&2
    exit 1
    ;;
esac

tag="${1:-}"
if [ -z "$tag" ]; then
  echo "Looking up the latest release..."
  tag="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"
fi
if [ -z "$tag" ]; then
  echo "Error: could not determine the latest release tag." >&2
  echo "Pass one explicitly: ./install.sh v0.2.0" >&2
  exit 1
fi

asset="ytmtui-${tag}-${target}.tar.gz"
url="https://github.com/$REPO/releases/download/${tag}/${asset}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $asset (release $tag)..."
if ! curl -fsSL "$url" -o "$tmp/$asset"; then
  echo "Error: failed to download $url" >&2
  echo "Check available releases: https://github.com/$REPO/releases" >&2
  exit 1
fi

tar xzf "$tmp/$asset" -C "$tmp"
staging_dir="$(find "$tmp" -maxdepth 1 -type d -name 'ytmtui-*')"
if [ -z "$staging_dir" ] || [ ! -f "$staging_dir/$BIN_NAME" ]; then
  echo "Error: unexpected archive layout in $asset." >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
install -m 755 "$staging_dir/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
echo "Installed: $INSTALL_DIR/$BIN_NAME"

echo ""
missing=()
for dep in yt-dlp ffmpeg deno; do
  command -v "$dep" >/dev/null 2>&1 || missing+=("$dep")
done
if [ "${#missing[@]}" -gt 0 ]; then
  echo "Missing runtime dependencies: ${missing[*]}"
  echo "  yt-dlp  -> pip install yt-dlp"
  echo "  ffmpeg  -> apt install ffmpeg   (or: brew install ffmpeg)"
  echo "  deno    -> https://deno.land"
  echo ""
fi

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo "$INSTALL_DIR is not in your PATH. Add this to your shell profile:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
    ;;
esac

echo "Done. Run 'ytmtui' to start (or '$INSTALL_DIR/$BIN_NAME' if not yet on PATH)."
