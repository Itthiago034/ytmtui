#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/home/.config/ytmtui"

cat >"$TMP/bin/yt-dlp" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
destination=""
while (($#)); do
  if [[ "$1" == "--cookies" ]]; then
    destination="$2"
    shift 2
  else
    shift
  fi
done
[[ -n "$destination" ]]
printf '# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t9999999999\tSAPISID\ttest\n' >"$destination"
FAKE
chmod +x "$TMP/bin/yt-dlp"

# Override XDG_CONFIG_HOME as well: CI runners export it, and the script
# prefers it over $HOME/.config, which would write outside the sandbox.
HOME="$TMP/home" XDG_CONFIG_HOME="$TMP/home/.config" PATH="$TMP/bin:$PATH" \
  "$ROOT/scripts/refresh-cookies.sh" brave >/dev/null
destination="$TMP/home/.config/ytmtui/cookies.txt"
[[ -s "$destination" ]]
[[ "$(stat -c '%a' "$destination")" == "600" ]]

printf 'old-cookie\n' >"$destination"
cat >"$TMP/bin/yt-dlp" <<'FAKE_FAIL'
#!/usr/bin/env bash
exit 1
FAKE_FAIL
chmod +x "$TMP/bin/yt-dlp"

if HOME="$TMP/home" XDG_CONFIG_HOME="$TMP/home/.config" PATH="$TMP/bin:$PATH" \
  "$ROOT/scripts/refresh-cookies.sh" brave >/dev/null 2>&1; then
  echo "expected refresh failure" >&2
  exit 1
fi
grep -qx 'old-cookie' "$destination"
echo "refresh-cookies tests passed"
