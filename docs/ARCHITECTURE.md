# Architecture

Deep technical notes for contributors. For user-facing docs, start with the
[README](../README.md), [Getting Started](GETTING_STARTED.md), or
[Features](FEATURES.md).

## System Overview

ytmtui is a Rust terminal client for YouTube Music. It uses:

- **Ratatui + crossterm** for the terminal UI and input loop.
- **Tokio** for background network and blocking work.
- **InnerTube** (`music.youtube.com/youtubei/v1/*`) for metadata, search,
  account, library, home shelves, lyrics, and radio.
- **yt-dlp + ffmpeg + rodio** for audio resolution, remuxing, decoding, and
  playback.
- **rustfft** for the real-time audio spectrum visualizer.
- **ratatui-image** for terminal album art where the terminal supports it.

```text
                 ┌──────────────────────────────────────────────┐
                 │                    main.rs                   │
                 │ terminal setup + synchronous event loop      │
                 └───────────────┬──────────────────────────────┘
                                 │ draw                 ▲ key events
                                 ▼                      │
        ┌────────────────────────────────┐     ┌────────┴────────┐
        │              ui/               │     │     event.rs    │
        │ nav · main_panel · now_playing │     │ keys → App APIs │
        └────────────────┬───────────────┘     └────────┬────────┘
                         ▲ reads state                  │ mutates
                         │                              ▼
                 ┌───────┴───────────────────────────────────────┐
                 │                    app.rs                       │
                 │ central state, queue, sections, tasks, messages │
                 └───┬───────────────┬───────────────┬────────────┘
                     │ spawn         │ spawn         │ config/theme
                     ▼               ▼               ▼
             ┌────────────┐  ┌────────────────┐  ┌────────────────┐
             │ ytmusic/   │  │    player/     │  │ config/theme   │
             │ InnerTube  │  │ yt-dlp+rodio   │  │ persistence    │
             └────────────┘  └────────────────┘  └────────────────┘
                     ▲               │
                     └── mpsc Msg ───┴──► App::drain_messages()
```

## Module Map

| Path | Responsibility |
|---|---|
| `src/main.rs` | Terminal setup, panic hook, image-protocol detection, initial app load, main loop. |
| `src/lib.rs` | Public module wiring for tests and examples. |
| `src/app.rs` | Central application state, async task coordination, queue, sections, playback orchestration, recent history. |
| `src/app/authentication.rs` | Browser-cookie discovery/import through `yt-dlp --cookies-from-browser`. |
| `src/config.rs` | Persistent JSON config: volume, shuffle, repeat, cookies, theme, username, sync interval. |
| `src/theme.rs` | Accent themes and tinted UI palette helpers. |
| `src/event.rs` | Keyboard events mapped to `App` methods. |
| `src/visualizer.rs` | FFT spectrum analyzer fed by decoded playback samples. |
| `src/lyrics.rs` | UI-facing lyrics state and active-line advancement. |
| `src/ytmusic/mod.rs` | InnerTube client: search, browse, home, library, account, lyrics, radio, likes. |
| `src/ytmusic/auth.rs` | Netscape cookie parsing and `SAPISIDHASH` authorization. |
| `src/ytmusic/models.rs` | Shared data models: tracks, playlists/albums, artists, home sections, lyrics, search results. |
| `src/ytmusic/parse.rs` | JSON traversal and extraction helpers for InnerTube's nested responses. |
| `src/player/mod.rs` | Audio thread, download/remux/cache/prefetch, playback commands. |
| `src/player/tap.rs` | Sample tap that feeds the visualizer without changing audio output. |
| `src/ui/mod.rs` | Root responsive layout, search input, status/shortcut bar. |
| `src/ui/nav.rs` | Sidebar identity, sections, account state, and album art area. |
| `src/ui/main_panel.rs` | Home, search, lists, queue, lyrics, help, and empty states. |
| `src/ui/now_playing.rs` | Compact now-playing line and progress display. |

## Runtime Model

The UI loop is synchronous and draws from `App` state. Expensive work never runs
inside drawing:

1. `terminal.draw(ui::draw)` renders current state.
2. `crossterm::event::poll` waits for key events.
3. `event::handle_key` calls methods on `App`.
4. Background work sends `Msg` values through an `mpsc` channel.
5. `App::drain_messages()` applies completed work to state.
6. `App::tick()` advances periodic behavior such as playback progress, synced
   lyrics, automatic queue advance, and background Home/Library sync.

The `rodio::OutputStream` is not `Send`, so playback lives on a dedicated audio
thread controlled by commands such as play, pause, stop, seek, and volume.

## Search Flow

`/` → query → `Enter` → `App::do_search` → `YtMusicClient::search`.

Search runs four sub-searches in parallel:

- songs
- artists
- albums
- playlists

Results are grouped in the UI. `Enter` on a song starts playback, on an artist
loads top tracks, and on an album/playlist loads tracks through browse and
continuation pagination.

## Playback Flow

`Enter` on a track calls `App::start_current`:

1. `player::download_audio` resolves the best audio through `yt-dlp`.
2. Cached audio is reused when available.
3. M4A/AAC is remuxed to ADTS via `ffmpeg -c copy`, avoiding re-encode.
4. `Msg::AudioReady(path)` returns to the app.
5. The audio thread plays the file with `rodio`.
6. In parallel, ytmtui fetches lyrics, artwork, and prefetches the next track.

Natural track end is detected through shared player state and handled in
`App::tick()`. Repeat, shuffle, and radio/autoplay determine what happens next.

## Authentication Flow

ytmtui has anonymous, authenticated, invalid-cookie, and expired-session states.

Cookie sources are resolved in this order:

1. `YTM_COOKIES`.
2. `config.cookies`.
3. `~/.config/ytmtui/cookies.txt`.

Pressing `g` triggers in-app sign-in through `src/app/authentication.rs`, which
uses `yt-dlp --cookies-from-browser` to import browser cookies and write the
default cookie file. The client reconnects without requiring a full restart.

Authenticated requests add `Cookie`, `Authorization: SAPISIDHASH ...`,
`X-Goog-AuthUser`, and `X-Origin` headers. Authenticated `401`/`403` responses
transition to expired state while public search and playback paths remain
available.

## Home and Recent History

Home uses YouTube Music shelves (`musicCarouselShelfRenderer`) rather than a
flat recommendation list. Deduplication is scoped per shelf because the same
album or playlist can legitimately appear in multiple sections.

Recently played tracks are stored locally in `recent.json` under the app config
directory. They render before remote Home sections and can be played directly.

Background sync reloads Home and Library periodically using
`sync_interval_secs`, preserving the current selection by `browse_id` when
possible.

## Lyrics

Lyrics resolution starts from the `next` endpoint to discover the lyrics
`browseId`.

1. ytmtui first tries the Android Music client identity for timed lyrics.
2. If per-line timestamps exist, the UI uses synced karaoke-style state.
3. Otherwise it falls back to plain text from the Web Remix path.

The active synced line advances during `App::tick()` by comparing player
position with lyric timestamps.

## Visuals

Album art support is detected at startup. Known capable terminals are queried
for image protocol support; unknown terminals receive a Unicode half-block
fallback to avoid blocking the input loop.

The visualizer receives decoded samples through `player/tap.rs` and computes a
real FFT spectrum in `visualizer.rs`.

## Persistence

Config file shape:

```json
{
  "volume": 0.8,
  "shuffle": false,
  "repeat": "off",
  "cookies": null,
  "theme": "Roxo",
  "username": null,
  "sync_interval_secs": 300
}
```

The config writer preserves meaningful existing `cookies` and `username`
values instead of overwriting them with empty values.

## Development Checks

```bash
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

Documentation-only changes should still run `git diff --check` and verify that
README links point to existing files.
