//! Estado central da aplicação e lógica de coordenação.
//!
//! O app fala com o serviço de música exclusivamente pelo contrato
//! [`MusicProvider`]; o provedor concreto (YouTube Music) só aparece na
//! raiz de composição ([`App::new`]).
//!
//! Este módulo guarda o *estado* — a struct [`App`] e os tipos que a UI lê
//! ([`Section`], [`Focus`], [`RepeatMode`], [`Msg`]) — junto dos dois
//! construtores. O *comportamento* está dividido por assunto nos submódulos
//! abaixo, cada um abrindo seu próprio `impl App`:
//!
//! | Submódulo | Assunto |
//! |---|---|
//! | [`tasks`] | Tarefas em voo, drain de `Msg`, `tick` periódico |
//! | [`animation`] | Janelas de animação e tiers de redraw |
//! | [`queue`] | Fila, shuffle em ciclo, repeat, próxima faixa |
//! | [`playback`] | Iniciar/trocar faixa, seek, capa da faixa atual |
//! | [`search`] | Busca e abertura de artistas/álbuns/playlists |
//! | [`home`] | Shelves de recomendação, biblioteca, histórico recente |
//! | [`navigation`] | Movimentação de seleção, seções e barra lateral |
//! | [`settings`] | Preferências, tema e identidade da conta |
//! | [`authentication`] | Sign-in em duas fases (preparar → ativar) |
//!
//! Métodos chamados entre submódulos são `pub(super)`: visíveis em todo o
//! módulo `app` sem virar superfície pública do crate.

mod animation;
mod authentication;
mod home;
mod navigation;
mod playback;
mod queue;
mod search;
mod settings;
mod tasks;
#[cfg(test)]
mod testing;

pub use crate::provider::AuthState;
pub use authentication::AuthenticationFlow;

use std::path::PathBuf;
use std::sync::Arc;

use ratatui::widgets::ListState;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::config::{AnimationSpeed, ArtworkMode, Config, HomeDensity, VisualizerStyle};
use crate::home::{HomeCardPayload, HomeDirection, HomeView};
use crate::models::{Artist, Playlist, SearchResults, Track};
use crate::player::AudioPlayer;
use crate::provider::{MusicProvider, ProviderError, SignInPreview};
use crate::visualizer::SpectrumAnalyzer;

/// Seções da barra lateral (também define o conteúdo do painel principal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Inicio,
    Buscar,
    Biblioteca,
    Playlists,
    Artistas,
    Fila,
    Letra,
    Ajuda,
}

impl Section {
    /// Ordem de exibição na barra lateral.
    pub const ALL: [Section; 8] = [
        Section::Inicio,
        Section::Buscar,
        Section::Biblioteca,
        Section::Playlists,
        Section::Artistas,
        Section::Fila,
        Section::Letra,
        Section::Ajuda,
    ];

    /// Label shown in the navigation column and the narrow-layout header.
    pub fn label(&self) -> &str {
        match self {
            Section::Inicio => "Home",
            Section::Buscar => "Search",
            Section::Biblioteca => "Library",
            Section::Playlists => "Playlists",
            Section::Artistas => "Artists",
            Section::Fila => "Queue",
            Section::Letra => "Lyrics",
            Section::Ajuda => "Help",
        }
    }

    /// Glyph shown next to the label in the navigation column and panel
    /// titles. Single-column Unicode only (no Nerd Font/emoji), so alignment
    /// holds on any terminal font.
    pub fn icon(&self) -> &'static str {
        match self {
            Section::Inicio => "⌂",
            Section::Buscar => "⌕",
            Section::Biblioteca => "♪",
            Section::Playlists => "♫",
            Section::Artistas => "◆",
            Section::Fila => "≡",
            Section::Letra => "¶",
            Section::Ajuda => "?",
        }
    }

    /// Índice desta seção na barra lateral.
    pub fn index(&self) -> usize {
        Section::ALL.iter().position(|s| s == self).unwrap_or(0)
    }
}

/// Onde está o foco do teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Main,
}

/// Modo de repetição da fila.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    /// Alterna ciclicamente: Off → All → One → Off.
    pub fn next(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }

    /// Rótulo curto para exibição.
    pub fn label(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::All => "todos",
            RepeatMode::One => "1",
        }
    }

    fn as_config(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        }
    }

    fn from_config(s: &str) -> Self {
        match s {
            "all" => RepeatMode::All,
            "one" => RepeatMode::One,
            _ => RepeatMode::Off,
        }
    }
}

/// Mensagens enviadas pelas tasks assíncronas de volta ao loop principal.
#[allow(dead_code)] // `Status` é reservado para mensagens de progresso futuras.
pub enum Msg {
    SearchResults(SearchResults),
    LibraryPlaylists(Vec<Playlist>),
    LibraryPlaylistsForSession {
        session_generation: u64,
        playlists: Vec<Playlist>,
    },
    HomeSections(Vec<crate::models::HomeSection>),
    HomeSectionsForSession {
        session_generation: u64,
        sections: Vec<crate::models::HomeSection>,
    },
    /// `load_home` failed with something other than `SessionExpired`. Kept
    /// distinct from the generic `Error` so its handler can leave `self.home`
    /// untouched — cached shelves stay on screen instead of the whole Home
    /// screen flipping to an error message.
    HomeFailed(String),
    HomeFailedForSession {
        session_generation: u64,
        message: String,
    },
    RadioTracks(Vec<Track>),
    AccountName(Option<String>),
    AccountNameForSession {
        session_generation: u64,
        name: Option<String>,
    },
    PlaylistTracks {
        title: String,
        tracks: Vec<Track>,
    },
    /// Lyrics for the track whose `video_id` is carried alongside them, so a
    /// slow fetch from a track the user has since skipped past can be told
    /// apart from the currently playing one and discarded.
    Lyrics {
        video_id: String,
        lyrics: Option<crate::models::Lyrics>,
    },
    /// Same rationale as `Lyrics`: the cover art is tagged with the track it
    /// belongs to.
    ArtworkBytes {
        video_id: String,
        bytes: Vec<u8>,
    },
    /// Same rationale as `Lyrics`: the downloaded audio is tagged with the
    /// track it belongs to, so a slow download for a track the user has
    /// since skipped past never gets played over the current one.
    AudioReady {
        video_id: String,
        path: PathBuf,
    },
    Status(String),
    Error(String),
    /// Cookies are present, but the API session is no longer valid.
    SessionExpired,
    SessionExpiredForSession {
        session_generation: u64,
    },
    /// Safe account choices prepared without changing the active session.
    SignInPrepared {
        operation_id: u64,
        preview: SignInPreview,
    },
    /// Confirmed activation finished successfully; only handling this
    /// message may publish account state and reload account-only data.
    SignedIn {
        operation_id: u64,
        preview_id: u64,
        method: String,
        credentials_path: Option<String>,
        account_name: String,
    },
    /// A preparation or activation failed. When activation had a preview,
    /// its id is retained so the provider can discard pending credentials.
    SignInFailed {
        operation_id: u64,
        message: String,
        preview_id: Option<u64>,
    },
    /// Radio built around `seed` (a track played from the search results):
    /// similar tracks to append to the queue *behind* what's playing —
    /// unlike `RadioTracks`, which starts playback when the queue ran out.
    RelatedTracks {
        seed: String,
        tracks: Vec<Track>,
    },
    /// Comando vindo do desktop via MPRIS (widget de mídia, playerctl,
    /// teclas multimídia). Reenviado pelo callback da `souvlaki` para que a
    /// mutação de estado aconteça no loop principal.
    Media(souvlaki::MediaControlEvent),
}

/// Máximo de faixas mantidas no histórico local da tela Início.
const RECENT_CAP: usize = 8;

/// Caminho do histórico local de reprodução (`recent.json`).
fn recent_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("ytmtui/recent.json"))
}

/// Carrega o histórico local, se existir; qualquer erro vira lista vazia.
fn load_recent() -> Vec<Track> {
    let mut recent: Vec<Track> = recent_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    recent.truncate(RECENT_CAP);
    recent
}

/// Um item selecionado nos resultados mistos da busca, já resolvido a partir
/// do índice achatado da lista. A ordem dos grupos na tela é a mesma da
/// resolução: músicas, artistas, álbuns, playlists.
#[derive(Debug, Clone)]
pub enum SearchHit {
    /// Índice dentro de `app.songs`.
    Song(usize),
    Artist(Artist),
    Album(Playlist),
    Playlist(Playlist),
}

fn client_error_message(context: &str, error: ProviderError) -> Msg {
    match error {
        ProviderError::SessionExpired => Msg::SessionExpired,
        other => Msg::Error(format!("{context}: {other}")),
    }
}

/// Estado completo da aplicação.
pub struct App {
    pub running: bool,
    /// Serviço de música por trás da UI. `Arc<dyn>`: as tasks assíncronas
    /// clonam o handle e falam apenas com o contrato.
    pub provider: Arc<dyn MusicProvider>,
    pub player: AudioPlayer,
    /// Real-time FFT spectrum feeding the Home screen's visualizer bars.
    pub visualizer: SpectrumAnalyzer,

    // Canais de comunicação com tasks assíncronas.
    pub tx: UnboundedSender<Msg>,
    pub rx: UnboundedReceiver<Msg>,

    // Navegação / foco.
    pub focus: Focus,
    pub section: Section,
    pub sidebar_index: usize,

    // Modo de digitação da busca.
    pub input_mode: bool,
    pub query: String,

    // Conteúdo das listas.
    pub songs: Vec<Track>,
    pub songs_title: String,
    pub playlists: Vec<Playlist>,
    pub artists: Vec<Artist>,
    /// Álbuns retornados pela última busca.
    pub albums: Vec<Playlist>,
    /// A seção Buscar está exibindo os resultados mistos da última busca
    /// (agrupados por tipo: músicas, artistas, álbuns, playlists) em vez de
    /// uma lista plana de faixas de playlist/artista.
    pub search_mixed: bool,
    /// Playlists da biblioteca do usuário logado.
    pub library: Vec<Playlist>,
    /// Recomendações da tela inicial, agrupadas nas mesmas seções nomeadas
    /// que o YouTube Music usa ("Quick picks", "Mixed for you", ...).
    pub home: Vec<crate::models::HomeSection>,
    /// Últimas faixas reproduzidas (histórico local em `recent.json`),
    /// exibidas como o primeiro grupo da tela Início e tocáveis com Enter.
    pub recent: Vec<Track>,
    /// Persistir o histórico em disco? `true` só em [`App::new`];
    /// [`App::with_provider`] mantém tudo em memória.
    persist_recent: bool,
    /// Set when the last `load_home` failed with something other than an
    /// expired session; cleared on the next successful load or the next
    /// loading attempt. Cached `home`/`recent` shelves are never cleared by
    /// a failed refresh, so this only drives the small retry banner/empty
    /// state — it never hides content that's still valid.
    pub home_error: Option<String>,
    /// videoIds curtidos nesta sessão (para alternar curtir/descurtir).
    pub liked: std::collections::HashSet<String>,
    /// Autoplay: continuar com uma rádio quando a fila termina.
    pub autoplay: bool,
    /// Faixa-semente de uma rádio pendente: setada quando o Enter toca uma
    /// música dos resultados da busca; consumida por `play_selected` para
    /// buscar as semelhantes após iniciar a reprodução.
    pending_radio_seed: Option<String>,
    /// Estado de autenticação atual (espelho do provedor para a UI).
    pub authentication: AuthState,
    /// Two-phase browser/account authentication workflow.
    pub authentication_flow: AuthenticationFlow,
    /// Monotonic token binding asynchronous authentication messages to the
    /// operation that started them.
    next_authentication_operation: u64,
    /// Increments only after a sign-in activation commits. Account-scoped
    /// network replies carry this token so an old account cannot overwrite
    /// the new one after the client is swapped.
    session_generation: u64,
    /// Nome de exibição da conta (personalizado na config ou vindo da API).
    pub account_name: Option<String>,
    /// Índice do tema de cores ativo (ver `crate::theme`).
    pub theme_index: usize,
    pub list_state: ListState,

    /// Presentation state: scroll offsets, grid geometry and animation
    /// timing. Lives in `ui` because only renderers read it — see
    /// [`crate::ui::state::UiState`].
    pub ui: crate::ui::state::UiState,

    // Reprodução.
    pub queue: Vec<Track>,
    pub queue_index: Option<usize>,
    pub current: Option<Track>,
    /// Próximo índice pré-calculado (usado por prefetch e auto-avanço).
    pub next_index: Option<usize>,
    /// Reprodução aleatória.
    pub shuffle: bool,
    /// Modo de repetição.
    pub repeat: RepeatMode,
    /// Estado do gerador pseudoaleatório (xorshift) para o shuffle.
    rng_state: u64,
    /// videoIds já tocados no ciclo atual do shuffle: cada faixa toca uma
    /// vez antes de qualquer repetição. Com repeat off, o esgotamento do
    /// ciclo encerra a fila (e o autoplay pode assumir), em vez do sorteio
    /// infinito. Zerado ao trocar a fila ou alternar o shuffle.
    shuffle_played: std::collections::HashSet<String>,

    // Extras.
    pub lyrics: crate::lyrics::LyricsState,
    /// Terminal image support detected at startup (Kitty/Sixel/iTerm2, with
    /// a Unicode half-block fallback). `None` until the main loop sets it.
    pub picker: Option<Picker>,
    /// Album art for the current track, prepared for the detected protocol.
    pub artwork: Option<StatefulProtocol>,
    /// Decoded cover image the protocol above was built from. Kept so the
    /// art can be re-transmitted after a terminal resize: Kitty/Sixel
    /// terminals (Konsole included) drop their graphics on resize, while the
    /// cached protocol still believes the image was already sent.
    pub artwork_source: Option<image::DynamicImage>,
    /// Set when the album art changed and the terminal needs a full clear
    /// before the next draw. Kitty/Sixel graphics live outside ratatui's
    /// cell buffer, so terminals (Konsole included) can leave the previous
    /// cover's pixels on screen when the widget briefly stops drawing there;
    /// only an explicit screen erase reliably removes them.
    pub clear_screen: bool,

    pub status: String,
    /// Caminho opcional para arquivo de cookies do yt-dlp.
    pub cookies: Option<String>,
    /// Um download de áudio está em andamento.
    pub loading_audio: bool,
    /// Quantas tarefas de carregamento (busca/playlist/artista/biblioteca/
    /// sign-in) estão em voo. Um contador, e não um bool: o sync periódico
    /// dispara Home e Biblioteca juntas, e a primeira resposta não pode
    /// apagar o spinner enquanto a outra ainda carrega.
    busy_tasks: usize,
    /// Quadro atual do spinner de carregamento (avança a cada tick).
    pub spinner_frame: usize,
    /// How often background sync (Home + Library) re-fetches.
    pub sync_interval: std::time::Duration,
    /// When the last background sync fired.
    pub last_synced: std::time::Instant,

    // Aparência / animações (Etapa 5). Todos os cinco vêm da config e ainda
    // não são editáveis em runtime — `save_config` sempre relê e preserva o
    // que já está em disco, no mesmo padrão de `sync_interval_secs`.
    /// Modo de exibição da capa do álbum (consumido por `main.rs::build_picker`).
    pub artwork_mode: ArtworkMode,
    /// Densidade dos cards da grade da tela Início (consumido por `ui::main_panel`).
    pub home_density: HomeDensity,
    /// Estilo do visualizador de espectro do player. Chamado `visualizer_style`
    /// (e não `visualizer`) para não colidir com o analisador FFT acima.
    pub visualizer_style: VisualizerStyle,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        // Carrega preferências persistidas.
        let config = Config::load();

        let mut player = AudioPlayer::new()?;
        player.set_volume(config.volume);

        // Raiz de composição: o único ponto em que o provedor concreto
        // aparece — daqui em diante o app só conhece o contrato.
        let (provider, bootstrap) = crate::ytmusic::YtMusic::from_environment(
            config.cookies.clone(),
            config.authentication.clone(),
        );
        let provider: Arc<dyn MusicProvider> = Arc::new(provider);
        let authentication = bootstrap.auth;
        let cookies = bootstrap.cookies;

        // Tema salvo e nome de exibição personalizado (opcional).
        let theme_index = crate::theme::index_by_name(&config.theme);
        let account_name = config.username.clone().filter(|s| !s.trim().is_empty());

        // Semente do PRNG a partir do relógio (evita dependência externa).
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1;

        let status = match authentication {
            AuthState::Authenticated => {
                "Signed in. Loading your library... Press / to search or ? for help.".to_string()
            }
            AuthState::InvalidCredentials => {
                "Cookie file is invalid. Press g to sign in from your browser.".to_string()
            }
            AuthState::Anonymous => match bootstrap.missing_requested_path.as_deref() {
                Some(path) => format!("Configured cookie file does not exist: {path}"),
                None => "Welcome to ytmtui. Press / to search or ? for help.".to_string(),
            },
            AuthState::Expired => {
                "Session expired. Press g to sign in again from your browser.".to_string()
            }
        };

        Ok(Self {
            running: true,
            provider,
            player,
            visualizer: SpectrumAnalyzer::new(),
            tx,
            rx,
            focus: Focus::Sidebar,
            section: Section::Inicio,
            sidebar_index: 0,
            input_mode: false,
            query: String::new(),
            songs: Vec::new(),
            songs_title: "Search results".to_string(),
            playlists: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            search_mixed: false,
            library: Vec::new(),
            home: Vec::new(),
            recent: load_recent(),
            persist_recent: true,
            home_error: None,
            liked: std::collections::HashSet::new(),
            autoplay: true,
            pending_radio_seed: None,
            authentication,
            authentication_flow: AuthenticationFlow::Idle,
            next_authentication_operation: 1,
            session_generation: 0,
            account_name,
            theme_index,
            list_state,
            ui: crate::ui::state::UiState::new(
                AnimationSpeed::from_config(&config.animation_speed),
                config.reduced_motion,
                config.splash,
            ),
            queue: Vec::new(),
            queue_index: None,
            current: None,
            next_index: None,
            shuffle: config.shuffle,
            repeat: RepeatMode::from_config(&config.repeat),
            rng_state: seed,
            shuffle_played: std::collections::HashSet::new(),
            lyrics: crate::lyrics::LyricsState::None,
            picker: None,
            artwork: None,
            artwork_source: None,
            clear_screen: false,
            status,
            cookies,
            loading_audio: false,
            busy_tasks: 0,
            spinner_frame: 0,
            // Defends against a hand-edited config value of 0 creating a
            // hot loop of re-fetches.
            sync_interval: std::time::Duration::from_secs(config.sync_interval_secs.max(30)),
            last_synced: std::time::Instant::now(),
            artwork_mode: ArtworkMode::from_config(&config.artwork_mode),
            home_density: HomeDensity::from_config(&config.home_density),
            visualizer_style: VisualizerStyle::from_config(&config.visualizer),
        })
    }
}

impl App {
    /// Raiz de composição alternativa: constrói o app em torno de um
    /// provedor já pronto, sem ler configuração, variáveis de ambiente,
    /// cookies ou histórico do disco. É o ponto de entrada dos testes de
    /// fronteira (mock) e de provedores selecionados externamente.
    pub fn with_provider(provider: Arc<dyn MusicProvider>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let authentication = if provider.is_authenticated() {
            AuthState::Authenticated
        } else {
            AuthState::Anonymous
        };

        Self {
            running: true,
            provider,
            player: AudioPlayer::new().expect("audio thread should start"),
            visualizer: SpectrumAnalyzer::new(),
            tx,
            rx,
            focus: Focus::Sidebar,
            section: Section::Inicio,
            sidebar_index: 0,
            input_mode: false,
            query: String::new(),
            songs: Vec::new(),
            songs_title: "Search results".to_string(),
            playlists: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            search_mixed: false,
            library: Vec::new(),
            home: Vec::new(),
            // Sem leitura do recent.json real: histórico começa vazio e
            // nunca é gravado de volta (persist_recent = false).
            recent: Vec::new(),
            persist_recent: false,
            home_error: None,
            liked: std::collections::HashSet::new(),
            autoplay: true,
            pending_radio_seed: None,
            authentication,
            authentication_flow: AuthenticationFlow::Idle,
            next_authentication_operation: 1,
            session_generation: 0,
            account_name: None,
            theme_index: 0,
            list_state,
            // Tests never want the entry animation covering the frame.
            ui: crate::ui::state::UiState::new(AnimationSpeed::Normal, false, false),
            queue: Vec::new(),
            queue_index: None,
            current: None,
            next_index: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            rng_state: 0x9E3779B97F4A7C15,
            shuffle_played: std::collections::HashSet::new(),
            lyrics: crate::lyrics::LyricsState::None,
            picker: None,
            artwork: None,
            artwork_source: None,
            clear_screen: false,
            status: "Ready.".to_string(),
            cookies: None,
            loading_audio: false,
            busy_tasks: 0,
            spinner_frame: 0,
            sync_interval: std::time::Duration::from_secs(300),
            last_synced: std::time::Instant::now(),
            artwork_mode: ArtworkMode::Auto,
            home_density: HomeDensity::Comfortable,
            visualizer_style: VisualizerStyle::Gradient,
        }
    }
}

#[cfg(test)]
impl App {
    /// Builds an `App` with fixed defaults for rendering tests.
    ///
    /// Unlike [`App::new`], this constructor never reads configuration files,
    /// environment variables, or cookie files, so tests are deterministic on
    /// any machine.
    pub(crate) fn new_for_tests() -> Self {
        Self::with_provider(Arc::new(crate::provider::mock::MockProvider::default()))
    }
}
