//! Estado central da aplicação e lógica de coordenação.

mod authentication;
mod messages;
mod playback;
mod search;

pub use authentication::AuthenticationState;
use authentication::{detect_browsers, export_browser_cookies, resolve_cookie_path};

use std::path::PathBuf;

use ratatui::widgets::ListState;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::config::Config;
use crate::player::{self, AudioPlayer};
use crate::visualizer::SpectrumAnalyzer;
use crate::ytmusic::{Artist, Playlist, SearchResults, Track, YtMusicClient, YtMusicError};

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
    HomeSections(Vec<crate::ytmusic::HomeSection>),
    RadioTracks(Vec<Track>),
    AccountName(Option<String>),
    PlaylistTracks {
        title: String,
        tracks: Vec<Track>,
    },
    /// Lyrics for the track whose `video_id` is carried alongside them, so a
    /// slow fetch from a track the user has since skipped past can be told
    /// apart from the currently playing one and discarded.
    Lyrics {
        video_id: String,
        lyrics: Option<crate::ytmusic::Lyrics>,
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
    /// In-app sign-in finished: browser cookies were exported to `path`.
    /// `browser` is the `--cookies-from-browser` value that worked (e.g.
    /// "brave" or "firefox:/path/to/profile").
    CookiesImported {
        path: String,
        browser: String,
        /// Set when the export succeeded but a non-fatal issue occurred
        /// (e.g. the cookie file's permissions could not be restricted).
        warning: Option<String>,
    },
    /// Radio built around `seed` (a track played from the search results):
    /// similar tracks to append to the queue *behind* what's playing —
    /// unlike `RadioTracks`, which starts playback when the queue ran out.
    RelatedTracks {
        seed: String,
        tracks: Vec<Track>,
    },
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

fn client_error_message(context: &str, error: YtMusicError) -> Msg {
    match error {
        YtMusicError::SessionExpired { .. } => Msg::SessionExpired,
        other => Msg::Error(format!("{context}: {other}")),
    }
}

/// Estado completo da aplicação.
pub struct App {
    pub running: bool,
    pub client: YtMusicClient,
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
    pub home: Vec<crate::ytmusic::HomeSection>,
    /// Últimas faixas reproduzidas (histórico local em `recent.json`),
    /// exibidas como o primeiro grupo da tela Início e tocáveis com Enter.
    pub recent: Vec<Track>,
    /// videoIds curtidos nesta sessão (para alternar curtir/descurtir).
    pub liked: std::collections::HashSet<String>,
    /// Autoplay: continuar com uma rádio quando a fila termina.
    pub autoplay: bool,
    /// Faixa-semente de uma rádio pendente: setada quando o Enter toca uma
    /// música dos resultados da busca; consumida por `play_selected` para
    /// buscar as semelhantes após iniciar a reprodução.
    pending_radio_seed: Option<String>,
    /// Current cookie authentication state.
    pub authentication: AuthenticationState,
    /// Nome de exibição da conta (personalizado na config ou vindo da API).
    pub account_name: Option<String>,
    /// Índice do tema de cores ativo (ver `crate::theme`).
    pub theme_index: usize,
    pub list_state: ListState,

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

    // Extras.
    pub lyrics: crate::lyrics::LyricsState,
    pub lyrics_scroll: u16,
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
    /// Uma tarefa de carregamento (busca/playlist/artista/biblioteca) está ativa.
    pub busy: bool,
    /// Quadro atual do spinner de carregamento (avança a cada tick).
    pub spinner_frame: usize,
    /// How often background sync (Home + Library) re-fetches.
    pub sync_interval: std::time::Duration,
    /// When the last background sync fired.
    pub last_synced: std::time::Instant,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        // Carrega preferências persistidas.
        let (config, config_warning) = Config::load();

        let default_cookies = dirs::config_dir().map(|dir| dir.join("ytmtui/cookies.txt"));
        let resolution = resolve_cookie_path(
            std::env::var("YTM_COOKIES").ok(),
            config.cookies.clone(),
            default_cookies,
        );
        let cookies = resolution.path;

        let (mut player, player_warning) = AudioPlayer::new();
        player.set_volume(config.volume);

        let (client, authentication) = match cookies.as_deref() {
            Some(path) => match YtMusicClient::with_cookies(path) {
                Ok(client) => (client, AuthenticationState::Authenticated),
                Err(_) => (YtMusicClient::new(), AuthenticationState::InvalidCookies),
            },
            None => (YtMusicClient::new(), AuthenticationState::Anonymous),
        };

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
            AuthenticationState::Authenticated => {
                "Signed in. Loading your library... Press / to search or ? for help.".to_string()
            }
            AuthenticationState::InvalidCookies => {
                "Cookie file is invalid. Press g to sign in from your browser.".to_string()
            }
            AuthenticationState::Anonymous => match resolution.missing_requested_path.as_deref() {
                Some(path) => format!("Configured cookie file does not exist: {path}"),
                None => "Welcome to ytmtui. Press / to search or ? for help.".to_string(),
            },
            AuthenticationState::Expired => {
                "Session expired. Press g to sign in again from your browser.".to_string()
            }
        };
        // Avisos de inicialização (config corrompida, thread de áudio) têm
        // prioridade sobre a mensagem de status padrão: são acionáveis e
        // não deveriam ser silenciosamente encobertos por ela.
        let status = player_warning.or(config_warning).unwrap_or(status);

        Ok(Self {
            running: true,
            client,
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
            liked: std::collections::HashSet::new(),
            autoplay: true,
            pending_radio_seed: None,
            authentication,
            account_name,
            theme_index,
            list_state,
            queue: Vec::new(),
            queue_index: None,
            current: None,
            next_index: None,
            shuffle: config.shuffle,
            repeat: RepeatMode::from_config(&config.repeat),
            rng_state: seed,
            lyrics: crate::lyrics::LyricsState::None,
            lyrics_scroll: 0,
            picker: None,
            artwork: None,
            artwork_source: None,
            clear_screen: false,
            status,
            cookies,
            loading_audio: false,
            busy: false,
            spinner_frame: 0,
            // Defends against a hand-edited config value of 0 creating a
            // hot loop of re-fetches.
            sync_interval: std::time::Duration::from_secs(config.sync_interval_secs.max(30)),
            last_synced: std::time::Instant::now(),
        })
    }

    pub fn is_authenticated(&self) -> bool {
        self.authentication.is_authenticated()
    }

    /// Há alguma tarefa de carregamento em andamento (rede ou áudio)?
    pub fn is_loading(&self) -> bool {
        self.busy || self.loading_audio
    }

    /// Whether the UI currently benefits from frequent redraws: a loading
    /// spinner is animating or playback progress is advancing. Idle frames
    /// can redraw far less often without losing feedback.
    pub fn needs_animation(&self) -> bool {
        self.is_loading() || (self.current.is_some() && !self.player.is_paused())
    }

    /// Whether the open section is actively animating and needs the fast
    /// redraw tier: the Home spectrum visualizer, or the synced-lyrics
    /// karaoke wipe — both must look like continuous motion. Only true while
    /// a track is audibly playing, so the cost is paid exactly when the
    /// animation is visible.
    pub fn needs_fast_animation(&self) -> bool {
        let animated_section = self.section == Section::Inicio
            || (self.section == Section::Letra
                && matches!(self.lyrics, crate::lyrics::LyricsState::Synced { .. }));
        animated_section && self.current.is_some() && !self.player.is_paused()
    }

    /// Consumes the pending full-clear flag set by [`Self::clear_artwork`].
    /// The main loop calls this right before drawing and, if set, erases the
    /// whole terminal so leftover Kitty/Sixel graphics from the previous
    /// cover don't linger behind the next frame.
    pub fn take_clear_screen(&mut self) -> bool {
        std::mem::take(&mut self.clear_screen)
    }

    /// Glifo atual do spinner de carregamento (braille animado).
    pub fn spinner(&self) -> char {
        const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        FRAMES[self.spinner_frame % FRAMES.len()]
    }

    /// Login com uma tecla: importa cookies do primeiro navegador instalado
    /// que tenha uma sessão do YouTube, salva em `~/.config/ytmtui/cookies.txt`
    /// e reconecta o cliente sem reiniciar o app. Também serve para renovar
    /// uma sessão expirada.
    pub fn sign_in(&mut self) {
        if self.busy {
            self.status = "Aguarde a tarefa atual terminar antes de conectar.".to_string();
            return;
        }
        let Some(home) = dirs::home_dir() else {
            self.status = "⚠ Não foi possível localizar o diretório home.".to_string();
            return;
        };
        let browsers = detect_browsers(&home);
        if browsers.is_empty() {
            self.status =
                "⚠ Nenhum navegador suportado encontrado (Brave/Chrome/Firefox…).".to_string();
            return;
        }
        let Some(dest) = dirs::config_dir().map(|d| d.join("ytmtui/cookies.txt")) else {
            self.status = "⚠ Não foi possível localizar o diretório de config.".to_string();
            return;
        };
        self.busy = true;
        let first = browsers[0].split(':').next().unwrap_or(&browsers[0]);
        self.status = format!("Conectando: importando cookies de {first}…");
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut last_error = String::new();
            for browser in browsers {
                let _ = tx.send(Msg::Status(format!("Importando cookies de {browser}…")));
                match export_browser_cookies(&browser, &dest) {
                    Ok(warning) => {
                        let _ = tx.send(Msg::CookiesImported {
                            path: dest.to_string_lossy().into_owned(),
                            browser,
                            warning,
                        });
                        return;
                    }
                    Err(e) => last_error = format!("{browser}: {e}"),
                }
            }
            let _ = tx.send(Msg::Error(format!("Falha ao conectar — {last_error}")));
        });
    }

    /// Carrega (em background) o nome da conta, se autenticado e sem nome
    /// personalizado já definido na config.
    pub fn load_account(&self) {
        if !self.is_authenticated() {
            return;
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.get_account_name().await {
                Ok(Some(name)) => {
                    let _ = tx.send(Msg::AccountName(Some(name)));
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = tx.send(client_error_message("Could not load account", error));
                }
            }
        });
    }

    /// Tema de cores ativo.
    pub fn theme(&self) -> &'static crate::theme::Theme {
        crate::theme::get(self.theme_index)
    }

    /// Alterna para o próximo tema de cores e salva a preferência.
    pub fn cycle_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % crate::theme::THEMES.len();
        self.status = format!("🎨 Tema: {}", self.theme().name);
        self.save_config();
    }

    /// Persiste as preferências atuais em disco.
    pub fn save_config(&self) {
        // Nunca apaga um caminho de cookies válido já salvo: se o app subiu
        // sem cookies (self.cookies == None), preserva o que estiver em disco.
        // O aviso de corrupção já foi mostrado uma vez em `App::new`; aqui
        // só precisamos do valor (o padrão é seguro caso o arquivo já tenha
        // sido reescrito nesse meio tempo).
        let (saved, _) = Config::load();
        let cookies = self.cookies.clone().or(saved.cookies);
        // Só persiste um username se for personalizado (mantém o que já existia
        // em vez de gravar o nome obtido da API automaticamente).
        let username = saved.username;
        Config {
            volume: self.player.volume(),
            shuffle: self.shuffle,
            repeat: self.repeat.as_config().to_string(),
            cookies,
            theme: self.theme().name.to_string(),
            username,
            // Not editable at runtime yet; preserve whatever's on disk
            // rather than overwriting it with the in-memory Duration.
            sync_interval_secs: saved.sync_interval_secs,
        }
        .save();
    }

    /// Número de itens na lista principal da seção atual.
    pub fn main_len(&self) -> usize {
        match self.section {
            Section::Inicio => self.home_total_count(),
            Section::Buscar if self.search_mixed => self.search_item_count(),
            Section::Buscar => self.songs.len(),
            Section::Biblioteca => self.library.len(),
            Section::Playlists => self.playlists.len(),
            Section::Artistas => self.artists.len(),
            Section::Fila => self.queue.len(),
            _ => 0,
        }
    }

    /// Move a seleção da lista principal.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.main_len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.list_state.select(Some(next));
    }

    /// Move a seleção da barra lateral.
    pub fn move_sidebar(&mut self, delta: isize) {
        let len = Section::ALL.len() as isize;
        let next = (self.sidebar_index as isize + delta).rem_euclid(len) as usize;
        self.sidebar_index = next;
        self.section = Section::ALL[next];
        // Reposiciona a seleção da lista ao trocar de seção.
        self.list_state.select(Some(0));
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
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            running: true,
            client: YtMusicClient::new(),
            player: AudioPlayer::new().0,
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
            // Tests must not read (or later write) the user's real
            // recent.json; they start with an empty in-memory history.
            recent: Vec::new(),
            liked: std::collections::HashSet::new(),
            autoplay: true,
            pending_radio_seed: None,
            authentication: AuthenticationState::Anonymous,
            account_name: None,
            theme_index: 0,
            list_state,
            queue: Vec::new(),
            queue_index: None,
            current: None,
            next_index: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            rng_state: 0x9E3779B97F4A7C15,
            lyrics: crate::lyrics::LyricsState::None,
            lyrics_scroll: 0,
            picker: None,
            artwork: None,
            artwork_source: None,
            clear_screen: false,
            status: "Ready.".to_string(),
            cookies: None,
            loading_audio: false,
            busy: false,
            spinner_frame: 0,
            sync_interval: std::time::Duration::from_secs(300),
            last_synced: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ytmusic::YtMusicError;
    use reqwest::StatusCode;

    #[test]
    fn background_home_refresh_preserves_selection_by_browse_id() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.home = vec![crate::ytmusic::HomeSection {
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
            .send(Msg::HomeSections(vec![crate::ytmusic::HomeSection {
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
        app.home = vec![crate::ytmusic::HomeSection {
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
            .send(Msg::HomeSections(vec![crate::ytmusic::HomeSection {
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
            .send(Msg::HomeSections(vec![crate::ytmusic::HomeSection {
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

        app.busy = true;
        assert!(app.needs_animation(), "loading shows the spinner");
        app.busy = false;

        app.current = Some(crate::ytmusic::Track::default());
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

    fn home_sections() -> Vec<crate::ytmusic::HomeSection> {
        vec![
            crate::ytmusic::HomeSection {
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
            crate::ytmusic::HomeSection {
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
        app.artists = vec![crate::ytmusic::Artist {
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
    fn session_expiry_maps_to_the_dedicated_message() {
        let message = client_error_message(
            "Could not load library",
            YtMusicError::SessionExpired {
                status: StatusCode::UNAUTHORIZED,
                endpoint: "browse".to_string(),
            },
        );

        assert!(matches!(message, Msg::SessionExpired));
    }
}
