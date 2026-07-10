//! Provedor de mentira para testes: devolve dados enlatados configuráveis e
//! nunca toca em rede, disco ou processos externos. Prova que a UI funciona
//! contra o contrato [`MusicProvider`](super::MusicProvider) puro, sem nada
//! de YouTube.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use super::{Capabilities, MusicProvider, ProviderError, Result, SignInSummary};
use crate::models::{HomeSection, Lyrics, Playlist, SearchResults, Track};

/// Dados enlatados; campos públicos para os testes montarem cenários.
#[derive(Default)]
pub struct MockProvider {
    pub search_results: SearchResults,
    pub home_sections: Vec<HomeSection>,
    pub library: Vec<Playlist>,
    pub playlist_tracks: Vec<Track>,
    pub artist_tracks: Vec<Track>,
    pub radio_tracks: Vec<Track>,
    pub lyrics: Option<Lyrics>,
    pub account: Option<String>,
    /// Quando setado, todo método async devolve este erro (por mensagem).
    pub fail_with: Option<String>,
    authenticated: AtomicBool,
}

impl MockProvider {
    pub fn authenticated() -> Self {
        let mock = Self::default();
        mock.authenticated.store(true, Ordering::Relaxed);
        mock
    }

    fn outcome<T>(&self, value: T) -> Result<T> {
        match &self.fail_with {
            Some(message) => Err(ProviderError::Message(message.clone())),
            None => Ok(value),
        }
    }
}

#[async_trait]
impl MusicProvider for MockProvider {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn display_name(&self) -> &'static str {
        "Mock Provider"
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
        self.authenticated.load(Ordering::Relaxed)
    }

    async fn search(&self, _query: &str) -> Result<SearchResults> {
        self.outcome(self.search_results.clone())
    }

    async fn home(&self) -> Result<Vec<HomeSection>> {
        self.outcome(self.home_sections.clone())
    }

    async fn library_playlists(&self) -> Result<Vec<Playlist>> {
        self.outcome(self.library.clone())
    }

    async fn playlist_tracks(&self, _browse_id: &str) -> Result<Vec<Track>> {
        self.outcome(self.playlist_tracks.clone())
    }

    async fn artist_tracks(&self, _browse_id: &str) -> Result<Vec<Track>> {
        self.outcome(self.artist_tracks.clone())
    }

    async fn radio(&self, _track_id: &str) -> Result<Vec<Track>> {
        self.outcome(self.radio_tracks.clone())
    }

    async fn lyrics(&self, _track_id: &str) -> Result<Option<Lyrics>> {
        self.outcome(self.lyrics.clone())
    }

    async fn rate_track(&self, _track_id: &str, _like: bool) -> Result<()> {
        self.outcome(())
    }

    async fn account_name(&self) -> Result<Option<String>> {
        self.outcome(self.account.clone())
    }

    async fn fetch_artwork(&self, _url: &str) -> Result<Vec<u8>> {
        self.outcome(Vec::new())
    }

    fn sign_in(
        &self,
        _progress: &(dyn Fn(String) + Send + Sync),
    ) -> std::result::Result<SignInSummary, String> {
        self.authenticated.store(true, Ordering::Relaxed);
        Ok(SignInSummary {
            method: "mock".to_string(),
            credentials_path: None,
        })
    }

    fn resolve_playable(&self, _track: &Track) -> anyhow::Result<PathBuf> {
        anyhow::bail!("o mock não resolve áudio")
    }
}
