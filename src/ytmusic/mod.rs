//! Cliente da API interna (InnerTube) do YouTube Music.
//!
//! Este módulo implementa diretamente as chamadas HTTP para a API
//! `music.youtube.com/youtubei/v1/*`, sem necessidade de autenticação.
//! Ele oferece busca de músicas/artistas/playlists, listagem de faixas de
//! uma playlist e obtenção de letras.

pub mod auth;
pub mod models;
mod parse;

use std::fmt;
use std::sync::Arc;

use serde_json::{json, Value};

pub use auth::Auth;
pub use models::{Artist, HomeSection, LyricLine, Lyrics, Playlist, SearchResults, Track};
use parse::*;

const BASE: &str = "https://music.youtube.com/youtubei/v1";
const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_VERSION: &str = "1.20240101.01.00";
/// Only used for the lyrics browse call: the WEB_REMIX client only ever
/// returns plain Musixmatch text, but the exact same browseId returns
/// per-line timestamped lyrics (`timedLyricsData`) when queried with the
/// Android app's client identity. Verified live against a real track.
const CLIENT_NAME_ANDROID: &str = "ANDROID_MUSIC";
const CLIENT_VERSION_ANDROID: &str = "6.51.53";

// Parâmetros de filtro de busca (identificados na API do YouTube Music).
const FILTER_SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";
const FILTER_ARTISTS: &str = "EgWKAQIgAWoKEAkQChAFEAMQBA%3D%3D";
const FILTER_PLAYLISTS: &str = "EgWKAQIoAWoKEAkQChAFEAMQBA%3D%3D";
const FILTER_ALBUMS: &str = "EgWKAQIYAWoKEAkQChAFEAMQBA%3D%3D";

// Paths de endpoint InnerTube usados por `YtMusicClient::post`/`post_anonymous`.
const EP_BROWSE: &str = "browse";
const EP_SEARCH: &str = "search";
const EP_NEXT: &str = "next";
const EP_LIKE: &str = "like/like";
const EP_REMOVE_LIKE: &str = "like/removelike";
const EP_ACCOUNT_MENU: &str = "account/account_menu";

/// Tempo máximo de espera por uma resposta da API antes de desistir da
/// requisição, evitando travar a UI indefinidamente numa chamada de rede presa.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub type YtMusicResult<T> = std::result::Result<T, YtMusicError>;

#[derive(Debug)]
pub enum YtMusicError {
    AuthenticationRequired,
    SessionExpired {
        status: reqwest::StatusCode,
        endpoint: String,
    },
    /// HTTP 429: o cliente está sendo limitado por taxa de requisições.
    RateLimited {
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
            Self::RateLimited { endpoint } => {
                write!(f, "rate limited while requesting {endpoint}")
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
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        // Pode ocorrer autenticado ou não, então é checado antes do bloco de
        // sessão expirada.
        YtMusicError::RateLimited {
            endpoint: endpoint.to_string(),
        }
    } else if authenticated
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

/// Retorna o primeiro erro entre os quatro resultados, na mesma ordem em que
/// aparecem. Usado por [`YtMusicClient::search`] quando todas as sub-buscas
/// falham, para propagar um erro representativo à UI.
fn first_error<A, B, C, D>(
    a: YtMusicResult<A>,
    b: YtMusicResult<B>,
    c: YtMusicResult<C>,
    d: YtMusicResult<D>,
) -> Option<YtMusicError> {
    a.err()
        .or_else(|| b.err())
        .or_else(|| c.err())
        .or_else(|| d.err())
}

/// Cliente HTTP reutilizável para o YouTube Music.
#[derive(Clone)]
pub struct YtMusicClient {
    http: reqwest::Client,
    /// Dados de autenticação (quando logado via cookies).
    auth: Option<Arc<Auth>>,
}

impl YtMusicClient {
    /// Cria um novo cliente anônimo (sem login).
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("falha ao construir cliente HTTP");
        Self { http, auth: None }
    }

    /// Cria um cliente autenticado a partir de um arquivo de cookies (Netscape).
    ///
    /// Se os cookies forem inválidos/incompletos, retorna um cliente anônimo.
    pub fn with_cookies(path: &str) -> std::result::Result<Self, auth::AuthError> {
        let mut client = Self::new();
        client.auth = Some(Arc::new(Auth::from_cookie_file(path)?));
        Ok(client)
    }

    /// Indica se o cliente está autenticado (login por cookies bem-sucedido).
    pub fn is_authenticated(&self) -> bool {
        self.auth.is_some()
    }

    /// Monta o objeto `context` obrigatório em toda requisição InnerTube.
    fn context(&self) -> Value {
        json!({
            "client": {
                "clientName": CLIENT_NAME,
                "clientVersion": CLIENT_VERSION,
                "hl": "pt",
                "gl": "BR"
            }
        })
    }

    /// Client context for the Android app identity — see `CLIENT_NAME_ANDROID`.
    fn context_android(&self) -> Value {
        json!({
            "client": {
                "clientName": CLIENT_NAME_ANDROID,
                "clientVersion": CLIENT_VERSION_ANDROID,
                "hl": "pt",
                "gl": "BR"
            }
        })
    }

    /// Executa uma chamada POST para um endpoint InnerTube, com autenticação
    /// por cookies quando logado.
    async fn post(&self, endpoint: &str, body: Value) -> YtMusicResult<Value> {
        self.post_with_auth(endpoint, body, true).await
    }

    /// Same as `post`, but never attaches this client's cookie-based auth
    /// headers, even when signed in. Needed for the ANDROID_MUSIC lyrics
    /// call: InnerTube rejects it with `400 Bad Request` when WEB-style
    /// `SAPISIDHASH` auth headers are combined with the Android client
    /// identity — a mismatched client/auth-mechanism combination.
    async fn post_anonymous(&self, endpoint: &str, body: Value) -> YtMusicResult<Value> {
        self.post_with_auth(endpoint, body, false).await
    }

    async fn post_with_auth(
        &self,
        endpoint: &str,
        body: Value,
        use_auth: bool,
    ) -> YtMusicResult<Value> {
        let url = format!("{BASE}/{endpoint}?prettyPrint=false");
        let mut req = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Origin", auth::ORIGIN)
            .json(&body);

        let authenticated = use_auth && self.auth.is_some();
        if authenticated {
            let a = self
                .auth
                .as_ref()
                .expect("checked by `authenticated` above");
            req = req
                .header("Cookie", a.cookie_header.clone())
                .header("Authorization", a.authorization_header())
                .header("X-Goog-AuthUser", "0")
                .header("X-Origin", auth::ORIGIN);
        }

        let response = req.send().await.map_err(YtMusicError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(authenticated, status, endpoint));
        }
        response
            .json::<Value>()
            .await
            .map_err(YtMusicError::InvalidResponse)
    }

    /// Lista as playlists da biblioteca do usuário logado.
    ///
    /// Requer autenticação (cookies). Retorna erro se o cliente for anônimo.
    pub async fn get_library_playlists(&self) -> YtMusicResult<Vec<Playlist>> {
        if !self.is_authenticated() {
            return Err(YtMusicError::AuthenticationRequired);
        }

        let body = json!({ "context": self.context(), "browseId": "FEmusic_liked_playlists" });
        let data = self.post(EP_BROWSE, body).await?;

        let mut renderers = Vec::new();
        collect_key(&data, "musicTwoRowItemRenderer", &mut renderers);

        // Resposta da biblioteca costuma vir em grid; o primeiro item pode ser
        // o botão "Nova playlist" — ignoramos entradas sem browseId VL*.
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in renderers {
            let browse_id = r
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(|b| b.as_str())
                .unwrap_or("");
            if !browse_id.starts_with("VL") {
                continue;
            }
            if !seen.insert(browse_id.to_string()) {
                continue;
            }
            let title = r.get("title").map(join_runs).unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let subtitle = r.get("subtitle").map(join_runs).unwrap_or_default();
            out.push(Playlist {
                browse_id: browse_id.to_string(),
                title,
                subtitle,
                thumbnail: extract_thumbnail(r),
            });
        }
        Ok(out)
    }

    /// Recomendações da tela inicial (`FEmusic_home`): playlists e álbuns.
    ///
    /// Funciona logado (personalizado) ou anônimo (recomendações genéricas).
    /// Fetches the Home feed, grouped into the same named shelves YouTube
    /// Music itself shows ("Quick picks", "Mixed for you", "Listen again",
    /// ...) instead of one flattened list.
    pub async fn get_home(&self) -> YtMusicResult<Vec<HomeSection>> {
        let body = json!({ "context": self.context(), "browseId": "FEmusic_home" });
        let data = self.post(EP_BROWSE, body).await?;
        Ok(parse_home_sections(&data))
    }

    /// Obtém as principais faixas de um artista a partir do seu `browseId`.
    pub async fn get_artist(&self, browse_id: &str) -> YtMusicResult<Vec<Track>> {
        let body = json!({ "context": self.context(), "browseId": browse_id });
        let data = self.post(EP_BROWSE, body).await?;
        let mut renderers = Vec::new();
        collect_key(&data, "musicResponsiveListItemRenderer", &mut renderers);
        let mut out = Vec::new();
        for r in renderers {
            if let Some(t) = self.parse_track_renderer(r) {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// Monta uma "rádio" (fila de relacionadas) a partir de uma faixa semente.
    pub async fn get_radio(&self, video_id: &str) -> YtMusicResult<Vec<Track>> {
        let body = json!({
            "context": self.context(),
            "videoId": video_id,
            "playlistId": format!("RDAMVM{video_id}"),
            "isAudioOnly": true,
        });
        let data = self.post(EP_NEXT, body).await?;
        let mut renderers = Vec::new();
        collect_key(&data, "playlistPanelVideoRenderer", &mut renderers);
        let mut out = Vec::new();
        let mut skipped_seed = false;
        for r in renderers {
            if let Some(t) = parse_panel_video(r) {
                // Ignora apenas a primeira ocorrência (a própria semente,
                // que costuma vir como o primeiro item); uma repetição
                // legítima mais adiante na fila da rádio é mantida.
                if !skipped_seed && t.video_id == video_id {
                    skipped_seed = true;
                    continue;
                }
                out.push(t);
            }
        }
        Ok(out)
    }

    /// Curte (`like`) ou remove a curtida (`removelike`) de uma faixa.
    /// Requer autenticação.
    pub async fn rate_song(&self, video_id: &str, like: bool) -> YtMusicResult<()> {
        if !self.is_authenticated() {
            return Err(YtMusicError::AuthenticationRequired);
        }
        let endpoint = if like { EP_LIKE } else { EP_REMOVE_LIKE };
        let body = json!({ "context": self.context(), "target": { "videoId": video_id } });
        self.post(endpoint, body).await?;
        Ok(())
    }

    /// Obtém o nome da conta logada (via `account/account_menu`).
    ///
    /// Retorna `None` se anônimo ou se o nome não puder ser extraído.
    pub async fn get_account_name(&self) -> YtMusicResult<Option<String>> {
        if !self.is_authenticated() {
            return Ok(None);
        }
        let body = json!({ "context": self.context() });
        let data = self.post(EP_ACCOUNT_MENU, body).await?;
        Ok(parse::parse_account_name(&data))
    }

    /// Busca completa: músicas, artistas, álbuns e playlists.
    ///
    /// As quatro sub-buscas rodam em paralelo (`tokio::join!`), reduzindo
    /// bastante a latência total. Se todas falharem, o erro é propagado para a
    /// UI; caso contrário, cada parte que falhou retorna vazia (busca parcial).
    pub async fn search(&self, query: &str) -> YtMusicResult<SearchResults> {
        let (songs, artists, albums, playlists) = tokio::join!(
            self.search_songs(query),
            self.search_artists(query),
            self.search_albums(query),
            self.search_playlists(query),
        );

        if songs.is_err() && artists.is_err() && albums.is_err() && playlists.is_err() {
            // Propaga o primeiro erro encontrado para que a UI possa exibi-lo.
            return Err(first_error(songs, artists, albums, playlists)
                .expect("guarda acima garante que ao menos um resultado é Err"));
        }

        Ok(SearchResults {
            songs: songs.unwrap_or_default(),
            artists: artists.unwrap_or_default(),
            albums: albums.unwrap_or_default(),
            playlists: playlists.unwrap_or_default(),
        })
    }

    /// Busca apenas músicas.
    pub async fn search_songs(&self, query: &str) -> YtMusicResult<Vec<Track>> {
        let body = json!({ "context": self.context(), "query": query, "params": FILTER_SONGS });
        let data = self.post(EP_SEARCH, body).await?;
        Ok(self.parse_song_shelf(&data))
    }

    /// Executa uma busca com o `filter` indicado e converte cada item do
    /// `musicShelfRenderer` retornado via `build`. Compartilhado entre
    /// `search_artists`, `search_albums` e `search_playlists`, que só
    /// diferem no tipo de saída e no filtro usado.
    async fn search_shelf<T>(
        &self,
        query: &str,
        filter: &str,
        build: impl Fn(&Value) -> T,
    ) -> YtMusicResult<Vec<T>> {
        let body = json!({ "context": self.context(), "query": query, "params": filter });
        let data = self.post(EP_SEARCH, body).await?;
        let mut out = Vec::new();
        if let Some(shelf) = find_key(&data, "musicShelfRenderer") {
            if let Some(items) = shelf.get("contents").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(r) = item.get("musicResponsiveListItemRenderer") {
                        out.push(build(r));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Busca apenas artistas.
    pub async fn search_artists(&self, query: &str) -> YtMusicResult<Vec<Artist>> {
        self.search_shelf(query, FILTER_ARTISTS, |r| {
            let texts = flex_texts(r);
            Artist {
                browse_id: top_browse_id(r),
                name: texts.first().cloned().unwrap_or_default(),
                subtitle: texts.get(1).cloned().unwrap_or_default(),
                thumbnail: extract_thumbnail(r),
            }
        })
        .await
    }

    /// Busca apenas álbuns. O modelo é o mesmo das playlists: o `browseId`
    /// (`MPRE…`) abre pelo endpoint `browse`, cujo parser já entende o
    /// `musicShelfRenderer` dos álbuns.
    pub async fn search_albums(&self, query: &str) -> YtMusicResult<Vec<Playlist>> {
        self.search_shelf(query, FILTER_ALBUMS, build_playlist)
            .await
    }

    /// Busca apenas playlists.
    pub async fn search_playlists(&self, query: &str) -> YtMusicResult<Vec<Playlist>> {
        self.search_shelf(query, FILTER_PLAYLISTS, build_playlist)
            .await
    }

    /// Faz o parsing do "shelf" de músicas retornado pela busca.
    fn parse_song_shelf(&self, data: &Value) -> Vec<Track> {
        let mut out = Vec::new();
        let Some(shelf) = find_key(data, "musicShelfRenderer") else {
            return out;
        };
        let Some(items) = shelf.get("contents").and_then(|c| c.as_array()) else {
            return out;
        };
        for item in items {
            if let Some(track) = self.parse_track_item(item) {
                out.push(track);
            }
        }
        out
    }

    /// Converte um item de lista (`musicResponsiveListItemRenderer`) em `Track`.
    fn parse_track_item(&self, item: &Value) -> Option<Track> {
        let r = item.get("musicResponsiveListItemRenderer")?;
        self.parse_track_renderer(r)
    }

    /// Converte o conteúdo de um `musicResponsiveListItemRenderer` em `Track`.
    fn parse_track_renderer(&self, r: &Value) -> Option<Track> {
        let texts = flex_texts(r);

        // videoId pode estar em playlistItemData ou em um watchEndpoint.
        let video_id = r
            .get("playlistItemData")
            .and_then(|p| p.get("videoId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                find_key(r, "watchEndpoint")
                    .and_then(|w| w.get("videoId"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })?;

        let title = texts.first().cloned().unwrap_or_default();

        // A segunda coluna costuma ser "Artista • Álbum • Duração".
        let meta = texts.get(1).cloned().unwrap_or_default();
        let segments: Vec<String> = meta
            .split('•')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let artist = segments.first().cloned().unwrap_or_default();
        // Duração: tenta fixedColumn, senão o último segmento no formato tempo.
        let duration = fixed_duration(r)
            .or_else(|| segments.iter().rev().find(|s| s.contains(':')).cloned())
            .unwrap_or_default();
        let album = pick_album(&segments);

        Some(Track {
            video_id,
            title,
            artist,
            album,
            duration_secs: parse_duration(&duration),
            duration,
            thumbnail: extract_thumbnail(r),
        })
    }

    /// Baixa bytes de uma URL (usado para obter a imagem da capa).
    pub async fn fetch_bytes(&self, url: &str) -> YtMusicResult<Vec<u8>> {
        let response = self.http.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(false, status, "artwork"));
        }
        Ok(response.bytes().await?.to_vec())
    }

    /// Obtém as faixas de uma playlist a partir do seu `browseId`.
    ///
    /// Segue *continuations* (paginação) para trazer também as faixas além da
    /// primeira página, até um limite de segurança de páginas/faixas.
    pub async fn get_playlist_tracks(&self, browse_id: &str) -> YtMusicResult<Vec<Track>> {
        // Limites de segurança para não paginar indefinidamente.
        const MAX_PAGES: usize = 8;
        const MAX_TRACKS: usize = 500;

        // Playlists usam prefixo "VL"; browse aceita o id como veio na busca.
        let body = json!({ "context": self.context(), "browseId": browse_id });
        let data = self.post(EP_BROWSE, body).await?;

        let mut out = Vec::new();
        // Playlists comuns: musicPlaylistShelfRenderer. Álbuns: musicShelfRenderer.
        let shelf = find_key(&data, "musicPlaylistShelfRenderer")
            .or_else(|| find_key(&data, "musicShelfRenderer"));
        if let Some(shelf) = shelf {
            if let Some(items) = shelf.get("contents").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(t) = self.parse_track_item(item) {
                        out.push(t);
                    }
                }
            }
        }

        // Paginação: segue os tokens de continuação (formato novo e antigo).
        let mut token = extract_continuation(&data);
        let mut pages = 0;
        while let Some(tok) = token.take() {
            if pages >= MAX_PAGES || out.len() >= MAX_TRACKS {
                break;
            }
            pages += 1;

            let body = json!({ "context": self.context(), "continuation": tok });
            let Ok(cont) = self.post(EP_BROWSE, body).await else {
                break;
            };

            let before = out.len();
            let items = find_key(&cont, "continuationItems")
                .and_then(|c| c.as_array())
                .or_else(|| find_key(&cont, "contents").and_then(|c| c.as_array()));
            if let Some(items) = items {
                for item in items {
                    if let Some(t) = self.parse_track_item(item) {
                        out.push(t);
                    }
                }
            }
            // Sem progresso => encerra para evitar laço infinito.
            if out.len() == before {
                break;
            }
            token = extract_continuation(&cont);
        }

        Ok(out)
    }

    /// Obtém a letra de uma música a partir do seu `videoId`.
    /// Retorna `None` quando não houver letra disponível.
    pub async fn get_lyrics(&self, video_id: &str) -> YtMusicResult<Option<Lyrics>> {
        // 1) endpoint "next" -> descobrir a aba de letras (browseId "MPLY...").
        let next_body = json!({ "context": self.context(), "videoId": video_id });
        let next = self.post(EP_NEXT, next_body).await?;

        let mut lyrics_id: Option<String> = None;
        if let Some(tabs) = find_key(&next, "tabs").and_then(|t| t.as_array()) {
            for tab in tabs {
                let Some(tr) = tab.get("tabRenderer") else {
                    continue;
                };
                let bid = tr
                    .get("endpoint")
                    .and_then(|e| e.get("browseEndpoint"))
                    .and_then(|b| b.get("browseId"))
                    .and_then(|b| b.as_str());
                let title = tr.get("title").and_then(|t| t.as_str()).unwrap_or("");
                if let Some(bid) = bid {
                    if bid.starts_with("MPLY")
                        || title.eq_ignore_ascii_case("Lyrics")
                        || title.eq_ignore_ascii_case("Letra")
                    {
                        lyrics_id = Some(bid.to_string());
                        break;
                    }
                }
            }
        }

        let Some(lyrics_id) = lyrics_id else {
            return Ok(None);
        };

        // 2a) Tenta letras sincronizadas (timestamps por linha), disponíveis
        // apenas com a identidade de cliente do app Android — o WEB_REMIX
        // só retorna o texto plano do Musixmatch para esse mesmo browseId.
        let android_body = json!({ "context": self.context_android(), "browseId": lyrics_id });
        if let Ok(data) = self.post_anonymous(EP_BROWSE, android_body).await {
            let lines = parse_timed_lyrics(&data);
            if !lines.is_empty() {
                return Ok(Some(Lyrics::Synced(lines)));
            }
        }

        // 2b) Sem letras sincronizadas: volta para o texto plano (WEB_REMIX).
        let body = json!({ "context": self.context(), "browseId": lyrics_id });
        let data = self.post(EP_BROWSE, body).await?;
        if let Some(desc) = find_key(&data, "description") {
            let text = join_runs(desc);
            if !text.is_empty() {
                return Ok(Some(Lyrics::Plain(text)));
            }
        }
        Ok(None)
    }
}

impl Default for YtMusicClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Converte um item de `musicShelfRenderer` em `Playlist`. Compartilhado por
/// `search_albums` e `search_playlists`, que montam o mesmo tipo a partir do
/// mesmo formato de item.
fn build_playlist(r: &Value) -> Playlist {
    let texts = flex_texts(r);
    Playlist {
        browse_id: top_browse_id(r),
        title: texts.first().cloned().unwrap_or_default(),
        subtitle: texts.get(1).cloned().unwrap_or_default(),
        thumbnail: extract_thumbnail(r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn authenticated_unauthorized_response_means_expired_session() {
        let error = classify_status(true, StatusCode::UNAUTHORIZED, EP_BROWSE);
        assert!(matches!(error, YtMusicError::SessionExpired { .. }));
    }

    #[test]
    fn anonymous_forbidden_response_remains_an_http_error() {
        let error = classify_status(false, StatusCode::FORBIDDEN, EP_BROWSE);
        assert!(matches!(error, YtMusicError::HttpStatus { .. }));
    }
}
