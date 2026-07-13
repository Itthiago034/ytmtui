//! Contrato entre a interface e os provedores de música.
//!
//! A UI só conhece [`MusicProvider`] e os modelos de `crate::models`; todo o
//! resto (InnerTube, cookies, yt-dlp…) vive dentro da implementação de cada
//! provedor. As [`Capabilities`] dizem quais ações fazem sentido — a UI
//! esconde curtir/letra/rádio/biblioteca quando o provedor não os suporta —
//! e o fluxo de autenticação é genérico o bastante para o mock de teste não
//! precisar saber o que é um cookie.

pub mod mock;

use std::path::PathBuf;

use async_trait::async_trait;

use crate::models::{HomeSection, Lyrics, Playlist, SearchResults, Track};

pub type Result<T> = std::result::Result<T, ProviderError>;

/// Erro na fronteira do provedor, já em termos que a UI entende.
#[derive(Debug)]
pub enum ProviderError {
    /// A sessão autenticada foi rejeitada; a UI oferece reautenticação.
    SessionExpired,
    /// Qualquer outra falha, com mensagem legível para a status bar.
    Message(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionExpired => write!(f, "session expired"),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Estado de autenticação do ponto de vista da UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    Anonymous,
    Authenticated,
    Expired,
    InvalidCredentials,
}

impl AuthState {
    pub fn is_authenticated(self) -> bool {
        matches!(self, Self::Authenticated)
    }
}

/// Ações opcionais que um provedor pode ou não oferecer. A UI usa isto para
/// decidir o que exibir; nada aqui é consultado no caminho quente de render.
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    /// Tem recomendações para a tela Início.
    pub home: bool,
    /// Tem biblioteca do usuário (exige autenticação).
    pub library: bool,
    /// Fornece letras.
    pub lyrics: bool,
    /// Gera rádio de faixas semelhantes (autoplay e rádio da busca).
    pub radio: bool,
    /// Suporta curtir/descurtir faixas.
    pub likes: bool,
    /// Tem fluxo interativo de sign-in (tecla `g`).
    pub sign_in: bool,
}

impl Capabilities {
    pub const fn all() -> Self {
        Self {
            home: true,
            library: true,
            lyrics: true,
            radio: true,
            likes: true,
            sign_in: true,
        }
    }

    pub const fn none() -> Self {
        Self {
            home: false,
            library: false,
            lyrics: false,
            radio: false,
            likes: false,
            sign_in: false,
        }
    }
}

/// Resultado de um sign-in bem-sucedido.
#[derive(Debug, Clone)]
pub struct SignInSummary {
    /// Como a sessão foi obtida, para feedback ao usuário (ex.: o navegador
    /// de onde os cookies vieram).
    pub method: String,
    /// Caminho de credenciais a persistir na configuração, quando houver.
    pub credentials_path: Option<String>,
    /// Nome de exibição da conta ativada.
    pub account_name: String,
    /// Índice enviado em `X-Goog-AuthUser` para selecionar a conta.
    pub account_index: u8,
}

/// Conta Google disponível dentro de uma sessão de cookies autenticada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInAccount {
    /// Valor enviado em `X-Goog-AuthUser` para selecionar esta conta.
    pub index: u8,
    /// Nome de exibição informado pelo menu de conta do YouTube Music.
    pub name: String,
    /// Handle público do canal, quando disponível.
    pub handle: Option<String>,
}

/// Resultado seguro da preparação de um sign-in, antes de qualquer mudança
/// na sessão ativa. Caminhos e conteúdo de credenciais permanecem privados no
/// provedor até a confirmação.
#[derive(Debug, Clone)]
pub struct SignInPreview {
    pub id: u64,
    pub method: String,
    pub profile_label: Option<String>,
    pub accounts: Vec<SignInAccount>,
    pub current_account_name: Option<String>,
}

/// Um serviço de música por trás da interface.
///
/// Contratos de execução:
/// - métodos `async` são baratos de cancelar e rodam em tasks tokio;
/// - preparação, ativação e [`Self::resolve_playable`] são **bloqueantes**
///   (processos externos ou disco) e devem rodar em `spawn_blocking`;
/// - implementações são compartilhadas via `Arc` entre tasks concorrentes,
///   daí `Send + Sync` e interior mutability para reautenticação.
#[async_trait]
pub trait MusicProvider: Send + Sync {
    /// Identificador estável (config, logs), ex.: `"ytmusic"`.
    fn id(&self) -> &'static str;
    /// Nome exibido ao usuário, ex.: `"YouTube Music"`.
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    /// Há uma sessão autenticada ativa agora (pode mudar após ativação).
    fn is_authenticated(&self) -> bool;

    async fn search(&self, query: &str) -> Result<SearchResults>;
    async fn home(&self) -> Result<Vec<HomeSection>>;
    async fn library_playlists(&self) -> Result<Vec<Playlist>>;
    /// Faixas de uma playlist ou álbum (`browse_id` vem dos modelos).
    async fn playlist_tracks(&self, browse_id: &str) -> Result<Vec<Track>>;
    /// Principais faixas de um artista.
    async fn artist_tracks(&self, browse_id: &str) -> Result<Vec<Track>>;
    /// Rádio de faixas semelhantes à faixa dada.
    async fn radio(&self, track_id: &str) -> Result<Vec<Track>>;
    async fn lyrics(&self, track_id: &str) -> Result<Option<Lyrics>>;
    async fn rate_track(&self, track_id: &str, like: bool) -> Result<()>;
    /// Nome de exibição da conta autenticada, se a API o fornecer.
    async fn account_name(&self) -> Result<Option<String>>;
    /// Bytes de uma imagem de capa (URL vinda dos modelos deste provedor).
    async fn fetch_artwork(&self, url: &str) -> Result<Vec<u8>>;

    /// Prepara credenciais e enumera contas sem alterar a sessão ativa.
    fn prepare_sign_in(
        &self,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> std::result::Result<SignInPreview, String>;

    /// Confirma uma preparação pendente e publica a conta selecionada.
    fn activate_sign_in(
        &self,
        preview_id: u64,
        account_index: u8,
    ) -> std::result::Result<SignInSummary, String>;

    /// Descarta somente a preparação pendente identificada por `preview_id`.
    fn cancel_sign_in(&self, preview_id: u64);

    /// Resolve a faixa em um arquivo de áudio local pronto para tocar.
    /// Bloqueante (download/remux); o cache fica a cargo do provedor.
    fn resolve_playable(&self, track: &Track) -> anyhow::Result<PathBuf>;
}
