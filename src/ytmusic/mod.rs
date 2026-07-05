//! Cliente da API interna (InnerTube) do YouTube Music.
//!
//! Este módulo implementa diretamente as chamadas HTTP para a API
//! `music.youtube.com/youtubei/v1/*`, sem necessidade de autenticação.
//! Ele oferece busca de músicas/artistas/playlists, listagem de faixas de
//! uma playlist e obtenção de letras.

pub mod auth;
pub mod models;
mod parse;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub use auth::Auth;
pub use models::{Artist, Playlist, SearchResults, Track};
use parse::*;

const BASE: &str = "https://music.youtube.com/youtubei/v1";
const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_VERSION: &str = "1.20240101.01.00";

// Parâmetros de filtro de busca (identificados na API do YouTube Music).
const FILTER_SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";
const FILTER_ARTISTS: &str = "EgWKAQIgAWoKEAkQChAFEAMQBA%3D%3D";
const FILTER_PLAYLISTS: &str = "EgWKAQIoAWoKEAkQChAFEAMQBA%3D%3D";

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
            .build()
            .expect("falha ao construir cliente HTTP");
        Self { http, auth: None }
    }

    /// Cria um cliente autenticado a partir de um arquivo de cookies (Netscape).
    ///
    /// Se os cookies forem inválidos/incompletos, retorna um cliente anônimo.
    pub fn with_cookies(path: &str) -> Self {
        let mut client = Self::new();
        client.auth = Auth::from_cookie_file(path).ok().map(Arc::new);
        client
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

    /// Executa uma chamada POST para um endpoint InnerTube.
    async fn post(&self, endpoint: &str, body: Value) -> Result<Value> {
        let url = format!("{BASE}/{endpoint}?prettyPrint=false");
        let mut req = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Origin", auth::ORIGIN)
            .json(&body);

        // Adiciona cabeçalhos de autenticação quando logado.
        if let Some(a) = &self.auth {
            req = req
                .header("Cookie", a.cookie_header.clone())
                .header("Authorization", a.authorization_header())
                .header("X-Goog-AuthUser", "0")
                .header("X-Origin", auth::ORIGIN);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("HTTP {} em {endpoint}", resp.status()));
        }
        Ok(resp.json::<Value>().await?)
    }

    /// Lista as playlists da biblioteca do usuário logado.
    ///
    /// Requer autenticação (cookies). Retorna erro se o cliente for anônimo.
    pub async fn get_library_playlists(&self) -> Result<Vec<Playlist>> {
        if !self.is_authenticated() {
            return Err(anyhow!(
                "não autenticado: configure YTM_COOKIES para ver a biblioteca"
            ));
        }

        let body = json!({ "context": self.context(), "browseId": "FEmusic_liked_playlists" });
        let data = self.post("browse", body).await?;

        let mut renderers = Vec::new();
        collect_key(&data, "musicTwoRowItemRenderer", &mut renderers);

        let mut out = Vec::new();
        for r in renderers {
            let browse_id = r
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(|b| b.as_str())
                .unwrap_or("");
            // Só playlists reais (ignora o botão "Nova playlist" e afins).
            if !browse_id.starts_with("VL") {
                continue;
            }
            let title = r.get("title").map(join_runs).unwrap_or_default();
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
    pub async fn get_home(&self) -> Result<Vec<Playlist>> {
        let body = json!({ "context": self.context(), "browseId": "FEmusic_home" });
        let data = self.post("browse", body).await?;

        let mut renderers = Vec::new();
        collect_key(&data, "musicTwoRowItemRenderer", &mut renderers);

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in renderers {
            let browse_id = r
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(|b| b.as_str())
                .unwrap_or("");
            // Apenas playlists (VL) e álbuns (MPRE) navegáveis; sem duplicatas.
            if !(browse_id.starts_with("VL") || browse_id.starts_with("MPRE")) {
                continue;
            }
            if !seen.insert(browse_id.to_string()) {
                continue;
            }
            let title = r.get("title").map(join_runs).unwrap_or_default();
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

    /// Obtém as principais faixas de um artista a partir do seu `browseId`.
    pub async fn get_artist(&self, browse_id: &str) -> Result<Vec<Track>> {
        let body = json!({ "context": self.context(), "browseId": browse_id });
        let data = self.post("browse", body).await?;
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
    pub async fn get_radio(&self, video_id: &str) -> Result<Vec<Track>> {
        let body = json!({
            "context": self.context(),
            "videoId": video_id,
            "playlistId": format!("RDAMVM{video_id}"),
            "isAudioOnly": true,
        });
        let data = self.post("next", body).await?;
        let mut renderers = Vec::new();
        collect_key(&data, "playlistPanelVideoRenderer", &mut renderers);
        let mut out = Vec::new();
        for r in renderers {
            if let Some(t) = parse_panel_video(r) {
                // Ignora a própria semente (costuma ser o primeiro item).
                if t.video_id != video_id {
                    out.push(t);
                }
            }
        }
        Ok(out)
    }

    /// Curte (`like`) ou remove a curtida (`removelike`) de uma faixa.
    /// Requer autenticação.
    pub async fn rate_song(&self, video_id: &str, like: bool) -> Result<()> {
        if !self.is_authenticated() {
            return Err(anyhow!(
                "não autenticado: conecte sua conta para curtir faixas"
            ));
        }
        let endpoint = if like { "like/like" } else { "like/removelike" };
        let body = json!({ "context": self.context(), "target": { "videoId": video_id } });
        self.post(endpoint, body).await?;
        Ok(())
    }

    /// Obtém o nome da conta logada (via `account/account_menu`).
    ///
    /// Retorna `None` se anônimo ou se o nome não puder ser extraído.
    pub async fn get_account_name(&self) -> Result<Option<String>> {
        if !self.is_authenticated() {
            return Ok(None);
        }
        let body = json!({ "context": self.context() });
        let data = self.post("account/account_menu", body).await?;
        if let Some(name) = find_key(&data, "accountName") {
            let text = join_runs(name);
            if !text.is_empty() {
                return Ok(Some(text));
            }
        }
        Ok(None)
    }

    /// Busca completa: músicas, artistas e playlists.
    ///
    /// As três sub-buscas rodam em paralelo (`tokio::join!`), reduzindo bastante
    /// a latência total. Se as três falharem, o erro é propagado para a UI; caso
    /// contrário, cada parte que falhou retorna vazia (busca parcial).
    pub async fn search(&self, query: &str) -> Result<SearchResults> {
        let (songs, artists, playlists) = tokio::join!(
            self.search_songs(query),
            self.search_artists(query),
            self.search_playlists(query),
        );

        if songs.is_err() && artists.is_err() && playlists.is_err() {
            // Propaga o primeiro erro encontrado para que a UI possa exibi-lo.
            return Err(songs.err().or(artists.err()).or(playlists.err()).unwrap());
        }

        Ok(SearchResults {
            songs: songs.unwrap_or_default(),
            artists: artists.unwrap_or_default(),
            playlists: playlists.unwrap_or_default(),
        })
    }

    /// Busca apenas músicas.
    pub async fn search_songs(&self, query: &str) -> Result<Vec<Track>> {
        let body = json!({ "context": self.context(), "query": query, "params": FILTER_SONGS });
        let data = self.post("search", body).await?;
        Ok(self.parse_song_shelf(&data))
    }

    /// Busca apenas artistas.
    pub async fn search_artists(&self, query: &str) -> Result<Vec<Artist>> {
        let body = json!({ "context": self.context(), "query": query, "params": FILTER_ARTISTS });
        let data = self.post("search", body).await?;
        let mut out = Vec::new();
        if let Some(shelf) = find_key(&data, "musicShelfRenderer") {
            if let Some(items) = shelf.get("contents").and_then(|c| c.as_array()) {
                for item in items {
                    let Some(r) = item.get("musicResponsiveListItemRenderer") else {
                        continue;
                    };
                    let texts = flex_texts(r);
                    let browse_id = top_browse_id(r);
                    out.push(Artist {
                        browse_id,
                        name: texts.first().cloned().unwrap_or_default(),
                        subtitle: texts.get(1).cloned().unwrap_or_default(),
                        thumbnail: extract_thumbnail(r),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Busca apenas playlists.
    pub async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        let body = json!({ "context": self.context(), "query": query, "params": FILTER_PLAYLISTS });
        let data = self.post("search", body).await?;
        let mut out = Vec::new();
        if let Some(shelf) = find_key(&data, "musicShelfRenderer") {
            if let Some(items) = shelf.get("contents").and_then(|c| c.as_array()) {
                for item in items {
                    let Some(r) = item.get("musicResponsiveListItemRenderer") else {
                        continue;
                    };
                    let texts = flex_texts(r);
                    let browse_id = top_browse_id(r);
                    out.push(Playlist {
                        browse_id,
                        title: texts.first().cloned().unwrap_or_default(),
                        subtitle: texts.get(1).cloned().unwrap_or_default(),
                        thumbnail: extract_thumbnail(r),
                    });
                }
            }
        }
        Ok(out)
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
        let album = segments
            .iter()
            .skip(1)
            .find(|s| !s.contains(':'))
            .cloned()
            .unwrap_or_default();

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
    pub async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("HTTP {} ao baixar imagem", resp.status()));
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// Obtém as faixas de uma playlist a partir do seu `browseId`.
    ///
    /// Segue *continuations* (paginação) para trazer também as faixas além da
    /// primeira página, até um limite de segurança de páginas/faixas.
    pub async fn get_playlist_tracks(&self, browse_id: &str) -> Result<Vec<Track>> {
        // Limites de segurança para não paginar indefinidamente.
        const MAX_PAGES: usize = 8;
        const MAX_TRACKS: usize = 500;

        // Playlists usam prefixo "VL"; browse aceita o id como veio na busca.
        let body = json!({ "context": self.context(), "browseId": browse_id });
        let data = self.post("browse", body).await?;

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
            let Ok(cont) = self.post("browse", body).await else {
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
    pub async fn get_lyrics(&self, video_id: &str) -> Result<Option<String>> {
        // 1) endpoint "next" -> descobrir a aba de letras (browseId "MPLY...").
        let next_body = json!({ "context": self.context(), "videoId": video_id });
        let next = self.post("next", next_body).await?;

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

        // 2) browse na aba de letras.
        let body = json!({ "context": self.context(), "browseId": lyrics_id });
        let data = self.post("browse", body).await?;
        if let Some(desc) = find_key(&data, "description") {
            let text = join_runs(desc);
            if !text.is_empty() {
                return Ok(Some(text));
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
