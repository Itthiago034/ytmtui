//! Structured, plain-text diagnostic reports.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::config::{AuthenticationConfig, Config};
use crate::ytmusic::auth::Auth;
use crate::ytmusic::{
    detect_browser_candidates, resolve_cookie_path, BrowserCandidate, CookiePathResolution,
    YtMusicClient,
};

const RUNTIME: &str = "Runtime";
const AUTHENTICATION: &str = "Authentication";
const CONNECTIVITY: &str = "Connectivity";

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

#[derive(Debug, Clone)]
struct SystemDoctorBackend {
    home: Option<PathBuf>,
    authentication: AuthenticationConfig,
    account: Result<Option<String>, String>,
    connectivity: Result<(), String>,
}

impl DoctorBackend for SystemDoctorBackend {
    fn tool_version(&self, command: &str) -> Result<String, String> {
        for flag in version_flags(command) {
            let output = Command::new(command)
                .arg(flag)
                .output()
                .map_err(|_| "not found".to_string())?;
            if !output.status.success() {
                continue;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout
                .lines()
                .find(|line| !line.trim().is_empty())
                .ok_or_else(|| "version unavailable".to_string())?;
            return Ok(sanitize_external(line, self.home.as_deref()));
        }
        Err("version check failed".into())
    }

    fn browser_candidates(&self) -> Vec<BrowserCandidate> {
        self.home
            .as_deref()
            .map(|home| detect_browser_candidates(home, &self.authentication))
            .unwrap_or_default()
    }

    fn cookie_metadata(&self, path: &Path) -> Result<CookieMetadata, String> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CookieMetadata {
                    exists: false,
                    is_file: false,
                    len: 0,
                    mode: None,
                    valid: false,
                });
            }
            Err(_) => return Err("metadata unavailable".into()),
        };
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            Some(metadata.permissions().mode() & 0o777)
        };
        #[cfg(not(unix))]
        let mode = None;

        Ok(CookieMetadata {
            exists: true,
            is_file: metadata.is_file(),
            len: metadata.len(),
            mode,
            valid: metadata.is_file() && Auth::validate_cookie_file(path).is_ok(),
        })
    }

    fn configured_account(&self, _path: &Path, _auth_user: u8) -> Result<Option<String>, String> {
        self.account.clone()
    }

    fn connectivity(&self) -> Result<(), String> {
        self.connectivity.clone()
    }
}

fn version_flags(command: &str) -> &'static [&'static str] {
    if command == "ffmpeg" {
        &["--version", "-version"]
    } else {
        &["--version"]
    }
}

/// Collects diagnostics without entering terminal mode or constructing app services.
pub async fn run() -> Report {
    let initial = tokio::task::spawn_blocking(|| {
        let config = Config::load();
        let resolution = diagnostic_cookie_path(
            std::env::var("YTM_COOKIES").ok(),
            config.cookies.clone(),
            Config::path().and_then(|path| path.parent().map(|parent| parent.join("cookies.txt"))),
        );
        let cookie_path = resolution.path.map(PathBuf::from);
        let missing_requested_path = resolution.missing_requested_path.is_some();
        let cookie_is_file = cookie_path.as_deref().is_some_and(is_regular_file);
        (
            config,
            cookie_path,
            missing_requested_path,
            cookie_is_file,
            dirs::home_dir(),
        )
    })
    .await;
    let Ok((config, cookie_path, missing_requested_path, cookie_is_file, home)) = initial else {
        return Report::new(vec![Check::failure(
            RUNTIME,
            "diagnostic worker",
            "local checks could not start",
            "try running ytmtui doctor again",
        )]);
    };

    let connectivity = probe_connectivity().await;
    let account = if cookie_is_file {
        probe_account(
            cookie_path
                .clone()
                .expect("cookie_is_file requires a cookie path"),
            config.authentication.auth_user,
        )
        .await
    } else {
        Ok(None)
    };
    let backend = SystemDoctorBackend {
        home,
        authentication: config.authentication.clone(),
        account,
        connectivity,
    };

    tokio::task::spawn_blocking(move || {
        collect_with_backend(
            &backend,
            &config,
            cookie_path.as_deref(),
            missing_requested_path,
        )
    })
    .await
    .unwrap_or_else(|_| {
        Report::new(vec![Check::failure(
            RUNTIME,
            "diagnostic worker",
            "local checks did not finish",
            "try running ytmtui doctor again",
        )])
    })
}

fn diagnostic_cookie_path(
    environment: Option<String>,
    configured: Option<String>,
    default: Option<PathBuf>,
) -> CookiePathResolution {
    resolve_cookie_path(environment, configured, default)
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

async fn probe_connectivity() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "HTTP client unavailable".to_string())?;
    let response = client
        .get("https://music.youtube.com/")
        .send()
        .await
        .map_err(|_| "YouTube Music is unreachable".to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err("YouTube Music returned an unsuccessful status".into())
    }
}

async fn probe_account(path: PathBuf, auth_user: u8) -> Result<Option<String>, String> {
    let client = tokio::task::spawn_blocking(move || {
        let path = path
            .to_str()
            .ok_or_else(|| "configured session path is unsupported".to_string())?;
        YtMusicClient::with_cookies_for_account(path, auth_user)
            .map_err(|_| "configured session is invalid".to_string())
    })
    .await
    .map_err(|_| "account check could not start".to_string())??;

    client
        .get_account_identity()
        .await
        .map(|identity| identity.map(|identity| identity.name))
        .map_err(|_| "configured account request failed".to_string())
}

fn collect_with_backend<B: DoctorBackend + ?Sized>(
    backend: &B,
    config: &Config,
    cookie_path: Option<&Path>,
    missing_requested_path: bool,
) -> Report {
    let mut checks = Vec::new();
    for (command, required) in [("yt-dlp", true), ("ffmpeg", true), ("deno", false)] {
        match backend.tool_version(command) {
            Ok(version) => checks.push(Check::ok(
                RUNTIME,
                command,
                sanitize_external(&version, dirs::home_dir().as_deref()),
            )),
            Err(_) if required => checks.push(Check::failure(
                RUNTIME,
                command,
                "not available",
                format!("install {command}"),
            )),
            Err(_) => checks.push(Check::warning(
                RUNTIME,
                command,
                "optional runtime not available",
            )),
        }
    }

    let candidates = backend.browser_candidates();
    if candidates.is_empty() {
        checks.push(Check::warning(
            AUTHENTICATION,
            "browser sessions",
            "no supported browser cookie store detected",
        ));
    } else {
        for candidate in candidates {
            checks.push(Check::ok(
                AUTHENTICATION,
                "browser session",
                browser_label(&candidate),
            ));
        }
    }

    if missing_requested_path {
        checks.push(Check::warning(
            AUTHENTICATION,
            "requested cookie file",
            if cookie_path.is_some() {
                "missing; using an available fallback"
            } else {
                "missing; no fallback session was found"
            },
        ));
    }

    match cookie_path {
        None => checks.push(Check::warning(
            AUTHENTICATION,
            "configured session",
            "no configured session",
        )),
        Some(path) => match backend.cookie_metadata(path) {
            Err(_) => checks.push(invalid_cookie_check("cookie metadata is unavailable")),
            Ok(metadata) if !metadata.exists => {
                checks.push(invalid_cookie_check("configured cookie file is missing"));
            }
            Ok(metadata) if !metadata.is_file => {
                checks.push(invalid_cookie_check("configured cookie path is not a file"));
            }
            Ok(metadata) if metadata.len == 0 => {
                checks.push(invalid_cookie_check("configured cookie file is empty"));
            }
            Ok(metadata) if !metadata.valid => {
                checks.push(invalid_cookie_check(
                    "configured authentication cookies are invalid",
                ));
            }
            Ok(metadata) => {
                match metadata.mode {
                    Some(0o600) => checks.push(Check::ok(
                        AUTHENTICATION,
                        "cookie file",
                        "valid and private (mode 0600)",
                    )),
                    Some(_) => checks.push(Check::new(
                        AUTHENTICATION,
                        Severity::Warning,
                        "cookie file",
                        "valid but permissions are not 0600",
                        Some("run chmod 600 on the configured cookie file".into()),
                    )),
                    None => checks.push(Check::ok(
                        AUTHENTICATION,
                        "cookie file",
                        "valid cookie file",
                    )),
                }

                match backend.configured_account(path, config.authentication.auth_user) {
                    Ok(Some(name)) => checks.push(Check::ok(
                        AUTHENTICATION,
                        "configured account",
                        format!(
                            "{} (account index {})",
                            sanitize_external(&name, dirs::home_dir().as_deref()),
                            config.authentication.auth_user
                        ),
                    )),
                    Ok(None) => checks.push(Check::failure(
                        AUTHENTICATION,
                        "configured account",
                        "no account identity was returned",
                        "press g to sign in again",
                    )),
                    Err(_) => checks.push(Check::failure(
                        AUTHENTICATION,
                        "configured account",
                        "account check failed",
                        "press g to sign in again",
                    )),
                }
            }
        },
    }

    match backend.connectivity() {
        Ok(()) => checks.push(Check::ok(
            CONNECTIVITY,
            "YouTube Music",
            "reachable over HTTPS",
        )),
        Err(_) => checks.push(Check::failure(
            CONNECTIVITY,
            "YouTube Music",
            "connectivity check failed",
            "check your network connection and try again",
        )),
    }

    Report::new(checks)
}

fn invalid_cookie_check(detail: &'static str) -> Check {
    Check::failure(
        AUTHENTICATION,
        "cookie file",
        detail,
        "press g to sign in again",
    )
}

fn browser_label(candidate: &BrowserCandidate) -> String {
    let method = match candidate.method.as_str() {
        "firefox" => "Firefox".to_string(),
        "brave" => "Brave".to_string(),
        "chrome" => "Chrome".to_string(),
        "chromium" => "Chromium".to_string(),
        "edge" => "Edge".to_string(),
        "vivaldi" => "Vivaldi".to_string(),
        "opera" => "Opera".to_string(),
        other => sanitize_external(other, dirs::home_dir().as_deref()),
    };
    let profile = candidate
        .profile_label
        .as_deref()
        .map(|label| sanitize_external(label, dirs::home_dir().as_deref()))
        .unwrap_or_else(|| {
            if candidate.method == "firefox" {
                "default".into()
            } else {
                "Default".into()
            }
        });
    format!("{method} / {profile}")
}

fn sanitize_external(detail: &str, home: Option<&Path>) -> String {
    let sanitized = sanitize_detail(detail, home);
    let lowercase = sanitized.to_ascii_lowercase();
    if ["sapisid=", "sapisidhash", "authorization:", "cookie:"]
        .iter()
        .any(|marker| lowercase.contains(marker))
    {
        "diagnostic detail omitted".into()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warning,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub section: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: Option<String>,
}

impl Check {
    pub fn ok(section: &'static str, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(section, Severity::Ok, title, detail, None)
    }

    pub fn warning(
        section: &'static str,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(section, Severity::Warning, title, detail, None)
    }

    pub fn failure(
        section: &'static str,
        title: impl Into<String>,
        detail: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self::new(section, Severity::Failure, title, detail, Some(hint.into()))
    }

    fn new(
        section: &'static str,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self {
            section,
            severity,
            title: title.into(),
            detail: detail.into(),
            hint,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn new(checks: Vec<Check>) -> Self {
        Self { checks }
    }

    pub fn exit_code(&self) -> i32 {
        i32::from(
            self.checks
                .iter()
                .any(|check| check.severity == Severity::Failure),
        )
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        let mut previous_section = None;
        let mut passed = 0;
        let mut warnings = 0;
        let mut failed = 0;

        for check in &self.checks {
            if previous_section != Some(check.section) {
                if previous_section.is_some() {
                    output.push('\n');
                }
                writeln!(output, "{}", check.section).expect("writing to a String cannot fail");
                previous_section = Some(check.section);
            }

            let label = match check.severity {
                Severity::Ok => {
                    passed += 1;
                    "ok"
                }
                Severity::Warning => {
                    warnings += 1;
                    "warn"
                }
                Severity::Failure => {
                    failed += 1;
                    "fail"
                }
            };
            writeln!(output, "  [{label}] {}: {}", check.title, check.detail)
                .expect("writing to a String cannot fail");

            if let Some(hint) = &check.hint {
                writeln!(output, "    Hint: {hint}").expect("writing to a String cannot fail");
            }
        }

        if !self.checks.is_empty() {
            output.push('\n');
        }
        writeln!(
            output,
            "Summary: {passed} passed, {warnings} warning{}, {failed} failed",
            if warnings == 1 { "" } else { "s" }
        )
        .expect("writing to a String cannot fail");
        output
    }
}

pub fn sanitize_detail(detail: &str, home: Option<&Path>) -> String {
    let home = home.and_then(Path::to_str).filter(|home| !home.is_empty());

    let sanitized = detail
        .split(['\r', '\n'])
        .filter(|line| !line.is_empty())
        .map(|line| sanitize_line(line, home))
        .collect::<Vec<_>>()
        .join(" ");

    match home {
        Some(home) => sanitized.replace(home, "$HOME"),
        None => sanitized,
    }
}

#[derive(Clone, Copy)]
enum CredentialMarker {
    Sapisid,
    Authorization,
    Cookie,
}

impl CredentialMarker {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Sapisid, "SAPISID="),
        (Self::Authorization, "Authorization:"),
        (Self::Cookie, "Cookie:"),
    ];
}

fn sanitize_line(line: &str, home: Option<&str>) -> String {
    let mut output = String::with_capacity(line.len());
    let mut remaining = line;

    while let Some((offset, marker, marker_text)) = next_marker(remaining) {
        output.push_str(&remaining[..offset + marker_text.len()]);
        remaining = &remaining[offset + marker_text.len()..];

        match marker {
            CredentialMarker::Sapisid => {
                let value_end = remaining.find(';').unwrap_or(remaining.len());
                output.push_str("[redacted]");
                remaining = &remaining[value_end..];
            }
            CredentialMarker::Authorization | CredentialMarker::Cookie => {
                let value_start = remaining.len() - remaining.trim_start().len();
                output.push(' ');
                output.push_str("[redacted]");
                let value = &remaining[value_start..];
                if let Some(home_token) = home.and_then(|home| actual_home_path_token(value, home))
                {
                    output.push(' ');
                    output.push_str(home_token);
                }
                remaining = "";
            }
        }
    }

    output.push_str(remaining);
    output
}

fn actual_home_path_token<'a>(value: &'a str, home: &str) -> Option<&'a str> {
    value.match_indices(home).find_map(|(offset, _)| {
        let starts_token = offset == 0
            || value[..offset]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let suffix = &value[offset + home.len()..];
        let extends_home_path = suffix.is_empty() || home.ends_with('/') || suffix.starts_with('/');
        if !starts_token || !extends_home_path {
            return None;
        }

        let suffix_end = suffix
            .find(|character: char| character == ';' || character.is_whitespace())
            .unwrap_or(suffix.len());
        if next_marker(&suffix[..suffix_end]).is_some() {
            return None;
        }

        Some(&value[offset..offset + home.len() + suffix_end])
    })
}

fn next_marker(input: &str) -> Option<(usize, CredentialMarker, &'static str)> {
    CredentialMarker::ALL
        .iter()
        .filter_map(|(kind, marker)| {
            find_ascii_case_insensitive(input, marker).map(|offset| (offset, *kind, *marker))
        })
        .min_by_key(|(offset, _, _)| *offset)
}

fn find_ascii_case_insensitive(input: &str, needle: &str) -> Option<usize> {
    input
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::ytmusic::BrowserCandidate;
    use std::path::Path;

    fn system_backend_for_test() -> SystemDoctorBackend {
        SystemDoctorBackend {
            home: None,
            authentication: AuthenticationConfig::default(),
            account: Ok(None),
            connectivity: Ok(()),
        }
    }

    #[test]
    fn missing_environment_cookie_uses_valid_configured_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing-environment.txt");
        let configured = directory.path().join("configured.txt");
        let default = directory.path().join("default.txt");
        std::fs::write(&configured, "configured").unwrap();
        std::fs::write(&default, "default").unwrap();

        let resolution = diagnostic_cookie_path(
            Some(missing.to_string_lossy().into_owned()),
            Some(configured.to_string_lossy().into_owned()),
            Some(default),
        );

        assert_eq!(
            resolution.path.as_deref(),
            Some(configured.to_string_lossy().as_ref())
        );
        assert_eq!(
            resolution.missing_requested_path.as_deref(),
            Some(missing.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn missing_configured_cookie_uses_valid_default_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing-configured.txt");
        let default = directory.path().join("default.txt");
        std::fs::write(&default, "default").unwrap();

        let resolution = diagnostic_cookie_path(
            None,
            Some(missing.to_string_lossy().into_owned()),
            Some(default.clone()),
        );

        assert_eq!(
            resolution.path.as_deref(),
            Some(default.to_string_lossy().as_ref())
        );
        assert_eq!(
            resolution.missing_requested_path.as_deref(),
            Some(missing.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn ffmpeg_falls_back_to_its_supported_version_flag() {
        assert_eq!(version_flags("ffmpeg"), &["--version", "-version"]);
        assert_eq!(version_flags("yt-dlp"), &["--version"]);
        assert_eq!(version_flags("deno"), &["--version"]);
    }

    #[cfg(unix)]
    #[test]
    fn cookie_metadata_rejects_a_symlink_as_not_a_regular_file() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("cookies.txt");
        let link = directory.path().join("cookies-link.txt");
        std::fs::write(
            &target,
            ".youtube.com\tTRUE\t/\tTRUE\t9999999999\tSAPISID\tsynthetic\n",
        )
        .unwrap();
        symlink(&target, &link).unwrap();

        let metadata = system_backend_for_test().cookie_metadata(&link).unwrap();

        assert!(!metadata.is_file);
        assert!(!metadata.valid);
        assert!(!is_regular_file(&link));
    }

    struct FakeDoctorBackend {
        failed: bool,
        mode: Option<u32>,
    }

    impl FakeDoctorBackend {
        fn healthy() -> Self {
            Self {
                failed: false,
                mode: Some(0o600),
            }
        }

        fn failed() -> Self {
            Self {
                failed: true,
                mode: Some(0o600),
            }
        }

        fn nonstandard_permissions() -> Self {
            Self {
                failed: false,
                mode: Some(0o400),
            }
        }
    }

    impl DoctorBackend for FakeDoctorBackend {
        fn tool_version(&self, command: &str) -> Result<String, String> {
            if self.failed && command == "ffmpeg" {
                Err("not found".into())
            } else {
                Ok("test-version".into())
            }
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
            if self.failed {
                Err("missing SAPISID".into())
            } else {
                Ok(CookieMetadata {
                    exists: true,
                    is_file: true,
                    len: 100,
                    mode: self.mode,
                    valid: true,
                })
            }
        }

        fn configured_account(
            &self,
            _path: &Path,
            _auth_user: u8,
        ) -> Result<Option<String>, String> {
            Ok(Some("Thiago Santos".into()))
        }

        fn connectivity(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn healthy_report_names_firefox_before_brave_and_account() {
        let report = collect_with_backend(
            &FakeDoctorBackend::healthy(),
            &Config::default(),
            Some(Path::new("/tmp/cookies")),
            false,
        );
        let text = report.render();
        assert!(
            text.find("Firefox / default-release").unwrap() < text.find("Brave / Default").unwrap()
        );
        assert!(text.contains("Thiago Santos"));
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn missing_required_tool_and_invalid_cookie_fail() {
        let report = collect_with_backend(
            &FakeDoctorBackend::failed(),
            &Config::default(),
            Some(Path::new("/tmp/cookies")),
            false,
        );
        assert_eq!(report.exit_code(), 1);
        assert!(report.render().contains("install ffmpeg"));
        assert!(report.render().contains("press g to sign in again"));
    }

    #[test]
    fn anonymous_authentication_is_a_warning() {
        let report = collect_with_backend(
            &FakeDoctorBackend::healthy(),
            &Config::default(),
            None,
            false,
        );
        assert_eq!(report.exit_code(), 0);
        assert!(report.render().contains("no configured session"));
    }

    #[test]
    fn missing_requested_cookie_is_reported_separately_from_the_selected_fallback() {
        let report = collect_with_backend(
            &FakeDoctorBackend::healthy(),
            &Config::default(),
            Some(Path::new("/tmp/selected-fallback")),
            true,
        );
        let text = report.render();

        assert_eq!(report.exit_code(), 0);
        assert!(text.contains("requested cookie file: missing; using an available fallback"));
        assert!(!text.contains("/tmp/selected-fallback"));
    }

    #[test]
    fn non_0600_cookie_permissions_warn_with_a_private_mode_hint() {
        let report = collect_with_backend(
            &FakeDoctorBackend::nonstandard_permissions(),
            &Config::default(),
            Some(Path::new("/tmp/cookies")),
            false,
        );

        assert_eq!(report.exit_code(), 0);
        let text = report.render();
        assert!(text.contains("permissions are not 0600"));
        assert!(!text.contains("broader"));
        assert!(text.contains("chmod 600"));
    }

    struct CredentialDetailBackend;

    impl DoctorBackend for CredentialDetailBackend {
        fn tool_version(&self, _command: &str) -> Result<String, String> {
            Ok("SAPISID=synthetic-tool-secret; /home/alice/tool".into())
        }

        fn browser_candidates(&self) -> Vec<BrowserCandidate> {
            vec![BrowserCandidate {
                method: "firefox".into(),
                profile_path: None,
                profile_label: Some("Cookie: synthetic-browser-secret".into()),
            }]
        }

        fn cookie_metadata(&self, _path: &Path) -> Result<CookieMetadata, String> {
            Ok(CookieMetadata {
                exists: true,
                is_file: true,
                len: 100,
                mode: Some(0o600),
                valid: true,
            })
        }

        fn configured_account(
            &self,
            _path: &Path,
            _auth_user: u8,
        ) -> Result<Option<String>, String> {
            Ok(Some(
                "Authorization: SAPISIDHASH synthetic-account-secret".into(),
            ))
        }

        fn connectivity(&self) -> Result<(), String> {
            Err("Cookie: synthetic-network-secret".into())
        }
    }

    #[test]
    fn collector_never_renders_external_credential_details() {
        let report = collect_with_backend(
            &CredentialDetailBackend,
            &Config::default(),
            Some(Path::new("/tmp/cookies")),
            false,
        );
        let text = report.render();
        let lowercase = text.to_ascii_lowercase();

        assert!(!lowercase.contains("sapisid="));
        assert!(!lowercase.contains("sapisidhash"));
        assert!(!lowercase.contains("authorization:"));
        assert!(!lowercase.contains("cookie:"));
        assert!(!lowercase.contains("synthetic"));
        assert!(!text.contains("/home/alice"));
    }

    #[test]
    fn warnings_do_not_fail_but_required_failures_do() {
        let warnings = Report::new(vec![Check::warning("Runtime", "deno", "optional")]);
        assert_eq!(warnings.exit_code(), 0);

        let failed = Report::new(vec![Check::failure(
            "Runtime",
            "ffmpeg",
            "missing",
            "install ffmpeg",
        )]);
        assert_eq!(failed.exit_code(), 1);
    }

    #[test]
    fn rendering_redacts_credentials_and_home_paths() {
        let source =
            "SAPISID=synthetic-secret; Authorization: SAPISIDHASH 1_synthetic-hash /home/alice/profile";
        let rendered = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!rendered.contains("synthetic-secret"));
        assert!(!rendered.contains("synthetic-hash"));
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
        assert_eq!(text.matches("Runtime\n").count(), 1);
        assert!(text.contains("Summary: 1 passed, 1 warning, 0 failed"));
    }

    #[test]
    fn sanitization_is_case_insensitive_and_collapses_newlines() {
        let source = concat!(
            "sApIsId=synthetic-cookie-value; safe\r\n",
            "AUTHORIZATION: SAPISIDHASH 1_synthetic-header-hash /home/alice/profile\n",
            "cOoKiE: PREF=synthetic-cookie-header; HSID=synthetic-second-cookie\r",
            "done"
        );

        let sanitized = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!sanitized.contains("synthetic-cookie-value"));
        assert!(!sanitized.contains("synthetic-header-hash"));
        assert!(!sanitized.contains("synthetic-cookie-header"));
        assert!(!sanitized.contains("synthetic-second-cookie"));
        assert!(!sanitized.contains("/home/alice"));
        assert!(sanitized.contains("$HOME/profile"));
        assert!(!sanitized.contains(['\r', '\n']));
        assert!(sanitized.contains("safe"));
        assert!(sanitized.ends_with("done"));
    }

    #[test]
    fn authorization_redacts_every_header_token() {
        let source =
            "Authorization: Digest synthetic-first-token synthetic-second-token\nsafe detail";

        let sanitized = sanitize_detail(source, None);

        assert!(!sanitized.contains("synthetic-first-token"));
        assert!(!sanitized.contains("synthetic-second-token"));
        assert!(sanitized.ends_with("safe detail"));
    }

    #[test]
    fn authorization_preserves_only_the_actual_home_path_token() {
        let source = concat!(
            "Authorization: SAPISIDHASH synthetic-credential ",
            "/home/alice/profile synthetic-second-token"
        );

        let sanitized = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!sanitized.contains("synthetic-credential"));
        assert!(!sanitized.contains("synthetic-second-token"));
        assert!(!sanitized.contains("/home/alice"));
        assert!(sanitized.contains("$HOME/profile"));
    }

    #[test]
    fn literal_home_placeholder_does_not_bypass_authorization_redaction() {
        let source = concat!(
            "Authorization: SAPISIDHASH synthetic-credential ",
            "$HOME/profile synthetic-second-token"
        );

        let sanitized = sanitize_detail(source, None);

        assert!(!sanitized.contains("synthetic-credential"));
        assert!(!sanitized.contains("synthetic-second-token"));
        assert!(!sanitized.contains("$HOME/profile"));
    }

    #[test]
    fn nested_sapisid_marker_invalidates_a_preserved_home_token() {
        let source = concat!(
            "Authorization: Bearer synthetic-outer-token ",
            "/home/alice/SAPISID=synthetic-inner-token"
        );

        let sanitized = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!sanitized.contains("synthetic-outer-token"));
        assert!(!sanitized.contains("synthetic-inner-token"));
        assert!(!sanitized.contains("SAPISID="));
    }

    #[test]
    fn nested_header_marker_invalidates_a_preserved_home_token() {
        let source = concat!(
            "Authorization: Bearer synthetic-outer-token ",
            "/home/alice/cOoKiE:synthetic-inner-token"
        );

        let sanitized = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!sanitized.contains("synthetic-outer-token"));
        assert!(!sanitized.contains("synthetic-inner-token"));
        assert!(!sanitized.to_ascii_lowercase().contains("cookie:"));
    }

    #[test]
    fn sapisid_redaction_continues_through_whitespace_to_cookie_delimiter() {
        let source = "SAPISID=\"synthetic-first synthetic-second\"; safe detail";

        let sanitized = sanitize_detail(source, None);

        assert!(!sanitized.contains("synthetic-first"));
        assert!(!sanitized.contains("synthetic-second"));
        assert!(sanitized.ends_with("; safe detail"));
    }

    #[test]
    fn report_indents_failure_hints() {
        let report = Report::new(vec![Check::failure(
            "Runtime",
            "ffmpeg",
            "missing",
            "install ffmpeg",
        )]);

        assert!(report.render().contains("\n    Hint: install ffmpeg\n"));
    }
}
