# Contributing to ytmtui

Thanks for considering a contribution! This is a small Rust TUI project;
the notes below should be enough to get a change from idea to PR.

## Prerequisites

- Rust 1.88+ (`rustup install stable` is fine; the MSRV is enforced in CI).
- `yt-dlp`, `ffmpeg`, and ALSA dev libs (`libasound2-dev` on Debian/Ubuntu) to
  actually run and exercise playback locally.

See the [README](README.md#requirements) for the full dependency table and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the pieces fit together
before touching `app.rs`, `player/`, or `ytmusic/`.

## Making a change

1. Fork/branch, make your change.
2. Run the checks CI runs, before pushing:
   ```bash
   cargo test
   cargo fmt --all --check
   cargo clippy --all-targets -- -D warnings
   ```
3. If you touched `Cargo.toml`/`Cargo.lock`, also run `cargo deny check`
   (install with `cargo install cargo-deny` if you don't have it) — CI blocks
   on new disallowed licenses or unresolved security advisories.
4. Keep commits focused; explain the *why* in the commit message/PR
   description, not just the *what*.

## Opening a PR

- Describe the problem the change addresses and how you verified it (which
  commands you ran, what you exercised manually for UI/playback changes).
- Small, reviewable PRs are strongly preferred over large ones. If a change
  naturally splits into independent pieces, consider separate PRs.
- Documentation-only changes should still pass `cargo fmt --all --check` and
  `git diff --check`, and any README links should point to files that exist.
