//! Provedor de mentira para testes: devolve dados enlatados configuráveis e
//! nunca toca em rede, disco ou processos externos. Prova que a UI funciona
//! contra o contrato [`MusicProvider`](super::MusicProvider) puro, sem nada
//! de YouTube.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use super::{
    Capabilities, MusicProvider, ProviderError, Result, SignInAccount, SignInPreview, SignInSummary,
};
use crate::models::{HomeSection, Lyrics, Playlist, SearchResults, Track};

/// Dados enlatados; campos públicos para os testes montarem cenários.
pub struct MockProvider {
    pub search_results: SearchResults,
    pub home_sections: Vec<HomeSection>,
    pub library: Vec<Playlist>,
    pub playlist_tracks: Vec<Track>,
    pub artist_tracks: Vec<Track>,
    pub radio_tracks: Vec<Track>,
    pub lyrics: Option<Lyrics>,
    pub account: Option<String>,
    /// O que este provedor "suporta" — os testes desligam ações para provar
    /// que a UI as esconde em vez de chamá-las.
    pub capabilities: Capabilities,
    /// Quando setado, todo método async devolve este erro (por mensagem).
    pub fail_with: Option<String>,
    /// Quando `true`, todo método async falha com `SessionExpired`.
    pub expire_session: bool,
    authenticated: AtomicBool,
    next_preview_id: AtomicU64,
    pending_preview_id: Mutex<Option<u64>>,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self {
            search_results: SearchResults::default(),
            home_sections: Vec::new(),
            library: Vec::new(),
            playlist_tracks: Vec::new(),
            artist_tracks: Vec::new(),
            radio_tracks: Vec::new(),
            lyrics: None,
            account: None,
            capabilities: Capabilities::all(),
            fail_with: None,
            expire_session: false,
            authenticated: AtomicBool::new(false),
            next_preview_id: AtomicU64::new(1),
            pending_preview_id: Mutex::new(None),
        }
    }
}

impl MockProvider {
    pub fn authenticated() -> Self {
        let mock = Self::default();
        mock.authenticated.store(true, Ordering::Relaxed);
        mock
    }

    fn outcome<T>(&self, value: T) -> Result<T> {
        if self.expire_session {
            return Err(ProviderError::SessionExpired);
        }
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
        self.capabilities
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
            account_name: "Mock Account 1".to_string(),
            account_index: 0,
        })
    }

    fn prepare_sign_in(
        &self,
        _progress: &(dyn Fn(String) + Send + Sync),
    ) -> std::result::Result<SignInPreview, String> {
        let id = self.next_preview_id.fetch_add(1, Ordering::Relaxed);
        *self.pending_preview_id.lock().unwrap() = Some(id);
        Ok(SignInPreview {
            id,
            method: "mock".to_string(),
            profile_label: None,
            accounts: vec![
                SignInAccount {
                    index: 0,
                    name: "Mock Account 1".to_string(),
                    handle: Some("@mock1".to_string()),
                },
                SignInAccount {
                    index: 1,
                    name: "Mock Account 2".to_string(),
                    handle: Some("@mock2".to_string()),
                },
            ],
            current_account_name: self.account.clone(),
        })
    }

    fn activate_sign_in(
        &self,
        preview_id: u64,
        account_index: u8,
    ) -> std::result::Result<SignInSummary, String> {
        let account_name = match account_index {
            0 => "Mock Account 1",
            1 => "Mock Account 2",
            _ => return Err("invalid mock account index".to_string()),
        };
        let mut pending = self.pending_preview_id.lock().unwrap();
        if *pending != Some(preview_id) {
            return Err("sign-in preview is no longer pending".to_string());
        }
        *pending = None;
        self.authenticated.store(true, Ordering::Relaxed);
        Ok(SignInSummary {
            method: "mock".to_string(),
            credentials_path: None,
            account_name: account_name.to_string(),
            account_index,
        })
    }

    fn cancel_sign_in(&self, preview_id: u64) {
        let mut pending = self.pending_preview_id.lock().unwrap();
        if *pending == Some(preview_id) {
            *pending = None;
        }
    }

    fn resolve_playable(&self, _track: &Track) -> anyhow::Result<PathBuf> {
        anyhow::bail!("o mock não resolve áudio")
    }
}
