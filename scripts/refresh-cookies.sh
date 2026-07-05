#!/usr/bin/env bash
# Refresh ~/.config/ytmtui/cookies.txt from a supported browser profile.
set -euo pipefail
umask 077

DEST="${XDG_CONFIG_HOME:-$HOME/.config}/ytmtui/cookies.txt"
mkdir -p "$(dirname "$DEST")"

if ! command -v yt-dlp >/dev/null; then
  echo "Error: yt-dlp was not found in PATH." >&2
  exit 1
fi

BROWSER="${1:-brave}"
TEMP="$(mktemp "${DEST}.tmp.XXXXXX")"
trap 'rm -f "$TEMP"' EXIT

echo "Exporting cookies from $BROWSER..."
yt-dlp --cookies-from-browser "$BROWSER" \
  --cookies "$TEMP" \
  --skip-download --no-warnings \
  -O '%(title)s' \
  'https://www.youtube.com/watch?v=jNQXAC9IVRw'

if [[ ! -s "$TEMP" ]]; then
  echo "Error: yt-dlp produced an empty cookie file." >&2
  exit 1
fi

if [[ -f "$DEST" ]]; then
  BACKUP="${DEST}.bak.$(date +%s)"
  cp -p "$DEST" "$BACKUP"
  echo "Backup: $BACKUP"
fi

chmod 600 "$TEMP"
mv -f "$TEMP" "$DEST"
trap - EXIT
echo "Saved: $DEST ($(stat -c %s "$DEST") bytes)"
echo "Restart ytmtui to use the refreshed session."
