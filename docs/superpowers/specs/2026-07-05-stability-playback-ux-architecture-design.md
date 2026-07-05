# ytmtui v0.2.0 Stability, Playback, UX, and Architecture Design

**Date:** 2026-07-05

**Status:** Approved design, pending written-spec review

**Target:** Linux-first stable release (`v0.2.0`)

## 1. Context

ytmtui is a Rust and Ratatui terminal client for YouTube Music. Its current feature set includes search, authenticated library access, playlists, playback through `yt-dlp`/`ffmpeg`/`rodio`, a queue, lyrics, themes, and radio/autoplay.

The current usability problems are:

- authentication is unreliable and expired sessions are not recovered clearly;
- playback does not reliably continue with related tracks after starting a song from search;
- the interface is visually dense and some motion does not communicate useful state;
- search has no live YouTube Music suggestions;
- the central application module has accumulated state, asynchronous coordination, and behavior that are difficult to test independently;
- the project contains Portuguese user-facing and technical text, while the intended public release should be entirely in English.

The working tree already contains authentication and expired-session changes. Those changes are explicitly part of the baseline and must be completed rather than discarded.

## 2. Goals

The release must:

1. make cookie authentication and session-expiry handling reliable and actionable;
2. make a track selected from search start a continuous related-track radio queue;
3. preserve existing playlist ordering and playback behavior when playback starts from a playlist;
4. provide real YouTube Music suggestions while the user types;
5. simplify the visual hierarchy and interaction model;
6. restrict animation to useful feedback and avoid unnecessary redraw work;
7. establish tests around the high-risk behavior before modularizing the application;
8. translate all maintained project content to English;
9. pass automated and manual Linux release gates before publication to GitHub.

## 3. Non-goals

This release will not:

- rewrite the application or replace Ratatui, Tokio, rodio, `yt-dlp`, or `ffmpeg`;
- change playlist playback into radio playback;
- provide equal validation coverage for macOS and Windows;
- add localization infrastructure or retain Portuguese as an optional language;
- redesign YouTube Music's private API client beyond what the required features and reliability fixes need;
- introduce account password login or store browser credentials.

## 4. Delivery strategy

Work will proceed as small, validated vertical slices. Each stage must leave the application usable and must add tests before behavior is reorganized.

### Stage 1: Authentication foundation

- Preserve and complete the current cookie-domain precedence work.
- Replace string matching for `401` and `403` responses with typed HTTP and authentication errors.
- Represent authentication explicitly as `Anonymous`, `Authenticated`, `Expired`, or `InvalidCookies`.
- Ensure an expired session clears authenticated-only state without damaging unrelated application state.
- Show a concise English recovery message and document the cookie-refresh workflow.
- Ensure logs never print cookie values or authorization hashes.
- Add unit tests for Netscape parsing, cookie precedence, SAPISID fallback, invalid files, and expiry transitions.

### Stage 2: Playback and contextual radio

- Record how the active queue was created using `QueueOrigin::{Search, Playlist, Radio, Manual}`.
- When a user starts a track from search, start that track immediately and request related radio tracks in the background.
- Append deduplicated related tracks without moving or restarting the current track.
- Continue requesting related tracks when the search-origin radio queue approaches exhaustion, subject to the autoplay setting.
- When playback starts from a playlist, preserve the playlist's existing order, repeat, shuffle, and end-of-queue behavior.
- Ignore stale radio responses after the user changes queue context.
- Keep audio errors recoverable and ensure the terminal is restored after failures.
- Add tests for queue origin, playlist preservation, radio appending, deduplication, stale results, and autoplay disabled behavior.

### Stage 3: Search suggestions and interface simplification

- Call the YouTube Music suggestion endpoint while the search field is active.
- Debounce input by 300 ms to avoid a request for every keystroke.
- Attach a monotonically increasing generation identifier to suggestion requests so late responses cannot replace newer results.
- Support keyboard selection of suggestions and allow normal free-text search at all times.
- Treat suggestion failure as non-blocking; ordinary search must remain available.
- Implement the approved balanced layout:
  - persistent, concise navigation;
  - compact Now Playing summary;
  - large primary content area;
  - short contextual shortcut/status bar;
  - a single-column fallback for narrow terminals.
- Reduce decorative icons and repeated labels where they compete with content.
- Retain only meaningful motion: loading spinner, playback progress, and short state transitions.
- Use adaptive redraw timing instead of a permanently elevated refresh rate.
- Add tests for debounce behavior, stale suggestion suppression, keyboard selection, narrow layout safety, and render invariants.

### Stage 4: Architecture and English migration

- Separate UI-visible data, user actions, background results, and external services.
- Move responsibilities out of `app.rs` only when the affected behavior is covered by tests.
- Keep rendering side-effect-free; rendering reads state and does not start I/O or mutate domain behavior.
- Keep authentication, YouTube Music networking, and playback as independent service boundaries.
- Translate all maintained content to English:
  - TUI labels, help, status, and error messages;
  - source comments and test names;
  - scripts and command output;
  - README, changelog, architecture and release documentation;
  - examples, package metadata, and CI/release text.
- Existing Git history and third-party text are not rewritten.

## 5. Architecture

The target boundaries will be introduced incrementally without a directory-wide rewrite.

### 5.1 Application state

`AppState` owns data required to present and control the application: active section, focus, search input, suggestions, queue, queue origin, current track, playback status, authentication status, selection, transient notices, and modal state. State must not perform network or audio I/O.

### 5.2 Actions

`AppAction` represents user intent, including editing search input, selecting a suggestion, submitting a search, starting a track, enqueueing a track, changing playback controls, navigating, and dismissing a notice. Input handling translates terminal events into actions.

### 5.3 Messages

`AppMessage` represents typed outcomes from asynchronous work. Messages include authentication results, suggestions, search results, radio tracks, audio readiness, playback failures, and artwork or lyrics results. Each request whose response can become stale carries an operation or generation identifier.

### 5.4 Services

- The authentication service parses cookie sources, creates authorization headers, and reports typed authentication failures.
- The YouTube Music service performs endpoint requests and converts responses into domain models.
- The playback service resolves, prepares, and controls audio without depending on terminal layout or selection state.

The application coordinator starts service operations in response to actions and applies resulting messages to state.

## 6. Data flows

### 6.1 Authentication

1. Resolve the cookie path from the supported configuration sources.
2. Parse and validate the Netscape cookie file without exposing secret values.
3. Initialize the client as authenticated or anonymous.
4. Map authenticated endpoint failures to typed errors.
5. On expiry, transition to `Expired`, clear account-only data, retain public functionality, and present the refresh action.

### 6.2 Live suggestions

1. The user edits the active search input.
2. The application increments the suggestion generation and schedules a debounced request.
3. A newer edit supersedes the pending request.
4. A response is applied only when its generation matches the current input generation.
5. The user may select a suggestion or submit their original text.

### 6.3 Search-origin playback

1. The user starts one track from search results.
2. The queue is initialized with that track and marked as search-origin radio.
3. Playback starts immediately.
4. A related-track request is issued with a queue-context identifier.
5. A matching response is deduplicated and appended.
6. When two playable tracks remain, request another related batch if autoplay is enabled and no radio request is already active.

### 6.4 Playlist-origin playback

1. The user starts a track from a playlist.
2. The existing playlist queue and selected index are preserved.
3. Normal playlist repeat, shuffle, and end behavior remain in control.
4. No search-origin radio request changes that queue.

## 7. Interface behavior

The balanced layout selected during visual review is the default wide-terminal presentation. It prioritizes content while keeping navigation and playback context visible. The interface must degrade safely for narrow or short terminals and must never panic from layout constraints.

Status communication follows these rules:

- persistent facts appear in stable UI regions;
- transient success notices expire without requiring dismissal;
- recoverable failures include one concrete next action;
- blocking failures use a focused dialog rather than competing status lines;
- technical diagnostics remain available in logs and do not clutter the main view.

Animations must communicate ongoing work or playback. Decorative continuous animation is excluded.

## 8. Error handling and security

- Network and authentication failures use typed categories, not formatted-string inspection.
- Cancellation and stale responses are normal control flow, not user-visible errors.
- Suggestion and artwork failures do not block search or playback.
- Playback preparation failures leave the queue controllable and expose a retry or skip path.
- Cookie contents, SAPISID values, authorization hashes, and full sensitive headers are never logged.
- Cookie refresh scripts create restrictive files and preserve a recoverable backup when replacing an existing file.

## 9. Testing and validation

Automated release gates:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```

The implementation plan may use narrower commands during a stage, but all gates above must pass before release.

Manual Linux validation covers:

- valid login, invalid cookie file, and expired-session recovery;
- public anonymous behavior after authentication failure;
- live suggestions, rapid typing, selection, and normal free-text search;
- immediate playback of a searched track followed by related tracks;
- unchanged playlist order and controls;
- pause, resume, seek, volume, next, previous, repeat, and shuffle;
- audio preparation failure and dependency diagnostics;
- wide and narrow terminal layouts;
- clean terminal restoration after normal exit and recoverable failure.

## 10. Documentation and release

The release target is `v0.2.0`. Documentation will explain installation, Linux dependencies, authentication, cookie refresh, controls, search radio behavior, autoplay configuration, troubleshooting, and architecture.

Publication occurs only after all automated and manual gates pass. Before any push or release operation, verify:

- the configured Git remote is the intended repository;
- the authenticated GitHub account is the repository owner or otherwise authorized;
- the working tree contains only intentional release changes;
- the stable commit and tag are explicit and reproducible;
- the GitHub release workflow completes and publishes the intended Linux artifact.

## 11. Risks and mitigations

- **Private API drift:** isolate response parsing and retain focused parser fixtures.
- **Stale asynchronous results:** use generation/context identifiers for suggestions, searches, queues, and radio requests.
- **Audio tool variability:** validate dependencies and keep failures recoverable.
- **Refactor regressions:** characterize behavior before moving it and refactor in bounded slices.
- **Translation omissions:** scan source, scripts, docs, examples, and workflow files for remaining Portuguese text before release.
- **Visual regressions on small terminals:** test minimum dimensions and render fallbacks explicitly.

## 12. Acceptance criteria

The design is complete when the implementation demonstrates all of the following:

- authentication has typed state and typed expiry handling;
- a searched track produces a continuous related queue when autoplay is enabled;
- playlist playback behavior remains unchanged;
- real YouTube Music suggestions appear while typing and cannot be overwritten by stale responses;
- the balanced layout is readable in wide terminals and safe in narrow terminals;
- animation is limited to meaningful feedback;
- covered responsibilities have been removed from the central application module without changing behavior;
- all maintained project content is English;
- automated and manual Linux release gates pass;
- `v0.2.0` is published only after remote and account verification.
