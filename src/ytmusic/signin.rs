//! Fluxo de sign-in do YouTube Music: resolução do arquivo de cookies e
//! importação da sessão a partir de um navegador instalado (via yt-dlp).
//! Tudo aqui é específico do YouTube — a UI só vê o contrato genérico de
//! `crate::provider::MusicProvider::sign_in`.

use std::path::{Path, PathBuf};

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

/// Best Firefox profile directory when Firefox keeps its profiles in the
/// XDG location (`~/.config/mozilla/firefox`) instead of `~/.mozilla` —
/// common on distros that patch Firefox for XDG dirs (e.g. CachyOS), where
/// yt-dlp's default lookup misses it. Prefers the `default-release` profile
/// and requires an actual `cookies.sqlite`, which also keeps
/// profile-sync-daemon `-backup` copies from shadowing the live profile.
fn firefox_xdg_profile(home: &Path) -> Option<PathBuf> {
    let base = home.join(".config/mozilla/firefox");
    let mut profiles: Vec<PathBuf> = std::fs::read_dir(base)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.join("cookies.sqlite").is_file())
        .collect();
    profiles.sort_by_key(|p| !p.to_string_lossy().ends_with("default-release"));
    profiles.into_iter().next()
}

/// Detects browsers with a usable cookie store under `home`, as
/// `--cookies-from-browser` argument values (possibly carrying an explicit
/// `firefox:<profile-path>` for XDG Firefox setups). Firefox is attempted
/// first; Chromium-family sessions remain ordered fallbacks.
pub fn detect_browsers(home: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if home.join(".mozilla/firefox").is_dir() {
        out.push("firefox".to_string());
    } else if let Some(profile) = firefox_xdg_profile(home) {
        out.push(format!("firefox:{}", profile.display()));
    }
    out.extend(
        CHROMIUM_CANDIDATES
            .iter()
            .filter(|(_, dir)| chromium_has_cookies(&home.join(dir)))
            .map(|(name, _)| name.to_string()),
    );
    out
}

/// Exports YouTube Music cookies from `browser` into `dest` (Netscape
/// format) using `yt-dlp --cookies-from-browser` — the same recipe as
/// `scripts/refresh-cookies.sh`, but callable from inside the app. Writes to
/// a temp file first and only replaces `dest` after confirming the export
/// contains a `SAPISID` cookie (what the API authentication derives from).
pub fn export_browser_cookies(browser: &str, dest: &Path) -> Result<(), String> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {dir:?}: {e}"))?;
    }
    let tmp = dest.with_extension(format!("import-{}.tmp", std::process::id()));
    let result = (|| {
        let output = std::process::Command::new("yt-dlp")
            .arg("--cookies-from-browser")
            .arg(browser)
            .arg("--cookies")
            .arg(&tmp)
            .args(["--skip-download", "--no-warnings", "-O", "%(title)s"])
            // Any watchable URL works; visiting one is what makes yt-dlp
            // load the browser jar and save it to --cookies on exit.
            .arg("https://www.youtube.com/watch?v=jNQXAC9IVRw")
            .output()
            .map_err(|e| format!("could not run yt-dlp: {e}"))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(err.lines().last().unwrap_or("yt-dlp failed").to_string());
        }
        let cookies = std::fs::read_to_string(&tmp)
            .map_err(|e| format!("yt-dlp produced no cookie file: {e}"))?;
        if !cookies.contains("SAPISID") {
            return Err(format!(
                "no YouTube session in {browser} — sign in to music.youtube.com there first"
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, dest).map_err(|e| format!("could not save cookies: {e}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
        std::fs::create_dir_all(home.path().join(".mozilla/firefox")).unwrap();
        assert_eq!(detect_browsers(home.path()), vec!["firefox"]);

        // Once the cookie db exists, Brave becomes the fallback; Firefox
        // stays first so `g` uses the account the user chose there.
        std::fs::write(
            home.path()
                .join(".config/BraveSoftware/Brave-Browser/Default/Cookies"),
            "",
        )
        .unwrap();
        assert_eq!(detect_browsers(home.path()), vec!["firefox", "brave"]);
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
