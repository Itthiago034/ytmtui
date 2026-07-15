# Provider-Ready Artwork Home Design

Date: 2026-07-10

## Summary

ytmtui will remain a Rust-first terminal application. Its next visual direction
is an adaptive, artwork-led Home screen backed by a provider-neutral application
boundary. The rich path uses terminal image protocols for cover grids; the
fallback path preserves the same content, navigation order, and actions as a
polished text layout. Customization stays native in phase one. Lua, a graphical
desktop client, and a second production provider remain future options rather
than new runtime dependencies.

The provider-boundary refactor in commit `f969036` already establishes the first
architectural step: shared models, `MusicProvider`, capability checks, a mock
provider, and a YouTube Music adapter. The next implementation plan must build
on that work rather than recreate it.

## Current-State Evaluation

The current Home screen has several strong foundations:

- the sidebar, themed borders, status line, and playback strip form a coherent
  terminal-native shell;
- the real FFT spectrum gives playback a distinctive identity;
- recent history and remote shelves provide useful content before search;
- wide, narrow, and short layouts already degrade without panicking;
- keyboard shortcuts, scrolling, selection preservation, and asynchronous
  loading are well covered by tests.

The main visual weakness is hierarchy. The spectrum dominates the upper panel,
while recommendations remain a flat stream of nearly identical rows. Large
terminals leave a broad unused field, cover art is isolated at the bottom of the
sidebar, and discovery content is harder to scan than it needs to be. Section
headers improve grouping but do not make albums, playlists, and recent tracks
feel meaningfully different.

The main architectural weakness was direct service coupling. That is now
substantially addressed by the new provider boundary, although the shared models
still contain legacy names such as `video_id` and `browse_id`. Those names are
acceptable while YouTube Music is the only production provider but must become
provider-qualified identifiers before a second provider ships.

## Goals

- Make Home visually distinctive, artwork-led, and easy to scan.
- Preserve full usability in terminals without image-protocol support.
- Make spatial keyboard navigation predictable across widths and resize events.
- Add meaningful native customization for artwork, density, motion, and the
  visualizer.
- Keep UI, queue, history, and playback coordination independent of YouTube
  Music.
- Preserve responsive rendering and never block input on network or image work.
- Prove provider independence through public-boundary tests and a mock provider.

## Non-Goals

- Implementing Spotify, Jellyfin, Subsonic, local files, or another production
  provider in this phase.
- Loading multiple providers simultaneously. Phase one continues to compose the
  app with one `Arc<dyn MusicProvider>`; a registry can replace that composition
  root later without changing UI contracts.
- Adding Lua, WebAssembly, TypeScript, Tauri, or another runtime language.
- Replacing the existing playback engine, queue semantics, MPRIS support, search
  experience, or lyrics experience.
- Requiring Nerd Fonts, emoji-width assumptions, or a specific terminal.

## Architecture

### Provider boundary

`MusicProvider` remains the only service interface consumed by `App`. A provider
converts remote responses to shared models before sending them to the
application. Blocking authentication and media resolution run through
`spawn_blocking`; asynchronous discovery, library, metadata, lyrics, rating, and
artwork work run as Tokio tasks.

`Capabilities` controls optional actions. Home, library, lyrics, radio, likes,
and sign-in must not be invoked when unsupported. Unsupported actions are hidden
where possible and produce a clear status message when reached through a global
shortcut.

The current single-provider composition is intentional for phase one. Before a
second production provider is added, shared identifiers will become explicit:

```text
ProviderId("ytmusic") + opaque item id -> MediaId
```

Persistence will retain serde aliases for existing `recent.json` data so the
migration does not discard user history.

### Home view model

Provider models do not encode terminal layout. A UI-facing Home projection will
normalize recent tracks and provider shelves into:

```text
HomeShelf
  title
  source label
  state: ready | refreshing | partial error
  cards: HomeCard[]

HomeCard
  stable selection key
  kind: track | album | playlist
  title, subtitle, duration
  artwork URL
  provider identity
  primary action
```

This projection is pure and independently testable. It lets the renderer switch
between image cards and text rows without changing selection or activation
semantics.

### Rendering and state boundaries

- Rendering reads state and draws widgets; it performs no network or disk work.
- Artwork decoding and fetching happen in background tasks.
- The application owns shelf/card selection, cache state, and refresh outcomes.
- The renderer computes the number of visible columns from the current area and
  maps the stable selection to a row and column for that frame.
- Resizing preserves the selected card by stable key, not by the old visual row.

## Home Experience

### Composition

The outer Home panel keeps the existing themed rounded border. Inside it:

1. A compact greeting row shows time-of-day context. A provider filter is hidden
   while only one provider is active.
2. `Continue listening` projects local recent history as the first shelf.
3. Provider discovery shelves follow in their returned order.
4. A compact persistent Now Playing strip remains at the bottom of the app. The
   large spectrum no longer consumes a fixed upper block on Home.
5. The selected card exposes title, subtitle, provider, and the available
   primary action without requiring a separate details screen.

The visualizer remains part of the product identity. Its default Home treatment
becomes a small ambient strip or compact mode rather than the dominant content
area. Users can select another visualizer style or disable it.

### Artwork modes

`auto` is the default:

- Kitty, Sixel, and iTerm2-capable terminals render real cover tiles.
- Terminals without a usable image protocol render compact text cards with a
  kind marker and the same metadata.
- `always-text` disables gallery images even when supported.

The UI never represents missing artwork with fake images, ASCII cover drawings,
or layout-breaking placeholders. A missing image falls back to the text-card
presentation for that item.

### Responsive layout

- Wide areas use a multi-column shelf whose card width remains readable.
- Medium areas reduce the column count before truncating essential metadata.
- Narrow areas collapse shelves to vertical rows with unchanged item order.
- Short areas remove secondary metadata and ambient visualization before
  removing actionable content.

No horizontal scrolling is required in phase one. Shelves wrap into rows inside
the vertical Home flow, which keeps keyboard and mouse-wheel behavior compatible
with existing terminal expectations.

### Navigation and actions

- Left and right move between cards in the current shelf.
- Up and down move to the nearest column in the adjacent shelf.
- `Home` and `End` select the first and last selectable card.
- `PageUp` and `PageDown` move by visible shelf/page boundaries.
- `Enter` plays a track or opens an album/playlist.
- `a` queues the selected playable item without interrupting playback.
- Resize and background refresh preserve selection by stable key when the item
  still exists, then clamp to the nearest valid card.

Mouse scrolling continues to move through Home content; pointer hit-testing is
not required in this phase.

## Customization and Motion

Configuration adds backward-compatible defaults for:

```text
artwork_mode: auto | always-text
home_density: comfortable | compact
visualizer_style: ambient | spectrum | off
motion: full | reduced | off
animation_speed: slow | normal | fast
```

Existing theme presets remain valid. Themes gain semantic colors for card
surfaces, selected cards, and provider labels rather than provider-specific hard
coding.

Motion communicates state rather than decorating idle frames. Full motion may
animate selection emphasis, metadata replacement, and visualizer transitions.
Reduced motion uses immediate selection with only loading and playback progress
updates. Off disables nonessential animation. Time-based transitions use elapsed
duration, so animation speed does not depend on terminal redraw rate.

## Data Flow

```text
MusicProvider::home
  -> provider converts response to shared HomeSection data
  -> Msg updates App while preserving stable selection
  -> Home projection merges local recent history and provider shelves
  -> renderer chooses gallery or text presentation for current terminal/width

card becomes visible
  -> artwork request checks bounded cache
  -> provider fetches bytes in background on miss
  -> decode result is tagged with stable card key
  -> stale results are discarded; valid results update only that card
```

Artwork prefetch is bounded to visible cards plus a small look-ahead. The cache
has a fixed memory budget and least-recently-used eviction. Home remains
interactive while images load.

## Errors and Empty States

- Existing shelves remain visible during background refresh.
- A failed provider refresh leaves cached content intact and reports a concise
  status message.
- A shelf-level failure renders a compact retryable row in that shelf when the
  provider can isolate the error.
- Artwork failure affects only the corresponding card and falls back to text.
- Playback errors name the originating provider and preserve the queue.
- Expired authentication changes the generic auth state and offers the
  provider's sign-in path without disabling anonymous features.
- An empty anonymous Home keeps the branded empty state and offers search; it
  mentions sign-in only when the provider supports sign-in.

## Testing Strategy

The existing suite remains the regression baseline.

Provider-boundary integration tests cover public construction with a mock
provider, search, Home, library, authentication, capability suppression, generic
errors, expired sessions, and playback failure with queue preservation. The
seven tests currently in `tests/provider_boundary.rs` are the initial proof.

Home tests will cover:

- pure projection from recent tracks and provider sections to shelves/cards;
- wide, medium, narrow, short, image-capable, and text-fallback rendering;
- loading, empty, stale-cache, partial-error, and missing-artwork states;
- two-dimensional movement, shelf boundaries, paging, resize, and refresh;
- activation and queue actions for tracks, albums, and playlists;
- full, reduced, and disabled motion using deterministic elapsed time;
- bounded artwork prefetch, LRU eviction, and stale-result rejection.

Tests use Ratatui buffers and deterministic mock data. They do not depend on a
real account, network access, audio device, image-capable terminal, or wall-clock
animation timing.

## Delivery Sequence

1. Finish and verify the public provider-boundary tests already in progress.
2. Add provider-neutral Home projection types and stable selection keys.
3. Introduce deterministic two-dimensional Home navigation with text rendering
   first.
4. Add bounded per-card artwork loading, caching, and image-protocol rendering.
5. Add responsive gallery composition and text fallback.
6. Add customization fields, semantic theme colors, and motion settings.
7. Update help, configuration documentation, architecture notes, screenshots,
   changelog, and English/PT-BR user documentation.
8. Run formatting, clippy, the full test suite, and a manual terminal matrix
   check before handoff.

## Programming-Language Decision

Rust remains the correct language for phase one. Ratatui, ratatui-image, Tokio,
and the existing FFT path already provide the required layout, animation, image,
and concurrency primitives. Adding another language would increase packaging and
failure modes without removing a terminal limitation.

Lua is the strongest later candidate for safe user automation, custom commands,
and declarative Home sections after the native provider and customization APIs
stabilize. It should not own playback, provider credentials, async networking,
or core rendering. A TypeScript/Tauri client is a separate future product option
if the project later needs graphical animation beyond terminal capabilities; the
provider-neutral Rust core keeps that path open.

## Acceptance Criteria

- Home visibly follows the approved Artwork Gallery direction.
- Image-capable and text-only terminals expose the same content and actions.
- Navigation remains predictable across resize and background refresh.
- No Home rendering path performs blocking network, disk, or decode work.
- Existing YouTube Music behavior and stored recent history remain compatible.
- Application and UI code use the provider contract rather than YouTube-specific
  clients or response types.
- Provider-boundary and Home-state tests pass alongside the existing suite.
- Configuration additions deserialize old files with safe defaults.
- The application remains responsive when artwork, Home refresh, or playback
  fails.
