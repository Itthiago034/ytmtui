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
