//! Adaptador do YouTube Music para o contrato [`MusicProvider`].
//!
//! O [`YtMusicClient`] continua sendo o cliente HTTP puro; este tipo o
//! embrulha com o estado mutável de autenticação (cookies podem ser
//! renovados em pleno voo pelo `sign_in`) e com a resolução de áudio via
//! yt-dlp. É a única porta pela qual o resto do app fala com o YouTube.

use std::path::PathBuf;
use std::sync::RwLock;

use async_trait::async_trait;

use super::{signin, stream, YtMusicClient, YtMusicError};
use crate::config::AuthenticationConfig;
use crate::models::{HomeSection, Lyrics, Playlist, SearchResults, Track};
use crate::provider::{
    AuthState, Capabilities, MusicProvider, ProviderError, Result, SignInSummary,
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

pub struct YtMusic {
    /// `RwLock`, e não campos fixos: o `sign_in` troca o cliente e o caminho
    /// de cookies com o app rodando, e as tasks concorrentes (que clonam o
    /// cliente barato na entrada de cada chamada) passam a usar a sessão
    /// nova automaticamente.
    state: RwLock<State>,
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
        self.client().get_artist(browse_id).await.map_err(Into::into)
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
        let home = dirs::home_dir()
            .ok_or_else(|| "não foi possível localizar o diretório home".to_string())?;
        let browsers = signin::detect_browsers(&home);
        if browsers.is_empty() {
            return Err("nenhum navegador suportado encontrado (Brave/Chrome/Firefox…)".into());
        }
        let dest = dirs::config_dir()
            .ok_or_else(|| "não foi possível localizar o diretório de config".to_string())?
            .join("ytmtui/cookies.txt");

        let mut last_error = String::new();
        for browser in browsers {
            progress(format!("Importando cookies de {browser}…"));
            match signin::export_browser_cookies(&browser, &dest) {
                Ok(()) => {
                    let path = dest.to_string_lossy().into_owned();
                    let client = YtMusicClient::with_cookies(&path)
                        .map_err(|e| format!("cookies importados são inválidos: {e}"))?;
                    let mut state = self.state.write().unwrap();
                    state.client = client;
                    state.cookies = Some(path.clone());
                    let method = browser.split(':').next().unwrap_or(&browser).to_string();
                    return Ok(SignInSummary {
                        method,
                        credentials_path: Some(path),
                    });
                }
                Err(e) => last_error = format!("{browser}: {e}"),
            }
        }
        Err(last_error)
    }

    fn resolve_playable(&self, track: &Track) -> anyhow::Result<PathBuf> {
        stream::download_audio(&track.video_id, self.cookies().as_deref())
    }
}
