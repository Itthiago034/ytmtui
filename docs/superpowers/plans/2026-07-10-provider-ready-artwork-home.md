# Provider-Ready Artwork Home Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver an adaptive artwork-gallery Home screen, native customization, and a verified provider boundary without regressing the current YouTube Music experience.

**Architecture:** `App` continues to own one `Arc<dyn MusicProvider>` and converts provider-neutral data into a pure `HomeView` projection. A focused Home module owns stable keys, shelf/card navigation, responsive geometry, and animation math; a bounded artwork cache owns decoded images and terminal protocols. Rendering remains network-free and selects an image or text presentation from the same view model.

**Tech Stack:** Rust 1.75+, Ratatui 0.29, ratatui-image 3.0, Crossterm 0.28, Tokio 1, serde/serde_json, image 0.25, existing `MusicProvider` and `MockProvider`.

## Global Constraints

- Keep one active production provider in phase one; do not add a registry or a second service.
- Do not add Lua, WebAssembly, TypeScript, Tauri, or another runtime language.
- Do not require Nerd Fonts, emoji-width assumptions, or a specific terminal.
- Rendering must not perform network, disk, image decoding, or other blocking work.
- Preserve existing `recent.json` and `config.json` compatibility through serde defaults.
- Keep current queue, MPRIS, search, lyrics, authentication, and playback behavior.
- Image and text modes must expose the same item order, selection, and actions.
- Keep existing uncommitted work isolated: stage only the files named by each task.

---

### Task 1: Finish the Public Provider-Boundary Proof

**Files:**
- Modify: `src/app.rs`
- Modify: `src/provider/mod.rs`
- Modify: `src/provider/mock.rs`
- Test: `tests/provider_boundary.rs`

**Interfaces:**
- Consumes: `MusicProvider`, `Capabilities`, `ProviderError`, and the provider refactor from commit `f969036`.
- Produces: `App::with_provider(Arc<dyn MusicProvider>)`, `Capabilities::all()`, `Capabilities::none()`, configurable mock failures/capabilities, and seven public integration tests.

- [ ] **Step 1: Review the already-written boundary delta**

Run:

```bash
git diff -- src/app.rs src/provider/mod.rs src/provider/mock.rs tests/provider_boundary.rs
```

Expected: the delta only exposes `App::with_provider`, makes the mock configurable, names the provider in playback errors, and adds the seven boundary tests. It must not contain Home-gallery implementation.

- [ ] **Step 2: Run the boundary tests**

Run:

```bash
cargo test --test provider_boundary -- --nocapture
```

Expected: `7 passed; 0 failed`.

- [ ] **Step 3: Run the full provider-refactor regression suite**

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: formatting and clippy exit 0; all unit, integration, and documentation tests pass. ALSA warnings are acceptable in the headless test environment; Rust test failures are not.

- [ ] **Step 4: Commit only the boundary proof**

```bash
git add src/app.rs src/provider/mod.rs src/provider/mock.rs tests/provider_boundary.rs
git commit -m "test: prove the generic provider boundary"
```

Expected: the commit contains the four named paths and `git status --short` no longer lists them.

---

### Task 2: Introduce the Pure Home Projection and Stable Keys

**Files:**
- Create: `src/home.rs`
- Modify: `src/lib.rs`
- Modify: `src/models.rs`
- Modify: `src/app.rs`
- Modify: `src/ytmusic/parse.rs`
- Test: `src/ytmusic/parse.rs`
- Test: `src/home.rs`

**Interfaces:**
- Consumes: `models::{HomeSection, Playlist, Track}` and `MusicProvider::id()`.
- Produces: provider-assigned `CollectionKind`, `HomeKey`, `HomeCardKind`, `HomeCardPayload`, `HomeCard`, `HomeShelf`, `HomeView::project`, `HomeView::flat_card`, and `HomeView::flat_index_of`.

- [ ] **Step 1: Write projection tests**

Add tests at the bottom of `src/home.rs` that build two recent tracks and two provider shelves and assert:

```rust
#[test]
fn projection_puts_recent_history_first_and_keeps_provider_order() {
    let view = HomeView::project("ytmusic", &recent_tracks(), &provider_sections());
    assert_eq!(view.shelves[0].title, "Continue listening");
    assert_eq!(view.shelves[1].title, "Quick picks");
    assert_eq!(view.shelves[2].title, "Made for you");
    assert_eq!(view.len(), 5);
    assert!(matches!(view.flat_card(0).unwrap().payload, HomeCardPayload::Track(_)));
    assert!(matches!(view.flat_card(2).unwrap().payload, HomeCardPayload::Collection(_)));
}

#[test]
fn stable_keys_distinguish_recent_tracks_from_provider_collections() {
    let view = HomeView::project("ytmusic", &recent_tracks(), &provider_sections());
    assert_eq!(view.flat_card(0).unwrap().key, HomeKey::new("local", "track", "t1"));
    assert_eq!(view.flat_card(2).unwrap().key, HomeKey::new("ytmusic", "collection", "p1"));
    assert_eq!(view.flat_index_of(&HomeKey::new("ytmusic", "collection", "p2")), Some(3));
}
```

- [ ] **Step 2: Run the projection tests to verify they fail**

Run:

```bash
cargo test home::tests::projection_puts_recent_history_first_and_keeps_provider_order
```

Expected: FAIL because `crate::home` and its types do not exist.

- [ ] **Step 3: Implement the provider-neutral Home types**

First add an explicit shared classification to `src/models.rs` so the generic
Home layer never interprets YouTube identifier prefixes:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CollectionKind {
    Album,
    #[default]
    Playlist,
}

#[derive(Debug, Clone, Default)]
pub struct Playlist {
    pub browse_id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: Option<String>,
    pub kind: CollectionKind,
}
```

Update YouTube parsing so album-filter results and `MPRE` Home entries receive
`CollectionKind::Album`; playlist results receive `CollectionKind::Playlist`.
Update existing `Playlist` struct literals to use `..Default::default()` where
they do not care about kind, and add parser assertions for both kinds.

Create `src/home.rs` with these public shapes and a projection that clones source models into cards:

```rust
use crate::models::{HomeSection, Playlist, Track};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HomeKey(String);

impl HomeKey {
    pub fn new(provider: &str, kind: &str, id: &str) -> Self {
        Self(format!("{provider}:{kind}:{id}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeCardKind { Track, Album, Playlist }

#[derive(Debug, Clone)]
pub enum HomeCardPayload {
    Track(Track),
    Collection(Playlist),
}

#[derive(Debug, Clone)]
pub struct HomeCard {
    pub key: HomeKey,
    pub kind: HomeCardKind,
    pub title: String,
    pub subtitle: String,
    pub duration: String,
    pub artwork_url: Option<String>,
    pub provider: String,
    pub payload: HomeCardPayload,
}

#[derive(Debug, Clone)]
pub struct HomeShelf { pub title: String, pub cards: Vec<HomeCard> }

#[derive(Debug, Clone, Default)]
pub struct HomeView { pub shelves: Vec<HomeShelf> }

impl HomeView {
    pub fn project(provider: &str, recent: &[Track], sections: &[HomeSection]) -> Self {
        let mut shelves = Vec::new();
        if !recent.is_empty() {
            shelves.push(HomeShelf {
                title: "Continue listening".into(),
                cards: recent.iter().cloned().map(|track| HomeCard {
                    key: HomeKey::new("local", "track", &track.video_id),
                    kind: HomeCardKind::Track,
                    title: track.title.clone(),
                    subtitle: track.artist.clone(),
                    duration: track.duration.clone(),
                    artwork_url: track.thumbnail.clone(),
                    provider: provider.into(),
                    payload: HomeCardPayload::Track(track),
                }).collect(),
            });
        }
        shelves.extend(sections.iter().map(|section| HomeShelf {
            title: section.title.clone(),
            cards: section.items.iter().cloned().map(|item| HomeCard {
                key: HomeKey::new(provider, "collection", &item.browse_id),
                kind: match item.kind {
                    crate::models::CollectionKind::Album => HomeCardKind::Album,
                    crate::models::CollectionKind::Playlist => HomeCardKind::Playlist,
                },
                title: item.title.clone(),
                subtitle: item.subtitle.clone(),
                duration: String::new(),
                artwork_url: item.thumbnail.clone(),
                provider: provider.into(),
                payload: HomeCardPayload::Collection(item),
            }).collect(),
        }));
        Self { shelves }
    }

    pub fn len(&self) -> usize { self.shelves.iter().map(|s| s.cards.len()).sum() }
    pub fn flat_card(&self, index: usize) -> Option<&HomeCard> {
        self.shelves.iter().flat_map(|s| &s.cards).nth(index)
    }
    pub fn flat_index_of(&self, key: &HomeKey) -> Option<usize> {
        self.shelves.iter().flat_map(|s| &s.cards).position(|card| &card.key == key)
    }
}
```

Export it from `src/lib.rs` with `pub mod home;`. Add `App::home_view()` returning `HomeView::project(self.provider.id(), &self.recent, &self.home)` and change `home_total_count()` to `self.home_view().len()`.

- [ ] **Step 4: Run focused and full tests**

Run:

```bash
cargo test home::tests
cargo test ytmusic::parse::tests
cargo test app::tests::home_
```

Expected: all new projection tests and existing Home indexing tests pass.

- [ ] **Step 5: Commit the projection**

```bash
git add src/home.rs src/lib.rs src/models.rs src/app.rs src/ytmusic/parse.rs
git commit -m "refactor: add provider-neutral Home projection"
```

---

### Task 3: Add Deterministic Two-Dimensional Home Navigation

**Files:**
- Modify: `src/home.rs`
- Modify: `src/app.rs`
- Modify: `src/event.rs`
- Test: `src/home.rs`
- Test: `src/event.rs`

**Interfaces:**
- Consumes: `HomeView` and the existing flat `ListState` selection.
- Produces: `HomeDirection`, `HomeView::move_index(current, direction, columns)`, and Home-specific event routing.

- [ ] **Step 1: Write movement tests**

Add deterministic cases for a view with shelf lengths `[3, 2, 4]`:

```rust
assert_eq!(view.move_index(1, HomeDirection::Right, 3), 2);
assert_eq!(view.move_index(2, HomeDirection::Right, 3), 0);
assert_eq!(view.move_index(2, HomeDirection::Down, 3), 4);
assert_eq!(view.move_index(4, HomeDirection::Down, 3), 7);
assert_eq!(view.move_index(7, HomeDirection::Up, 3), 4);
```

Add an event test proving `Right` on Home moves cards while `Right` on Search only moves focus to Main.

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test home::tests::movement event::tests::home_
```

Expected: FAIL because `HomeDirection` and Home-specific routing do not exist.

- [ ] **Step 3: Implement movement by shelf and column**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeDirection { Left, Right, Up, Down }

impl HomeView {
    pub fn move_index(&self, current: usize, direction: HomeDirection, columns: usize) -> usize {
        let columns = columns.max(1);
        let mut base = 0usize;
        let Some((shelf_index, local)) = self.shelves.iter().enumerate().find_map(|(i, shelf)| {
            let end = base + shelf.cards.len();
            let found = (current < end).then_some((i, current - base));
            base = end;
            found
        }) else { return 0 };
        let shelf = &self.shelves[shelf_index];
        match direction {
            HomeDirection::Left => base - shelf.cards.len() + (local + shelf.cards.len() - 1) % shelf.cards.len(),
            HomeDirection::Right => base - shelf.cards.len() + (local + 1) % shelf.cards.len(),
            HomeDirection::Up | HomeDirection::Down => {
                let target = match direction {
                    HomeDirection::Up => shelf_index.checked_sub(1),
                    HomeDirection::Down => (shelf_index + 1 < self.shelves.len()).then_some(shelf_index + 1),
                    _ => unreachable!(),
                };
                let Some(target) = target else { return current };
                let column = local % columns;
                let target_base: usize = self.shelves[..target].iter().map(|s| s.cards.len()).sum();
                target_base + column.min(self.shelves[target].cards.len().saturating_sub(1))
            }
        }
    }
}
```

Store `home_columns: usize` in `App`, defaulting to `1`. The Home renderer updates this geometry value each frame. Add `App::move_home(HomeDirection)` to select `home_view().move_index(...)`. In `event.rs`, route arrow/hjkl keys to `move_home` only when Home has Main focus; retain existing sidebar-focus behavior.

- [ ] **Step 4: Switch Home activation to card payloads**

Replace the recent-length arithmetic in `open_selected_home()` with:

```rust
let Some(card) = self.home_view().flat_card(idx).cloned() else { return };
match card.payload {
    crate::home::HomeCardPayload::Track(track) => {
        self.queue = vec![track];
        self.queue_index = Some(0);
        self.shuffle_played.clear();
        self.start_current();
    }
    crate::home::HomeCardPayload::Collection(collection) => self.load_playlist(collection),
}
```

Preserve selection on `Msg::HomeSections` by saving `HomeKey` and resolving it through `flat_index_of` after replacing the sections.

- [ ] **Step 5: Run navigation regressions and commit**

Run:

```bash
cargo test home::tests
cargo test event::tests
cargo test ui::tests::home_
```

Expected: all movement, event, and existing Home tests pass.

```bash
git add src/home.rs src/app.rs src/event.rs
git commit -m "feat: add spatial Home navigation"
```

---

### Task 4: Add Backward-Compatible Home Customization

**Files:**
- Modify: `src/config.rs`
- Modify: `src/app.rs`
- Modify: `src/theme.rs`
- Test: `src/config.rs`

**Interfaces:**
- Produces: `ArtworkMode`, `HomeDensity`, `VisualizerStyle`, `MotionMode`, `AnimationSpeed`, plus semantic `Theme::surface`, `Theme::selected_card`, and `Theme::provider_badge` colors.

- [ ] **Step 1: Write old-config and round-trip tests**

```rust
#[test]
fn old_config_uses_safe_home_defaults() {
    let config: Config = serde_json::from_str(r#"{"volume":0.5,"theme":"Roxo"}"#).unwrap();
    assert_eq!(config.artwork_mode, ArtworkMode::Auto);
    assert_eq!(config.home_density, HomeDensity::Comfortable);
    assert_eq!(config.visualizer_style, VisualizerStyle::Ambient);
    assert_eq!(config.motion, MotionMode::Full);
    assert_eq!(config.animation_speed, AnimationSpeed::Normal);
}
```

Add a serde round-trip test for every enum value.

- [ ] **Step 2: Run the config test to verify it fails**

Run: `cargo test config::tests::old_config_uses_safe_home_defaults`

Expected: FAIL because the enum fields do not exist.

- [ ] **Step 3: Implement serialized enums and defaults**

Define each enum with `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize` and `#[serde(rename_all = "kebab-case")]`. Implement these defaults:

```rust
impl Default for ArtworkMode { fn default() -> Self { Self::Auto } }
impl Default for HomeDensity { fn default() -> Self { Self::Comfortable } }
impl Default for VisualizerStyle { fn default() -> Self { Self::Ambient } }
impl Default for MotionMode { fn default() -> Self { Self::Full } }
impl Default for AnimationSpeed { fn default() -> Self { Self::Normal } }
```

Add the five fields to `Config`, initialize them in `Default`, load them into `App`, and persist the in-memory values in `save_config()` rather than copying stale disk values.

- [ ] **Step 4: Add semantic theme colors**

Extend `Theme` with:

```rust
pub surface: Color,
pub selected_card: Color,
pub provider_badge: Color,
```

Give each existing preset a dark tinted `surface`, a stronger tinted `selected_card`, and a readable `provider_badge`. Do not add provider-specific colors.

- [ ] **Step 5: Run config/UI tests and commit**

Run:

```bash
cargo test config::tests
cargo test ui::tests
```

Expected: old JSON loads with defaults, enum round trips pass, and all themes render.

```bash
git add src/config.rs src/app.rs src/theme.rs
git commit -m "feat: add Home appearance preferences"
```

---

### Task 5: Build a Bounded Asynchronous Home Artwork Cache

**Files:**
- Create: `src/home/artwork.rs`
- Modify: `src/home.rs`
- Modify: `src/app.rs`
- Test: `src/home/artwork.rs`

**Interfaces:**
- Consumes: `HomeKey`, `MusicProvider::fetch_artwork`, `Picker`, and `StatefulProtocol`.
- Produces: `HomeArtworkCache::new`, `mark_pending`, `insert_image`, `fail`, `protocol_mut`, `rebuild_protocols`, and bounded LRU behavior.

- [ ] **Step 1: Write cache tests**

Use generated 1x1 `DynamicImage` values; no files or network:

```rust
#[test]
fn cache_evicts_the_least_recently_used_entry() {
    let mut cache = HomeArtworkCache::new(2);
    cache.insert_image(key("a"), image([1, 0, 0]));
    cache.insert_image(key("b"), image([0, 1, 0]));
    cache.touch(&key("a"));
    cache.insert_image(key("c"), image([0, 0, 1]));
    assert!(cache.contains(&key("a")));
    assert!(!cache.contains(&key("b")));
    assert!(cache.contains(&key("c")));
}

#[test]
fn failed_and_completed_requests_leave_pending_state() {
    let mut cache = HomeArtworkCache::new(2);
    assert!(cache.mark_pending(key("a")));
    assert!(!cache.mark_pending(key("a")));
    cache.fail(&key("a"));
    assert!(cache.mark_pending(key("a")));
}
```

- [ ] **Step 2: Run the cache tests to verify they fail**

Run: `cargo test home::artwork::tests`

Expected: FAIL because `HomeArtworkCache` does not exist.

- [ ] **Step 3: Implement the cache**

Use `HashMap<HomeKey, HomeArtworkEntry>`, `HashSet<HomeKey>` for pending requests, and `VecDeque<HomeKey>` for LRU order. Each entry owns the decoded `DynamicImage` and an optional `StatefulProtocol`. Enforce the limit after every insertion and remove evicted keys from all three collections.

The public entry point used by the renderer is:

```rust
pub fn protocol_mut(&mut self, key: &HomeKey) -> Option<&mut StatefulProtocol> {
    self.touch(key);
    self.entries.get_mut(key)?.protocol.as_mut()
}
```

`rebuild_protocols(&mut self, picker: &mut Picker)` recreates every protocol from its stored image after resize.

- [ ] **Step 4: Wire background requests through App messages**

Add:

```rust
Msg::HomeArtworkBytes { key: HomeKey, bytes: Vec<u8> },
Msg::HomeArtworkFailed { key: HomeKey },
```

Store `home_artwork: HomeArtworkCache` and `visible_home_cards: Vec<(HomeKey, String)>` in `App`. The renderer only records visible key/URL pairs. During `tick()`, clone at most the visible set plus four look-ahead cards, call `mark_pending`, then spawn `provider.fetch_artwork(&url)`. Decode bytes only when draining `HomeArtworkBytes`; discard a result whose key is no longer present in the current `HomeView`.

- [ ] **Step 5: Test stale, failure, and bounded request behavior**

Add App tests that inject artwork messages and assert stale keys are ignored, failures clear pending state, duplicate visible keys spawn one request, and the cache never exceeds 24 entries.

Run:

```bash
cargo test home::artwork::tests
cargo test app::tests::home_artwork
```

Expected: all cache and message-flow tests pass.

- [ ] **Step 6: Commit the artwork pipeline**

```bash
git add src/home.rs src/home/artwork.rs src/app.rs
git commit -m "feat: cache Home artwork asynchronously"
```

---

### Task 6: Replace the Flat Home List with the Responsive Gallery

**Files:**
- Modify: `src/home.rs`
- Modify: `src/ui/main_panel.rs`
- Modify: `src/ui/tests.rs`

**Interfaces:**
- Consumes: `HomeView`, `HomeArtworkCache`, semantic theme colors, and customization settings.
- Produces: `HomeGeometry::for_area`, text-card fallback, image-card rendering, visible-card reporting, and the approved gallery composition.

- [ ] **Step 1: Write geometry and fallback rendering tests**

Test exact geometry rules:

```rust
assert_eq!(HomeGeometry::for_width(120, HomeDensity::Comfortable).columns, 4);
assert_eq!(HomeGeometry::for_width(90, HomeDensity::Comfortable).columns, 3);
assert_eq!(HomeGeometry::for_width(55, HomeDensity::Comfortable).columns, 1);
assert_eq!(HomeGeometry::for_width(90, HomeDensity::Compact).columns, 4);
```

Add buffer tests proving a 100x30 Home contains `Continue listening`, provider shelf titles, selected-card metadata, and no fixed seven-row spectrum panel. Add a 50x20 test proving the same cards appear as vertical rows in the same order.

- [ ] **Step 2: Run the rendering tests to verify they fail**

Run:

```bash
cargo test home::tests::geometry
cargo test ui::tests::home_gallery
```

Expected: FAIL because `HomeGeometry` and the gallery renderer do not exist.

- [ ] **Step 3: Implement responsive geometry**

Add:

```rust
pub struct HomeGeometry { pub columns: usize, pub card_width: u16, pub card_height: u16 }

impl HomeGeometry {
    pub fn for_width(width: u16, density: HomeDensity) -> Self {
        let minimum = match density { HomeDensity::Comfortable => 24, HomeDensity::Compact => 20 };
        let columns = if width < 70 { 1 } else { ((width.saturating_sub(2)) / minimum).clamp(1, 4) as usize };
        let card_width = width.saturating_sub(2) / columns as u16;
        let card_height = if columns == 1 { 3 } else { 8 };
        Self { columns, card_width, card_height }
    }
}
```

- [ ] **Step 4: Implement one renderer over two presentations**

Replace `draw_home_sections` with a shelf loop that lays cards into rows. The selected card uses `theme.selected_card`; unselected cards use `theme.surface`. Every card renders kind, truncated title, subtitle, and provider label. When `artwork_mode == Auto`, a picker exists, and `protocol_mut(key)` returns a protocol, render the cover area with:

```rust
f.render_stateful_widget(
    ratatui_image::StatefulImage::new(None)
        .resize(ratatui_image::Resize::Fit(None)),
    cover_area,
    protocol,
);
```

Otherwise render the text card. Record only cards whose rectangles intersect the Home viewport in `app.visible_home_cards`.

- [ ] **Step 5: Remove the dominant fixed player panel**

Delete `PLAYER_PANEL_HEIGHT` and the fixed `draw_player_panel` split. Keep the greeting, gallery, and the existing bottom Now Playing component. Add a one-row ambient visualizer at the bottom of Home only when `VisualizerStyle::Ambient` and height permits; retain the multi-row bars only for `VisualizerStyle::Spectrum`.

- [ ] **Step 6: Run UI tests and commit**

Run:

```bash
cargo test ui::tests
cargo test home::tests
```

Expected: wide, narrow, tiny, text fallback, selected card, and image fallback tests pass.

```bash
git add src/home.rs src/ui/main_panel.rs src/ui/tests.rs
git commit -m "feat: redesign Home as an adaptive artwork gallery"
```

---

### Task 7: Add Meaningful Motion and Resize Recovery

**Files:**
- Modify: `src/home.rs`
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Modify: `src/ui/main_panel.rs`
- Test: `src/home.rs`
- Test: `src/app.rs`

**Interfaces:**
- Produces: pure `selection_emphasis(elapsed, motion, speed)`, selection transition timestamps, and protocol rebuilding for current-track and Home artwork.

- [ ] **Step 1: Write deterministic motion tests**

```rust
assert_eq!(selection_emphasis(Duration::ZERO, MotionMode::Off, AnimationSpeed::Normal), 1.0);
assert_eq!(selection_emphasis(Duration::from_millis(75), MotionMode::Full, AnimationSpeed::Normal), 0.5);
assert_eq!(selection_emphasis(Duration::from_millis(500), MotionMode::Full, AnimationSpeed::Fast), 1.0);
assert_eq!(selection_emphasis(Duration::from_millis(20), MotionMode::Reduced, AnimationSpeed::Slow), 1.0);
```

- [ ] **Step 2: Run the motion tests to verify they fail**

Run: `cargo test home::tests::selection_emphasis`

Expected: FAIL because the function does not exist.

- [ ] **Step 3: Implement elapsed-time animation math**

Use target durations of 220ms slow, 150ms normal, and 90ms fast. `Reduced` and `Off` return `1.0` immediately. `Full` returns `(elapsed / target).clamp(0.0, 1.0)`. Store `home_selection_changed_at: Instant` in App and reset it only when the selected `HomeKey` changes.

- [ ] **Step 4: Use motion only for state communication**

Blend selected-card emphasis between `theme.surface` and `theme.selected_card` using the pure phase. Do not animate card geometry or terminal images. Update `needs_fast_animation()` so it returns true during an incomplete Home selection transition or active visualizer, and false for an idle gallery.

- [ ] **Step 5: Rebuild all image protocols on resize**

Rename `rebuild_artwork()` to `rebuild_artwork_protocols()` and rebuild both the current-track artwork and `home_artwork` protocols from stored decoded images. Keep the existing `clear_screen` behavior. Update `Event::Resize` in `main.rs` to call the renamed method.

- [ ] **Step 6: Run timing, resize, and CPU-tier tests**

Run:

```bash
cargo test home::tests::selection_emphasis
cargo test app::tests::resize_rebuilds
cargo test app::tests::animation
```

Expected: deterministic phase tests pass, resize restores both image groups, and idle Home uses the non-fast poll tier.

- [ ] **Step 7: Commit motion and resize work**

```bash
git add src/home.rs src/app.rs src/main.rs src/ui/main_panel.rs
git commit -m "feat: add configurable Home motion"
```

---

### Task 8: Add Home-Specific Failure and Retry Behavior

**Files:**
- Modify: `src/app.rs`
- Modify: `src/event.rs`
- Modify: `src/ui/main_panel.rs`
- Test: `src/app.rs`
- Test: `src/event.rs`
- Test: `src/ui/tests.rs`

**Interfaces:**
- Produces: `Msg::HomeFailed(String)`, `App::retry_home()`, preserved cached shelves, and a compact retry row.

- [ ] **Step 1: Write failure-state tests**

Create an app with one existing Home shelf, inject a Home failure, and assert the shelf remains while `home_error` is set. Render it and assert both the old card and `Press R to retry` are visible. Add an event test proving uppercase `R` calls Home loading only while Home has Main focus.

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test app::tests::home_failure_preserves_cached_shelves
cargo test event::tests::home_retry
cargo test ui::tests::home_partial_error
```

Expected: FAIL because Home failures still use generic `Msg::Error`.

- [ ] **Step 3: Implement typed Home failure state**

Change `load_home()` to send `Msg::HomeFailed(error.to_string())`. Add `home_error: Option<String>` to App. On success, clear it after replacing shelves; on failure, call `finish_task()`, retain `home`, store the message, and set a concise status.

Add:

```rust
pub fn retry_home(&mut self) {
    if self.section == Section::Inicio && !self.is_loading() {
        self.load_home();
    }
}
```

Render the retry row after cached shelves or as the Home empty state. Route `KeyCode::Char('R')` to `retry_home()` only for Home/Main focus.

- [ ] **Step 4: Verify failure recovery and commit**

Run:

```bash
cargo test app::tests::home_
cargo test event::tests::home_
cargo test ui::tests::home_
```

Expected: cached content survives, retry is scoped, and success clears the error.

```bash
git add src/app.rs src/event.rs src/ui/main_panel.rs src/ui/tests.rs
git commit -m "feat: preserve Home content across provider failures"
```

---

### Task 9: Document, Capture, and Verify the Complete Experience

**Files:**
- Modify: `README.md`
- Modify: `README.pt-BR.md`
- Modify: `CHANGELOG.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/FEATURES.md`
- Modify: `docs/FEATURES.pt-BR.md`
- Modify: `docs/KEYMAP.md`
- Modify: `docs/KEYMAP.pt-BR.md`
- Modify: `docs/TROUBLESHOOTING.md`
- Modify: `docs/TROUBLESHOOTING.pt-BR.md`
- Modify: `docs/screenshots/home.png`

**Interfaces:**
- Consumes: the completed provider boundary, gallery, settings, keymap, and verified terminal behavior.
- Produces: accurate bilingual documentation and a current Home screenshot.

- [ ] **Step 1: Update architecture documentation**

Document these exact boundaries in `docs/ARCHITECTURE.md`: `App -> Arc<dyn MusicProvider>`, shared models, capability gating, blocking `sign_in`/`resolve_playable`, async metadata/artwork methods, pure `HomeView`, bounded artwork cache, and image/text rendering paths. State explicitly that phase one has one active provider.

- [ ] **Step 2: Update user-facing docs**

In English and PT-BR counterparts, document:

```text
Home: adaptive artwork shelves with a text fallback
Navigation: Left/Right within a shelf; Up/Down between shelves; R retries Home
Config: artwork_mode, home_density, visualizer_style, motion, animation_speed
Compatibility: Kitty/Sixel/iTerm2 images are optional; all actions remain available in text mode
```

Do not claim a second provider, Lua support, pointer hit-testing, or a graphical client.

- [ ] **Step 3: Run automated verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
git diff --check
```

Expected: all commands exit 0; the test count is at least the 102-test baseline plus the new Home tests.

- [ ] **Step 4: Perform the terminal matrix check**

Verify these states in a real terminal and record any terminal-specific limitation in `docs/TROUBLESHOOTING.md` and its PT-BR counterpart:

```text
100x30 image-capable Home
100x30 always-text Home
69x20 narrow fallback
50x12 short fallback
resize while gallery artwork is visible
provider Home failure with cached shelves
reduced-motion and motion-off selection
```

Expected: no cropped titles outside card bounds, no stale terminal graphics after resize, keyboard navigation reaches every card, and image failure falls back per card.

- [ ] **Step 5: Capture and inspect the new Home screenshot**

Capture the actual running application at approximately 120 columns with real Home data and inspect the saved PNG before replacing `docs/screenshots/home.png`. The screenshot must show the artwork gallery, selected-card metadata, navigation, and compact Now Playing strip; it must not show loading, an error, another window, or private account data beyond the existing display-name convention.

- [ ] **Step 6: Commit documentation and screenshot**

```bash
git add README.md README.pt-BR.md CHANGELOG.md docs/ARCHITECTURE.md docs/FEATURES.md docs/FEATURES.pt-BR.md docs/KEYMAP.md docs/KEYMAP.pt-BR.md docs/TROUBLESHOOTING.md docs/TROUBLESHOOTING.pt-BR.md docs/screenshots/home.png
git commit -m "docs: document the provider-ready artwork Home"
```

- [ ] **Step 7: Run final branch verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
git diff --check
git status --short --branch
```

Expected: all checks pass and the worktree contains no implementation changes that were omitted from a task commit.
