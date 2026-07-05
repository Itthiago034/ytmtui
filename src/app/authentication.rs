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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_authenticated_state_is_authenticated() {
        assert!(AuthenticationState::Authenticated.is_authenticated());
        assert!(!AuthenticationState::Anonymous.is_authenticated());
        assert!(!AuthenticationState::Expired.is_authenticated());
        assert!(!AuthenticationState::InvalidCookies.is_authenticated());
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
