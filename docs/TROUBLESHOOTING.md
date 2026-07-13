# Troubleshooting

**English** · [Português](TROUBLESHOOTING.pt-BR.md)

Fast fixes for the issues most likely to interrupt playback. If yours is not
listed, open an issue with your OS, terminal emulator, install method, and the
exact error message.

Useful companion docs:

- [Getting Started](GETTING_STARTED.md)
- [Authentication](AUTHENTICATION.md)
- [Keymap](KEYMAP.md)

## Start with Doctor

Run `ytmtui doctor` outside the TUI. It checks runtime tools, supported
browsers, cookie-file permissions and validity, connectivity, and the
configured YouTube account. It never refreshes or replaces cookies. Exit code
`0` means no required check failed, even if optional warnings remain; `1`
means at least one required check failed. Sensitive details are redacted, but
review the output before sharing it.

## Missing Dependencies at Startup

ytmtui checks for `yt-dlp`, `ffmpeg`, and `deno` at launch. Playback needs
`yt-dlp` and `ffmpeg`; `deno` is recommended for recent `yt-dlp` JavaScript
challenges.

| Missing | Fix |
|---|---|
| `yt-dlp` | `pip install yt-dlp` or use your distro package |
| `ffmpeg` | `apt install ffmpeg`, `brew install ffmpeg`, or platform equivalent |
| `deno` | Install from https://deno.land |

Restart the app after installing missing tools.

## Session Expired

If account data disappears or the UI reports an expired session, refresh cookies.

Inside ytmtui:

```text
press g
```

Or from the shell:

```bash
./scripts/refresh-cookies.sh brave
```

Make sure the browser is signed in to
[music.youtube.com](https://music.youtube.com). Public search, public browsing,
and lyrics keep working while account-only data is cleared.

The in-app flow always checks Firefox first. Another supported browser is tried
only after export or account validation fails. When the account preview opens,
confirm the intended account with `Enter`; `Esc` cancels without replacing the
current cookies or session. A confirmed browser/profile and account index are
saved, including non-zero account indexes used after restart.

## YouTube Says "Sign in to confirm you're not a bot"

This usually affects datacenter/server IPs. Use a cookie file for audio
resolution even if you do not need account library features:

```bash
export YTM_COOKIES="/path/to/cookies.txt"
ytmtui
```

Generate cookies with `g` in the app, `./scripts/refresh-cookies.sh <browser>`,
or a Netscape-format browser export.

## No Sound

Check the audio stack first:

1. Confirm the system has an output device.
2. On Linux, install ALSA development libraries if building from source:
   `apt install libasound2-dev`.
3. Avoid headless/server/container environments unless they expose an audio
   device.
4. Confirm another local audio app can play sound.

If there is no output device, playback controls may appear to work but there is
nowhere for `rodio` to send audio.

## Album Art Does Not Render

ytmtui only queries terminals it knows can answer image-protocol requests.
Recognized terminals include Kitty, Ghostty, WezTerm, iTerm2, foot, and Konsole.

If the terminal is unknown, ytmtui uses a Unicode half-block fallback. If the
terminal is recognized but images still do not render, check that its image
protocol support is enabled.

## Album Art Ghosting

Kitty/Sixel graphics can outlive the terminal cells that showed them. Current
builds force-clear the terminal on track changes and resize events. If ghosting
still happens on a current build, open an issue with your terminal name and
version.

## Lyrics Are Plain Text

Some tracks do not have timed lyrics in YouTube Music's catalog. ytmtui tries
synced lyrics first and falls back to plain Musixmatch-style text when
timestamps are unavailable.

## Background Sync Is Too Fast or Too Slow

Edit `sync_interval_secs` in `~/.config/ytmtui/config.json`.

```json
{
  "sync_interval_secs": 300
}
```

Values below 30 seconds are raised to a 30-second floor.

## Search Works But Account Library Does Not

Search can run anonymously. Library, account name, private playlists, likes, and
personalized recommendations require valid cookies. Press `g` or see
[Authentication](AUTHENTICATION.md).
