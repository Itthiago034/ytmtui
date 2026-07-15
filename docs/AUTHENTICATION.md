# Authentication

**English** · [Português](AUTHENTICATION.pt-BR.md)

ytmtui does not ask for your password. Account features use browser cookies in
Netscape format, the same style consumed by `yt-dlp`.

## Anonymous Mode

Without cookies, ytmtui still supports:

- Search across songs, artists, albums, and playlists.
- Public playlist/album browsing.
- Playback.
- Lyrics when available.
- Themes, queue, radio/autoplay, and local recently played history.

Account-only features need cookies: private playlists, account name, library
data, personalized recommendations, and like/unlike actions.

## In-App Sign-In

1. Sign in to [music.youtube.com](https://music.youtube.com) in a supported
   browser.
2. Open ytmtui.
3. Press `g`.
4. ytmtui tries Firefox first. It tries Brave, Chrome, Chromium, Edge, Vivaldi,
   or Opera in that order only if exporting or validating the earlier
   candidate fails.
5. Review the detected browser/profile and YouTube account list. The selected
   profile is passed directly to `yt-dlp`, so Firefox or Brave can use the
   same profile where you signed in. Move with
   `Up`/`Down` or `k`/`j`, then press `Enter` to confirm the selected account.
6. ytmtui installs the prepared cookies at
   `~/.config/ytmtui/cookies.txt` and reconnects without requiring a full app
   restart.

Preparation and confirmation are separate. The account preview appears before
the live cookie file or active client is replaced. Pressing `Esc` in the
preview cancels the prepared sign-in and preserves the current cookies,
account, library, and session.

Before replacing the active cookies, ytmtui rechecks the selected account. If
the browser session expired or switched accounts in the meantime, the active
session remains unchanged.

After confirmation, ytmtui saves the successful browser/profile and selected
YouTube account index in `~/.config/ytmtui/config.json`. On restart, a non-zero
account index is reused. The saved browser/profile preference persists without
moving that browser ahead of Firefox in discovery order.

If the browser has no valid YouTube Music session, sign in there first and
press `g` again.

## Diagnose Without Changing Cookies

Run `ytmtui doctor` outside the TUI to check runtime tools, supported browsers,
cookie-file permissions and validity, connectivity, and the configured YouTube
account. The command never refreshes or replaces cookies. Exit code `0` means
no required check failed, even if optional warnings remain; `1` means at least
one required check failed. Sensitive details are redacted, but review the
output before sharing it.

## Script-Based Refresh

You can refresh cookies outside the app:

```bash
./scripts/refresh-cookies.sh brave
```

Use `firefox` or another supported browser value when needed. The script writes
the cookie file atomically with mode `600`; if export fails, the previous file
stays intact.

## Cookie Path Precedence

ytmtui resolves cookies in this order:

1. `YTM_COOKIES` environment variable.
2. `cookies` field in `~/.config/ytmtui/config.json`.
3. `~/.config/ytmtui/cookies.txt`.

## Expired Sessions

YouTube sessions expire naturally. When an authenticated request returns
`401` or `403`, ytmtui marks the session as expired, clears account-only data,
and keeps public search/playback paths alive. Press `g` or rerun the refresh
script to renew the session.

## Anti-Bot Playback Blocks

Some datacenter/server IPs trigger YouTube's "Sign in to confirm you're not a
bot" page. Use cookies for playback resolution even if you do not care about
library features:

```bash
export YTM_COOKIES="/path/to/cookies.txt"
ytmtui
```

On a personal residential connection, this is usually not needed.

## Privacy Notes

- ytmtui never asks for or stores your password.
- Cookie files are local to your machine.
- Prepared and installed cookie files use mode `0600` on Unix.
- Treat cookies like account credentials: do not commit them, paste them into
  issues, or share them.
