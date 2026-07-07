# Troubleshooting

**English** · [Português](TROUBLESHOOTING.pt-BR.md)

Step-by-step fixes for the most common issues. If yours isn't here, please
open an issue with your OS, terminal emulator, and the exact error message.

## "Missing dependencies" warning at startup

ytmtui checks for `yt-dlp`, `ffmpeg`, and `deno` at startup
(`player::missing_dependencies()` in `src/player/mod.rs`) and shows a
warning in the status bar if any essential one (`yt-dlp`, `ffmpeg`) is
missing — playback will fail or hang without them.

1. Install `yt-dlp`: `pip install yt-dlp` (or your distro's package).
2. Install `ffmpeg`: `apt install ffmpeg` (Debian/Ubuntu) or
   `brew install ffmpeg` (macOS).
3. Install `deno` (optional, but required by recent `yt-dlp` versions for
   some JS challenges): see https://deno.land.
4. Restart ytmtui — the warning only shows once at launch.

## Session expired / "Session expired. Refresh browser cookies and restart"

Your cookie file's session is no longer valid (YouTube Music sessions
naturally expire). Fix:

```bash
./scripts/refresh-cookies.sh brave   # or: firefox
```

Make sure you're actually signed in to
[music.youtube.com](https://music.youtube.com) in that browser first — the
script exports whatever session cookies the browser currently has. Then
restart ytmtui. Search, public playlists, and lyrics keep working during
this time; only account-specific data (your library, liked songs) is
cleared until you refresh.

## YouTube blocks playback with "Sign in to confirm you're not a bot"

Common on datacenter/server IPs, not on personal residential connections.
Export a cookie file and point `YTM_COOKIES` at it — this doesn't require
using an account for the library, it's purely to satisfy the bot check:

```bash
export YTM_COOKIES="/path/to/cookies.txt"
./target/release/ytmtui
```

You can generate this file the same way as for signing in
(`./scripts/refresh-cookies.sh <browser>`), or export one manually from your
browser (e.g. the "Get cookies.txt" extension), in Netscape format.

## No sound at all, no error shown

`rodio`'s `OutputStream::try_default()` (in `src/player/mod.rs`) fails
silently if no audio output device is available — the audio thread simply
exits without surfacing an error in the UI. Check:

1. Is any audio device actually available on the system?
   (`aplay -l` on Linux, or check your OS's sound settings.)
2. On Linux, is ALSA installed? (`apt install libasound2-dev` — needed to
   build; the runtime library is usually already present.)
3. On a headless/server/container environment, there may be no audio
   device at all — playback controls will appear to do nothing, since
   there's nowhere to send the sound.

## Album art doesn't appear (just blank space or half-blocks when I expected a real image)

ytmtui detects terminal image-protocol support at startup
(`env_reports_image_support` in `src/main.rs`) and only *queries* terminals
it recognizes (Kitty, Ghostty, WezTerm, iTerm2, foot, Konsole) — querying an
unrecognized terminal risks it never answering and stealing key presses
from the event loop, so unknown terminals always get the Unicode
half-block fallback instead. If your terminal supports Kitty, Sixel, or
iTerm2 graphics but isn't recognized, this is a known limitation, not a
bug — feel free to open an issue naming your terminal and its
`$TERM`/`$TERM_PROGRAM` values.

If your terminal *is* one of the recognized ones and you still only see
half-blocks, double-check its image-protocol support is actually enabled
(some terminals gate it behind a setting).

## Album art briefly shows the previous track's cover ("ghosting")

This was a real bug (Kitty/Sixel graphics can outlive the terminal cell
that displayed them) and is fixed as of the version that added the
real-time spectrum visualizer — the terminal is now force-cleared on every
track change. If you still see it on a current build, please open an issue
with your terminal emulator.

## Lyrics show up as plain text instead of synced/highlighted

This is expected behavior for tracks that don't have per-line timed lyrics
in YouTube Music's catalog — ytmtui always tries the synced-lyrics path
first and only falls back to plain Musixmatch-sourced text when timestamps
aren't available for that specific track. It's not something you can force;
it depends entirely on what YouTube Music has indexed for that song.

## Background sync feels too frequent / infrequent

Adjust `sync_interval_secs` in `~/.config/ytmtui/config.json` (seconds
between automatic Home/Library refreshes; default `300`). Values below 30
are raised to a 30-second floor to avoid a hot-loop of API calls.
