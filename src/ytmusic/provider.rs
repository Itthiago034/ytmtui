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
        if let Some(previous) = self
            .pending_sign_in
            .lock()
            .map_err(|_| "sign-in state unavailable".to_string())?
            .take()
        {
            let _ = std::fs::remove_file(previous.prepared.path);
        }

        let home = dirs::home_dir()
            .ok_or_else(|| "não foi possível localizar o diretório home".to_string())?;
        let config_dir = dirs::config_dir()
            .ok_or_else(|| "não foi possível localizar o diretório de config".to_string())?
            .join("ytmtui");

        let authentication = Config::load().authentication;
        let candidates = signin::detect_browser_candidates(&home, &authentication);
        let prepared = signin::prepare_with_backend(
            candidates,
            &config_dir,
            &signin::SystemSignInBackend,
            progress,
        )
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
        *self
            .pending_sign_in
            .lock()
            .map_err(|_| "sign-in state unavailable".to_string())? =
            Some(PendingSignIn { id, prepared });
        Ok(preview)
    }

    fn activate_sign_in(
        &self,
        preview_id: u64,
        account_index: u8,
    ) -> std::result::Result<SignInSummary, String> {
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
        let method = pending.prepared.candidate.method.clone();
        let profile = pending.prepared.candidate.profile_label.clone();
        let prepared_path_string = prepared_path.to_string_lossy().into_owned();
        let client = YtMusicClient::with_cookies_for_account(&prepared_path_string, account_index)
            .map_err(|_| "prepared credentials are invalid".to_string())?;
        let active = dirs::config_dir()
            .ok_or_else(|| "não foi possível localizar o diretório de config".to_string())?
            .join("ytmtui/cookies.txt");
        let active_string = active.to_string_lossy().into_owned();

        let mut state = self
            .state
            .write()
            .map_err(|_| "provider state unavailable".to_string())?;
        signin::install_prepared_credentials(&prepared_path, &active, || {
            let mut config = Config::load();
            config.cookies = Some(active_string.clone());
            config.authentication = AuthenticationConfig {
                browser: Some(method.clone()),
                profile: profile.clone(),
                auth_user: account_index,
            };
            config
                .try_save()
                .map_err(|error| std::io::Error::other(error.to_string()))
        })
        .map_err(|error| format!("could not activate prepared credentials: {error}"))?;

        state.client = client;
        state.cookies = Some(active_string.clone());
        *pending_guard = None;
        Ok(SignInSummary {
            method,
            credentials_path: Some(active_string),
            account_name: account.name,
            account_index,
        })
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
