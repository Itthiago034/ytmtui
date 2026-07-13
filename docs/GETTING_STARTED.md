# Getting Started

**English** · [Português](GETTING_STARTED.pt-BR.md)

Get from a fresh machine to your first track in a few minutes.

## What You Need

| Dependency | Required for | Notes |
|---|---|---|
| `yt-dlp` | Resolving YouTube Music audio | Essential for playback |
| `ffmpeg` | Remuxing AAC/M4A to ADTS | Essential for reliable playback |
| `deno` | Recent `yt-dlp` JavaScript challenges | Recommended |
| Rust 1.75+ | Building from source | Not needed for prebuilt releases |
| ALSA dev libs | Linux audio builds | Usually `libasound2-dev` on Debian/Ubuntu |

On macOS and Windows, `rodio` uses the native audio stack. On Linux, make
sure an audio device is available before testing playback.

## Fast Install

The install script downloads the latest prebuilt binary, installs it to
`~/.local/bin`, and warns about missing runtime dependencies.

```bash
curl -fsSL https://raw.githubusercontent.com/Itthiago034/ytmtui/master/scripts/install.sh | bash
```

Then run:

```bash
ytmtui
```

## Build From Source

Use this when you want another platform, local changes, or a development build.

```bash
git clone https://github.com/Itthiago034/ytmtui.git
cd ytmtui
cargo build --release
./target/release/ytmtui
```

For development:

```bash
cargo run
```

To install your local build as `ytmtui`:

```bash
cargo install --path .
```

## First Track

1. Start `ytmtui`.
2. Press `/`.
3. Type a song, artist, album, or playlist.
4. Press `Enter`.
5. Move with `j`/`k` or arrow keys.
6. Press `Enter` on a song to play it.

Search works without sign-in. Account-only features such as private playlists,
likes, and personalized library data need cookies.

## Optional Sign-In

Press `g` inside the app to import cookies from a supported browser. Sign in to
[music.youtube.com](https://music.youtube.com) in that browser first.

Discovery tries Firefox first and moves to another supported browser only when
export or account validation fails. Review the browser account preview, choose
an account, and press `Enter` to activate it. Press `Esc` to cancel without
changing the current session. The confirmed browser/profile and account index
are saved for the next launch.

If you prefer a script:

```bash
./scripts/refresh-cookies.sh brave
```

See [Authentication](AUTHENTICATION.md) for the full flow, cookie paths, and
anti-bot playback fixes.

## Useful First Keys

| Key | Action |
|---|---|
| `/` | Search |
| `Enter` | Play/open selected item |
| `Space` | Play/pause |
| `n` / `p` | Next / previous |
| `a` | Add selected track to queue |
| `g` | Sign in or refresh browser cookies |
| `t` | Cycle theme |
| `?` | Help |
| `q` | Quit |

See the full [Keymap](KEYMAP.md).

## If Something Breaks

Start with [Troubleshooting](TROUBLESHOOTING.md). The most common issues are
missing `yt-dlp`/`ffmpeg`, expired cookies, blocked datacenter IPs, and audio
devices not being available.
