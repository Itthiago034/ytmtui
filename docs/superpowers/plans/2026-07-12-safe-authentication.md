# Safe Authentication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace one-shot cookie import with account-aware two-phase sign-in that tries Firefox first and preserves the active session until confirmed activation succeeds.

**Architecture:** `ytmusic::signin` owns typed browser discovery, temporary export, and validation behind a test seam. `MusicProvider` exposes prepare/activate/cancel; `App` owns provider-neutral confirmation state; `YtMusic` privately owns pending credentials and atomically activates them. `YtMusicClient` sends the persisted Google account index instead of fixed `X-Goog-AuthUser: 0`.

**Tech Stack:** Rust 2021, Tokio, async-trait, reqwest, serde, Ratatui, crossterm, tempfile, yt-dlp.

## Global Constraints

- Browser order is Firefox, Brave, Chrome, Chromium, Edge, Vivaldi, Opera.
- Fallback occurs only after export or account validation fails; cancellation never falls back.
- Never expose cookie/header/SAPISID/hash/API-body values.
- Prepared and installed cookie files use mode `0600`.
- Legacy config defaults to account index `0` and no preferred browser/profile.
- Active cookies and client remain unchanged until confirmed activation commits.
- No password/OAuth/webview flow, new runtime, or broad `app.rs` refactor.
- Use red-green-refactor and keep every task independently buildable.

## File Map

- Create `src/app/authentication.rs`: application authentication state machine.
- Create `src/ui/authentication.rs`: confirmation modal; no I/O.
- Modify `src/config.rs`: authentication preference and atomic save.
- Modify `src/provider/mod.rs` and `src/provider/mock.rs`: generic contract and mock.
- Modify `src/ytmusic/{mod,parse,signin,provider}.rs`: identity, preparation, activation.
- Modify `src/{app,event}.rs` and `src/ui/{mod,tests}.rs`: workflow/input/rendering.
- Modify `tests/provider_boundary.rs`: public behavior coverage.
- Modify English/Portuguese authentication, getting-started, troubleshooting, keymap, README, architecture, and changelog docs.

---

### Task 1: Persist account preference and use it in HTTP headers

**Files:** Modify `src/config.rs`, `src/app.rs`, `src/ytmusic/mod.rs`.

**Interfaces:** Produces `AuthenticationConfig`, `Config::try_save`, and `YtMusicClient::with_cookies_for_account`.

- [ ] **Step 1: Write failing tests**

Add to config tests:

```rust
#[test]
fn legacy_config_defaults_to_account_zero() {
    let config: Config = serde_json::from_str(r#"{"cookies":"/tmp/cookies"}"#).unwrap();
    assert_eq!(config.authentication, AuthenticationConfig::default());
    assert_eq!(config.authentication.auth_user, 0);
}

#[test]
fn authentication_preference_roundtrips() {
    let mut config = Config::default();
    config.authentication = AuthenticationConfig {
        browser: Some("firefox".into()),
        profile: Some("default-release".into()),
        auth_user: 2,
    };
    let decoded: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    assert_eq!(decoded.authentication, config.authentication);
}
```

Add to `ytmusic/mod.rs` tests:

```rust
#[test]
fn authenticated_client_keeps_selected_auth_user() {
    let client = YtMusicClient::new_with_auth_user_for_test(3);
    assert_eq!(client.auth_user(), 3);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test legacy_config_defaults_to_account_zero && cargo test authentication_preference_roundtrips && cargo test authenticated_client_keeps_selected_auth_user`

Expected: compile failure for missing preference/client APIs.

- [ ] **Step 3: Implement minimal types**

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthenticationConfig {
    pub browser: Option<String>,
    pub profile: Option<String>,
    pub auth_user: u8,
}
```

Add `authentication` to `Config`, initialize it in `Default`, and preserve `saved.authentication` in `App::save_config`. Make `Config::try_save` serialize to `NamedTempFile` in the config directory, `sync_all`, then `persist`; keep `save` as a wrapper that ignores the result for old call sites.

Add `auth_user: u8` to `YtMusicClient` and:

```rust
pub fn with_cookies_for_account(path: &str, auth_user: u8) -> Result<Self, auth::AuthError> {
    let mut client = Self::new();
    client.auth = Some(Arc::new(Auth::from_cookie_file(path)?));
    client.auth_user = auth_user;
    Ok(client)
}

pub fn with_cookies(path: &str) -> Result<Self, auth::AuthError> {
    Self::with_cookies_for_account(path, 0)
}
```

Authenticated requests use `.header("X-Goog-AuthUser", self.auth_user.to_string())`.
Change `YtMusic::from_environment` to accept `AuthenticationConfig` and build the startup client with `with_cookies_for_account(path, authentication.auth_user)`; update `App::new` to pass the loaded preference.

- [ ] **Step 4: Verify GREEN and commit**

Run: `cargo test config::tests && cargo test authenticated_client_keeps_selected_auth_user`

Expected: PASS.

```bash
git add src/config.rs src/app.rs src/ytmusic/mod.rs
git commit -m "feat: persist YouTube account selection"
```

---

### Task 2: Parse and enumerate Google account identities

**Files:** Modify `src/provider/mod.rs`, `src/ytmusic/parse.rs`, `src/ytmusic/mod.rs`.

**Interfaces:** Produces `SignInAccount`, `get_account_identity`, and bounded `enumerate_cookie_accounts`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn account_identity_keeps_name_and_handle() {
    let data = serde_json::json!({"activeAccountHeaderRenderer": {
        "accountName": {"runs": [{"text": "Thiago Santos"}]},
        "channelHandle": {"runs": [{"text": "@thiagosantos"}]}
    }});
    let identity = parse_account_identity(&data).unwrap();
    assert_eq!(identity.name, "Thiago Santos");
    assert_eq!(identity.handle.as_deref(), Some("@thiagosantos"));
}

#[test]
fn account_probe_stops_after_two_empty_slots_after_match() {
    let slots = [Some("A"), Some("B"), None, None, Some("ignored")];
    assert_eq!(account_indices_from_slots(&slots), vec![0, 1]);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test account_identity_keeps_name_and_handle && cargo test account_probe_stops_after_two_empty_slots_after_match`

Expected: compile failure for missing parser/policy.

- [ ] **Step 3: Implement identity model and parser**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInAccount { pub index: u8, pub name: String, pub handle: Option<String> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdentity { pub name: String, pub handle: Option<String> }
```

`parse_account_identity` reads both fields from the same `activeAccountHeaderRenderer`; `parse_account_name` delegates to it. `get_account_identity` posts to `account/account_menu` and returns the identity.

- [ ] **Step 4: Implement bounded enumeration**

Probe indices `0..=9` with `with_cookies_for_account`. Count `Ok(None)` and index-specific unauthorized as empty; after the first match, stop on two consecutive empties. Return transport/invalid-response errors immediately. Deduplicate `(name, handle)` while retaining the lowest index.

Use this pure stopping helper in both test and production enumeration:

```rust
fn account_indices_from_slots<T>(slots: &[Option<T>]) -> Vec<u8> {
    let mut found = Vec::new();
    let mut empty_after_match = 0;
    for (index, slot) in slots.iter().take(10).enumerate() {
        match slot {
            Some(_) => { found.push(index as u8); empty_after_match = 0; }
            None if !found.is_empty() => {
                empty_after_match += 1;
                if empty_after_match == 2 { break; }
            }
            None => {}
        }
    }
    found
}
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test ytmusic::parse::tests && cargo test account_probe_stops_after_two_empty_slots_after_match`

```bash
git add src/provider/mod.rs src/ytmusic/parse.rs src/ytmusic/mod.rs
git commit -m "feat: enumerate YouTube account identities"
```

---

### Task 3: Prepare credentials with typed Firefox-first fallback

**Files:** Modify `src/ytmusic/signin.rs`.

**Interfaces:** Produces `BrowserCandidate`, `PreparedCredentials`, `SignInError`, `SignInBackend`, and `prepare_with_backend`.

- [ ] **Step 1: Write failing behavior tests with a fake backend**

```rust
struct FakeBackend {
    attempts: std::sync::Mutex<Vec<String>>,
    fail_firefox: bool,
}

impl FakeBackend {
    fn successful() -> Self {
        Self { attempts: std::sync::Mutex::new(Vec::new()), fail_firefox: false }
    }
    fn firefox_export_failure() -> Self {
        Self { attempts: std::sync::Mutex::new(Vec::new()), fail_firefox: true }
    }
    fn attempts(&self) -> Vec<String> {
        self.attempts.lock().unwrap().clone()
    }
}

fn test_candidates() -> Vec<BrowserCandidate> {
    vec![BrowserCandidate::firefox(None), BrowserCandidate::chromium("brave")]
}

impl SignInBackend for FakeBackend {
    fn export(&self, candidate: &BrowserCandidate, destination: &Path) -> Result<(), SignInError> {
        self.attempts.lock().unwrap().push(candidate.method.clone());
        if self.fail_firefox && candidate.method == "firefox" {
            return Err(SignInError::ExportFailed("synthetic failure".into()));
        }
        std::fs::write(destination, "synthetic cookie jar")
            .map_err(|error| SignInError::Io(error.to_string()))
    }
    fn accounts(&self, _path: &Path) -> Result<Vec<SignInAccount>, SignInError> {
        Ok(vec![SignInAccount { index: 0, name: "Thiago Santos".into(), handle: None }])
    }
}

#[test]
fn successful_firefox_never_invokes_brave() {
    let temp = tempfile::tempdir().unwrap();
    let backend = FakeBackend::successful();
    let prepared = prepare_with_backend(test_candidates(), temp.path(), &backend, &|_| {}).unwrap();
    assert_eq!(prepared.candidate.method, "firefox");
    assert_eq!(backend.attempts(), vec!["firefox"]);
}

#[test]
fn failed_firefox_records_reason_then_uses_brave() {
    let temp = tempfile::tempdir().unwrap();
    let backend = FakeBackend::firefox_export_failure();
    let prepared = prepare_with_backend(test_candidates(), temp.path(), &backend, &|_| {}).unwrap();
    assert_eq!(prepared.candidate.method, "brave");
    assert_eq!(prepared.failures[0].reason, "browser export failed");
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test successful_firefox_never_invokes_brave && cargo test failed_firefox_records_reason_then_uses_brave`

Expected: compile failure for missing typed preparation API.

- [ ] **Step 3: Implement candidate/backend types**

```rust
pub struct BrowserCandidate {
    pub method: String,
    pub profile_path: Option<PathBuf>,
    pub profile_label: Option<String>,
}

pub trait SignInBackend: Send + Sync {
    fn export(&self, candidate: &BrowserCandidate, destination: &Path) -> Result<(), SignInError>;
    fn accounts(&self, path: &Path) -> Result<Vec<SignInAccount>, SignInError>;
}
```

Detection returns typed candidates in the global order. `yt_dlp_argument` emits `firefox:<path>` for XDG Firefox and the method otherwise. The system backend calls yt-dlp and Task 2 enumeration; raw stderr stays private.
Discovery accepts the saved `AuthenticationConfig` only to choose the saved profile within a browser; it never moves a Chromium method ahead of Firefox.

- [ ] **Step 4: Implement safe preparation**

For each candidate, create a mode-`0600` temp file in the supplied config directory, export, validate at least one account, and return on the first success. Delete failed temp files. Progress/failure history contains fixed sanitized messages only. Never write the production cookie path during preparation.
Use the filename prefix `.ytmtui-signin-`; before preparing, delete only files with that prefix owned by the current user and older than 24 hours. Add a unit test with one stale owned file and one unrelated file, asserting only the stale ytmtui file is removed.

- [ ] **Step 5: Verify and commit**

Run: `cargo test ytmusic::signin::tests`

```bash
git add src/ytmusic/signin.rs
git commit -m "feat: prepare Firefox-first sign-in safely"
```

---

### Task 4: Add provider previews and atomic activation

**Files:** Modify `src/provider/mod.rs`, `src/provider/mock.rs`, `src/ytmusic/signin.rs`, `src/ytmusic/provider.rs`, `tests/provider_boundary.rs`.

**Interfaces:** Produces `SignInPreview`, extended `SignInSummary`, and prepare/activate/cancel methods.

- [ ] **Step 1: Write failing boundary/rollback tests**

```rust
#[tokio::test]
async fn prepare_cancel_preserves_old_session() {
    let provider = Arc::new(MockProvider::authenticated());
    let preview = provider.prepare_sign_in(&|_| {}).unwrap();
    provider.cancel_sign_in(preview.id);
    assert!(provider.is_authenticated());
}

#[tokio::test]
async fn activation_uses_selected_account() {
    let provider = Arc::new(MockProvider::default());
    let preview = provider.prepare_sign_in(&|_| {}).unwrap();
    let summary = provider.activate_sign_in(preview.id, 1).unwrap();
    assert_eq!((summary.account_name.as_str(), summary.account_index), ("Mock Account 2", 1));
}
```

Also add a unit test where persistence closure returns `io::Error` and assert the old cookie file contents remain.

- [ ] **Step 2: Verify RED**

Run: `cargo test prepare_cancel && cargo test activation_uses_selected_account && cargo test failed_persistence_restores`

Expected: compile failure for missing provider methods/installer.

- [ ] **Step 3: Add exact contract types**

```rust
#[derive(Debug, Clone)]
pub struct SignInPreview {
    pub id: u64,
    pub method: String,
    pub profile_label: Option<String>,
    pub accounts: Vec<SignInAccount>,
    pub current_account_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SignInSummary {
    pub method: String,
    pub credentials_path: Option<String>,
    pub account_name: String,
    pub account_index: u8,
}
```

Add provider methods `prepare_sign_in`, `activate_sign_in(preview_id, account_index)`, and `cancel_sign_in` while temporarily retaining old `sign_in` until Task 5.

- [ ] **Step 4: Implement atomic activation**

`install_prepared_credentials(prepared, active, persist)` copies the old file to a mode-`0600` backup, renames prepared over active, calls `persist`, restores backup/removes new file on failure, and deletes backup on success. `YtMusic` privately stores one pending preparation/id. It validates the selected index and constructs the new client before filesystem mutation; persists config after rename; publishes the client only after persistence succeeds. Cancellation deletes only matching pending temp state.

Implement the filesystem transaction as:

```rust
pub fn install_prepared_credentials<F>(prepared: &Path, active: &Path, persist: F) -> std::io::Result<()>
where F: FnOnce() -> std::io::Result<()>,
{
    let backup = active.with_extension("activation-backup");
    let had_active = active.is_file();
    if had_active { copy_with_mode_600(active, &backup)?; }
    std::fs::rename(prepared, active)?;
    if let Err(error) = persist() {
        if had_active { std::fs::rename(&backup, active)?; }
        else { std::fs::remove_file(active)?; }
        return Err(error);
    }
    if had_active { let _ = std::fs::remove_file(backup); }
    Ok(())
}
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test --test provider_boundary prepare_ && cargo test ytmusic::signin::tests`

```bash
git add src/provider/mod.rs src/provider/mock.rs src/ytmusic/signin.rs src/ytmusic/provider.rs tests/provider_boundary.rs
git commit -m "feat: activate prepared sessions atomically"
```

---

### Task 5: Move App to a two-phase authentication state machine

**Files:** Create `src/app/authentication.rs`; modify `src/app.rs`, provider implementations, and `tests/provider_boundary.rs`.

**Interfaces:** Produces `AuthenticationFlow` and App prepare/select/confirm/cancel methods; removes one-shot sign-in.

- [ ] **Step 1: Write failing App workflow tests**

```rust
#[tokio::test]
async fn app_prepares_then_confirms_selected_account() {
    let mut app = App::with_provider(Arc::new(MockProvider::default()));
    app.prepare_sign_in();
    drain_until_idle(&mut app).await;
    assert!(app.sign_in_preview().is_some());
    assert_eq!(app.authentication, AuthState::Anonymous);
    app.select_next_sign_in_account();
    app.confirm_sign_in();
    drain_until_idle(&mut app).await;
    assert_eq!(app.account_name.as_deref(), Some("Mock Account 2"));
}
```

Add cancellation test asserting current account/auth state remain unchanged.

- [ ] **Step 2: Verify RED**

Run: `cargo test --test provider_boundary app_prepares && cargo test --test provider_boundary cancelling_a_preview`

Expected: compile failure for missing App flow.

- [ ] **Step 3: Implement state/messages**

```rust
pub enum AuthenticationFlow {
    Idle,
    Preparing,
    AwaitingConfirmation { preview: SignInPreview, selected: usize },
    Activating,
}
```

Preparation uses `spawn_blocking` and sends `Msg::SignInPrepared`. Confirmation sends preview id/account index to activation. Cancellation calls provider cancellation and returns `Idle`. `SignInPrepared` changes no account data; `SignedIn` updates authentication/name/path and reloads Home/Library.
When handling `SignInPrepared`, copy `self.account_name` into `preview.current_account_name` before storing it. Add a `#[cfg(test)] pub(crate) fn set_sign_in_preview_for_test(&mut self, preview: SignInPreview)` helper that installs `AwaitingConfirmation { preview, selected: 0 }` for renderer/input unit tests.

- [ ] **Step 4: Remove one-shot API and verify**

Replace the `g` action with `prepare_sign_in`, initialize flow in both constructors, then remove `MusicProvider::sign_in`, implementations, and `App::sign_in`.

Run: `cargo test --test provider_boundary && cargo test app::tests`

Expected: PASS and `rg -n 'fn sign_in\(|\.sign_in\(' src tests` returns no matches.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/app/authentication.rs src/provider/mod.rs src/provider/mock.rs src/ytmusic/provider.rs tests/provider_boundary.rs
git commit -m "feat: add two-phase authentication workflow"
```

---

### Task 6: Add keyboard account picker and modal

**Files:** Create `src/ui/authentication.rs`; modify `src/ui/mod.rs`, `src/event.rs`, `src/ui/tests.rs`, both keymaps.

- [ ] **Step 1: Write failing input/render tests**

```rust
fn prepared_sign_in_app() -> App {
    let mut app = App::new_for_tests();
    app.set_sign_in_preview_for_test(SignInPreview {
        id: 1,
        method: "firefox".into(),
        profile_label: Some("default-release".into()),
        accounts: vec![
            SignInAccount { index: 0, name: "Mock Account 1".into(), handle: None },
            SignInAccount { index: 1, name: "Mock Account 2".into(), handle: Some("@mock2".into()) },
        ],
        current_account_name: None,
    });
    app
}

#[test]
fn sign_in_modal_consumes_navigation_and_escape() {
    let mut app = prepared_sign_in_app();
    handle_key(&mut app, key(KeyCode::Down));
    assert_eq!(app.sign_in_preview().unwrap().1, 1);
    handle_key(&mut app, key(KeyCode::Esc));
    assert!(app.sign_in_preview().is_none());
}

#[test]
fn sign_in_modal_renders_browser_accounts_and_controls() {
    let mut app = prepared_sign_in_app();
    let content = text(&render(&mut app, 100, 30));
    for expected in ["Connect an account", "Firefox", "Mock Account 1", "Enter confirm", "Esc cancel"] {
        assert!(content.contains(expected));
    }
}
```

Add the `prepared_sign_in_app` helper separately to `event.rs` and `ui/tests.rs`; the latter already provides `render` and `text`.

- [ ] **Step 2: Verify RED**

Run: `cargo test sign_in_modal_`

Expected: tests fail because focused input/rendering is absent.

- [ ] **Step 3: Implement modal-first input**

At the top of `handle_key`, when a preview exists, map Up/k, Down/j, Enter, Esc to selection/confirm/cancel and return for every key so normal shortcuts cannot leak through.

- [ ] **Step 4: Implement renderer**

`ui::authentication::draw` uses `Clear`, a centered bounded rectangle, browser/profile header, selectable name/handle list, current-account marker, and footer `↑/↓ select  Enter confirm  Esc cancel`. Draw it after all normal UI layers. Clamp safely for very small terminals.

Its entry point and early bounds are:

```rust
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some((preview, selected)) = app.sign_in_preview() else { return };
    let width = area.width.saturating_sub(4).min(64);
    let height = (preview.accounts.len() as u16 + 8).min(area.height.saturating_sub(2));
    if width < 20 || height < 6 { return; }
    let popup = centered_rect(width, height, area);
    f.render_widget(Clear, popup);
    render_account_list(f, popup, preview, selected, app.theme());
}
```

- [ ] **Step 5: Verify, document, commit**

Run: `cargo test sign_in_ && cargo test ui::tests::very_small_terminals_never_panic && cargo test ui::tests`

```bash
git add src/event.rs src/ui/mod.rs src/ui/authentication.rs src/ui/tests.rs docs/KEYMAP.md docs/KEYMAP.pt-BR.md
git commit -m "feat: confirm browser account before sign-in"
```

---

### Task 7: Synchronize docs and pass gates

**Files:** Modify both READMEs, authentication/getting-started/troubleshooting docs in both languages, architecture, and changelog.

- [ ] **Step 1: Document canonical English behavior**

State: Firefox is first; another browser is tried only when export/validation fails; account preview precedes replacement; cancellation preserves session; non-zero account index and preference persist.

- [ ] **Step 2: Mirror Portuguese content**

Use the same claims and examples without expanding scope.

- [ ] **Step 3: Run automated gates**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
bash scripts/test-refresh-cookies.sh
git diff --check
```

Expected: every command exits zero and the script prints `refresh-cookies tests passed`.

- [ ] **Step 4: Run manual Linux acceptance**

Firefox/Brave different accounts; Firefox success without Brave attempt; Firefox technical failure then Brave preview; cancellation checksum unchanged; non-zero Firefox account survives restart.

- [ ] **Step 5: Commit docs**

```bash
git add README.md README.pt-BR.md CHANGELOG.md docs/AUTHENTICATION.md docs/AUTHENTICATION.pt-BR.md docs/GETTING_STARTED.md docs/GETTING_STARTED.pt-BR.md docs/TROUBLESHOOTING.md docs/TROUBLESHOOTING.pt-BR.md docs/ARCHITECTURE.md
git commit -m "docs: explain safe account-aware sign-in"
```
