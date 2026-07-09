<h1 align="center">ytmtui</h1>

<p align="center">
  <strong>YouTube Music for your terminal.</strong><br />
  Search, play, queue, follow lyrics, and keep the music close to your shell.
</p>

<p align="center">
  <a href="https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml">
    <img src="https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml/badge.svg" alt="CI" />
  </a>
  <a href="https://github.com/Itthiago034/ytmtui/releases">
    <img src="https://img.shields.io/github/v/release/Itthiago034/ytmtui?include_prereleases&sort=semver&label=release&color=ff2d46" alt="Release" />
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license" />
  </a>
  <img src="https://img.shields.io/badge/Rust-Ratatui-f97316?logo=rust&logoColor=white" alt="Rust + Ratatui" />
</p>

<p align="center">
  <a href="https://git.io/typing-svg">
    <img src="https://readme-typing-svg.demolab.com?font=Fira+Code&weight=500&size=16&duration=2600&pause=900&color=FF2D46&center=true&vCenter=true&width=700&lines=Keyboard-first+music+navigation;Synced+lyrics+and+real-time+visualizer;Rust-powered+terminal+playback" alt="ytmtui animated terminal highlights" />
  </a>
</p>

<p align="center">
  <strong>English</strong> · <a href="README.pt-BR.md">Português</a>
</p>

**ytmtui** is a terminal client (TUI) for **YouTube Music**, written in
**Rust** with **[Ratatui](https://ratatui.rs)**. It brings search, playback,
queue management, synced lyrics, themes, album art, and a real-time audio
visualizer into a keyboard-first terminal interface.

```text
 ♫ ytmtui        ┌ Search ─────────────────────────────────────────────┐
─────────────────│ 🔍 coldplay yellow                                  │
  T  Thiago S.   └─────────────────────────────────────────────────────┘
                 ┌ Search results ─────────────────────────────────────┐
┌ Menu ─────────┐│ ▶  1  Yellow — Coldplay                        4:27 │
│ 🔍 Search     ││    2  Viva La Vida — Coldplay                  4:03 │
│ 📚 Library    ││    3  The Scientist — Coldplay                 5:10 │
│ 🎵 Playlists  ││                                                     │
│ 👤 Artists    ││                                                     │
│ 📃 Queue      ││                                                     │
│ 📝 Lyrics     ││                                                     │
│ ❓ Help       ││                                                     │
└───────────────┘└─────────────────────────────────────────────────────┘
┌ ▶ Player ───────────────────────────────────────────────────────────────┐
│ ▀▀▀▀▀  Yellow                                                            │
│ ▀▀▀▀▀  Coldplay  •  Parachutes                                           │
│ ▀▀▀▀▀  ██████████░░░░░░░░░░░░  1:45 / 4:27                                │
│        ▶ playing  🔊 ████████░░ 80%  🔀 off  🔁 off  🎨 Purple           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## Table of Contents

- [Screenshots](#-screenshots)
- [Quick install](#-quick-install)
- [Features](#-features)
- [Requirements](#-requirements)
- [Building from source](#-building-from-source)
- [Keyboard shortcuts](#️-keyboard-shortcuts)
- [How to use](#-how-to-use)
- [Authentication and cookies](#-authentication-and-cookies)
- [Customization](#-customization)
- [Troubleshooting](#-troubleshooting)
- [Project architecture](#️-project-architecture)
- [Development](#-development)
- [Legal notice and license](#️-legal-notice)

---

## 📸 Screenshots

| Home (visualizer + sections) | Synced lyrics |
|---|---|
| ![ytmtui's home screen with the spectrum visualizer and sections](docs/screenshots/home.png) | ![Synced lyrics highlighting the current line](docs/screenshots/lyrics-synced.png) |

| Search | Help |
|---|---|
| ![Search results for songs, artists, and playlists](docs/screenshots/search.png) | ![Help screen with all keyboard shortcuts](docs/screenshots/help.png) |

---

## 🚀 Quick install

The fastest way to install, no Rust toolchain required — downloads the
prebuilt binary from the [latest release](https://github.com/Itthiago034/ytmtui/releases)
for Linux (x86_64) or macOS (Apple Silicon):

```bash
curl -fsSL https://raw.githubusercontent.com/Itthiago034/ytmtui/master/scripts/install.sh | bash
```

The script detects your system, installs the binary to `~/.local/bin`, and
warns about any missing runtime dependency (`yt-dlp`, `ffmpeg`, `deno` — see
[Requirements](#-requirements)). Then just run `ytmtui`.

> On a different platform, or want to build it yourself? See
> [Building from source](#-building-from-source).

---

## ✨ Features

- 🔍 **Search** for songs, artists, and playlists on YouTube Music (no
  authentication needed). The three sub-searches run **in parallel** for
  lower latency.
- 🎵 **Playback** with streaming (via `yt-dlp`). `m4a`/AAC audio is
  **remuxed** to ADTS (`ffmpeg -c copy`, no re-encode), for reliable, fast
  playback.
- 🔐 **Automatic sign-in**: detects cookies at `~/.config/ytmtui/cookies.txt`,
  shows your **account name** and **your playlists** (📚 Library section).
- 🏠 **Home organized into sections** — "Quick picks", "Mixed for you", and
  others, exactly as YouTube Music itself groups your recommendations —
  with a **real-time spectrum visualizer** (real FFT over the audio
  playing, Cava-style) above the sections.
- 🎤 **Synced lyrics** (karaoke-style): when available, the current line is
  highlighted and the view auto-scrolls with the song. When YouTube Music
  only has untimed lyrics (Musixmatch), the app shows plain text with
  manual scroll (`j`/`k`).
- 🔄 **Background sync**: Home and Library refresh themselves periodically
  (every 5 minutes by default, configurable) — whatever you like/follow on
  another device shows up without restarting.
- 👤 **Artist page**: `Enter` on an artist lists their top tracks.
- 📻 **Radio/autoplay**: continues with related tracks once the queue ends.
- ➕ **Queue**: `a` adds the selected track to the queue without
  interrupting the current one.
- 💚 **Like/unlike** the current track (`f`) on your account.
- 🎨 **Color themes** (Purple, YT Red, Green, Ocean, Amber, Pink) switchable
  in real time with `t`; the choice is remembered across sessions.
- ⚡ **Cache + prefetch**: the queue's next track is pre-downloaded, and
  already-played tracks play back instantly on repeat.
- ⏯️ **Player controls**: play/pause, next, previous, stop, **seek (±5s)**,
  volume.
- 🔀 **Shuffle** and 🔁 **repeat** (off / all / one track).
- 📊 **Progress bar** with current/total time.
- 🖼️ **Album art** rendered natively in the terminal (Kitty/Sixel/iTerm2
  image protocols), with a Unicode half-block fallback elsewhere.
- 📃 **Playback queue** with automatic advance and **pagination** of long
  playlists.
- ⚙️ **Persistent configuration** (volume, shuffle, repeat, and the sync
  interval are remembered across sessions).
- ⌨️ **Keyboard navigation** in *vim* style (`h/j/k/l`) or arrow keys.
- 🧩 **Panel-based interface** (sidebar menu, main list, and player).
- ⏳ **Loading spinner** during searches, playlists, and downloads.
- 🩺 **Dependency check** at startup, warning if `yt-dlp`/`ffmpeg` is missing.

---

## 📦 Requirements

Before building/running, you'll need:

| Dependency | For | Install |
|-------------|----------|------------|
| **Rust** (1.75+) and Cargo | building the project | https://rustup.rs |
| **yt-dlp** | resolving/downloading song audio | `pip install yt-dlp` |
| **deno** | JS runtime required by recent yt-dlp versions | https://deno.land |
| **ffmpeg** | remuxes `m4a`/AAC to ADTS before playback (reliability) | `apt install ffmpeg` / `brew install ffmpeg` |
| **ALSA** (Linux) | audio output | `apt install libasound2-dev` |

> On macOS and Windows, audio output works natively (CoreAudio / WASAPI), so
> ALSA isn't needed.

---

## 🔧 Building from source

Need another platform (Windows, Linux ARM), want to test a change, or simply
prefer to build it yourself:

```bash
# 1. Clone the repository
git clone https://github.com/Itthiago034/ytmtui.git
cd ytmtui

# 2. Build in release mode (recommended)
cargo build --release

# 3. Run
./target/release/ytmtui

# — or, for development —
cargo run
```

### Install as a command (`ytmtui`)

To install the binary onto your `PATH` (`~/.cargo/bin`):

```bash
cargo install --path .
```

Then just run `ytmtui` from anywhere.

---

## ⌨️ Keyboard shortcuts

### Navigation
| Key | Action |
|-------|------|
| `↑` / `↓` or `k` / `j` | Move selection up/down |
| `←` / `→` or `h` / `l` | Switch between the sidebar menu and the list |
| `Tab` | Toggle focus (menu ↔ list) |
| `Enter` | Play the song / open the playlist / open the artist |
| `a` | Add the selected track to the queue |

### Search
| Key | Action |
|-------|------|
| `/` | Open the search field |
| *(type)* + `Enter` | Run the search |
| `Esc` | Cancel the search |

### Playback
| Key | Action |
|-------|------|
| `Space` | Play / Pause |
| `n` | Next track |
| `p` | Previous track |
| `[` / `]` | Seek back / forward 5 seconds |
| `z` | Toggle shuffle |
| `r` | Cycle repeat mode (off / all / one) |
| `f` | Like / unlike the current track (requires a signed-in account) |
| `s` | Stop |
| `+` / `=` | Volume up |
| `-` / `_` | Volume down |

### Appearance
| Key | Action |
|-------|------|
| `t` | Cycle the color theme (saved automatically) |

### General
| Key | Action |
|-------|------|
| `?` | Open the help screen |
| `q` or `Ctrl+C` | Quit |

---

## 🧭 How to use

1. Press `/`, type a song or artist name, and hit `Enter`.
2. Use `j`/`k` (or arrows) to navigate results and `Enter` to play. The
   results list becomes the **playback queue**, and the next song plays
   automatically once one ends.
3. Access **🎵 Playlists** or **👤 Artists** in the sidebar to see those
   results. In *Playlists*, press `Enter` to load the tracks.
4. See the current track's **📝 Lyrics** in its section (use `j`/`k` to
   scroll the plain-text fallback; synced lyrics auto-scroll on their own).
5. Follow along on the **Player** bar at the bottom, with cover art,
   progress, and volume.

---

## 🔑 Authentication and cookies

ytmtui accesses your YouTube Music library through a cookie file (Netscape
format). It **never** asks for or stores your password. The file path is
resolved in this order: `YTM_COOKIES` env var → `cookies` field in
`config.json` → `~/.config/ytmtui/cookies.txt` (default).

### Signing in (playlists, likes, recommendations)

1. Sign in at [music.youtube.com](https://music.youtube.com) in your
   browser.
2. Generate/refresh the local cookie file:

   ```bash
   ./scripts/refresh-cookies.sh brave   # or: firefox
   ```

   The script writes the new file atomically, with `600` permissions. If
   the export fails, the previous file stays intact.
3. Restart ytmtui and open **📚 Library** — a valid session shows your
   account name and private playlists.

An invalid cookie file starts the app in anonymous mode. An HTTP `401`/`403`
on an authenticated call marks the session as expired and clears only
account data — search, public playlists, and lyrics keep working normally.
Run the script again and restart to sign in again.

### Working around YouTube's anti-bot block

In some environments/IPs (e.g. datacenter servers), YouTube may require
verification ("*Sign in to confirm you're not a bot*") and block `yt-dlp`
from resolving the stream. The same cookie file fixes this — just point to
it with `YTM_COOKIES`, even without using the account for the library:

```bash
export YTM_COOKIES="/path/to/cookies.txt"
./target/release/ytmtui
```

> **Search, playlists, and lyrics work normally without cookies** — only
> audio playback may require them in blocked environments. On personal
> machines (residential IP), it's usually **not** needed.

---

## 🎨 Customization

Preferences live in **`~/.config/ytmtui/config.json`** (Linux) and can be
hand-edited:

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

- **`theme`** — color theme. Values: `Roxo`, `YT Vermelho`, `Verde Spotify`,
  `Oceano`, `Âmbar`, `Rosa`. Also switchable with `t` inside the app.
- **`username`** — custom display name in the sidebar. If `null`, the app
  uses your real YouTube Music account name.
- **`cookies`** — path to the cookie file (optional; the app defaults to
  `~/.config/ytmtui/cookies.txt`).
- **`sync_interval_secs`** — interval, in seconds, between automatic
  background refreshes of Home and Library (default: `300` = 5 minutes).
  Very low values are raised to a 30s minimum.

---

## 🩺 Troubleshooting

Installation, expired cookies, YouTube's anti-bot block, no audio output,
album art not showing — full step-by-step guide in
**[`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)**.

---

## 🏗️ Project architecture

```
src/
├── main.rs            # Entry point: terminal setup + event loop
├── lib.rs             # Module exposure (enables tests/examples)
├── app.rs              # Central state and async task coordination
├── app/
│   └── authentication.rs # Cookie path resolution and session state
├── config.rs           # Persistent configuration (volume, shuffle, repeat, cookies, theme)
├── theme.rs             # Color themes (accent presets) for the UI
├── event.rs             # Key handling → actions
├── visualizer.rs        # Spectrum analyzer (FFT) for the Home visualizer
├── lyrics.rs             # UI-facing lyrics state and synced-line advance
├── ytmusic/
│   ├── mod.rs          # Internal API client (InnerTube): search, library, account
│   ├── auth.rs          # Cookie-based authentication (SAPISIDHASH)
│   ├── models.rs        # Models: Track, Playlist, Artist, HomeSection, Lyrics, SearchResults
│   └── parse.rs         # Parsing helpers for the API's nested JSON
├── player/
│   ├── mod.rs           # Audio player (rodio) + download via yt-dlp
│   └── tap.rs            # Intercepts decoded samples for the visualizer
└── ui/
    ├── mod.rs           # Root layout (wide/narrow), search input and status bar
    ├── nav.rs            # Navigation column (identity, account, sections)
    ├── main_panel.rs     # Main list panel (tracks/playlists/artists/queue/lyrics/help)
    └── now_playing.rs    # Compact playback summary (track line + progress)
```

### Technical details
- **InnerTube API**: calls use the internal API
  (`music.youtube.com/youtubei/v1/*`) with the `WEB_REMIX` client.
  Search/lyrics work without cookies; the library and account name use
  cookie-based authentication (`SAPISIDHASH`).
- **Synced lyrics**: the same lyrics call is repeated with the Android
  app's client identity (`ANDROID_MUSIC`), which exposes per-line
  timestamps when available; without them, it falls back to plain text
  (`WEB_REMIX`, via Musixmatch).
- **Sectioned Home**: `get_home()` groups the response by the named
  shelves (`musicCarouselShelfRenderer`) YouTube Music itself uses, instead
  of flattening everything into one list.
- **Background sync**: `App::tick()` re-runs the same Home/Library loads
  every `sync_interval_secs`, preserving the current selection by
  `browse_id` instead of resetting the list.
- **Concurrency**: the UI runs on the main loop (synchronous, via
  `crossterm`), while searches, lyrics, cover downloads, and audio
  resolution run in **Tokio** tasks, communicating with the UI over an
  `mpsc` channel.
- **Audio**: rodio's `OutputStream` (which isn't `Send`) runs on a
  dedicated thread; `yt-dlp` downloads the best audio track, which is
  **remuxed** to ADTS before being decoded and played.

> 📐 A full description of the architecture (modules, flows, threading, and
> extension points) is in **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)**.

---

## 🧪 Development

```bash
# Unit tests (parsing, durations, themes, etc.)
cargo test

# Formatting and lints (CI requires both clean)
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings

# Internal API documentation (rustdoc), opens in the browser
cargo doc --no-deps --open
```

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs `fmt`,
`clippy`, and `test` on every push/PR. When a `v*` tag is created, the
release workflow ([`.github/workflows/release.yml`](.github/workflows/release.yml))
builds and publishes binaries for Linux and macOS.

Change history lives in **[`CHANGELOG.md`](CHANGELOG.md)**.

---

## ⚠️ Legal notice

This project is for **educational** purposes. Use of YouTube Music must
comply with YouTube's [Terms of Service](https://www.youtube.com/t/terms).
The authors are not responsible for misuse.

## 📄 License

MIT — see **[`LICENSE`](LICENSE)**.
