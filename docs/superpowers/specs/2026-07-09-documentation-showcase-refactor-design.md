# Documentation Showcase Refactor Design

## Context

The current documentation is useful but overloaded. The main README tries to be a product showcase, installation guide, feature reference, troubleshooting entrypoint, and architecture summary at the same time. That makes it complete, but less exciting for someone who opens the repository for the first time.

The refactor should make the project feel polished and high-energy while staying accurate and maintainable. English remains the primary language, with complete PT-BR counterparts kept in parallel.

## Current Documentation

Existing public documentation:

- `README.md` — English README with hero, screenshots, install, features, requirements, usage, auth, customization, architecture, development, legal, and license.
- `README.pt-BR.md` — Portuguese counterpart.
- `docs/ARCHITECTURE.md` — deep architecture guide, currently mostly PT-BR and partially stale.
- `docs/TROUBLESHOOTING.md` — English troubleshooting guide.
- `docs/TROUBLESHOOTING.pt-BR.md` — Portuguese troubleshooting guide.
- `CHANGELOG.md` — Portuguese changelog.
- `docs/screenshots/*.png` — existing screenshots for Home, Search, Lyrics, and Help.

Important project facts verified against code and docs:

- Search includes songs, artists, albums, and playlists.
- In-app sign-in exists on `g` and imports cookies via `yt-dlp --cookies-from-browser`.
- Cookie-based auth still supports `YTM_COOKIES`, configured cookie path, and `~/.config/ytmtui/cookies.txt`.
- Recently played history is stored locally in `recent.json`.
- The UI includes synced lyrics, real-time FFT visualizer, album art, themes, queue, radio/autoplay, cache/prefetch, and a keyboard-first workflow.

## Goals

- Turn the README into an attractive showcase and documentation hub.
- Keep deep details available without forcing first-time readers through a long manual.
- Make the docs more visual through screenshots, compact tables, callout-style sections, and clear reading paths.
- Keep English-first documentation and complete PT-BR equivalents.
- Correct stale or inconsistent feature claims across README, architecture, troubleshooting, and new guides.
- Make it easy for users to install, play their first track, sign in, discover features, learn shortcuts, and troubleshoot.
- Make it easy for contributors to find architecture and development information.

## Non-Goals

- No Rust source changes.
- No workflow or release automation changes.
- No generated static documentation site.
- No fake screenshots, fake metrics, or unverified claims.
- No private/local-only assets.
- No dependency on JavaScript-rendered docs.

## Documentation Structure

Create or maintain this public documentation structure:

- `README.md` — English showcase and docs hub.
- `README.pt-BR.md` — full PT-BR version of the README.
- `docs/GETTING_STARTED.md` — English install and first-run guide.
- `docs/GETTING_STARTED.pt-BR.md` — PT-BR install and first-run guide.
- `docs/FEATURES.md` — English feature showcase.
- `docs/FEATURES.pt-BR.md` — PT-BR feature showcase.
- `docs/AUTHENTICATION.md` — English sign-in, cookies, session, and anti-bot guide.
- `docs/AUTHENTICATION.pt-BR.md` — PT-BR auth guide.
- `docs/KEYMAP.md` — English keyboard shortcuts reference.
- `docs/KEYMAP.pt-BR.md` — PT-BR keyboard shortcuts reference.
- `docs/TROUBLESHOOTING.md` — refreshed English troubleshooting guide.
- `docs/TROUBLESHOOTING.pt-BR.md` — refreshed PT-BR troubleshooting guide.
- `docs/ARCHITECTURE.md` — refreshed architecture guide with stale claims corrected.
- `CHANGELOG.md` — keep as history; do not rewrite as part of this refactor unless link text needs adjustment.

## README Design

The README should be high-impact but not noisy:

- Centered hero with title, concise tagline, badges, and a subtle typing animation.
- Large screenshot or screenshot grid placed early.
- "Why ytmtui?" section with compact table/cards that explain the value quickly.
- "What it feels like" section using existing screenshots:
  - Home / visualizer / recommendations.
  - Search / grouped results.
  - Synced lyrics.
  - Help / keyboard flow.
- Quick install section that remains immediately actionable.
- "Choose your path" documentation hub linking to getting started, features, auth, keymap, troubleshooting, architecture, changelog, and PT-BR.
- Feature summary organized by user value instead of a single long bullet list.
- Contributor/development section kept concise, linking to architecture.
- Legal and license kept at the end.

## Guide Design

### Getting Started

Purpose: get a new user from zero to first playback.

Include:

- Requirements by platform.
- Quick install from latest release.
- Build from source.
- First run.
- First search/play flow.
- Optional sign-in pointer to auth guide.
- Troubleshooting pointer.

### Features

Purpose: let users feel the product depth without bloating the README.

Include:

- Search across songs, artists, albums, playlists.
- Playback pipeline.
- Home recommendations and recently played.
- Synced lyrics.
- Visualizer and album art.
- Queue, radio/autoplay, shuffle/repeat/seek.
- Themes and terminal-first UI.
- Library/account features.
- Cache/prefetch.

### Authentication

Purpose: explain sign-in clearly and honestly.

Include:

- Anonymous mode.
- In-app sign-in with `g`.
- Cookie file paths and precedence.
- Browser/session requirements.
- Session expired behavior.
- Anti-bot workaround with `YTM_COOKIES`.
- Privacy note: no password storage.

### Keymap

Purpose: quick reference for keyboard-driven use.

Include:

- Navigation.
- Search.
- Playback.
- Queue.
- Account/auth.
- Appearance.
- General/help/quit.

### Troubleshooting

Purpose: solve common issues quickly.

Keep existing issue coverage, but make sections easier to scan:

- Missing dependencies.
- Expired session.
- Anti-bot playback block.
- No sound.
- Album art support.
- Lyrics fallback.
- Background sync interval.

### Architecture

Purpose: contributor-level technical reference.

Correct known stale content:

- Search now runs four sub-searches: songs, artists, albums, playlists.
- In-app sign-in via `g` and `app/authentication.rs` should be documented.
- Recently played local history should be documented.
- Existing mixed English/PT-BR phrases should be cleaned up.

## Visual Style

- Use GitHub-compatible Markdown and HTML only.
- Use existing screenshots from `docs/screenshots/`.
- Use tables for cards where they improve scanning.
- Use callout-style blockquotes sparingly for important notes.
- Keep animation subtle and limited to GitHub-compatible image URLs such as `readme-typing-svg`.
- Avoid excessive badge walls, fake stats, or clutter.

## Language Policy

- `README.md` and English docs are primary.
- PT-BR docs should be complete counterparts, not partial summaries.
- Cross-link English and PT-BR files near the top of each guide.
- Technical terms can remain in English when they are standard (`shuffle`, `repeat`, `seek`, `Home`, `InnerTube`) but prose should be natural in each language.

## Validation

- Run `git diff --check`.
- Search for stale claims:
  - English: `three sub-searches`, `songs, artists, and playlists` where albums should be included, `restart` where in-app sign-in should be mentioned.
  - PT-BR: `três sub-buscas`, `músicas, artistas e playlists` where albums should be included, `reinicie` where in-app sign-in should be mentioned.
- Verify all new links point to existing files.
- Since this is documentation-only, do not run the Rust test suite unless code examples or source files change.
