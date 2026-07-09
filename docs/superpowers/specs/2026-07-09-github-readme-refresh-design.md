# GitHub README Refresh Design

## Context

The GitHub profile and the `ytmtui` project README should feel more polished and lightly animated without looking noisy or template-heavy. English is the primary language, with PT-BR still clearly available and represented.

The project context inspected before this design:

- `ytmtui` is a Rust terminal client for YouTube Music using Ratatui.
- It uses InnerTube for metadata/search/library/account operations.
- It uses `yt-dlp`, `ffmpeg`, and `rodio` for audio resolution and playback.
- It supports optional cookie-based authentication, synced lyrics, a real-time FFT visualizer, theme switching, queue/radio/autoplay, cache/prefetch, album art, and keyboard-first navigation.
- `README.md` is English and `README.pt-BR.md` is Portuguese.
- The public profile README currently highlights `ytmtui`, but includes wording that should be corrected to avoid overstating features, such as "albums" in unified search and "one-key login".

## Goals

- Make the GitHub profile more elegant, animated, and credible.
- Make the `ytmtui` README header more visually refined while preserving the existing thorough documentation.
- Keep English as the main language and preserve a clear PT-BR path.
- Correct profile wording so it matches the actual project behavior.
- Avoid adding unverified features, fake stats, excessive badges, or loud visual clutter.

## Non-Goals

- No code changes to `ytmtui`.
- No feature claims that are not backed by the current README, architecture docs, or source layout.
- No large rewrite of installation, authentication, troubleshooting, or architecture sections unless needed for consistency.
- No dependency on private assets or local-only images.

## Profile README Design

The profile README should use a restrained terminal-oriented identity:

- Centered animated typing header.
- Short English positioning statement: embedded systems, low-level programming, Linux, Rust, and terminal tools.
- One small PT-BR line to make bilingual support visible without mixing languages throughout the document.
- Featured `ytmtui` section with accurate bullets:
  - YouTube Music in the terminal.
  - Rust + Ratatui.
  - Search for songs, artists, and playlists.
  - Cookie-based optional sign-in for library/account features.
  - Synced lyrics, visualizer, themes, queue/radio/autoplay.
- One screenshot pulled from the `ytmtui` repository.
- Compact technology badges.
- Short closing note that older projects may be archived and active work is shown first.

## `ytmtui` README Design

The project README should keep its detailed structure but get a stronger first viewport:

- Centered title and concise product tagline.
- Badges grouped directly below the title.
- Language switch kept near the top: English primary, PT-BR link visible.
- A small animated typing SVG or subtle terminal-flavored visual element is acceptable, but it must not distract from install/use information.
- Existing terminal preview and screenshots remain useful and should not be replaced by vague decorative art.
- The current technical feature list remains accurate and can be lightly tightened, but the implementation facts should stay intact.

## PT-BR Treatment

- `README.md` remains English-first.
- `README.pt-BR.md` remains the Portuguese version.
- Profile README may include one PT-BR sentence and link users to the Portuguese project README.
- If a claim is edited in English, the matching PT-BR wording should be kept consistent where applicable.

## Implementation Notes

- The local checkout contains `Itthiago034/ytmtui`.
- The profile repository `Itthiago034/Itthiago034` is not currently checked out locally. Its current public README was inspected through GitHub raw content. Editing it will require cloning that profile repository or receiving its local path.
- Any animation should be GitHub-compatible Markdown/HTML, such as `readme-typing-svg`, not JavaScript.
- Use existing screenshots from `docs/screenshots/`.

## Validation

- Review READMEs for factual accuracy against `README.md`, `README.pt-BR.md`, `docs/ARCHITECTURE.md`, and `Cargo.toml`.
- Check rendered Markdown structure manually enough to ensure no broken table/layout syntax.
- Run no Rust test suite for README-only edits unless code or examples are changed.
