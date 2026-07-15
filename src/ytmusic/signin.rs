//! Fluxo de sign-in do YouTube Music: resolução do arquivo de cookies e
//! importação da sessão a partir de um navegador instalado (via yt-dlp).
//! Tudo aqui é específico do YouTube — a UI só vê o contrato genérico de
//! `crate::provider::MusicProvider::sign_in`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::AuthenticationConfig;
use crate::provider::SignInAccount;

const PREPARED_FILE_PREFIX: &str = ".ytmtui-signin-";
const NETSCAPE_COOKIE_HEADER: &str = "# Netscape HTTP Cookie File\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCandidate {
    pub method: String,
    pub profile_path: Option<PathBuf>,
    pub profile_label: Option<String>,
}

impl BrowserCandidate {
    pub fn firefox(profile_path: Option<PathBuf>) -> Self {
        let profile_label = profile_path
            .as_deref()
            .and_then(Path::file_name)
            .map(|label| label.to_string_lossy().into_owned());
        Self {
            method: "firefox".into(),
            profile_path,
            profile_label,
        }
    }

    pub fn chromium(method: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            profile_path: None,
            profile_label: None,
        }
    }

    pub fn yt_dlp_argument(&self) -> String {
        self.profile_path
            .as_deref()
            .map(|profile| format!("{}:{}", self.method, profile.display()))
            .or_else(|| {
                self.profile_label
                    .as_deref()
                    .map(|profile| format!("{}:{profile}", self.method))
            })
            .unwrap_or_else(|| self.method.clone())
    }

    fn progress_label(&self) -> String {
        let browser = match self.method.as_str() {
            "firefox" => "Firefox",
            "brave" => "Brave",
            "chrome" => "Chrome",
            "chromium" => "Chromium",
            "edge" => "Edge",
            "vivaldi" => "Vivaldi",
            "opera" => "Opera",
            _ => "browser",
        };
        self.profile_label
            .as_deref()
            .map(|profile| format!("{browser} / {profile}"))
            .unwrap_or_else(|| browser.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateFailure {
    pub method: String,
    pub profile_label: Option<String>,
    pub reason: String,
}

impl CandidateFailure {
    fn sanitized(candidate: &BrowserCandidate, reason: String) -> Self {
        Self {
            method: candidate.method.clone(),
            profile_label: candidate.profile_label.clone(),
            reason,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedCredentials {
    pub path: PathBuf,
    pub candidate: BrowserCandidate,
    pub accounts: Vec<SignInAccount>,
    pub failures: Vec<CandidateFailure>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum SignInError {
    BrowserNotFound,
    ExportFailed(String),
    NoYouTubeSession,
    NoIdentifiableAccount,
    AccountValidationFailed(String),
    Io(String),
    AllCandidatesFailed(Vec<CandidateFailure>),
}

impl fmt::Debug for SignInError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.sanitized_reason())
    }
}

impl SignInError {
    fn sanitized_reason(&self) -> &'static str {
        match self {
            Self::BrowserNotFound => "browser not found",
            Self::ExportFailed(_) => "browser export failed",
            Self::NoYouTubeSession => "no YouTube session",
            Self::NoIdentifiableAccount => "no identifiable account",
            Self::AccountValidationFailed(_) => "account validation failed",
            Self::Io(_) => "credential file operation failed",
            Self::AllCandidatesFailed(_) => "all browser attempts failed",
        }
    }
}

impl fmt::Display for SignInError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.sanitized_reason())
    }
}

impl std::error::Error for SignInError {}

pub trait SignInBackend: Send + Sync {
    fn export(&self, candidate: &BrowserCandidate, destination: &Path) -> Result<(), SignInError>;
    fn accounts(&self, path: &Path) -> Result<Vec<SignInAccount>, SignInError>;
}

pub fn prepare_with_backend<B>(
    candidates: Vec<BrowserCandidate>,
    config_dir: &Path,
    backend: &B,
    progress: &(dyn Fn(String) + Send + Sync),
) -> Result<PreparedCredentials, SignInError>
where
    B: SignInBackend + ?Sized,
{
    std::fs::create_dir_all(config_dir).map_err(|error| SignInError::Io(error.to_string()))?;
    cleanup_stale_prepared_files(config_dir);
    if candidates.is_empty() {
        return Err(SignInError::BrowserNotFound);
    }

    let mut failures = Vec::new();
    for candidate in candidates {
        let candidate_label = candidate.progress_label();
        progress(format!("Trying {candidate_label}…"));
        let temporary = tempfile::Builder::new()
            .prefix(PREPARED_FILE_PREFIX)
            .tempfile_in(config_dir)
            .map_err(|error| SignInError::Io(error.to_string()))?;
        restrict_permissions(temporary.path())?;
        // `yt-dlp --cookies` treats an existing destination as an input file
        // first. A secure, pre-created but empty tempfile is therefore
        // rejected before browser export. Keep the private tempfile and give
        // it the minimal valid Netscape cookie-jar header it expects.
        std::fs::write(temporary.path(), NETSCAPE_COOKIE_HEADER)
            .map_err(|error| SignInError::Io(error.to_string()))?;

        if let Err(error) = backend.export(&candidate, temporary.path()) {
            let reason = error.sanitized_reason().to_string();
            progress(format!("{candidate_label}: {reason}"));
            failures.push(CandidateFailure::sanitized(&candidate, reason));
            continue;
        }
        restrict_permissions(temporary.path())?;

        let accounts = match backend.accounts(temporary.path()) {
            Ok(accounts) if !accounts.is_empty() => accounts,
            Ok(_) => {
                let error = SignInError::NoIdentifiableAccount;
                let reason = error.sanitized_reason().to_string();
                progress(format!("{candidate_label}: {reason}"));
                failures.push(CandidateFailure::sanitized(&candidate, reason));
                continue;
            }
            Err(error) => {
                let reason = error.sanitized_reason().to_string();
                progress(format!("{candidate_label}: {reason}"));
                failures.push(CandidateFailure::sanitized(&candidate, reason));
                continue;
            }
        };

        let path = temporary
            .into_temp_path()
            .keep()
            .map_err(|error| SignInError::Io(error.error.to_string()))?;
        return Ok(PreparedCredentials {
            path,
            candidate,
            accounts,
            failures,
        });
    }

    Err(SignInError::AllCandidatesFailed(failures))
}

fn restrict_permissions(path: &Path) -> Result<(), SignInError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| SignInError::Io(error.to_string()))?;
    }
    Ok(())
}

fn cleanup_stale_prepared_files(config_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(config_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        if !file_name
            .to_string_lossy()
            .starts_with(PREPARED_FILE_PREFIX)
        {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.file_type().is_file() || !owned_by_current_user(&metadata) {
            continue;
        }
        let is_stale = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(24 * 60 * 60));
        if is_stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(unix)]
fn owned_by_current_user(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    extern "C" {
        fn geteuid() -> u32;
    }

    // SAFETY: `geteuid` takes no arguments and has no preconditions.
    metadata.uid() == unsafe { geteuid() }
}

#[cfg(not(unix))]
fn owned_by_current_user(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookiePathResolution {
    pub path: Option<String>,
    pub missing_requested_path: Option<String>,
}

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

/// Chromium-family browsers the in-app sign-in can import cookies from, in
/// order of preference, with the configuration directory (relative to
/// `$HOME`) that holds their profiles. The names are the exact identifiers
/// `yt-dlp --cookies-from-browser` accepts.
const CHROMIUM_CANDIDATES: &[(&str, &str)] = &[
    ("brave", ".config/BraveSoftware/Brave-Browser"),
    ("chrome", ".config/google-chrome"),
    ("chromium", ".config/chromium"),
    ("edge", ".config/microsoft-edge"),
    ("vivaldi", ".config/vivaldi"),
    ("opera", ".config/opera"),
];

/// Whether a Chromium-family configuration directory contains an actual
/// cookie database. An installed-but-unused browser (directory present, no
/// `Cookies` sqlite) would otherwise be tried first and fail, hiding the
/// browser the user really signs in with.
fn chromium_has_cookies(base: &Path) -> bool {
    ["Default", "Profile 1", "Profile 2", "Profile 3"]
        .iter()
        .map(|profile| base.join(profile))
        .any(|p| p.join("Cookies").is_file() || p.join("Network/Cookies").is_file())
}

/// Finds a Firefox profile with a real cookie database. Both the traditional
/// `~/.mozilla` and XDG locations can coexist; callers preserve their order
/// and try every usable Firefox candidate before Chromium fallbacks.
fn firefox_profile(base: &Path, preferred: Option<&str>) -> Option<PathBuf> {
    preferred
        .map(|profile| base.join(profile))
        .filter(|profile| profile.join("cookies.sqlite").is_file())
        .or_else(|| {
            ["default-release", "default"]
                .iter()
                .map(|profile| base.join(profile))
                .find(|profile| profile.join("cookies.sqlite").is_file())
        })
        .or_else(|| firefox_profile_from_directory(base))
}

fn firefox_profile_from_directory(base: &Path) -> Option<PathBuf> {
    let mut profiles: Vec<PathBuf> = std::fs::read_dir(base)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.join("cookies.sqlite").is_file())
        .collect();
    profiles.sort_by_key(|p| !p.to_string_lossy().ends_with("default-release"));
    profiles.into_iter().next()
}

fn profile_has_cookies(profile: &Path) -> bool {
    profile.join("Cookies").is_file()
        || profile.join("Network/Cookies").is_file()
        || profile.join("cookies.sqlite").is_file()
}

fn preferred_profile_label(
    base: &Path,
    preferred: Option<&str>,
    defaults: &[&str],
) -> Option<String> {
    preferred
        .filter(|profile| profile_has_cookies(&base.join(profile)))
        .or_else(|| {
            defaults
                .iter()
                .copied()
                .find(|profile| profile_has_cookies(&base.join(profile)))
        })
        .map(str::to_string)
}

/// Detects typed browser candidates in the global authentication priority.
/// Saved configuration may select a profile inside its browser, but never
/// changes the browser's position in that priority.
pub fn detect_browser_candidates(
    home: &Path,
    authentication: &AuthenticationConfig,
) -> Vec<BrowserCandidate> {
    let preferred_for = |method: &str| {
        (authentication.browser.as_deref() == Some(method))
            .then_some(authentication.profile.as_deref())
            .flatten()
    };

    let mut candidates = Vec::new();
    for firefox_base in [
        home.join(".mozilla/firefox"),
        home.join(".config/mozilla/firefox"),
    ] {
        if let Some(profile) = firefox_profile(&firefox_base, preferred_for("firefox")) {
            candidates.push(BrowserCandidate::firefox(Some(profile)));
        }
    }

    candidates.extend(
        CHROMIUM_CANDIDATES
            .iter()
            .filter_map(|(method, directory)| {
                let base = home.join(directory);
                let preferred = preferred_for(method);
                let preferred_has_cookies =
                    preferred.is_some_and(|profile| profile_has_cookies(&base.join(profile)));
                if !chromium_has_cookies(&base) && !preferred_has_cookies {
                    return None;
                }
                let mut candidate = BrowserCandidate::chromium(*method);
                candidate.profile_label = preferred_profile_label(
                    &base,
                    preferred,
                    &["Default", "Profile 1", "Profile 2", "Profile 3"],
                );
                Some(candidate)
            }),
    );
    candidates
}

/// Detects browsers with a usable cookie store under `home`, as
/// `--cookies-from-browser` argument values (possibly carrying an explicit
/// `firefox:<profile-path>` for XDG Firefox setups). Firefox is attempted
/// first; Chromium-family sessions remain ordered fallbacks.
#[cfg(test)]
pub fn detect_browsers(home: &Path) -> Vec<String> {
    detect_browser_candidates(home, &AuthenticationConfig::default())
        .into_iter()
        .map(|candidate| candidate.yt_dlp_argument())
        .collect()
}

/// Production backend for typed sign-in preparation. Process stderr and API
/// details are intentionally discarded at this boundary; callers receive only
/// typed errors whose display text is sanitized.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSignInBackend;

fn copy_with_mode_600(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::copy(source, destination)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Instala credenciais preparadas e só as mantém se a persistência da
/// configuração também for concluída. Em qualquer falha após a troca, o
/// arquivo ativo anterior é restaurado (ou o novo é removido).
pub fn install_prepared_credentials<F>(
    prepared: &Path,
    active: &Path,
    persist: F,
) -> std::io::Result<()>
where
    F: FnOnce() -> std::io::Result<()>,
{
    let backup = active.with_extension("activation-backup");
    let had_active = active.is_file();
    if had_active {
        copy_with_mode_600(active, &backup)?;
    }
    std::fs::rename(prepared, active)?;
    if let Err(error) = persist() {
        if had_active {
            std::fs::rename(&backup, active)?;
        } else {
            std::fs::remove_file(active)?;
        }
        return Err(error);
    }
    if had_active {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

pub(super) fn block_on_current_runtime<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    tokio::runtime::Handle::current().block_on(future)
}

impl SignInBackend for SystemSignInBackend {
    fn export(&self, candidate: &BrowserCandidate, destination: &Path) -> Result<(), SignInError> {
        let output = std::process::Command::new("yt-dlp")
            .arg("--cookies-from-browser")
            .arg(candidate.yt_dlp_argument())
            .arg("--cookies")
            .arg(destination)
            .args(["--skip-download", "--no-warnings", "-O", "%(title)s"])
            .arg("https://www.youtube.com/watch?v=jNQXAC9IVRw")
            .output()
            .map_err(|error| SignInError::ExportFailed(error.to_string()))?;
        if !output.status.success() {
            return Err(SignInError::ExportFailed("yt-dlp failed".into()));
        }
        let cookies = std::fs::read_to_string(destination)
            .map_err(|error| SignInError::Io(error.to_string()))?;
        if !cookies.contains("SAPISID") {
            return Err(SignInError::NoYouTubeSession);
        }
        Ok(())
    }

    fn accounts(&self, path: &Path) -> Result<Vec<SignInAccount>, SignInError> {
        let path = path.to_string_lossy().into_owned();
        block_on_current_runtime(super::YtMusicClient::enumerate_cookie_accounts(&path))
            .map_err(|_| SignInError::AccountValidationFailed("account enumeration failed".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeBackend {
        attempts: std::sync::Mutex<Vec<String>>,
        fail_firefox: bool,
        empty_firefox_accounts: bool,
    }

    impl FakeBackend {
        fn successful() -> Self {
            Self {
                attempts: std::sync::Mutex::new(Vec::new()),
                fail_firefox: false,
                empty_firefox_accounts: false,
            }
        }

        fn firefox_export_failure() -> Self {
            Self {
                attempts: std::sync::Mutex::new(Vec::new()),
                fail_firefox: true,
                empty_firefox_accounts: false,
            }
        }

        fn firefox_without_accounts() -> Self {
            Self {
                attempts: std::sync::Mutex::new(Vec::new()),
                fail_firefox: false,
                empty_firefox_accounts: true,
            }
        }

        fn attempts(&self) -> Vec<String> {
            self.attempts.lock().unwrap().clone()
        }
    }

    fn test_candidates() -> Vec<BrowserCandidate> {
        vec![
            BrowserCandidate::firefox(None),
            BrowserCandidate::chromium("brave"),
        ]
    }

    impl SignInBackend for FakeBackend {
        fn export(
            &self,
            candidate: &BrowserCandidate,
            destination: &Path,
        ) -> Result<(), SignInError> {
            self.attempts.lock().unwrap().push(candidate.method.clone());
            if self.fail_firefox && candidate.method == "firefox" {
                return Err(SignInError::ExportFailed("synthetic failure".into()));
            }
            std::fs::write(destination, &candidate.method)
                .map_err(|error| SignInError::Io(error.to_string()))
        }

        fn accounts(&self, path: &Path) -> Result<Vec<SignInAccount>, SignInError> {
            if self.empty_firefox_accounts && std::fs::read_to_string(path).unwrap() == "firefox" {
                return Ok(Vec::new());
            }
            Ok(vec![SignInAccount {
                index: 0,
                name: "Thiago Santos".into(),
                handle: None,
            }])
        }
    }

    struct NetscapeHeaderBackend;

    impl SignInBackend for NetscapeHeaderBackend {
        fn export(
            &self,
            _candidate: &BrowserCandidate,
            destination: &Path,
        ) -> Result<(), SignInError> {
            let initial = std::fs::read_to_string(destination)
                .map_err(|error| SignInError::Io(error.to_string()))?;
            if !initial.starts_with("# Netscape HTTP Cookie File") {
                return Err(SignInError::ExportFailed(
                    "invalid cookie file header".into(),
                ));
            }
            std::fs::write(destination, "exported cookies")
                .map_err(|error| SignInError::Io(error.to_string()))
        }

        fn accounts(&self, _path: &Path) -> Result<Vec<SignInAccount>, SignInError> {
            Ok(vec![SignInAccount {
                index: 0,
                name: "Thiago Santos".into(),
                handle: None,
            }])
        }
    }

    #[test]
    fn initializes_private_destination_as_a_netscape_cookie_file() {
        let temp = tempfile::tempdir().unwrap();

        let prepared = prepare_with_backend(
            vec![BrowserCandidate::firefox(None)],
            temp.path(),
            &NetscapeHeaderBackend,
            &|_| {},
        )
        .unwrap();

        assert_eq!(prepared.candidate.method, "firefox");
        std::fs::remove_file(prepared.path).unwrap();
    }

    #[test]
    fn successful_firefox_never_invokes_brave() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::successful();
        let prepared =
            prepare_with_backend(test_candidates(), temp.path(), &backend, &|_| {}).unwrap();
        assert_eq!(prepared.candidate.method, "firefox");
        assert_eq!(prepared.accounts.len(), 1);
        assert_eq!(backend.attempts(), vec!["firefox"]);
    }

    #[test]
    fn failed_firefox_records_reason_then_uses_brave() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::firefox_export_failure();
        let prepared =
            prepare_with_backend(test_candidates(), temp.path(), &backend, &|_| {}).unwrap();
        assert_eq!(prepared.candidate.method, "brave");
        assert_eq!(prepared.failures[0].reason, "browser export failed");
    }

    #[test]
    fn firefox_without_an_identifiable_account_falls_back_to_brave() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::firefox_without_accounts();

        let prepared =
            prepare_with_backend(test_candidates(), temp.path(), &backend, &|_| {}).unwrap();

        assert_eq!(prepared.candidate.method, "brave");
        assert_eq!(backend.attempts(), vec!["firefox", "brave"]);
        assert_eq!(prepared.failures[0].reason, "no identifiable account");
    }

    #[test]
    fn failed_attempt_deletes_temp_file_and_hides_backend_detail() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::firefox_export_failure();
        let progress = std::sync::Mutex::new(Vec::new());

        let error = prepare_with_backend(
            vec![BrowserCandidate::firefox(None)],
            temp.path(),
            &backend,
            &|message| progress.lock().unwrap().push(message),
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "all browser attempts failed");
        assert_eq!(
            progress.lock().unwrap().last().unwrap(),
            "Firefox: browser export failed"
        );
        assert!(!progress
            .lock()
            .unwrap()
            .iter()
            .any(|message| message.contains("synthetic failure")));
        assert!(!std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".ytmtui-signin-")
            }));
    }

    #[test]
    fn typed_error_debug_output_is_sanitized() {
        let errors = [
            SignInError::ExportFailed("cookie=secret raw stderr".into()),
            SignInError::NoYouTubeSession,
            SignInError::AccountValidationFailed("raw API body".into()),
        ];

        assert_eq!(format!("{:?}", errors[0]), "browser export failed");
        assert_eq!(format!("{:?}", errors[1]), "no YouTube session");
        assert_eq!(format!("{:?}", errors[2]), "account validation failed");
    }

    #[tokio::test]
    async fn blocking_backend_work_uses_the_existing_runtime() {
        let _backend = SystemSignInBackend;
        let value = tokio::task::spawn_blocking(|| block_on_current_runtime(async { 42 }))
            .await
            .unwrap();

        assert_eq!(value, 42);
    }

    #[test]
    fn preparation_removes_only_stale_owned_signin_files() {
        use std::time::{Duration, SystemTime};

        let temp = tempfile::tempdir().unwrap();
        let stale = temp.path().join(".ytmtui-signin-stale");
        let fresh = temp.path().join(".ytmtui-signin-fresh");
        let unrelated = temp.path().join("unrelated-stale");
        for path in [&stale, &fresh, &unrelated] {
            std::fs::write(path, "fixture").unwrap();
        }
        let old = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
        for path in [&stale, &unrelated] {
            let file = std::fs::File::options().write(true).open(path).unwrap();
            file.set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        }

        let prepared = prepare_with_backend(
            vec![BrowserCandidate::firefox(None)],
            temp.path(),
            &FakeBackend::successful(),
            &|_| {},
        )
        .unwrap();

        assert!(!stale.exists());
        assert!(fresh.exists());
        assert!(unrelated.exists());
        std::fs::remove_file(prepared.path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn prepared_file_is_private_and_does_not_replace_production_cookies() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let production = temp.path().join("cookies.txt");
        std::fs::write(&production, "active production cookies").unwrap();

        let prepared = prepare_with_backend(
            vec![BrowserCandidate::firefox(None)],
            temp.path(),
            &FakeBackend::successful(),
            &|_| {},
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&production).unwrap(),
            "active production cookies"
        );
        assert_ne!(prepared.path, production);
        assert!(prepared
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".ytmtui-signin-"));
        assert_eq!(
            std::fs::metadata(&prepared.path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::remove_file(prepared.path).unwrap();
    }

    #[test]
    fn failed_persistence_restores_old_cookie_file() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("cookies.txt");
        let prepared = temp.path().join("prepared-cookies.txt");
        std::fs::write(&active, "old active cookies").unwrap();
        std::fs::write(&prepared, "new prepared cookies").unwrap();

        let error = install_prepared_credentials(&prepared, &active, || {
            Err(std::io::Error::other("synthetic persistence failure"))
        })
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(
            std::fs::read_to_string(&active).unwrap(),
            "old active cookies"
        );
        assert!(!prepared.exists());
        assert!(!active.with_extension("activation-backup").exists());
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(&active).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn successful_install_keeps_active_credentials_private() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("cookies.txt");
        let prepared = temp.path().join("prepared-cookies.txt");
        std::fs::write(&active, "old active credentials").unwrap();
        std::fs::write(&prepared, "new prepared credentials").unwrap();
        std::fs::set_permissions(&prepared, std::fs::Permissions::from_mode(0o600)).unwrap();

        install_prepared_credentials(&prepared, &active, || Ok(())).unwrap();

        assert_eq!(
            std::fs::metadata(&active).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!active.with_extension("activation-backup").exists());
    }

    #[test]
    fn detect_browsers_prefers_firefox_and_requires_chromium_cookie_store() {
        let home = tempfile::tempdir().expect("temporary home");
        assert!(detect_browsers(home.path()).is_empty());

        // An installed-but-never-used Brave (no Cookies db) is skipped.
        std::fs::create_dir_all(
            home.path()
                .join(".config/BraveSoftware/Brave-Browser/Default"),
        )
        .unwrap();
        let firefox = home.path().join(".mozilla/firefox/default-release");
        std::fs::create_dir_all(&firefox).unwrap();
        std::fs::write(firefox.join("cookies.sqlite"), "").unwrap();
        assert_eq!(
            detect_browsers(home.path()),
            vec![format!("firefox:{}", firefox.display())]
        );

        // Once the cookie db exists, Brave becomes the fallback; Firefox
        // stays first so `g` uses the account the user chose there.
        std::fs::write(
            home.path()
                .join(".config/BraveSoftware/Brave-Browser/Default/Cookies"),
            "",
        )
        .unwrap();
        assert_eq!(
            detect_browsers(home.path()),
            vec![
                format!("firefox:{}", firefox.display()),
                "brave:Default".to_string()
            ]
        );
    }

    #[test]
    fn typed_detection_keeps_firefox_first_when_brave_profile_is_saved() {
        let home = tempfile::tempdir().expect("temporary home");
        let firefox = home.path().join(".mozilla/firefox/default-release");
        std::fs::create_dir_all(&firefox).unwrap();
        std::fs::write(firefox.join("cookies.sqlite"), "").unwrap();
        for profile in ["Default", "Profile 1"] {
            let directory = home
                .path()
                .join(".config/BraveSoftware/Brave-Browser")
                .join(profile);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("Cookies"), "").unwrap();
        }
        let chrome = home.path().join(".config/google-chrome/Default");
        std::fs::create_dir_all(&chrome).unwrap();
        std::fs::write(chrome.join("Cookies"), "").unwrap();
        let saved = crate::config::AuthenticationConfig {
            browser: Some("brave".into()),
            profile: Some("Profile 1".into()),
            auth_user: 0,
        };

        let detected = detect_browser_candidates(home.path(), &saved);

        assert_eq!(
            detected
                .iter()
                .map(|candidate| candidate.method.as_str())
                .collect::<Vec<_>>(),
            vec!["firefox", "brave", "chrome"]
        );
        assert_eq!(detected[1].profile_label.as_deref(), Some("Profile 1"));
        assert_eq!(detected[1].yt_dlp_argument(), "brave:Profile 1");
    }

    #[test]
    fn typed_detection_uses_saved_xdg_firefox_profile_without_reordering() {
        let home = tempfile::tempdir().expect("temporary home");
        let firefox = home.path().join(".config/mozilla/firefox");
        for profile in ["aaa.default-release", "zzz.work"] {
            let directory = firefox.join(profile);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("cookies.sqlite"), "").unwrap();
        }
        let brave = home
            .path()
            .join(".config/BraveSoftware/Brave-Browser/Default");
        std::fs::create_dir_all(&brave).unwrap();
        std::fs::write(brave.join("Cookies"), "").unwrap();
        let saved = crate::config::AuthenticationConfig {
            browser: Some("firefox".into()),
            profile: Some("zzz.work".into()),
            auth_user: 0,
        };

        let detected = detect_browser_candidates(home.path(), &saved);

        assert_eq!(detected[0].profile_label.as_deref(), Some("zzz.work"));
        assert!(detected[0].yt_dlp_argument().starts_with("firefox:"));
        assert_eq!(detected[1].method, "brave");
    }

    #[test]
    fn typed_detection_accepts_saved_custom_chromium_profile() {
        let home = tempfile::tempdir().expect("temporary home");
        let work = home.path().join(".config/BraveSoftware/Brave-Browser/Work");
        std::fs::create_dir_all(work.join("Network")).unwrap();
        std::fs::write(work.join("Network/Cookies"), "").unwrap();
        let saved = crate::config::AuthenticationConfig {
            browser: Some("brave".into()),
            profile: Some("Work".into()),
            auth_user: 0,
        };

        let detected = detect_browser_candidates(home.path(), &saved);

        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].method, "brave");
        assert_eq!(detected[0].profile_label.as_deref(), Some("Work"));
    }

    #[test]
    fn detect_browsers_finds_xdg_firefox_profiles_with_explicit_path() {
        let home = tempfile::tempdir().expect("temporary home");
        let base = home.path().join(".config/mozilla/firefox");
        // A psd `-backup` copy and the live profile: the live one wins.
        for profile in ["abc.default-release-backup", "abc.default-release"] {
            let dir = base.join(profile);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("cookies.sqlite"), "").unwrap();
        }

        let detected = detect_browsers(home.path());
        assert_eq!(detected.len(), 1);
        assert!(
            detected[0].starts_with("firefox:") && detected[0].ends_with("abc.default-release"),
            "explicit profile path: {detected:?}"
        );
    }

    #[test]
    fn empty_legacy_firefox_directory_does_not_hide_a_usable_xdg_profile() {
        let home = tempfile::tempdir().expect("temporary home");
        std::fs::create_dir_all(home.path().join(".mozilla/firefox")).unwrap();
        let xdg_profile = home
            .path()
            .join(".config/mozilla/firefox/abc.default-release");
        std::fs::create_dir_all(&xdg_profile).unwrap();
        std::fs::write(xdg_profile.join("cookies.sqlite"), "").unwrap();

        let detected = detect_browsers(home.path());

        assert_eq!(detected, vec![format!("firefox:{}", xdg_profile.display())]);
    }

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
            Some(fallback.clone()),
        );

        assert_eq!(resolution.path.as_deref(), fallback.to_str());
        assert_eq!(
            resolution.missing_requested_path.as_deref(),
            missing.to_str()
        );
    }
}
