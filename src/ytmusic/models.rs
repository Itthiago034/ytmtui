//! Modelos de dados usados pelo cliente do YouTube Music.
//!
//! Alguns campos (ex.: thumbnails de playlist/artista) ainda não são exibidos
//! na interface atual, mas fazem parte do modelo para uso futuro.
#![allow(dead_code)]

/// Representa uma faixa (música) do YouTube Music.
#[derive(Debug, Clone, Default)]
pub struct Track {
    /// Identificador do vídeo no YouTube (usado para streaming).
    pub video_id: String,
    /// Título da música.
    pub title: String,
    /// Nome do(s) artista(s).
    pub artist: String,
    /// Nome do álbum (quando disponível).
    pub album: String,
    /// Duração formatada, ex.: "4:27".
    pub duration: String,
    /// Duração em segundos (0 quando desconhecida).
    pub duration_secs: u64,
    /// URL da capa/thumbnail em melhor resolução.
    pub thumbnail: Option<String>,
}

impl Track {
    /// Retorna a URL de reprodução no YouTube Music para esta faixa.
    pub fn watch_url(&self) -> String {
        format!("https://music.youtube.com/watch?v={}", self.video_id)
    }
}

/// Representa uma playlist ou álbum.
#[derive(Debug, Clone, Default)]
pub struct Playlist {
    /// browseId / playlistId usado para buscar o conteúdo.
    pub browse_id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: Option<String>,
}

/// Um artista retornado na busca.
#[derive(Debug, Clone, Default)]
pub struct Artist {
    pub browse_id: String,
    pub name: String,
    pub subtitle: String,
    pub thumbnail: Option<String>,
}

/// Resultado agregado de uma busca.
#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub songs: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub artists: Vec<Artist>,
}

/// One karaoke-style timed line of lyrics.
#[derive(Debug, Clone, Default)]
pub struct LyricLine {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Result of a lyrics fetch: real per-line timestamps when the API exposes
/// them for the track (see `YtMusicClient::get_lyrics`), or the plain
/// Musixmatch-sourced text otherwise.
#[derive(Debug, Clone)]
pub enum Lyrics {
    Synced(Vec<LyricLine>),
    Plain(String),
}

/// A named shelf on the Home screen (e.g. "Quick picks", "Mixed for you"),
/// as YouTube Music itself groups recommendations.
#[derive(Debug, Clone, Default)]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<Playlist>,
}
