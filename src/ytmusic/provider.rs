//! Adaptador do YouTube Music para o contrato [`MusicProvider`].
//!
//! O [`YtMusicClient`] continua sendo o cliente HTTP puro; este tipo o
//! embrulha com o estado mutável de autenticação (cookies podem ser
//! renovados em pleno voo pelo `sign_in`) e com a resolução de áudio via
//! yt-dlp. É a única porta pela qual o resto do app fala com o YouTube.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

use async_trait::async_trait;

use super::{signin, stream, YtMusicClient, YtMusicError};
use crate::config::{AuthenticationConfig, Config};
use crate::models::{HomeSection, Lyrics, Playlist, SearchResults, Track};
use crate::provider::{
    AuthState, Capabilities, MusicProvider, ProviderError, Result, SignInPreview, SignInSummary,
};

impl From<YtMusicError> for ProviderError {
    fn from(error: YtMusicError) -> Self {
        match error {
            YtMusicError::SessionExpired { .. } => ProviderError::SessionExpired,
            other => ProviderError::Message(other.to_string()),
        }
    }
}

/// O que o app precisa saber logo após construir o provedor: o estado de
/// autenticação inicial e, para diagnóstico, um caminho de cookies que foi
/// pedido (env/config) mas não existe no disco.
pub struct Bootstrap {
    pub auth: AuthState,
    pub missing_requested_path: Option<String>,
    /// Caminho de cookies em uso (persistido na config).
    pub cookies: Option<String>,
}

struct State {
    client: YtMusicClient,
    cookies: Option<String>,
}

struct PendingSignIn {
    id: u64,
    prepared: signin::PreparedCredentials,
}

struct ActivationConfig {
    method: String,
    profile: Option<String>,
    account_index: u8,
    credentials_path: String,
}

pub struct YtMusic {
    /// `RwLock`, e não campos fixos: o `sign_in` troca o cliente e o caminho
    /// de cookies com o app rodando, e as tasks concorrentes (que clonam o
    /// cliente barato na entrada de cada chamada) passam a usar a sessão
    /// nova automaticamente.
    state: RwLock<State>,
    next_preview_id: AtomicU64,
    pending_sign_in: Mutex<Option<PendingSignIn>>,
}

impl YtMusic {
    /// Constrói o provedor resolvendo os cookies de `$YTM_COOKIES`, da
    /// configuração ou do caminho padrão (`~/.config/ytmtui/cookies.txt`),
    /// nesta ordem.
    pub fn from_environment(
        configured_cookies: Option<String>,
        authentication: AuthenticationConfig,
    ) -> (Self, Bootstrap) {
        let default = dirs::config_dir().map(|dir| dir.join("ytmtui/cookies.txt"));
        let resolution = signin::resolve_cookie_path(
            std::env::var("YTM_COOKIES").ok(),
            configured_cookies,
            default,
        );
        let (client, auth) = match resolution.path.as_deref() {
            Some(path) => {
                match YtMusicClient::with_cookies_for_account(path, authentication.auth_user) {
                    Ok(client) => (client, AuthState::Authenticated),
                    Err(_) => (YtMusicClient::new(), AuthState::InvalidCredentials),
                }
            }
            None => (YtMusicClient::new(), AuthState::Anonymous),
        };
        let provider = Self {
            state: RwLock::new(State {
                client,
                cookies: resolution.path.clone(),
            }),
            next_preview_id: AtomicU64::new(1),
            pending_sign_in: Mutex::new(None),
        };
        let bootstrap = Bootstrap {
            auth,
            missing_requested_path: resolution.missing_requested_path,
            cookies: resolution.path,
        };
        (provider, bootstrap)
    }

    /// Clone barato do cliente atual (reqwest + Arc da auth). Cada chamada
    /// pega o cliente na entrada, então o guard do lock nunca cruza um
    /// `.await`.
    fn client(&self) -> YtMusicClient {
        self.state.read().unwrap().client.clone()
    }

    fn cookies(&self) -> Option<String> {
        self.state.read().unwrap().cookies.clone()
    }

    fn prepare_sign_in_with<B>(
        &self,
        candidates: Vec<signin::BrowserCandidate>,
        config_dir: &std::path::Path,
        backend: &B,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> std::result::Result<SignInPreview, String>
    where
        B: signin::SignInBackend + ?Sized,
    {
        if let Some(previous) = self
            .pending_sign_in
            .lock()
            .map_err(|_| "sign-in state unavailable".to_string())?
            .take()
        {
            let _ = std::fs::remove_file(previous.prepared.path);
        }

        let prepared = signin::prepare_with_backend(candidates, config_dir, backend, progress)
            .map_err(|error| error.to_string())?;
        if !prepared.failures.is_empty() {
            progress("browser fallback prepared successfully".to_string());
        }
        let id = self.next_preview_id.fetch_add(1, Ordering::Relaxed);
        let preview = SignInPreview {
            id,
            method: prepared.candidate.method.clone(),
            profile_label: prepared.candidate.profile_label.clone(),
            accounts: prepared.accounts.clone(),
            current_account_name: None,
        };

        let displaced = match self.pending_sign_in.lock() {
            Ok(mut pending) => pending.replace(PendingSignIn { id, prepared }),
            Err(_) => {
                let _ = std::fs::remove_file(prepared.path);
                return Err("sign-in state unavailable".to_string());
            }
        };
        if let Some(displaced) = displaced {
            let _ = std::fs::remove_file(displaced.prepared.path);
        }
        Ok(preview)
    }

    fn activate_sign_in_with<P, B>(
        &self,
        preview_id: u64,
        account_index: u8,
        active: &std::path::Path,
        persist: P,
        build_client: B,
    ) -> std::result::Result<SignInSummary, String>
    where
        P: FnOnce(&ActivationConfig) -> std::io::Result<()>,
        B: FnOnce(&str, u8) -> std::result::Result<YtMusicClient, String>,
    {
        let mut pending_guard = self
            .pending_sign_in
            .lock()
            .map_err(|_| "sign-in state unavailable".to_string())?;
        let pending = pending_guard
            .as_ref()
            .filter(|pending| pending.id == preview_id)
            .ok_or_else(|| "sign-in preview is no longer pending".to_string())?;
        let account = pending
            .prepared
            .accounts
            .iter()
            .find(|account| account.index == account_index)
            .cloned()
            .ok_or_else(|| "selected account is not in this sign-in preview".to_string())?;

        let prepared_path = pending.prepared.path.clone();
        let prepared_path_string = prepared_path.to_string_lossy().into_owned();
        let client = build_client(&prepared_path_string, account_index)?;
        let active_string = active.to_string_lossy().into_owned();
        let activation = ActivationConfig {
            method: pending.prepared.candidate.method.clone(),
            profile: pending.prepared.candidate.profile_label.clone(),
            account_index,
            credentials_path: active_string.clone(),
        };

        let mut state = self
            .state
            .write()
            .map_err(|_| "provider state unavailable".to_string())?;
        if let Err(error) =
            signin::install_prepared_credentials(&prepared_path, active, || persist(&activation))
        {
            if !prepared_path.is_file() {
                *pending_guard = None;
            }
            return Err(format!("could not activate prepared credentials: {error}"));
        }

        state.client = client;
        state.cookies = Some(active_string.clone());
        *pending_guard = None;
        Ok(SignInSummary {
            method: activation.method,
            credentials_path: Some(active_string),
            account_name: account.name,
            account_index,
        })
    }
}

#[async_trait]
impl MusicProvider for YtMusic {
    fn id(&self) -> &'static str {
        "ytmusic"
    }

    fn display_name(&self) -> &'static str {
        "YouTube Music"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            home: true,
            library: true,
            lyrics: true,
            radio: true,
            likes: true,
            sign_in: true,
        }
    }

    fn is_authenticated(&self) -> bool {
        self.state.read().unwrap().client.is_authenticated()
    }

    async fn search(&self, query: &str) -> Result<SearchResults> {
        self.client().search(query).await.map_err(Into::into)
    }

    async fn home(&self) -> Result<Vec<HomeSection>> {
        self.client().get_home().await.map_err(Into::into)
    }

    async fn library_playlists(&self) -> Result<Vec<Playlist>> {
        self.client()
            .get_library_playlists()
            .await
            .map_err(Into::into)
    }

    async fn playlist_tracks(&self, browse_id: &str) -> Result<Vec<Track>> {
        self.client()
            .get_playlist_tracks(browse_id)
            .await
            .map_err(Into::into)
    }

    async fn artist_tracks(&self, browse_id: &str) -> Result<Vec<Track>> {
        self.client()
            .get_artist(browse_id)
            .await
            .map_err(Into::into)
    }

    async fn radio(&self, track_id: &str) -> Result<Vec<Track>> {
        self.client().get_radio(track_id).await.map_err(Into::into)
    }

    async fn lyrics(&self, track_id: &str) -> Result<Option<Lyrics>> {
        self.client().get_lyrics(track_id).await.map_err(Into::into)
    }

    async fn rate_track(&self, track_id: &str, like: bool) -> Result<()> {
        self.client()
            .rate_song(track_id, like)
            .await
            .map_err(Into::into)
    }

    async fn account_name(&self) -> Result<Option<String>> {
        self.client().get_account_name().await.map_err(Into::into)
    }

    async fn fetch_artwork(&self, url: &str) -> Result<Vec<u8>> {
        self.client().fetch_bytes(url).await.map_err(Into::into)
    }

    fn sign_in(
        &self,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> std::result::Result<SignInSummary, String> {
        let preview = self.prepare_sign_in(progress)?;
        let account_index = preview
            .accounts
            .first()
            .ok_or_else(|| "no identifiable account".to_string())?
            .index;
        self.activate_sign_in(preview.id, account_index)
    }

    fn prepare_sign_in(
        &self,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> std::result::Result<SignInPreview, String> {
        let home = dirs::home_dir()
            .ok_or_else(|| "não foi possível localizar o diretório home".to_string())?;
        let config_dir = dirs::config_dir()
            .ok_or_else(|| "não foi possível localizar o diretório de config".to_string())?
            .join("ytmtui");

        let authentication = Config::load().authentication;
        let candidates = signin::detect_browser_candidates(&home, &authentication);
        self.prepare_sign_in_with(
            candidates,
            &config_dir,
            &signin::SystemSignInBackend,
            progress,
        )
    }

    fn activate_sign_in(
        &self,
        preview_id: u64,
        account_index: u8,
    ) -> std::result::Result<SignInSummary, String> {
        let active = dirs::config_dir()
            .ok_or_else(|| "não foi possível localizar o diretório de config".to_string())?
            .join("ytmtui/cookies.txt");
        self.activate_sign_in_with(
            preview_id,
            account_index,
            &active,
            |activation| {
                let mut config = Config::load();
                config.cookies = Some(activation.credentials_path.clone());
                config.authentication = AuthenticationConfig {
                    browser: Some(activation.method.clone()),
                    profile: activation.profile.clone(),
                    auth_user: activation.account_index,
                };
                config
                    .try_save()
                    .map_err(|error| std::io::Error::other(error.to_string()))
            },
            |path, account_index| {
                YtMusicClient::with_cookies_for_account(path, account_index)
                    .map_err(|_| "prepared credentials are invalid".to_string())
            },
        )
    }

    fn cancel_sign_in(&self, preview_id: u64) {
        let Ok(mut pending) = self.pending_sign_in.lock() else {
            return;
        };
        if pending
            .as_ref()
            .is_some_and(|pending| pending.id == preview_id)
        {
            if let Some(pending) = pending.take() {
                let _ = std::fs::remove_file(pending.prepared.path);
            }
        }
    }

    fn resolve_playable(&self, track: &Track) -> anyhow::Result<PathBuf> {
        stream::download_audio(&track.video_id, self.cookies().as_deref())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Barrier};

    use super::*;
    use crate::provider::SignInAccount;
    use crate::ytmusic::signin::{BrowserCandidate, SignInBackend, SignInError};

    struct ConcurrentBackend {
        exports_ready: Arc<Barrier>,
    }

    impl SignInBackend for ConcurrentBackend {
        fn export(
            &self,
            candidate: &BrowserCandidate,
            destination: &Path,
        ) -> std::result::Result<(), SignInError> {
            std::fs::write(destination, &candidate.method)
                .map_err(|error| SignInError::Io(error.to_string()))?;
            self.exports_ready.wait();
            Ok(())
        }

        fn accounts(&self, _path: &Path) -> std::result::Result<Vec<SignInAccount>, SignInError> {
            Ok(vec![SignInAccount {
                index: 0,
                name: "Prepared Account".to_string(),
                handle: None,
            }])
        }
    }

    fn provider_for_test() -> YtMusic {
        YtMusic {
            state: RwLock::new(State {
                client: YtMusicClient::new(),
                cookies: Some("old-active-path".to_string()),
            }),
            next_preview_id: AtomicU64::new(1),
            pending_sign_in: Mutex::new(None),
        }
    }

    fn prepared_credentials(path: PathBuf) -> signin::PreparedCredentials {
        signin::PreparedCredentials {
            path,
            candidate: BrowserCandidate::firefox(None),
            accounts: vec![SignInAccount {
                index: 0,
                name: "Prepared Account".to_string(),
                handle: None,
            }],
            failures: Vec::new(),
        }
    }

    #[test]
    fn concurrent_preparations_leave_one_pending_private_file() {
        let temp = tempfile::tempdir().unwrap();
        let provider = provider_for_test();
        let barrier = Arc::new(Barrier::new(3));
        let backend = ConcurrentBackend {
            exports_ready: Arc::clone(&barrier),
        };
        let previews = Mutex::new(Vec::new());

        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    let preview = provider
                        .prepare_sign_in_with(
                            vec![BrowserCandidate::firefox(None)],
                            temp.path(),
                            &backend,
                            &|_| {},
                        )
                        .unwrap();
                    previews.lock().unwrap().push(preview);
                });
            }
            barrier.wait();
        });

        let pending = provider.pending_sign_in.lock().unwrap();
        let pending = pending.as_ref().expect("one preparation remains pending");
        assert!(pending.prepared.path.is_file());
        assert!(previews
            .lock()
            .unwrap()
            .iter()
            .any(|preview| preview.id == pending.id));
        let prepared_file_count = std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".ytmtui-signin-")
            })
            .count();
        assert_eq!(prepared_file_count, 1);
    }

    #[test]
    fn terminal_activation_failure_clears_consumed_pending_state() {
        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("cookies.txt");
        let prepared = temp.path().join("prepared.txt");
        std::fs::write(&active, "old active credentials").unwrap();
        std::fs::write(&prepared, "new prepared credentials").unwrap();
        let provider = provider_for_test();
        *provider.pending_sign_in.lock().unwrap() = Some(PendingSignIn {
            id: 7,
            prepared: prepared_credentials(prepared.clone()),
        });

        let error = provider
            .activate_sign_in_with(
                7,
                0,
                &active,
                |_| Err(std::io::Error::other("synthetic persistence failure")),
                |_, account_index| Ok(YtMusicClient::new_with_auth_user_for_test(account_index)),
            )
            .unwrap_err();

        assert!(error.contains("could not activate prepared credentials"));
        assert_eq!(
            std::fs::read_to_string(&active).unwrap(),
            "old active credentials"
        );
        assert!(!prepared.exists());
        assert!(provider.pending_sign_in.lock().unwrap().is_none());
        assert!(!provider.is_authenticated());
        assert_eq!(provider.cookies().as_deref(), Some("old-active-path"));
    }
}
