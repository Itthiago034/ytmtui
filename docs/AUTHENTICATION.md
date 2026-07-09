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
4. Choose or let ytmtui detect a browser session.
5. ytmtui imports cookies through `yt-dlp --cookies-from-browser`, saves them
   to `~/.config/ytmtui/cookies.txt`, and reconnects without requiring a full
   app restart.

If the browser has no valid YouTube Music session, sign in there first and
press `g` again.

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
- The default cookie file is written with restrictive permissions.
- Treat cookies like account credentials: do not commit them, paste them into
  issues, or share them.
