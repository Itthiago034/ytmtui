# Features

**English** · [Português](FEATURES.pt-BR.md)

ytmtui is built around one idea: YouTube Music should feel fast, visual, and
keyboard-native inside a terminal.

## Search That Feels Like Music

Search runs across **songs, artists, albums, and playlists**. Results are
grouped by type so you can play a track, open an artist, or load an album or
playlist without leaving the keyboard.

| Flow | What happens |
|---|---|
| Song | `Enter` starts playback and builds the queue |
| Artist | `Enter` opens top tracks |
| Album | `Enter` loads the album tracks |
| Playlist | `Enter` loads playlist tracks with pagination |

## Playback Pipeline

ytmtui uses `yt-dlp` to resolve the audio stream, `ffmpeg` to remux M4A/AAC to
ADTS without re-encoding, and `rodio` to play decoded audio. The app also keeps
the next track warm through cache and prefetch so repeat plays and transitions
feel faster.

## Home, Recommendations, and Recent Tracks

The Home screen uses YouTube Music's own grouped shelves, such as quick picks
and mixes, instead of flattening everything into one anonymous list. Local
recently played tracks are stored in `recent.json` and shown before
recommendations so you can jump back into what you were hearing.

## Lyrics

ytmtui first tries to load synced lyrics with per-line timestamps. When they
exist, the current line follows playback like a karaoke view. If YouTube Music
only has plain lyrics for a track, ytmtui falls back to readable text with
manual scrolling.

## Visualizer and Album Art

The visualizer is based on a real FFT over playback samples, not a fake
animation. Album art renders through supported terminal image protocols
(Kitty/Sixel/iTerm2-style support) with a Unicode half-block fallback when a
terminal cannot display images.

## Queue and Radio

The queue is meant for flow:

- `a` adds the selected track without interrupting playback.
- `n` and `p` move between tracks.
- `z` toggles shuffle.
- `r` cycles repeat mode.
- When the queue ends, radio/autoplay can continue with related tracks.

## Themes and Terminal UI

Themes are not just accent colors. The UI carries tinted text, muted colors,
borders, progress bars, and panel styling so the whole terminal changes mood
together. Cycle themes with `t`; the choice is persisted.

## Account Features

Anonymous mode supports search, public browsing, playback, and lyrics. With
cookies, ytmtui can show your account name, private playlists, personalized
library data, recommendations, and like/unlike actions.

## Built for Keyboard Memory

The app follows familiar terminal movement: `h/j/k/l`, arrows, `/` for search,
`?` for help, and `q` to quit. The full map is in [Keymap](KEYMAP.md).

## Custom themes

ytmtui ships eleven themes (`t` cycles them). To add your own, drop a `.toml`
file into `~/.config/ytmtui/themes/`:

```toml
name = "My theme"
accent = "#89b4fa"

# Everything below is optional. Omitted colors are derived from the accent,
# following the same tinted neutral scale the built-in themes use.
secondary     = "#94e2d5"   # artists, subtitles
accent_fg     = "#1e1e2e"   # text on the selected row
player        = "#89b4fa"   # progress bar and player border
highlight_bg  = "#313244"   # selected row background
selected_card = "#313244"   # selected Home card background
provider_badge = "#94e2d5"
text          = "#cdd6f4"
subtext       = "#a6adc8"
muted         = "#6c7086"
border        = "#45475a"
```

Only `name` and `accent` are required. Colors are `#rrggbb` (the leading `#`
is optional). A malformed color falls back to the derived value; a file
without a usable `accent` is skipped and reported in the status bar at
startup, so one broken file never stops the app from opening.

User themes are listed after the built-in ones, sorted by filename.
