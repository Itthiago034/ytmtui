# Authentication Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ytmtui cookie authentication explicit, typed, secure, and testable while preserving anonymous operation and the user's current uncommitted authentication work.

**Architecture:** Cookie parsing returns a dedicated `AuthError`; the YouTube Music client returns a dedicated `YtMusicError`; and the application stores an explicit `AuthenticationState`. Network tasks map typed expiry errors into one application message, while path selection and state transitions stay pure enough for unit tests.

**Tech Stack:** Rust 2021, Tokio, Reqwest, Serde JSON, Ratatui, Bash, Cargo test/Clippy/rustfmt.

---

## Scope and working-tree constraints

This is the first of four implementation plans derived from `docs/superpowers/specs/2026-07-05-stability-playback-ux-architecture-design.md`. Playback/radio, search/UI, and architecture/translation receive separate plans after this stage lands.

The working tree already contains approved user changes in:

- `src/app.rs`
- `src/main.rs`
- `src/ytmusic/auth.rs`
- `src/ytmusic/mod.rs`
- `src/ytmusic/parse.rs`
- `scripts/refresh-cookies.sh`

Preserve them. This stage intentionally incorporates the authentication-related changes in `app.rs`, `ytmusic/auth.rs`, `ytmusic/mod.rs`, `ytmusic/parse.rs`, and `scripts/refresh-cookies.sh`. Do not stage `src/main.rs`; its refresh-rate change belongs to the later UI/animation stage.

Baseline recorded on 2026-07-05: `cargo test --all-targets --all-features` passes 8 tests.

## File map

- Create `src/app/authentication.rs`: authentication state and deterministic cookie-path resolution.
- Modify `src/app.rs`: declare the authentication module, store typed state, and map typed client failures.
- Modify `src/ytmusic/auth.rs`: typed cookie parsing errors and secret-safe data handling.
- Modify `src/ytmusic/mod.rs`: typed request errors and authenticated-client construction.
- Modify `src/ytmusic/parse.rs`: preserve and test the current account-name parser improvement.
- Modify `src/ui/sidebar.rs`: derive account presentation from typed authentication state.
- Modify `src/ui/main_panel.rs`: derive library/home presentation from typed authentication state.
- Modify `src/lib.rs`: expose only the modules required by tests.
- Modify `scripts/refresh-cookies.sh`: atomic, permission-safe cookie replacement with English output.
- Create `scripts/test-refresh-cookies.sh`: Linux shell regression tests with a fake `yt-dlp`.
- Modify `.github/workflows/ci.yml`: execute the cookie refresh regression test.
- Modify `.gitignore`: ignore local `.superpowers/` visual-companion artifacts.

### Task 1: Protect local artifacts and confirm the baseline

**Files:**
- Modify: `.gitignore:1-11`

- [ ] **Step 1: Ignore visual-companion runtime files**

Add this exact section to `.gitignore`:

```gitignore

# Local design mockups
/.superpowers
```

- [ ] **Step 2: Confirm only intended pre-existing changes remain visible**

Run:

```bash
git status --short
```

Expected: `.superpowers/` is absent; the approved source changes, `scripts/`, and `.gitignore` remain visible.

- [ ] **Step 3: Run the baseline test suite**

Run:

```bash
cargo test --all-targets --all-features
```

Expected: 8 tests pass before new tests are added.

- [ ] **Step 4: Commit the local-artifact rule**

```bash
git add .gitignore
git commit -m "chore: ignore local design artifacts"
```

### Task 2: Return typed errors from cookie parsing

**Files:**
- Modify: `src/ytmusic/auth.rs:9-163`

- [ ] **Step 1: Write failing parser error tests**

Add these tests to `src/ytmusic/auth.rs`:

```rust
#[test]
fn rejects_cookie_text_without_sapisid() {
    let text = ".youtube.com\tTRUE\t/\tTRUE\t9999999999\tSID\tsid_only\n";
    assert!(matches!(
        Auth::from_cookie_text(text),
        Err(AuthError::MissingSapisid)
    ));
}

#[test]
fn rejects_cookie_file_that_cannot_be_read() {
    let error = match Auth::from_cookie_file("/path/that/does/not/exist") {
        Ok(_) => panic!("missing file must fail"),
        Err(error) => error,
    };
    assert!(matches!(error, AuthError::ReadFile { .. }));
}
```

Update the two existing success tests to call `.expect("valid cookie fixture")` on a `Result`.

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```bash
cargo test ytmusic::auth::tests --lib
```

Expected: compilation fails because `AuthError` does not exist and the constructors still return `Option<Auth>`.

- [ ] **Step 3: Implement `AuthError` and result-based parsing**

Add the following error type and implementations above `Auth`:

```rust
use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AuthError {
    ReadFile { path: PathBuf, source: io::Error },
    MissingSapisid,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFile { path, .. } => {
                write!(f, "could not read cookie file {}", path.display())
            }
            Self::MissingSapisid => write!(f, "cookie file does not contain a SAPISID cookie"),
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadFile { source, .. } => Some(source),
            Self::MissingSapisid => None,
        }
    }
}
```

Remove `Debug` from `Auth` so a future debug log cannot expose cookie material:

```rust
#[derive(Clone)]
pub struct Auth {
    pub cookie_header: String,
    pub sapisid: String,
}
```

Change the constructors to:

```rust
pub fn from_cookie_file(path: &str) -> Result<Auth, AuthError> {
    let content = std::fs::read_to_string(path).map_err(|source| AuthError::ReadFile {
        path: PathBuf::from(path),
        source,
    })?;
    Self::from_cookie_text(&content)
}

fn from_cookie_text(content: &str) -> Result<Auth, AuthError> {
    let mut chosen: HashMap<String, (String, String, u8)> = HashMap::new();

    for raw in content.lines() {
        let line = raw.strip_prefix("#HttpOnly_").unwrap_or(raw);
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 {
            continue;
        }
        let domain = fields[0].trim();
        let name = fields[5].trim();
        let value = fields[6].trim();
        if name.is_empty() || !is_allowed_domain(domain) {
            continue;
        }

        let priority = domain_priority(domain);
        match chosen.get(name) {
            Some((_, _, existing_priority)) if *existing_priority <= priority => {}
            _ => {
                chosen.insert(
                    name.to_string(),
                    (value.to_string(), domain.to_string(), priority),
                );
            }
        }
    }

    let sapisid = chosen
        .get("__Secure-3PAPISID")
        .filter(|(_, domain, _)| domain.contains("youtube.com"))
        .map(|(value, _, _)| value.clone())
        .or_else(|| chosen.get("__Secure-3PAPISID").map(|(value, _, _)| value.clone()))
        .or_else(|| chosen.get("SAPISID").map(|(value, _, _)| value.clone()))
        .ok_or(AuthError::MissingSapisid)?;

    let mut pairs: Vec<_> = chosen
        .into_iter()
        .map(|(name, (value, _, _))| (name, value))
        .collect();
    pairs.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let cookie_header = pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");

    Ok(Auth {
        cookie_header,
        sapisid,
    })
}
```

This removes the production `expect("name in map")` while retaining deterministic header order.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test ytmusic::auth::tests --lib
```

Expected: 4 authentication parser tests pass.

- [ ] **Step 5: Commit typed cookie parsing**

```bash
git add src/ytmusic/auth.rs
git commit -m "fix: return typed cookie parsing errors"
```

### Task 3: Return typed errors from the YouTube Music client

**Files:**
- Modify: `src/ytmusic/mod.rs:12-101`
- Test: `src/ytmusic/mod.rs` inline test module

- [ ] **Step 1: Write failing status-classification tests**

Append this test module to `src/ytmusic/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn authenticated_unauthorized_response_means_expired_session() {
        let error = classify_status(true, StatusCode::UNAUTHORIZED, "browse");
        assert!(matches!(error, YtMusicError::SessionExpired { .. }));
    }

    #[test]
    fn anonymous_forbidden_response_remains_an_http_error() {
        let error = classify_status(false, StatusCode::FORBIDDEN, "browse");
        assert!(matches!(error, YtMusicError::HttpStatus { .. }));
    }
}
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```bash
cargo test ytmusic::tests --lib
```

Expected: compilation fails because `classify_status` and `YtMusicError` do not exist.

- [ ] **Step 3: Define the client error contract**

Replace the module-level `anyhow::Result` import with these definitions:

```rust
use std::fmt;

pub type YtMusicResult<T> = std::result::Result<T, YtMusicError>;

#[derive(Debug)]
pub enum YtMusicError {
    AuthenticationRequired,
    SessionExpired {
        status: reqwest::StatusCode,
        endpoint: String,
    },
    HttpStatus {
        status: reqwest::StatusCode,
        endpoint: String,
    },
    Transport(reqwest::Error),
    InvalidResponse(reqwest::Error),
}

impl fmt::Display for YtMusicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationRequired => write!(f, "authentication is required"),
            Self::SessionExpired { status, endpoint } => {
                write!(f, "session expired while requesting {endpoint} ({status})")
            }
            Self::HttpStatus { status, endpoint } => {
                write!(f, "request to {endpoint} failed with {status}")
            }
            Self::Transport(error) => write!(f, "request failed: {error}"),
            Self::InvalidResponse(error) => write!(f, "invalid API response: {error}"),
        }
    }
}

impl std::error::Error for YtMusicError {}

impl From<reqwest::Error> for YtMusicError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error)
    }
}

fn classify_status(
    authenticated: bool,
    status: reqwest::StatusCode,
    endpoint: &str,
) -> YtMusicError {
    if authenticated
        && matches!(
            status,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        )
    {
        YtMusicError::SessionExpired {
            status,
            endpoint: endpoint.to_string(),
        }
    } else {
        YtMusicError::HttpStatus {
            status,
            endpoint: endpoint.to_string(),
        }
    }
}
```

- [ ] **Step 4: Apply the typed contract to construction and requests**

Change authenticated construction and `post` to:

```rust
pub fn with_cookies(path: &str) -> std::result::Result<Self, auth::AuthError> {
    let mut client = Self::new();
    client.auth = Some(Arc::new(Auth::from_cookie_file(path)?));
    Ok(client)
}

async fn post(&self, endpoint: &str, body: Value) -> YtMusicResult<Value> {
    let url = format!("{BASE}/{endpoint}?prettyPrint=false");
    let mut request = self
        .http
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Origin", auth::ORIGIN)
        .json(&body);

    if let Some(auth) = &self.auth {
        request = request
            .header("Cookie", auth.cookie_header.clone())
            .header("Authorization", auth.authorization_header())
            .header("X-Goog-AuthUser", "0")
            .header("X-Origin", auth::ORIGIN);
    }

    let response = request.send().await.map_err(YtMusicError::Transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(classify_status(self.auth.is_some(), status, endpoint));
    }
    response
        .json::<Value>()
        .await
        .map_err(YtMusicError::InvalidResponse)
}
```

Change every public client method from `anyhow::Result<T>` to `YtMusicResult<T>`. Replace authenticated precondition errors with:

```rust
if !self.is_authenticated() {
    return Err(YtMusicError::AuthenticationRequired);
}
```

Retain the approved library deduplication and `parse_account_name` changes already present in the working tree.

- [ ] **Step 5: Run client and parser tests**

Run:

```bash
cargo test ytmusic --lib
```

Expected: all authentication, status-classification, and parser tests pass.

- [ ] **Step 6: Commit the typed client contract**

```bash
git add src/ytmusic/mod.rs src/ytmusic/parse.rs
git commit -m "fix: classify youtube music request failures"
```

### Task 4: Add explicit authentication state and cookie-path resolution

**Files:**
- Create: `src/app/authentication.rs`
- Modify: `src/app.rs:1-308`
- Modify: `src/ui/sidebar.rs:1-90`
- Modify: `src/ui/main_panel.rs:160-230`

- [ ] **Step 1: Write the authentication-state and path-resolution tests**

Create `src/app/authentication.rs` with the public types followed by these initially failing tests:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationState {
    Anonymous,
    Authenticated,
    Expired,
    InvalidCookies,
}

impl AuthenticationState {
    pub fn is_authenticated(self) -> bool {
        matches!(self, Self::Authenticated)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookiePathResolution {
    pub path: Option<String>,
    pub missing_requested_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_path_wins_when_it_exists() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let env = dir.path().join("env.txt");
        let configured = dir.path().join("configured.txt");
        std::fs::write(&env, "env").expect("environment fixture");
        std::fs::write(&configured, "configured").expect("configured fixture");

        let resolution = resolve_cookie_path(
            Some(env.to_string_lossy().into_owned()),
            Some(configured.to_string_lossy().into_owned()),
            None,
        );

        assert_eq!(resolution.path.as_deref(), env.to_str());
        assert_eq!(resolution.missing_requested_path, None);
    }

    #[test]
    fn missing_environment_path_falls_back_and_is_reported() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let missing = dir.path().join("missing.txt");
        let fallback = dir.path().join("cookies.txt");
        std::fs::write(&fallback, "fallback").expect("fallback fixture");

        let resolution = resolve_cookie_path(
            Some(missing.to_string_lossy().into_owned()),
            None,
            Some(fallback),
        );

        assert_eq!(resolution.path.as_deref(), fallback.to_str());
        assert_eq!(resolution.missing_requested_path.as_deref(), missing.to_str());
    }
}
```

- [ ] **Step 2: Verify the new tests fail**

Declare `mod authentication;` near the top of `src/app.rs`, then run:

```bash
cargo test app::authentication::tests --lib
```

Expected: compilation fails because `resolve_cookie_path` is undefined.

- [ ] **Step 3: Implement deterministic path resolution**

Add this function before the tests in `src/app/authentication.rs`:

```rust
pub fn resolve_cookie_path(
    environment: Option<String>,
    configured: Option<String>,
    default: Option<PathBuf>,
) -> CookiePathResolution {
    let missing_requested_path = environment
        .as_ref()
        .filter(|path| !path.is_empty() && !std::path::Path::new(path).is_file())
        .cloned()
        .or_else(|| {
            configured
                .as_ref()
                .filter(|path| !path.is_empty() && !std::path::Path::new(path).is_file())
                .cloned()
        });

    let default = default.map(|path| path.to_string_lossy().into_owned());
    let path = [environment, configured, default]
        .into_iter()
        .flatten()
        .find(|path| !path.is_empty() && std::path::Path::new(path).is_file());

    CookiePathResolution {
        path,
        missing_requested_path,
    }
}
```

- [ ] **Step 4: Replace `logged_in` with typed application state**

Import the new types in `src/app.rs`:

```rust
mod authentication;

pub use authentication::AuthenticationState;
use authentication::resolve_cookie_path;
```

Replace `pub logged_in: bool` with:

```rust
pub authentication: AuthenticationState,
```

In `App::new`, resolve paths and construct the client with:

```rust
let default_cookies = dirs::config_dir().map(|dir| dir.join("ytmtui/cookies.txt"));
let resolution = resolve_cookie_path(
    std::env::var("YTM_COOKIES").ok(),
    config.cookies.clone(),
    default_cookies,
);
let cookies = resolution.path;

let (client, authentication) = match cookies.as_deref() {
    Some(path) => match YtMusicClient::with_cookies(path) {
        Ok(client) => (client, AuthenticationState::Authenticated),
        Err(_) => (YtMusicClient::new(), AuthenticationState::InvalidCookies),
    },
    None => (YtMusicClient::new(), AuthenticationState::Anonymous),
};

let status = if let Some(path) = resolution.missing_requested_path.as_deref() {
    format!("Configured cookie file does not exist: {path}")
} else {
    match authentication {
        AuthenticationState::Authenticated => {
            "Signed in. Loading your library... Press / to search or ? for help.".to_string()
        }
        AuthenticationState::InvalidCookies => {
            "Cookie file is invalid. Refresh it with ./scripts/refresh-cookies.sh.".to_string()
        }
        AuthenticationState::Anonymous => {
            "Welcome to ytmtui. Press / to search or ? for help.".to_string()
        }
        AuthenticationState::Expired => unreachable!("a new application cannot start expired"),
    }
};
```

Add this query method:

```rust
pub fn is_authenticated(&self) -> bool {
    self.authentication.is_authenticated()
}
```

Apply these exact condition replacements without changing the surrounding rendering code:

```rust
// src/app.rs: load_library, like_current, load_account
if !self.is_authenticated() {
    return;
}

// src/ui/main_panel.rs: home empty state
} else if app.is_authenticated() {
    "No recommendations are available. Press / to search.".to_string()
}

// src/ui/main_panel.rs: library state
if !app.is_authenticated() {
    let message = "You are not signed in.\n\nSave a Netscape cookie file to:\n\n~/.config/ytmtui/cookies.txt\n\nRestart ytmtui after refreshing the file.";
}
```

In `src/ui/sidebar.rs`, replace only `if app.logged_in {` with `if app.is_authenticated() {`; its account-name rendering body remains byte-for-byte unchanged.

Remove the old `logged_in` field initialization and replace all touched status strings with the English strings shown above.

- [ ] **Step 5: Run authentication and UI compilation tests**

Run:

```bash
cargo test --all-targets --all-features
```

Expected: all tests pass and no `logged_in` references remain.

- [ ] **Step 6: Commit typed application authentication state**

```bash
git add src/app.rs src/app/authentication.rs src/ui/sidebar.rs src/ui/main_panel.rs
git commit -m "refactor: model authentication state explicitly"
```

### Task 5: Map typed client failures at the application boundary

**Files:**
- Modify: `src/app.rs:110-129, 323-345, 459-480, 862-957`
- Test: `src/app.rs` inline test module

- [ ] **Step 1: Write a failing message-mapping test**

Add a pure mapper and test target to `src/app.rs` by first adding only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn session_expiry_maps_to_the_dedicated_message() {
        let message = client_error_message(
            "Could not load library",
            YtMusicError::SessionExpired {
                status: StatusCode::UNAUTHORIZED,
                endpoint: "browse".to_string(),
            },
        );

        assert!(matches!(message, Msg::SessionExpired));
    }
}
```

- [ ] **Step 2: Verify the mapper test fails**

Run:

```bash
cargo test app::tests::session_expiry_maps_to_the_dedicated_message --lib
```

Expected: compilation fails because `client_error_message` and the imported `YtMusicError` are unavailable.

- [ ] **Step 3: Implement the typed mapper**

Import the type with `use crate::ytmusic::YtMusicError;` and add:

```rust
fn client_error_message(context: &str, error: YtMusicError) -> Msg {
    match error {
        YtMusicError::SessionExpired { .. } => Msg::SessionExpired,
        other => Msg::Error(format!("{context}: {other}")),
    }
}
```

Replace string inspection in `load_library` and `load_account` with:

```rust
Err(error) => {
    let _ = tx.send(client_error_message("Could not load library", error));
}
```

and:

```rust
Err(error) => {
    let _ = tx.send(client_error_message("Could not load account", error));
}
```

Handle expiry with typed state and an English recovery action:

```rust
Msg::SessionExpired => {
    self.busy = false;
    self.authentication = AuthenticationState::Expired;
    self.library.clear();
    self.account_name = None;
    self.status = "Session expired. Run ./scripts/refresh-cookies.sh with music.youtube.com signed in, then restart ytmtui.".to_string();
}
```

- [ ] **Step 4: Prove string-based status inspection is gone**

Run:

```bash
rg -n 'contains\("HTTP 401"\)|contains\("HTTP 403"\)' src
```

Expected: no output.

- [ ] **Step 5: Run all Rust tests**

Run:

```bash
cargo test --all-targets --all-features
```

Expected: all tests pass, including the new message-mapping test.

- [ ] **Step 6: Commit typed failure mapping**

```bash
git add src/app.rs
git commit -m "fix: handle expired sessions without string matching"
```

### Task 6: Make cookie refresh atomic and permission-safe

**Files:**
- Modify: `scripts/refresh-cookies.sh:1-29`
- Create: `scripts/test-refresh-cookies.sh`
- Modify: `.github/workflows/ci.yml:35-44`

- [ ] **Step 1: Write the shell regression test**

Create executable `scripts/test-refresh-cookies.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/home/.config/ytmtui"

cat >"$TMP/bin/yt-dlp" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
destination=""
while (($#)); do
  if [[ "$1" == "--cookies" ]]; then
    destination="$2"
    shift 2
  else
    shift
  fi
done
[[ -n "$destination" ]]
printf '# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t9999999999\tSAPISID\ttest\n' >"$destination"
FAKE
chmod +x "$TMP/bin/yt-dlp"

HOME="$TMP/home" PATH="$TMP/bin:$PATH" "$ROOT/scripts/refresh-cookies.sh" brave >/dev/null
destination="$TMP/home/.config/ytmtui/cookies.txt"
[[ -s "$destination" ]]
[[ "$(stat -c '%a' "$destination")" == "600" ]]

printf 'old-cookie\n' >"$destination"
cat >"$TMP/bin/yt-dlp" <<'FAKE_FAIL'
#!/usr/bin/env bash
exit 1
FAKE_FAIL
chmod +x "$TMP/bin/yt-dlp"

if HOME="$TMP/home" PATH="$TMP/bin:$PATH" "$ROOT/scripts/refresh-cookies.sh" brave >/dev/null 2>&1; then
  echo "expected refresh failure" >&2
  exit 1
fi
grep -qx 'old-cookie' "$destination"
echo "refresh-cookies tests passed"
```

- [ ] **Step 2: Run the regression test and verify it fails**

Run:

```bash
chmod +x scripts/test-refresh-cookies.sh
bash scripts/test-refresh-cookies.sh
```

Expected: FAIL because the current script writes directly to the destination and does not enforce mode `600`.

- [ ] **Step 3: Implement atomic cookie replacement**

Replace `scripts/refresh-cookies.sh` with:

```bash
#!/usr/bin/env bash
# Refresh ~/.config/ytmtui/cookies.txt from a supported browser profile.
set -euo pipefail
umask 077

DEST="${XDG_CONFIG_HOME:-$HOME/.config}/ytmtui/cookies.txt"
mkdir -p "$(dirname "$DEST")"

if ! command -v yt-dlp >/dev/null; then
  echo "Error: yt-dlp was not found in PATH." >&2
  exit 1
fi

BROWSER="${1:-brave}"
TEMP="$(mktemp "${DEST}.tmp.XXXXXX")"
trap 'rm -f "$TEMP"' EXIT

echo "Exporting cookies from $BROWSER..."
yt-dlp --cookies-from-browser "$BROWSER" \
  --cookies "$TEMP" \
  --skip-download --no-warnings \
  -O '%(title)s' \
  'https://www.youtube.com/watch?v=jNQXAC9IVRw'

if [[ ! -s "$TEMP" ]]; then
  echo "Error: yt-dlp produced an empty cookie file." >&2
  exit 1
fi

if [[ -f "$DEST" ]]; then
  BACKUP="${DEST}.bak.$(date +%s)"
  cp -p "$DEST" "$BACKUP"
  echo "Backup: $BACKUP"
fi

chmod 600 "$TEMP"
mv -f "$TEMP" "$DEST"
trap - EXIT
echo "Saved: $DEST ($(stat -c %s "$DEST") bytes)"
echo "Restart ytmtui to use the refreshed session."
```

- [ ] **Step 4: Add the shell test to CI**

Add this step after checkout in the existing `test` job:

```yaml
      - name: Test maintenance scripts
        run: bash scripts/test-refresh-cookies.sh
```

- [ ] **Step 5: Run shell and Rust tests**

Run:

```bash
bash scripts/test-refresh-cookies.sh
cargo test --all-targets --all-features
```

Expected: shell output ends with `refresh-cookies tests passed`; all Rust tests pass.

- [ ] **Step 6: Commit the secure refresh flow**

```bash
git add scripts/refresh-cookies.sh scripts/test-refresh-cookies.sh .github/workflows/ci.yml
git commit -m "fix: refresh browser cookies atomically"
```

### Task 7: Run the stage gate and record authentication behavior

**Files:**
- Modify: `README.md` authentication and troubleshooting sections
- Modify: `docs/ARCHITECTURE.md` authentication and error-flow sections
- Modify: `CHANGELOG.md` unreleased fixes

- [ ] **Step 1: Update touched authentication documentation in English**

Document these exact facts in the three files:

```text
- Cookie precedence is YTM_COOKIES, configured path, then ~/.config/ytmtui/cookies.txt.
- Invalid cookie files fall back to anonymous mode.
- HTTP 401/403 from authenticated endpoints marks the session expired without disabling public search.
- scripts/refresh-cookies.sh performs an atomic replacement, creates mode-600 output, and preserves the old file on failure.
```

Do not translate unrelated sections in this stage; full-project translation belongs to Stage 4.

- [ ] **Step 2: Run formatting**

Run:

```bash
cargo fmt --all --check
```

Expected: PASS with no diff. If it fails, run `cargo fmt --all`, inspect the changes, and rerun the check.

- [ ] **Step 3: Run Clippy as an error gate**

Run:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: PASS with no warnings.

- [ ] **Step 4: Run complete automated validation**

Run:

```bash
bash scripts/test-refresh-cookies.sh
cargo test --all-targets --all-features
```

Expected: shell regression passes and every Rust test passes.

- [ ] **Step 5: Run a release build**

Run:

```bash
cargo build --release
```

Expected: `target/release/ytmtui` is produced successfully.

- [ ] **Step 6: Commit Stage 1 documentation**

```bash
git add README.md docs/ARCHITECTURE.md CHANGELOG.md
git commit -m "docs: explain authentication recovery"
```

- [ ] **Step 7: Confirm the stage boundary**

Run:

```bash
git status --short
git log -7 --oneline
```

Expected: only the pre-existing `src/main.rs` refresh-rate change may remain unstaged; the log shows focused authentication commits and the earlier design/plan commits.
