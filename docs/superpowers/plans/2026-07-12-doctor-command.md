# Doctor Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only `ytmtui doctor` command that diagnoses dependencies, browser discovery, cookie safety, connectivity, and the configured YouTube Music account without entering the TUI or exposing credentials.

**Architecture:** A new `doctor` module owns structured checks, sanitization, exit status, and plain-text rendering. It reuses `Config`, typed browser discovery, cookie parsing, and account-aware `YtMusicClient` from the safe-authentication plan. `main.rs` dispatches `doctor` before raw mode, audio, artwork, or MPRIS.

**Tech Stack:** Rust 2021, Tokio, reqwest, serde, standard `Command`, existing authentication modules.

## Global Constraints

- Complete `2026-07-12-safe-authentication.md` first.
- Doctor never imports, refreshes, replaces, or uploads cookies.
- Never print credential/header/hash/API-body values, unrestricted temp paths, or raw process stderr.
- Exit `0` when required checks pass despite optional warnings; exit `1` on any required failure.
- Dispatch before terminal raw mode, alternate screen, audio, artwork, or MPRIS.
- Do not add clap or a general diagnostics plugin framework.
- Keep English/Portuguese docs synchronized and use red-green-refactor.

## File Map

- Create `src/doctor.rs`: report types, sanitizer, collectors, renderer, test seam.
- Modify `src/lib.rs` and `src/main.rs`: export and early CLI dispatch.
- Modify `src/config.rs`: expose resolved config path read-only.
- Modify `src/ytmusic/auth.rs`: safe cookie validity probe.
- Modify `src/ytmusic/signin.rs`: crate-visible candidate display data.
- Modify `src/ytmusic/mod.rs`: account query reuse and connectivity probe.
- Modify docs, changelog, README, and CI smoke coverage.

---

### Task 1: Structured reports, sanitization, and exit semantics

**Files:** Create `src/doctor.rs`; modify `src/lib.rs`.

**Interfaces:** Produces `Severity`, `Check`, `Report`, `Report::exit_code`, `Report::render`, and `sanitize_detail`.

- [ ] **Step 1: Write failing report tests**

```rust
#[test]
fn warnings_do_not_fail_but_required_failures_do() {
    let warnings = Report::new(vec![Check::warning("Runtime", "deno", "optional")]);
    assert_eq!(warnings.exit_code(), 0);
    let failed = Report::new(vec![Check::failure("Runtime", "ffmpeg", "missing", "install ffmpeg")]);
    assert_eq!(failed.exit_code(), 1);
}

#[test]
fn rendering_redacts_credentials_and_home_paths() {
    let source = "SAPISID=secret; Authorization: SAPISIDHASH 1_deadbeef /home/alice/profile";
    let rendered = sanitize_detail(source, Some(Path::new("/home/alice")));
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains("deadbeef"));
    assert!(!rendered.contains("/home/alice"));
    assert!(rendered.contains("$HOME"));
}

#[test]
fn report_groups_sections_and_prints_summary() {
    let report = Report::new(vec![
        Check::ok("Runtime", "yt-dlp", "2026.07.04"),
        Check::warning("Runtime", "deno", "not found"),
    ]);
    let text = report.render();
    assert!(text.contains("[ok] yt-dlp"));
    assert!(text.contains("Summary: 1 passed, 1 warning, 0 failed"));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test doctor::tests`

Expected: compile failure for missing report APIs.

- [ ] **Step 3: Implement exact model**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity { Ok, Warning, Failure }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub section: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Report { pub checks: Vec<Check> }
```

`render` groups adjacent section names, renders `[ok]`, `[warn]`, `[fail]`, indents hints, and counts severities. `exit_code` is one iff any failure exists. Sanitization replaces the home prefix with `$HOME`, replaces values following case-insensitive `SAPISID=`, `Authorization:`, and `Cookie:` markers with `[redacted]`, and collapses newlines. Use synthetic secrets only in tests. Export `pub mod doctor` from `lib.rs`.

- [ ] **Step 4: Verify GREEN and commit**

Run: `cargo test doctor::tests`

```bash
git add src/doctor.rs src/lib.rs
git commit -m "feat: add structured diagnostic reports"
```

---

### Task 2: Collect runtime, authentication, browser, and connectivity checks

**Files:** Modify `src/doctor.rs`, `src/config.rs`, `src/ytmusic/auth.rs`, `src/ytmusic/signin.rs`, `src/ytmusic/mod.rs`, `src/main.rs`.

**Interfaces:** Produces `doctor::run() -> Report`, `Config::path`, and safe diagnostic probes.

- [ ] **Step 1: Write failing collector tests behind an injected backend**

```rust
trait DoctorBackend: Send + Sync {
    fn tool_version(&self, command: &str) -> Result<String, String>;
    fn browser_candidates(&self) -> Vec<BrowserCandidate>;
    fn cookie_metadata(&self, path: &Path) -> Result<CookieMetadata, String>;
    fn configured_account(&self, path: &Path, auth_user: u8) -> Result<Option<String>, String>;
    fn connectivity(&self) -> Result<(), String>;
}

#[derive(Debug, Clone)]
struct CookieMetadata {
    exists: bool,
    is_file: bool,
    len: u64,
    mode: Option<u32>,
    valid: bool,
}

struct FakeDoctorBackend { failed: bool }

impl FakeDoctorBackend {
    fn healthy() -> Self { Self { failed: false } }
    fn failed() -> Self { Self { failed: true } }
}

impl DoctorBackend for FakeDoctorBackend {
    fn tool_version(&self, command: &str) -> Result<String, String> {
        if self.failed && command == "ffmpeg" { Err("not found".into()) } else { Ok("test-version".into()) }
    }
    fn browser_candidates(&self) -> Vec<BrowserCandidate> {
        vec![
            BrowserCandidate {
                method: "firefox".into(),
                profile_path: None,
                profile_label: Some("default-release".into()),
            },
            BrowserCandidate::chromium("brave"),
        ]
    }
    fn cookie_metadata(&self, _path: &Path) -> Result<CookieMetadata, String> {
        if self.failed { Err("missing SAPISID".into()) } else {
            Ok(CookieMetadata { exists: true, is_file: true, len: 100, mode: Some(0o600), valid: true })
        }
    }
    fn configured_account(&self, _path: &Path, _auth_user: u8) -> Result<Option<String>, String> {
        Ok(Some("Thiago Santos".into()))
    }
    fn connectivity(&self) -> Result<(), String> { Ok(()) }
}

#[test]
fn healthy_report_names_firefox_before_brave_and_account() {
    let report = collect_with_backend(&FakeDoctorBackend::healthy(), &Config::default(), Some(Path::new("/tmp/cookies")));
    let text = report.render();
    assert!(text.find("Firefox / default-release").unwrap() < text.find("Brave / Default").unwrap());
    assert!(text.contains("Thiago Santos"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn missing_required_tool_and_invalid_cookie_fail() {
    let report = collect_with_backend(&FakeDoctorBackend::failed(), &Config::default(), Some(Path::new("/tmp/cookies")));
    assert_eq!(report.exit_code(), 1);
    assert!(report.render().contains("install ffmpeg"));
    assert!(report.render().contains("press g to sign in again"));
}

#[test]
fn anonymous_authentication_is_a_warning() {
    let report = collect_with_backend(&FakeDoctorBackend::healthy(), &Config::default(), None);
    assert_eq!(report.exit_code(), 0);
    assert!(report.render().contains("no configured session"));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test doctor::tests::healthy_report && cargo test doctor::tests::missing_required && cargo test doctor::tests::anonymous_authentication`

Expected: compile failure for missing collector/backend.

- [ ] **Step 3: Implement runtime and local checks**

`SystemDoctorBackend::tool_version` runs `--version`, consumes only the first sanitized line, and treats `yt-dlp`/`ffmpeg` as required and `deno` as optional. `Config::path()` exposes `config_path()`. `Auth::validate_cookie_file(&Path)` calls existing parsing and discards secrets.

Cookie metadata checks existence, regular file, nonzero size, parse validity, and Unix mode. Mode other than `0600` warns with a `chmod 600` hint; invalid authentication cookies fail. Browser detection shows method/profile labels only and never exports.

- [ ] **Step 4: Implement connectivity and account checks**

Use a reqwest client with a 10-second timeout for `https://music.youtube.com/`. When configured cookies exist, construct `YtMusicClient::with_cookies_for_account(path, config.authentication.auth_user)` and call `get_account_identity`; show name and index only. Network/HTTP failures are typed required failures when a configured authenticated session is being checked. Anonymous lack of account is a warning.

Blocking process/filesystem checks run in `spawn_blocking`; network checks await normally. `collect_with_backend` stays synchronous and deterministic for unit tests.

- [ ] **Step 5: Dispatch before terminal setup**

At the start of async `main`, before the panic hook/setup:

```rust
let mut args = std::env::args_os();
let _program = args.next();
if matches!(args.next().as_deref(), Some(value) if value == OsStr::new("doctor")) {
    let report = ytmtui::doctor::run().await;
    print!("{}", report.render());
    std::process::exit(report.exit_code());
}
```

This branch must not construct `App`, `AudioPlayer`, `Picker`, or `Mpris`.

- [ ] **Step 6: Verify and commit**

Run: `cargo test doctor::tests && cargo run -- doctor`

Expected: tests pass; local output has Runtime, Authentication, Connectivity, Summary and no `SAPISID=`, `SAPISIDHASH`, `Cookie:`, or raw home path.

```bash
git add src/doctor.rs src/config.rs src/ytmusic/auth.rs src/ytmusic/signin.rs src/ytmusic/mod.rs src/main.rs
git commit -m "feat: add ytmtui doctor command"
```

---

### Task 3: Document doctor, add CI smoke coverage, and pass gates

**Files:** Modify both READMEs, both getting-started/troubleshooting/authentication docs, architecture, changelog, `.github/workflows/ci.yml`.

- [ ] **Step 1: Add non-TTY CI smoke check**

After Rust tests, run doctor and assert section headings and absence of credential markers:

```yaml
- name: Doctor command starts without a TTY
  run: |
    output="$(cargo run --quiet -- doctor || true)"
    grep -q '^Runtime' <<<"$output"
    grep -q '^Authentication' <<<"$output"
    grep -q '^Connectivity' <<<"$output"
    grep -q '^Summary:' <<<"$output"
    ! grep -Eq 'SAPISID=|SAPISIDHASH|Cookie:' <<<"$output"
```

- [ ] **Step 2: Document English behavior**

State that `ytmtui doctor` checks tools, browsers, cookie permissions, connectivity, and configured account; it does not refresh/replace cookies; output should still be reviewed before sharing; exit zero means no required failure and one means a required failure.

- [ ] **Step 3: Mirror Portuguese behavior**

Keep claims identical and use `Execute ytmtui doctor fora da TUI` as the troubleshooting entry point.

- [ ] **Step 4: Run full gates**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
bash scripts/test-refresh-cookies.sh
cargo run --quiet -- doctor
git diff --check
```

Expected: format/lint/tests/build/script succeed; doctor prints four sections and no credential marker. Doctor may exit one only for an actual required environmental failure, which must remain visible.

- [ ] **Step 5: Commit**

```bash
git add README.md README.pt-BR.md CHANGELOG.md docs/GETTING_STARTED.md docs/GETTING_STARTED.pt-BR.md docs/TROUBLESHOOTING.md docs/TROUBLESHOOTING.pt-BR.md docs/AUTHENTICATION.md docs/AUTHENTICATION.pt-BR.md docs/ARCHITECTURE.md .github/workflows/ci.yml
git commit -m "docs: add doctor troubleshooting workflow"
```
