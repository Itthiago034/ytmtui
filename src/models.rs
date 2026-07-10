//! Modelos compartilhados entre a UI e os provedores de música.
//!
//! Todo provedor converte suas respostas para estes tipos antes de qualquer
//! coisa chegar à interface (ver `crate::provider::MusicProvider`), então a
//! UI não conhece formatos específicos de nenhum serviço.
//!
//! Alguns campos (ex.: thumbnails de playlist/artista) ainda não são exibidos
//! na interface atual, mas fazem parte do modelo para uso futuro.
#![allow(dead_code)]

/// Representa uma faixa (música).
///
/// Serializável para persistir o histórico local de reprodução
/// (`recent.json`) entre sessões.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Track {
    /// Identificador da faixa no provedor de origem. O nome do campo é
    /// histórico (YouTube) e persiste no `recent.json` — renomear exigiria
    /// um alias de serde; fica para quando houver um segundo provedor.
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
    /// Álbuns (browseId `MPRE…`; abrem pelo mesmo `browse` das playlists).
    pub albums: Vec<Playlist>,
}

/// One karaoke-style timed line of lyrics.
#[derive(Debug, Clone, Default)]
pub struct LyricLine {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Result of a lyrics fetch: real per-line timestamps when the provider
/// exposes them for the track, or plain unsynced text otherwise.
#[derive(Debug, Clone)]
pub enum Lyrics {
    Synced(Vec<LyricLine>),
    Plain(String),
}

/// A named shelf on the Home screen (e.g. "Quick picks", "Mixed for you"),
/// as the provider itself groups recommendations.
#[derive(Debug, Clone, Default)]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<Playlist>,
}
