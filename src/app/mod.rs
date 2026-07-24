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
    /// Number of Home card columns available in the current layout.
    pub home_columns: usize,

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
    pub lyrics_scroll: u16,
    /// Rolagem manual da tela de Ajuda (a lista de atalhos é maior que
    /// terminais baixos). Clampada na renderização ao tamanho real do texto.
    pub help_scroll: u16,
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
    /// Velocidade das animações: escala a janela de [`Self::kick_animation`]
    /// e os estágios de revelação/fade-in lidos por `ui::main_panel` e
    /// `ui::now_playing`.
    pub animation_speed: AnimationSpeed,
    /// Reduz/desativa animações não essenciais: [`Self::kick_animation`]
    /// vira no-op, o marquee de títulos longos volta a truncar com '…', o
    /// wipe do karaokê mostra a linha ativa já inteira "cantada", e a
    /// revelação em estágios do card selecionado/metadados do now-playing
    /// pula direto para o estado final.
    pub reduced_motion: bool,
    /// Instante até quando uma animação de transição (seleção da Home,
    /// troca de faixa) ainda está em curso; `None` quando nenhuma está
    /// ativa. Enquanto `animating()` é verdadeiro, [`Self::needs_fast_animation`]
    /// segura o tier de redraw de 60ms só pela duração da transição, em vez
    /// de indefinidamente — ver [`Self::kick_animation`].
    animate_until: Option<std::time::Instant>,
    /// Instante da última mudança de seleção na grade Início; consumido por
    /// `ui::main_panel::draw_card` para a revelação em estágios do card
    /// selecionado (fundo → título accent → badge). `None` antes de
    /// qualquer navegação, o que já corresponde ao estado final completo.
    pub(crate) selection_changed_at: Option<std::time::Instant>,
    /// Instante em que a faixa atual mudou pela última vez (setado em
    /// [`Self::start_current`]); consumido por `ui::now_playing` para o
    /// fade-in de duas etapas do título. Nunca `None`: antes da primeira
    /// faixa o valor é irrelevante, pois `current` ainda é `None`.
    pub(crate) track_changed_at: std::time::Instant,
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
            home_columns: 1,
            queue: Vec::new(),
            queue_index: None,
            current: None,
            next_index: None,
            shuffle: config.shuffle,
            repeat: RepeatMode::from_config(&config.repeat),
            rng_state: seed,
            shuffle_played: std::collections::HashSet::new(),
            lyrics: crate::lyrics::LyricsState::None,
            lyrics_scroll: 0,
            help_scroll: 0,
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
            animation_speed: AnimationSpeed::from_config(&config.animation_speed),
            reduced_motion: config.reduced_motion,
            animate_until: None,
            selection_changed_at: None,
            track_changed_at: std::time::Instant::now(),
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
            home_columns: 1,
            queue: Vec::new(),
            queue_index: None,
            current: None,
            next_index: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            rng_state: 0x9E3779B97F4A7C15,
            shuffle_played: std::collections::HashSet::new(),
            lyrics: crate::lyrics::LyricsState::None,
            lyrics_scroll: 0,
            help_scroll: 0,
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
            animation_speed: AnimationSpeed::Normal,
            reduced_motion: false,
            animate_until: None,
            selection_changed_at: None,
            track_changed_at: std::time::Instant::now(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_helper_installs_the_first_account_selection() {
        let mut app = App::new_for_tests();
        app.set_sign_in_preview_for_test(SignInPreview {
            id: 7,
            method: "mock".to_string(),
            profile_label: None,
            accounts: vec![crate::provider::SignInAccount {
                index: 3,
                name: "Preview Account".to_string(),
                handle: None,
            }],
            current_account_name: None,
        });

        let (preview, selected) = app.sign_in_preview().expect("preview installed");
        assert_eq!(preview.id, 7);
        assert_eq!(selected, 0);
    }

    #[test]
    fn stale_sign_in_messages_cannot_publish_or_cancel_another_operation() {
        let mut app = App::new_for_tests();
        app.authentication = AuthState::Authenticated;
        app.account_name = Some("Existing Account".to_string());
        app.authentication_flow = AuthenticationFlow::Activating {
            operation_id: 4,
            preview_id: 7,
        };

        app.tx
            .send(Msg::SignedIn {
                operation_id: 99,
                preview_id: 7,
                method: "firefox".to_string(),
                credentials_path: Some("wrong-cookies.txt".to_string()),
                account_name: "Wrong Account".to_string(),
            })
            .unwrap();
        app.tx
            .send(Msg::SignInFailed {
                operation_id: 99,
                message: "stale failure".to_string(),
                preview_id: Some(7),
            })
            .unwrap();
        app.drain_messages();

        assert!(matches!(
            app.authentication_flow,
            AuthenticationFlow::Activating {
                operation_id: 4,
                preview_id: 7
            }
        ));
        assert_eq!(app.authentication, AuthState::Authenticated);
        assert_eq!(app.account_name.as_deref(), Some("Existing Account"));
        assert_ne!(app.cookies.as_deref(), Some("wrong-cookies.txt"));
    }

    #[test]
    fn stale_session_payloads_cannot_overwrite_a_newly_activated_account() {
        let mut app = App::new_for_tests();
        app.session_generation = 2;
        app.authentication = AuthState::Authenticated;
        app.account_name = Some("New Account".to_string());
        app.home = vec![crate::models::HomeSection {
            title: "New home".to_string(),
            items: vec![],
        }];
        app.library = vec![Playlist {
            title: "New library".to_string(),
            ..Default::default()
        }];

        app.tx
            .send(Msg::HomeSectionsForSession {
                session_generation: 1,
                sections: vec![],
            })
            .unwrap();
        app.tx
            .send(Msg::LibraryPlaylistsForSession {
                session_generation: 1,
                playlists: vec![],
            })
            .unwrap();
        app.tx
            .send(Msg::AccountNameForSession {
                session_generation: 1,
                name: Some("Old Account".to_string()),
            })
            .unwrap();
        app.tx
            .send(Msg::SessionExpiredForSession {
                session_generation: 1,
            })
            .unwrap();
        app.drain_messages();

        assert_eq!(app.authentication, AuthState::Authenticated);
        assert_eq!(app.account_name.as_deref(), Some("New Account"));
        assert_eq!(app.home[0].title, "New home");
        assert_eq!(app.library[0].title, "New library");
    }

    #[tokio::test]
    async fn a_session_expiry_queued_before_sign_in_cannot_expire_the_new_account() {
        let mut app = App::new_for_tests();
        // `confirm_sign_in` advances this before the provider can commit.
        app.session_generation = 1;
        app.authentication_flow = AuthenticationFlow::Activating {
            operation_id: 4,
            preview_id: 7,
        };
        app.tx
            .send(Msg::SessionExpiredForSession {
                session_generation: 0,
            })
            .unwrap();
        app.tx
            .send(Msg::SignedIn {
                operation_id: 4,
                preview_id: 7,
                method: "firefox".to_string(),
                credentials_path: None,
                account_name: "New Account".to_string(),
            })
            .unwrap();

        app.drain_messages();

        assert_eq!(app.authentication, AuthState::Authenticated);
        assert_eq!(app.account_name.as_deref(), Some("New Account"));
    }

    #[tokio::test]
    async fn confirming_sign_in_retires_the_previous_session_before_activation_runs() {
        let mut app = App::new_for_tests();
        app.set_sign_in_preview_for_test(SignInPreview {
            id: 7,
            method: "firefox".to_string(),
            profile_label: None,
            accounts: vec![crate::provider::SignInAccount {
                index: 0,
                name: "New Account".to_string(),
                handle: None,
            }],
            current_account_name: None,
        });

        app.confirm_sign_in();

        assert_eq!(app.session_generation, 1);
    }

    #[test]
    fn background_home_refresh_preserves_selection_by_browse_id() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.home = vec![crate::models::HomeSection {
            title: "Quick picks".to_string(),
            items: vec![
                Playlist {
                    browse_id: "VL1".to_string(),
                    ..Default::default()
                },
                Playlist {
                    browse_id: "VL2".to_string(),
                    ..Default::default()
                },
            ],
        }];
        // Selects "VL2" (flattened index 1).
        app.list_state.select(Some(1));

        // A background refresh reorders VL2 ahead of VL1.
        app.tx
            .send(Msg::HomeSections(vec![crate::models::HomeSection {
                title: "Quick picks".to_string(),
                items: vec![
                    Playlist {
                        browse_id: "VL2".to_string(),
                        ..Default::default()
                    },
                    Playlist {
                        browse_id: "VL1".to_string(),
                        ..Default::default()
                    },
                ],
            }]))
            .unwrap();
        app.drain_messages();

        assert_eq!(
            app.list_state.selected(),
            Some(0),
            "selection follows VL2 to its new position"
        );
    }

    #[test]
    fn background_home_refresh_clamps_when_the_selected_item_is_gone() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.home = vec![crate::models::HomeSection {
            title: "Quick picks".to_string(),
            items: vec![
                Playlist {
                    browse_id: "VL1".to_string(),
                    ..Default::default()
                },
                Playlist {
                    browse_id: "VL2".to_string(),
                    ..Default::default()
                },
            ],
        }];
        app.list_state.select(Some(1)); // VL2

        // VL2 is gone from the refreshed data.
        app.tx
            .send(Msg::HomeSections(vec![crate::models::HomeSection {
                title: "Quick picks".to_string(),
                items: vec![Playlist {
                    browse_id: "VL1".to_string(),
                    ..Default::default()
                }],
            }]))
            .unwrap();
        app.drain_messages();

        assert_eq!(
            app.list_state.selected(),
            Some(0),
            "clamps to the nearest valid index instead of resetting to the top"
        );
    }

    #[test]
    fn background_library_refresh_preserves_selection_by_browse_id() {
        let mut app = App::new_for_tests();
        app.section = Section::Biblioteca;
        app.library = vec![
            Playlist {
                browse_id: "L1".to_string(),
                ..Default::default()
            },
            Playlist {
                browse_id: "L2".to_string(),
                ..Default::default()
            },
        ];
        app.list_state.select(Some(1)); // L2

        app.tx
            .send(Msg::LibraryPlaylists(vec![
                Playlist {
                    browse_id: "L2".to_string(),
                    ..Default::default()
                },
                Playlist {
                    browse_id: "L1".to_string(),
                    ..Default::default()
                },
            ]))
            .unwrap();
        app.drain_messages();

        assert_eq!(
            app.list_state.selected(),
            Some(0),
            "selection follows L2 to its new position"
        );
    }

    #[test]
    fn first_home_load_still_selects_the_top_item() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        assert!(app.home.is_empty());

        app.tx
            .send(Msg::HomeSections(vec![crate::models::HomeSection {
                title: "Quick picks".to_string(),
                items: vec![Playlist {
                    browse_id: "VL1".to_string(),
                    ..Default::default()
                }],
            }]))
            .unwrap();
        app.drain_messages();

        assert_eq!(
            app.list_state.selected(),
            Some(0),
            "the very first load still selects the top item"
        );
    }

    #[test]
    fn animation_is_only_needed_while_loading_or_playing() {
        let mut app = App::new_for_tests();
        assert!(!app.needs_animation(), "idle app needs no animation");

        app.begin_task();
        assert!(app.needs_animation(), "loading shows the spinner");
        app.finish_task();

        app.current = Some(crate::models::Track::default());
        assert!(app.needs_animation(), "playback progress animates");
    }

    #[test]
    fn finishing_the_queue_clears_the_album_art() {
        let mut app = App::new_for_tests();
        let mut picker = ratatui_image::picker::Picker::from_fontsize((8, 16));
        let cover = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            8,
            image::Rgb([1, 2, 3]),
        ));
        app.artwork = Some(picker.new_resize_protocol(cover));
        app.current = Some(Track::default());
        app.queue = vec![Track::default()];
        app.queue_index = Some(0);

        // An empty radio batch ends playback; the cover must not linger.
        app.tx.send(Msg::RadioTracks(Vec::new())).unwrap();
        app.drain_messages();

        assert!(app.current.is_none(), "playback ended");
        assert!(
            app.artwork.is_none(),
            "stale cover must not outlive playback"
        );
    }

    fn home_sections() -> Vec<crate::models::HomeSection> {
        vec![
            crate::models::HomeSection {
                title: "Quick picks".to_string(),
                items: vec![
                    Playlist {
                        browse_id: "VL1".to_string(),
                        title: "First".to_string(),
                        ..Default::default()
                    },
                    Playlist {
                        browse_id: "VL2".to_string(),
                        title: "Second".to_string(),
                        ..Default::default()
                    },
                ],
            },
            crate::models::HomeSection {
                title: "Mixed for you".to_string(),
                items: vec![Playlist {
                    browse_id: "VL3".to_string(),
                    title: "Third".to_string(),
                    ..Default::default()
                }],
            },
        ]
    }

    #[test]
    fn resize_rebuilds_artwork_from_the_stored_cover() {
        let mut app = App::new_for_tests();
        app.picker = Some(ratatui_image::picker::Picker::from_fontsize((8, 16)));
        app.artwork_source = Some(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            32,
            32,
            image::Rgb([10, 20, 30]),
        )));
        app.artwork = None;
        app.clear_screen = false;

        app.rebuild_artwork();
        assert!(app.artwork.is_some(), "protocol re-created from the source");
        assert!(app.clear_screen, "full clear requested after resize");

        // Without a stored cover (nothing playing) it must not fabricate art.
        let mut idle = App::new_for_tests();
        idle.picker = Some(ratatui_image::picker::Picker::from_fontsize((8, 16)));
        idle.rebuild_artwork();
        assert!(idle.artwork.is_none());
    }

    fn mixed_search_app() -> App {
        let mut app = App::new_for_tests();
        app.search_mixed = true;
        app.songs = vec![
            Track {
                video_id: "s1".to_string(),
                title: "Song one".to_string(),
                ..Default::default()
            },
            Track {
                video_id: "s2".to_string(),
                title: "Song two".to_string(),
                ..Default::default()
            },
        ];
        app.artists = vec![crate::models::Artist {
            browse_id: "UC1".to_string(),
            name: "Artist one".to_string(),
            ..Default::default()
        }];
        app.albums = vec![Playlist {
            browse_id: "MPRE1".to_string(),
            title: "Album one".to_string(),
            ..Default::default()
        }];
        app.playlists = vec![Playlist {
            browse_id: "VLPL1".to_string(),
            title: "Playlist one".to_string(),
            ..Default::default()
        }];
        app
    }

    // Results arriving while the user browses elsewhere must pull both the
    // section *and* the sidebar cursor to Search, or the next `j`/`k` walks
    // from a stale base into an unrelated section.
    #[test]
    fn arriving_search_results_move_the_section_and_the_sidebar_together() {
        let mut app = App::new_for_tests();
        app.section = Section::Fila;
        app.sidebar_index = Section::Fila.index();

        app.tx
            .send(Msg::SearchResults(crate::models::SearchResults::default()))
            .expect("channel open");
        app.drain_messages();

        assert_eq!(app.section, Section::Buscar);
        assert_eq!(app.sidebar_index, Section::Buscar.index());
    }

    #[test]
    fn search_hit_at_resolves_groups_in_display_order() {
        let app = mixed_search_app();
        assert_eq!(app.search_item_count(), 5);
        assert!(matches!(app.search_hit_at(0), Some(SearchHit::Song(0))));
        assert!(matches!(app.search_hit_at(1), Some(SearchHit::Song(1))));
        assert!(matches!(app.search_hit_at(2), Some(SearchHit::Artist(a)) if a.browse_id == "UC1"));
        assert!(
            matches!(app.search_hit_at(3), Some(SearchHit::Album(p)) if p.browse_id == "MPRE1")
        );
        assert!(
            matches!(app.search_hit_at(4), Some(SearchHit::Playlist(p)) if p.browse_id == "VLPL1")
        );
        assert!(app.search_hit_at(5).is_none());
    }

    #[test]
    fn entering_a_song_in_mixed_results_seeds_a_radio_queue() {
        let mut app = mixed_search_app();
        app.section = Section::Buscar;
        app.list_state.select(Some(1)); // "Song two"
        assert!(
            app.prepare_selection_for_playback(),
            "songs start playback directly"
        );
        // Like YT Music: the queue starts with just the chosen song, and a
        // radio of similar tracks is scheduled to fill it.
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue[0].video_id, "s2");
        assert_eq!(app.queue_index, Some(0));
        assert_eq!(app.pending_radio_seed.as_deref(), Some("s2"));
    }

    #[test]
    fn related_tracks_append_behind_the_playing_seed_without_duplicates() {
        let seed = Track {
            video_id: "s2".to_string(),
            title: "Song two".to_string(),
            ..Default::default()
        };
        let mut app = App::new_for_tests();
        app.queue = vec![seed.clone()];
        app.queue_index = Some(0);
        app.current = Some(seed.clone());

        let radio = vec![
            seed.clone(), // radios echo the seed back — must not duplicate
            Track {
                video_id: "r1".to_string(),
                ..Default::default()
            },
            Track {
                video_id: "r2".to_string(),
                ..Default::default()
            },
        ];
        assert_eq!(app.append_related("s2", radio.clone()), 2);
        assert_eq!(app.queue.len(), 3);
        assert_eq!(app.queue_index, Some(0), "playback position untouched");
        assert_eq!(app.next_index, Some(1), "next track recomputed");

        // A late radio for a track the user already skipped is discarded.
        app.current = Some(Track {
            video_id: "other".to_string(),
            ..Default::default()
        });
        assert_eq!(app.append_related("s2", radio), 0);
        assert_eq!(app.queue.len(), 3);
    }

    #[test]
    fn enqueue_in_mixed_results_rejects_non_song_rows() {
        let mut app = mixed_search_app();
        app.section = Section::Buscar;
        app.list_state.select(Some(3)); // the album row
        app.enqueue_selected();
        assert!(
            app.queue.is_empty(),
            "albums must not be enqueued as tracks"
        );
        assert!(
            app.status.contains("músicas"),
            "explains why: {}",
            app.status
        );
    }

    #[tokio::test]
    async fn entering_a_recent_home_card_preserves_history_order_and_selected_index() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.recent = (1..=3)
            .map(|i| Track {
                video_id: format!("r{i}"),
                title: format!("Recent {i}"),
                ..Default::default()
            })
            .collect();
        app.list_state.select(Some(1));

        app.open_selected_home();

        assert_eq!(
            app.queue
                .iter()
                .map(|track| track.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["r1", "r2", "r3"]
        );
        assert_eq!(app.queue_index, Some(1));
        assert_eq!(
            app.current.as_ref().map(|track| track.video_id.as_str()),
            Some("r2")
        );
    }

    #[test]
    fn enqueueing_a_recent_home_track_does_not_interrupt_playback() {
        let playing = Track {
            video_id: "playing".into(),
            title: "Playing".into(),
            ..Default::default()
        };
        let recent = Track {
            video_id: "recent".into(),
            title: "Recent".into(),
            ..Default::default()
        };
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.recent = vec![recent];
        app.queue = vec![playing.clone()];
        app.queue_index = Some(0);
        app.current = Some(playing);
        app.list_state.select(Some(0));

        app.enqueue_selected();

        assert_eq!(
            app.queue
                .iter()
                .map(|track| track.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["playing", "recent"]
        );
        assert_eq!(app.queue_index, Some(0));
        assert_eq!(
            app.current.as_ref().map(|track| track.video_id.as_str()),
            Some("playing")
        );
        assert!(app.status.contains("adicionada à fila"));
    }

    #[test]
    fn home_item_count_sums_across_sections_excluding_headers() {
        let mut app = App::new_for_tests();
        app.home = home_sections();
        assert_eq!(app.home_item_count(), 3);
    }

    #[test]
    fn home_total_count_puts_recent_tracks_before_recommendations() {
        let mut app = App::new_for_tests();
        app.home = home_sections();
        app.recent = vec![
            Track {
                video_id: "r1".to_string(),
                ..Default::default()
            },
            Track {
                video_id: "r2".to_string(),
                ..Default::default()
            },
        ];
        assert_eq!(app.home_total_count(), 5);
        // Recommendation lookups skip past the recent group.
        assert_eq!(
            app.home_item_at(5 - app.recent.len() - 1)
                .map(|p| p.browse_id.as_str()),
            Some("VL3")
        );
    }

    #[test]
    fn home_item_at_flattens_across_section_boundaries() {
        let mut app = App::new_for_tests();
        app.home = home_sections();
        assert_eq!(
            app.home_item_at(0).map(|p| p.browse_id.as_str()),
            Some("VL1")
        );
        assert_eq!(
            app.home_item_at(1).map(|p| p.browse_id.as_str()),
            Some("VL2")
        );
        assert_eq!(
            app.home_item_at(2).map(|p| p.browse_id.as_str()),
            Some("VL3")
        );
        assert!(app.home_item_at(3).is_none());
    }

    #[test]
    fn home_flat_index_of_finds_items_regardless_of_section() {
        let mut app = App::new_for_tests();
        app.home = home_sections();
        assert_eq!(app.home_flat_index_of("VL1"), Some(0));
        assert_eq!(app.home_flat_index_of("VL3"), Some(2));
        assert_eq!(app.home_flat_index_of("missing"), None);
    }

    #[test]
    fn stop_clears_the_now_playing_state_but_keeps_the_queue() {
        let mut app = App::new_for_tests();
        app.current = Some(Track::default());
        app.loading_audio = true;
        app.lyrics = crate::lyrics::LyricsState::Plain("la la".to_string());
        app.artwork_source = Some(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            8,
            image::Rgb([1, 2, 3]),
        )));
        app.queue = vec![Track::default(), Track::default()];
        app.queue_index = Some(1);

        app.stop_playback();

        assert!(app.current.is_none(), "no track shown as playing");
        assert!(!app.loading_audio);
        assert!(app.artwork_source.is_none(), "cover cleared");
        assert!(app.clear_screen, "graphics leftovers get erased");
        assert!(matches!(app.lyrics, crate::lyrics::LyricsState::None));
        assert_eq!(app.queue.len(), 2, "queue survives for a later resume");

        // Stopping when idle must not request a screen clear (no flicker).
        let mut idle = App::new_for_tests();
        idle.stop_playback();
        assert!(!idle.clear_screen);
    }

    #[test]
    fn concurrent_loads_keep_the_spinner_until_the_last_one_finishes() {
        let mut app = App::new_for_tests();
        // Simulates `sync_home_and_library`: two counted tasks in flight.
        app.begin_task();
        app.begin_task();

        app.tx.send(Msg::HomeSections(Vec::new())).unwrap();
        app.drain_messages();
        assert!(
            app.is_loading(),
            "first response must not hide the spinner while the second load is in flight"
        );

        app.tx.send(Msg::LibraryPlaylists(Vec::new())).unwrap();
        app.drain_messages();
        assert!(!app.is_loading());
    }

    #[test]
    fn stray_errors_never_underflow_the_busy_counter() {
        let mut app = App::new_for_tests();
        // An uncounted task (audio download, like) reporting an error while
        // nothing counted is in flight must saturate at zero...
        app.tx.send(Msg::Error("boom".to_string())).unwrap();
        app.drain_messages();
        assert!(!app.is_loading());

        // ...so a counted task started right after still shows its spinner.
        app.begin_task();
        assert!(app.is_loading());
    }

    #[test]
    fn background_library_refresh_does_not_clobber_the_status_bar() {
        let mut app = App::new_for_tests();
        app.library = vec![Playlist {
            browse_id: "L1".to_string(),
            ..Default::default()
        }];
        app.status = "▶ Tocando: Song — Artist".to_string();

        app.tx
            .send(Msg::LibraryPlaylists(vec![Playlist {
                browse_id: "L1".to_string(),
                ..Default::default()
            }]))
            .unwrap();
        app.drain_messages();

        assert_eq!(
            app.status, "▶ Tocando: Song — Artist",
            "periodic refresh must not overwrite what the user is reading"
        );
    }

    fn track(id: &str) -> Track {
        Track {
            video_id: id.to_string(),
            title: format!("Track {id}"),
            ..Default::default()
        }
    }

    fn queue_app() -> App {
        let mut app = App::new_for_tests();
        app.section = Section::Fila;
        app.queue = vec![track("a"), track("b"), track("c"), track("d")];
        app.queue_index = Some(1);
        app.current = Some(track("b"));
        app
    }

    #[test]
    fn removing_a_track_before_the_current_one_shifts_the_playing_index() {
        let mut app = queue_app();
        app.list_state.select(Some(0));
        app.queue_remove_selected();
        assert_eq!(app.queue.len(), 3);
        assert_eq!(app.queue_index, Some(0), "current track followed its move");
        assert_eq!(app.queue[0].video_id, "b");
    }

    #[test]
    fn the_playing_track_cannot_be_removed_from_the_queue() {
        let mut app = queue_app();
        app.list_state.select(Some(1)); // the playing track
        app.queue_remove_selected();
        assert_eq!(app.queue.len(), 4, "queue unchanged");
        assert_eq!(app.queue_index, Some(1));
    }

    #[test]
    fn removing_after_the_current_track_keeps_the_playing_index() {
        let mut app = queue_app();
        app.list_state.select(Some(3));
        app.queue_remove_selected();
        assert_eq!(app.queue.len(), 3);
        assert_eq!(app.queue_index, Some(1));
        // Selection clamps to the new last row instead of dangling.
        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn moving_a_track_follows_selection_and_repoints_the_playing_index() {
        let mut app = queue_app();
        app.list_state.select(Some(2)); // "c"
        app.queue_move_selected(-1); // swaps with "b" (the playing track)
        assert_eq!(app.queue[1].video_id, "c");
        assert_eq!(app.queue[2].video_id, "b");
        assert_eq!(app.queue_index, Some(2), "playing track followed the swap");
        assert_eq!(
            app.list_state.selected(),
            Some(1),
            "selection followed the move"
        );

        // Edges saturate: can't move the first row further up.
        app.list_state.select(Some(0));
        app.queue_move_selected(-1);
        assert_eq!(app.queue[0].video_id, "a");
    }

    #[test]
    fn clearing_the_queue_keeps_only_the_playing_track() {
        let mut app = queue_app();
        app.queue_clear();
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue[0].video_id, "b");
        assert_eq!(app.queue_index, Some(0));

        // With nothing playing, the queue empties entirely.
        let mut stopped = queue_app();
        stopped.current = None;
        stopped.queue_clear();
        assert!(stopped.queue.is_empty());
        assert_eq!(stopped.queue_index, None);
    }

    #[test]
    fn shuffle_visits_every_track_once_then_ends_when_repeat_is_off() {
        let mut app = App::new_for_tests();
        app.queue = vec![track("a"), track("b"), track("c"), track("d")];
        app.shuffle = true;
        // Simulates `start_current` for the first track.
        app.queue_index = Some(0);
        app.shuffle_played.insert("a".to_string());

        let mut visited = vec!["a".to_string()];
        let mut idx = 0;
        while let Some(next) = app.compute_next(idx, false) {
            let id = app.queue[next].video_id.clone();
            assert!(
                !visited.contains(&id),
                "shuffle must not repeat a track within a cycle"
            );
            visited.push(id.clone());
            app.shuffle_played.insert(id);
            idx = next;
        }
        assert_eq!(visited.len(), 4, "every track played exactly once");
    }

    #[test]
    fn shuffle_starts_a_new_cycle_when_repeat_all_wraps() {
        let mut app = App::new_for_tests();
        app.queue = vec![track("a"), track("b"), track("c")];
        app.shuffle = true;
        // Cycle exhausted: everything already played.
        for id in ["a", "b", "c"] {
            app.shuffle_played.insert(id.to_string());
        }
        let next = app.compute_next(1, true);
        assert!(next.is_some(), "repeat all recycles the queue");
        assert_ne!(
            next,
            Some(1),
            "never repeats the current track back-to-back"
        );
    }

    #[test]
    fn number_keys_jump_to_sections() {
        let mut app = App::new_for_tests();
        app.jump_to_section(5);
        assert_eq!(app.section, Section::Fila);
        assert_eq!(app.sidebar_index, 5);
        assert_eq!(app.focus, Focus::Main);
        // Out of range is a no-op.
        app.jump_to_section(99);
        assert_eq!(app.section, Section::Fila);
    }

    #[test]
    fn page_selection_saturates_at_the_list_edges() {
        let mut app = App::new_for_tests();
        app.section = Section::Fila;
        app.queue = vec![track("a"), track("b"), track("c")];
        app.list_state.select(Some(1));
        app.page_selection(10);
        assert_eq!(app.list_state.selected(), Some(2), "clamps to the end");
        app.page_selection(-10);
        assert_eq!(app.list_state.selected(), Some(0), "clamps to the start");
    }

    #[test]
    fn session_expiry_maps_to_the_dedicated_message() {
        let message = client_error_message("Could not load library", ProviderError::SessionExpired);
        assert!(matches!(message, Msg::SessionExpired));
    }

    #[test]
    fn home_failed_preserves_cached_shelves_and_clears_the_spinner() {
        let mut app = App::new_for_tests();
        app.home = home_sections();
        app.begin_task();

        app.tx.send(Msg::HomeFailed("boom".to_string())).unwrap();
        app.drain_messages();

        assert_eq!(
            app.home.len(),
            2,
            "cached shelves survive a failed background refresh"
        );
        assert_eq!(app.home_error.as_deref(), Some("boom"));
        assert!(!app.is_loading(), "the spinner is released on failure");
        assert!(
            app.status.contains('R'),
            "status hints at the retry key: {}",
            app.status
        );
    }

    #[test]
    fn home_sections_success_clears_a_previous_error() {
        let mut app = App::new_for_tests();
        app.home_error = Some("boom".to_string());

        app.tx.send(Msg::HomeSections(Vec::new())).unwrap();
        app.drain_messages();

        assert!(
            app.home_error.is_none(),
            "a successful load clears the stale error"
        );
    }

    #[test]
    fn load_home_without_the_home_capability_creates_no_task() {
        let mut mock = crate::provider::mock::MockProvider::default();
        mock.capabilities.home = false;
        let mut app = App::with_provider(std::sync::Arc::new(mock));

        app.load_home();

        assert!(
            !app.is_loading(),
            "no capability means no task, hence no spinner"
        );
    }

    // --- Etapa 6: animações time-based + reduced motion ---------------

    #[test]
    fn kick_animation_is_a_no_op_under_reduced_motion() {
        let mut app = App::new_for_tests();
        app.reduced_motion = true;
        app.kick_animation(std::time::Duration::from_millis(500));
        assert!(
            !app.animating(),
            "reduced motion must never hold the fast redraw tier open"
        );
    }

    #[test]
    fn kick_animation_scales_the_window_by_animation_speed() {
        // Same base duration, three speeds: the resulting deadline must
        // order Slow > Normal > Fast, matching `AnimationSpeed::factor`.
        let base = std::time::Duration::from_millis(200);
        let deadline_for = |speed: AnimationSpeed| {
            let mut app = App::new_for_tests();
            app.animation_speed = speed;
            let before = std::time::Instant::now();
            app.kick_animation(base);
            app.animate_until.expect("kick sets a deadline") - before
        };
        let fast = deadline_for(AnimationSpeed::Fast);
        let normal = deadline_for(AnimationSpeed::Normal);
        let slow = deadline_for(AnimationSpeed::Slow);
        assert!(
            fast < normal,
            "fast ({fast:?}) must be shorter than normal ({normal:?})"
        );
        assert!(
            normal < slow,
            "normal ({normal:?}) must be shorter than slow ({slow:?})"
        );
    }

    #[test]
    fn animating_expires_after_the_kicked_window_elapses() {
        let mut app = App::new_for_tests();
        // A 1ms kick is effectively already expired by the time the assert
        // below runs — no sleep needed in the test.
        app.kick_animation(std::time::Duration::from_millis(1));
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(!app.animating(), "the animation window must expire");
    }

    #[test]
    fn needs_fast_animation_is_true_while_animating_even_with_nothing_playing() {
        let mut app = App::new_for_tests();
        assert!(!app.needs_fast_animation(), "idle app needs no animation");
        app.kick_animation(std::time::Duration::from_millis(500));
        assert!(
            app.needs_fast_animation(),
            "a kicked-off transition holds the fast tier even without playback"
        );
    }

    #[test]
    fn reduced_motion_drops_the_fast_tier_even_while_the_visualizer_would_animate() {
        let mut app = App::new_for_tests();
        app.reduced_motion = true;
        app.section = Section::Inicio;
        app.current = Some(Track::default());
        // Not paused: without `reduced_motion` this would need the fast tier
        // (Home visualizer). Under `reduced_motion`, it must not.
        assert!(!app.player.is_paused());
        assert!(
            !app.needs_fast_animation(),
            "reduced motion falls back to the 200ms tier for continuous drivers"
        );
        assert!(
            app.needs_animation(),
            "playback progress still animates at the economy tier"
        );
    }

    #[test]
    fn moving_the_home_selection_marks_the_change_and_kicks_the_fast_tier() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.home = home_sections();
        app.list_state.select(Some(0));
        assert!(app.selection_changed_at.is_none());

        app.move_home(HomeDirection::Down);

        assert!(
            app.selection_changed_at.is_some(),
            "move_home marks the selection as just-changed"
        );
        assert!(
            app.needs_fast_animation(),
            "the selection-change kick holds the fast tier"
        );
    }

    #[test]
    fn move_selection_marks_the_change_only_in_the_home_section() {
        let mut app = App::new_for_tests();
        app.section = Section::Fila;
        app.queue = vec![track("a"), track("b")];
        app.move_selection(1);
        assert!(
            app.selection_changed_at.is_none(),
            "the queue section has no card-reveal transition to drive"
        );
    }

    #[tokio::test]
    async fn starting_a_track_marks_track_changed_at_and_kicks_the_fast_tier() {
        // `start_current` spawns background tasks (audio resolution, lyrics,
        // artwork), so this needs a real Tokio runtime, like
        // `entering_a_recent_home_card_preserves_history_order_and_selected_index`
        // above.
        let mut app = App::new_for_tests();
        app.queue = vec![track("a")];
        app.queue_index = Some(0);
        let before = std::time::Instant::now();

        app.start_current();

        assert!(app.track_changed_at >= before);
        assert!(
            app.needs_fast_animation(),
            "starting a track kicks the fast tier for the metadata fade-in"
        );
    }
}
