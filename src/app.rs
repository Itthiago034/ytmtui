//! Estado central da aplicação e lógica de coordenação.

mod authentication;

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
    /// Quantas tarefas de carregamento (busca/playlist/artista/biblioteca/
    /// sign-in) estão em voo. Um contador, e não um bool: o sync periódico
    /// dispara Home e Biblioteca juntas, e a primeira resposta não pode
    /// apagar o spinner enquanto a outra ainda carrega.
    busy_tasks: usize,
    /// Um sign-in (importação de cookies) está em andamento. Separado de
    /// `busy_tasks` para que um sync de fundo não bloqueie a tecla `g`.
    signing_in: bool,
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
        let config = Config::load();

        let default_cookies = dirs::config_dir().map(|dir| dir.join("ytmtui/cookies.txt"));
        let resolution = resolve_cookie_path(
            std::env::var("YTM_COOKIES").ok(),
            config.cookies.clone(),
            default_cookies,
        );
        let cookies = resolution.path;

        let mut player = AudioPlayer::new()?;
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
            busy_tasks: 0,
            signing_in: false,
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

    /// Há alguma tarefa de carregamento de conteúdo (rede) em andamento?
    pub fn busy(&self) -> bool {
        self.busy_tasks > 0
    }

    /// Registra o início de uma tarefa contada no spinner. Cada tarefa
    /// iniciada por aqui deve terminar em exatamente uma mensagem que chame
    /// [`Self::finish_task`] (payload, `SessionExpired` ou `Error`).
    pub(crate) fn begin_task(&mut self) {
        self.busy_tasks += 1;
    }

    /// Registra o fim de uma tarefa contada. Saturante: tarefas não contadas
    /// (download de áudio, curtir, rádio de autoplay) também reportam erros
    /// pelo canal, e um decremento a mais não pode enlouquecer o contador.
    fn finish_task(&mut self) {
        self.busy_tasks = self.busy_tasks.saturating_sub(1);
    }

    /// Há alguma tarefa de carregamento em andamento (rede ou áudio)?
    pub fn is_loading(&self) -> bool {
        self.busy() || self.loading_audio
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

    /// Carrega (em background) as playlists da biblioteca, se autenticado.
    pub fn load_library(&mut self) {
        if !self.is_authenticated() {
            return;
        }
        self.begin_task();
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.get_library_playlists().await {
                Ok(pls) => {
                    let _ = tx.send(Msg::LibraryPlaylists(pls));
                }
                Err(error) => {
                    let _ = tx.send(client_error_message("Could not load library", error));
                }
            }
        });
    }

    /// Carrega (em background) as recomendações da tela inicial.
    pub fn load_home(&mut self) {
        self.begin_task();
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.get_home().await {
                Ok(sections) => {
                    let _ = tx.send(Msg::HomeSections(sections));
                }
                Err(error) => {
                    let _ = tx.send(client_error_message(
                        "Could not load recommendations",
                        error,
                    ));
                }
            }
        });
    }

    /// Periodic background refresh of Home and Library, called from
    /// `tick()`. Reuses the existing one-shot loaders — no new HTTP call
    /// shapes — so the only user-visible effect while browsing is the small
    /// spinner glyph blinking briefly; selection is preserved in
    /// `drain_messages` rather than reset to the top.
    pub fn sync_home_and_library(&mut self) {
        self.load_home();
        self.load_library(); // already a no-op when unauthenticated.
    }

    /// Flattened selectable-item count across all Home sections; section
    /// header rows aren't counted since they aren't selectable.
    pub fn home_item_count(&self) -> usize {
        self.home.iter().map(|s| s.items.len()).sum()
    }

    /// Maps a flattened selection index (as used by `list_state`) back to
    /// the `Playlist` it refers to.
    pub fn home_item_at(&self, index: usize) -> Option<&Playlist> {
        let mut remaining = index;
        for section in &self.home {
            if remaining < section.items.len() {
                return section.items.get(remaining);
            }
            remaining -= section.items.len();
        }
        None
    }

    /// Finds the flattened index of the item with the given `browse_id`, if
    /// still present after a Home refresh. Used to preserve the selection
    /// across a background sync.
    pub fn home_flat_index_of(&self, browse_id: &str) -> Option<usize> {
        let mut flat = 0;
        for section in &self.home {
            for item in &section.items {
                if item.browse_id == browse_id {
                    return Some(flat);
                }
                flat += 1;
            }
        }
        None
    }

    /// Total de itens selecionáveis na tela Início: o histórico recente vem
    /// primeiro, seguido dos itens das seções de recomendações.
    pub fn home_total_count(&self) -> usize {
        self.recent.len() + self.home_item_count()
    }

    /// Registra uma faixa no histórico local (topo da lista, sem duplicatas,
    /// limitado a [`RECENT_CAP`]) e persiste em `recent.json`. Persistência é
    /// melhor-esforço: falhas de disco nunca interrompem a reprodução.
    fn remember_recent(&mut self, track: &Track) {
        self.recent.retain(|t| t.video_id != track.video_id);
        self.recent.insert(0, track.clone());
        self.recent.truncate(RECENT_CAP);
        let Some(path) = recent_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.recent) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Total de itens selecionáveis nos resultados mistos da busca, na ordem
    /// em que são exibidos: músicas, artistas, álbuns, playlists.
    pub fn search_item_count(&self) -> usize {
        self.songs.len() + self.artists.len() + self.albums.len() + self.playlists.len()
    }

    /// Resolve um índice achatado da seleção (como usado pelo `list_state`)
    /// para o item dos resultados mistos a que ele se refere.
    pub fn search_hit_at(&self, index: usize) -> Option<SearchHit> {
        let mut i = index;
        if i < self.songs.len() {
            return Some(SearchHit::Song(i));
        }
        i -= self.songs.len();
        if i < self.artists.len() {
            return Some(SearchHit::Artist(self.artists[i].clone()));
        }
        i -= self.artists.len();
        if i < self.albums.len() {
            return Some(SearchHit::Album(self.albums[i].clone()));
        }
        i -= self.albums.len();
        self.playlists.get(i).cloned().map(SearchHit::Playlist)
    }

    /// Abre o item selecionado na tela inicial: faixas do histórico recente
    /// tocam na hora (a fila vira o próprio histórico); recomendações abrem
    /// como playlist.
    pub fn open_selected_home(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        if idx < self.recent.len() {
            self.queue = self.recent.clone();
            self.queue_index = Some(idx);
            self.start_current();
            return;
        }
        let Some(pl) = self.home_item_at(idx - self.recent.len()).cloned() else {
            return;
        };
        self.load_playlist(pl);
    }

    /// Abre o artista selecionado, carregando suas principais faixas.
    pub fn open_selected_artist(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let Some(artist) = self.artists.get(idx).cloned() else {
            return;
        };
        self.load_artist(artist);
    }

    /// Dispara o carregamento (assíncrono) das principais faixas do artista.
    fn load_artist(&mut self, artist: Artist) {
        if artist.browse_id.is_empty() {
            self.status = "Artista sem página disponível.".to_string();
            return;
        }
        self.status = format!("Carregando artista \"{}\"...", artist.name);
        self.begin_task();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let title = format!("Artist: {}", artist.name);
        tokio::spawn(async move {
            match client.get_artist(&artist.browse_id).await {
                Ok(tracks) => {
                    let _ = tx.send(Msg::PlaylistTracks { title, tracks });
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("Erro ao abrir artista: {e}")));
                }
            }
        });
    }

    /// Adiciona a faixa selecionada ao fim da fila (sem interromper a atual).
    pub fn enqueue_selected(&mut self) {
        let track = match self.section {
            // Nos resultados mistos, apenas músicas podem ir para a fila.
            Section::Buscar if self.search_mixed => {
                match self
                    .list_state
                    .selected()
                    .and_then(|i| self.search_hit_at(i))
                {
                    Some(SearchHit::Song(i)) => self.songs.get(i).cloned(),
                    Some(_) => {
                        self.status = "Somente músicas podem ser adicionadas à fila.".to_string();
                        return;
                    }
                    None => None,
                }
            }
            Section::Buscar => self
                .list_state
                .selected()
                .and_then(|i| self.songs.get(i))
                .cloned(),
            Section::Fila => None, // já está na fila
            _ => None,
        };
        let Some(track) = track else { return };
        let title = track.title.clone();
        self.queue.push(track);
        // Nada tocando ainda? começa a tocar o que foi enfileirado.
        if self.current.is_none() {
            self.queue_index = Some(self.queue.len() - 1);
            self.start_current();
        } else {
            // Recalcula o próximo (a fila mudou de tamanho).
            if let Some(idx) = self.queue_index {
                self.next_index = self.compute_next(idx, self.repeat != RepeatMode::Off);
            }
            self.status = format!(
                "➕ \"{title}\" adicionada à fila ({} na fila).",
                self.queue.len()
            );
        }
    }

    /// Login com uma tecla: importa cookies do primeiro navegador instalado
    /// que tenha uma sessão do YouTube, salva em `~/.config/ytmtui/cookies.txt`
    /// e reconecta o cliente sem reiniciar o app. Também serve para renovar
    /// uma sessão expirada.
    pub fn sign_in(&mut self) {
        if self.signing_in {
            self.status = "Aguarde: a conexão anterior ainda está em andamento.".to_string();
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
        self.begin_task();
        self.signing_in = true;
        let first = browsers[0].split(':').next().unwrap_or(&browsers[0]);
        self.status = format!("Conectando: importando cookies de {first}…");
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut last_error = String::new();
            for browser in browsers {
                let _ = tx.send(Msg::Status(format!("Importando cookies de {browser}…")));
                match export_browser_cookies(&browser, &dest) {
                    Ok(()) => {
                        let _ = tx.send(Msg::CookiesImported {
                            path: dest.to_string_lossy().into_owned(),
                            browser,
                        });
                        return;
                    }
                    Err(e) => last_error = format!("{browser}: {e}"),
                }
            }
            let _ = tx.send(Msg::Error(format!("Falha ao conectar — {last_error}")));
        });
    }

    /// Curte ou descurte a faixa atual (alterna com base no estado da sessão).
    pub fn like_current(&mut self) {
        let Some(track) = self.current.clone() else {
            self.status = "Nada tocando para curtir.".to_string();
            return;
        };
        if !self.is_authenticated() {
            self.status = "⚠ Conecte sua conta para curtir faixas.".to_string();
            return;
        }
        let vid = track.video_id.clone();
        let like = !self.liked.contains(&vid);
        if like {
            self.liked.insert(vid.clone());
            self.status = format!("💚 Curtiu: {}", track.title);
        } else {
            self.liked.remove(&vid);
            self.status = format!("🤍 Removeu a curtida: {}", track.title);
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Err(e) = client.rate_song(&vid, like).await {
                let _ = tx.send(Msg::Error(format!("Não foi possível curtir: {e}")));
            }
        });
    }

    /// Carrega (em background) o nome da conta, se autenticado e sem nome
    /// personalizado já definido na config.
    pub fn load_account(&mut self) {
        if !self.is_authenticated() {
            return;
        }
        self.begin_task();
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.get_account_name().await {
                // `None` também é enviado: toda tarefa contada precisa
                // terminar em exatamente uma mensagem (ver `begin_task`).
                Ok(name) => {
                    let _ = tx.send(Msg::AccountName(name));
                }
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
        let saved = Config::load();
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

    /// Gera o próximo número pseudoaleatório (xorshift64).
    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }

    /// Alterna a reprodução aleatória.
    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.status = if self.shuffle {
            "🔀 Aleatório ativado.".to_string()
        } else {
            "➡ Aleatório desativado.".to_string()
        };
        // Recalcula o próximo com base no novo modo.
        if let Some(idx) = self.queue_index {
            self.next_index = self.compute_next(idx, self.repeat != RepeatMode::Off);
        }
    }

    /// Alterna o modo de repetição (Off → Todos → Um).
    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.next();
        self.status = format!("🔁 Repetição: {}.", self.repeat.label());
        if let Some(idx) = self.queue_index {
            self.next_index = self.compute_next(idx, self.repeat != RepeatMode::Off);
        }
    }

    /// Para a reprodução e limpa todo o estado "tocando agora" (faixa, capa,
    /// letra e download em andamento) — diferente de `player.stop()` sozinho,
    /// que silencia o áudio mas deixaria a UI mostrando a faixa como ativa.
    /// A fila é preservada: Enter na Fila retoma de onde o usuário quiser.
    pub fn stop_playback(&mut self) {
        let had_track = self.current.is_some() || self.loading_audio;
        self.player.stop();
        self.current = None;
        self.loading_audio = false;
        self.lyrics = crate::lyrics::LyricsState::None;
        self.lyrics_scroll = 0;
        self.visualizer.reset();
        if had_track {
            self.clear_artwork();
            self.status = "⏹ Reprodução parada.".to_string();
        }
    }

    /// Avança 5s na faixa atual.
    pub fn seek_forward(&mut self) {
        if self.current.is_some() {
            self.player.seek_forward(5);
        }
    }

    /// Retrocede 5s na faixa atual.
    pub fn seek_backward(&mut self) {
        if self.current.is_some() {
            self.player.seek_backward(5);
        }
    }

    /// Calcula o índice da próxima faixa a partir de `idx`.
    ///
    /// `allow_wrap` indica se, ao chegar ao fim em ordem sequencial, deve voltar
    /// ao início. No modo aleatório, escolhe um índice diferente do atual.
    fn compute_next(&mut self, idx: usize, allow_wrap: bool) -> Option<usize> {
        let len = self.queue.len();
        if len == 0 {
            return None;
        }
        if len == 1 {
            return if allow_wrap { Some(0) } else { None };
        }
        if self.shuffle {
            let mut n = idx;
            while n == idx {
                n = (self.next_rand() % len as u64) as usize;
            }
            Some(n)
        } else if idx + 1 < len {
            Some(idx + 1)
        } else if allow_wrap {
            Some(0)
        } else {
            None
        }
    }

    /// Pré-baixa (em background) o áudio da faixa de índice `idx` para o cache.
    fn prefetch(&self, idx: usize) {
        let Some(track) = self.queue.get(idx) else {
            return;
        };
        if track.video_id.is_empty() {
            return;
        }
        let url = track.watch_url();
        let vid = track.video_id.clone();
        let cookies = self.cookies.clone();
        tokio::task::spawn_blocking(move || {
            let _ = player::download_audio(&url, &vid, cookies.as_deref());
        });
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

    /// Dispara uma busca assíncrona com a query atual.
    pub fn do_search(&mut self) {
        let q = self.query.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.status = format!("Buscando por \"{q}\"...");
        self.begin_task();
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.search(&q).await {
                Ok(res) => {
                    let _ = tx.send(Msg::SearchResults(res));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("Erro na busca: {e}")));
                }
            }
        });
    }

    /// Abre a playlist da biblioteca selecionada, carregando suas faixas.
    pub fn open_selected_library_playlist(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let Some(pl) = self.library.get(idx).cloned() else {
            return;
        };
        self.load_playlist(pl);
    }

    /// Abre a playlist selecionada, carregando suas faixas.
    pub fn open_selected_playlist(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let Some(pl) = self.playlists.get(idx).cloned() else {
            return;
        };
        self.load_playlist(pl);
    }

    /// Dispara o carregamento (assíncrono) das faixas de uma playlist.
    fn load_playlist(&mut self, pl: Playlist) {
        self.load_browse(pl, "Playlist");
    }

    /// Dispara o carregamento das faixas de uma playlist ou álbum; `kind`
    /// rotula o painel de resultados ("Playlist"/"Album").
    fn load_browse(&mut self, pl: Playlist, kind: &str) {
        self.status = format!("Carregando \"{}\"...", pl.title);
        self.begin_task();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let title = format!("{kind}: {}", pl.title);
        tokio::spawn(async move {
            match client.get_playlist_tracks(&pl.browse_id).await {
                Ok(tracks) => {
                    let _ = tx.send(Msg::PlaylistTracks { title, tracks });
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("Erro ao abrir playlist: {e}")));
                }
            }
        });
    }

    /// Reproduz a faixa selecionada na lista atual (busca ou fila),
    /// definindo a fila de reprodução a partir da lista.
    pub fn play_selected(&mut self) {
        if self.prepare_selection_for_playback() {
            self.start_current();
            // A searched song seeds a radio of similar tracks (fetched in
            // the background and appended behind the one now playing).
            if let Some(seed) = self.pending_radio_seed.take() {
                self.fetch_related(seed);
            }
        }
    }

    /// Busca (em background) a rádio de faixas semelhantes à `seed` para
    /// completar a fila atrás do que está tocando.
    fn fetch_related(&self, seed: String) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(tracks) = client.get_radio(&seed).await {
                let _ = tx.send(Msg::RelatedTracks { seed, tracks });
            }
        });
    }

    /// Anexa as faixas semelhantes ao fim da fila, sem duplicar o que já
    /// está nela e só enquanto a `seed` ainda é a faixa atual (resultados
    /// atrasados de uma faixa já pulada são descartados). Retorna quantas
    /// entraram. Separado do handler para ser testável sem runtime.
    fn append_related(&mut self, seed: &str, tracks: Vec<Track>) -> usize {
        if !self.is_current_track(seed) {
            return 0;
        }
        let before = self.queue.len();
        for t in tracks {
            if self.queue.iter().all(|q| q.video_id != t.video_id) {
                self.queue.push(t);
            }
        }
        let added = self.queue.len() - before;
        if added > 0 {
            if let Some(idx) = self.queue_index {
                self.next_index = self.compute_next(idx, self.repeat != RepeatMode::Off);
            }
        }
        added
    }

    /// Resolve o Enter da lista atual: monta a fila (retornando `true` para
    /// iniciar a reprodução) ou dispara o carregamento de artista/álbum/
    /// playlist (retornando `false`). Separado de [`Self::play_selected`]
    /// para ser testável sem um runtime tokio ativo.
    fn prepare_selection_for_playback(&mut self) -> bool {
        match self.section {
            // Resultados mistos: a ação do Enter depende do tipo do item.
            Section::Buscar if self.search_mixed => {
                let Some(hit) = self
                    .list_state
                    .selected()
                    .and_then(|i| self.search_hit_at(i))
                else {
                    return false;
                };
                match hit {
                    // Like YT Music: playing a searched song starts a radio
                    // around it — the queue holds the song and gets filled
                    // with similar tracks, not with the other search hits.
                    SearchHit::Song(i) => {
                        let Some(track) = self.songs.get(i).cloned() else {
                            return false;
                        };
                        self.pending_radio_seed = Some(track.video_id.clone());
                        self.queue = vec![track];
                        self.queue_index = Some(0);
                    }
                    SearchHit::Artist(artist) => {
                        self.load_artist(artist);
                        return false;
                    }
                    SearchHit::Album(pl) => {
                        self.load_browse(pl, "Album");
                        return false;
                    }
                    SearchHit::Playlist(pl) => {
                        self.load_playlist(pl);
                        return false;
                    }
                }
            }
            Section::Buscar => {
                if self.songs.is_empty() {
                    return false;
                }
                // A stale selection (e.g. left over from a longer list shown
                // before this one) must not index past the current list.
                let idx = self
                    .list_state
                    .selected()
                    .unwrap_or(0)
                    .min(self.songs.len() - 1);
                self.queue = self.songs.clone();
                self.queue_index = Some(idx);
            }
            Section::Fila => {
                if self.queue.is_empty() {
                    return false;
                }
                let idx = self
                    .list_state
                    .selected()
                    .unwrap_or(0)
                    .min(self.queue.len() - 1);
                self.queue_index = Some(idx);
            }
            _ => return false,
        }
        true
    }

    /// Whether `video_id` matches the currently playing track. Used to
    /// discard results from a slow async fetch (audio download, lyrics,
    /// artwork) started for a track the user has since skipped past.
    fn is_current_track(&self, video_id: &str) -> bool {
        self.current
            .as_ref()
            .is_some_and(|t| t.video_id == video_id)
    }

    /// Clears the current album art and flags the terminal for a full clear
    /// on the next draw, so Kitty/Sixel graphics left over by the previous
    /// cover don't linger behind whatever gets drawn next.
    fn clear_artwork(&mut self) {
        self.artwork = None;
        self.artwork_source = None;
        self.clear_screen = true;
    }

    /// Rebuilds the album-art protocol from the stored cover image and asks
    /// for a full screen clear. Called on terminal resize, where graphics
    /// protocols discard their placements but the cached protocol state
    /// would otherwise never re-transmit the image.
    pub fn rebuild_artwork(&mut self) {
        if let (Some(picker), Some(img)) = (self.picker.as_mut(), self.artwork_source.as_ref()) {
            self.artwork = Some(picker.new_resize_protocol(img.clone()));
        }
        self.clear_screen = true;
    }

    /// Inicia a reprodução da faixa apontada por `queue_index`.
    fn start_current(&mut self) {
        let Some(idx) = self.queue_index else { return };
        let Some(track) = self.queue.get(idx).cloned() else {
            return;
        };
        self.current = Some(track.clone());
        self.remember_recent(&track);
        self.lyrics = crate::lyrics::LyricsState::None;
        self.lyrics_scroll = 0;
        self.clear_artwork();
        self.visualizer.reset();
        self.loading_audio = true;
        self.status = format!("Baixando \"{}\"...", track.title);

        // 1) Download / resolução do áudio (bloqueante) em task dedicada.
        let tx = self.tx.clone();
        let url = track.watch_url();
        let vid_audio = track.video_id.clone();
        let cookies = self.cookies.clone();
        tokio::task::spawn_blocking(move || {
            match player::download_audio(&url, &vid_audio, cookies.as_deref()) {
                Ok(path) => {
                    let _ = tx.send(Msg::AudioReady {
                        video_id: vid_audio,
                        path,
                    });
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("Falha ao obter áudio: {e}")));
                }
            }
        });

        // Pré-calcula e pré-baixa a próxima faixa para transição mais suave.
        self.next_index = self.compute_next(idx, self.repeat != RepeatMode::Off);
        if let Some(n) = self.next_index {
            self.prefetch(n);
        }

        // 2) Letras.
        let client = self.client.clone();
        let tx2 = self.tx.clone();
        let vid = track.video_id.clone();
        tokio::spawn(async move {
            if let Ok(lyr) = client.get_lyrics(&vid).await {
                let _ = tx2.send(Msg::Lyrics {
                    video_id: vid,
                    lyrics: lyr,
                });
            }
        });

        // 3) Capa (artwork).
        if let Some(url) = track.thumbnail.clone() {
            let tx3 = self.tx.clone();
            let http = self.client.clone();
            let vid_art = track.video_id.clone();
            tokio::spawn(async move {
                if let Ok(bytes) = http.fetch_bytes(&url).await {
                    let _ = tx3.send(Msg::ArtworkBytes {
                        video_id: vid_art,
                        bytes,
                    });
                }
            });
        }
    }

    /// Avança para a próxima faixa da fila (comando manual `n`).
    ///
    /// Ao contrário do auto-avanço, o pulo manual sempre segue para uma próxima
    /// faixa (com wrap), independentemente do modo de repetição.
    pub fn next_track(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        let idx = self.queue_index.unwrap_or(0);
        let next = self.compute_next(idx, true).unwrap_or(0);
        self.queue_index = Some(next);
        self.start_current();
    }

    /// Volta para a faixa anterior da fila.
    pub fn prev_track(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        let idx = self.queue_index.unwrap_or(0);
        let prev = if self.shuffle && self.queue.len() > 1 {
            let mut n = idx;
            while n == idx {
                n = (self.next_rand() % self.queue.len() as u64) as usize;
            }
            n
        } else if idx == 0 {
            self.queue.len() - 1
        } else {
            idx - 1
        };
        self.queue_index = Some(prev);
        self.start_current();
    }

    /// Auto-avanço ao terminar a faixa (respeita os modos de repetição).
    fn advance_auto(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if self.repeat == RepeatMode::One {
            // Repete a mesma faixa.
            self.start_current();
            return;
        }
        match self.next_index.or_else(|| {
            self.queue_index
                .and_then(|idx| self.compute_next(idx, self.repeat != RepeatMode::Off))
        }) {
            Some(n) => {
                self.queue_index = Some(n);
                self.start_current();
            }
            None => {
                // Fim da fila: tenta continuar com uma rádio (autoplay).
                if self.autoplay {
                    if let Some(seed) = self.current.as_ref().map(|t| t.video_id.clone()) {
                        if !seed.is_empty() {
                            self.status = "📻 Fila concluída — carregando rádio...".to_string();
                            let client = self.client.clone();
                            let tx = self.tx.clone();
                            tokio::spawn(async move {
                                match client.get_radio(&seed).await {
                                    Ok(tracks) => {
                                        let _ = tx.send(Msg::RadioTracks(tracks));
                                    }
                                    Err(error) => {
                                        let _ = tx.send(client_error_message(
                                            "Could not load radio",
                                            error,
                                        ));
                                    }
                                }
                            });
                            return;
                        }
                    }
                }
                // Sem autoplay/semente: encerra a reprodução.
                self.player.stop();
                self.current = None;
                self.clear_artwork();
                self.loading_audio = false;
                self.status = "Fila concluída.".to_string();
            }
        }
    }

    /// Processa mensagens recebidas das tasks assíncronas.
    pub fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::SearchResults(res) => {
                    self.finish_task();
                    self.songs = res.songs;
                    self.songs_title = "Search results".to_string();
                    self.playlists = res.playlists;
                    self.artists = res.artists;
                    self.albums = res.albums;
                    self.search_mixed = true;
                    self.status = format!(
                        "{} músicas, {} artistas, {} álbuns, {} playlists.",
                        self.songs.len(),
                        self.artists.len(),
                        self.albums.len(),
                        self.playlists.len()
                    );
                    // `songs`/`playlists`/`artists` were all just replaced,
                    // so any list_state selection now refers to whichever of
                    // them is visible — reset it regardless of section, or a
                    // stale index from a longer previous list survives and
                    // desyncs Enter-key handling from what's on screen.
                    self.list_state.select(Some(0));
                }
                Msg::LibraryPlaylists(pls) => {
                    self.finish_task();
                    // A background sync (Feature 3) re-runs this same load
                    // periodically; preserve the current selection by
                    // `browse_id` instead of always resetting to the top, or
                    // background refreshes would jerk the list back to
                    // index 0 while the user is mid-browse.
                    let was_empty = self.library.is_empty();
                    let previous_id = (self.section == Section::Biblioteca)
                        .then(|| self.list_state.selected())
                        .flatten()
                        .and_then(|i| self.library.get(i))
                        .map(|p| p.browse_id.clone());
                    self.library = pls;
                    if self.section == Section::Biblioteca {
                        let new_index = previous_id
                            .and_then(|id| self.library.iter().position(|p| p.browse_id == id))
                            .or(if was_empty {
                                Some(0)
                            } else {
                                self.list_state.selected()
                            })
                            .map(|i| i.min(self.library.len().saturating_sub(1)));
                        self.list_state
                            .select((!self.library.is_empty()).then_some(new_index).flatten());
                    }
                    // Só o primeiro carregamento anuncia na status bar: o
                    // sync periódico repassa por aqui a cada poucos minutos
                    // e não pode apagar o que o usuário estava lendo
                    // ("▶ Tocando…", um erro, etc.).
                    if was_empty && !self.library.is_empty() {
                        self.status = format!(
                            "Library loaded: {} playlist(s). Open Library in the menu.",
                            self.library.len()
                        );
                    }
                }
                Msg::HomeSections(sections) => {
                    self.finish_task();
                    let was_empty = self.home.is_empty();
                    // Selection indices on Home count the local recent-tracks
                    // group first; recommendation lookups must skip past it.
                    let recent_len = self.recent.len();
                    let previous_id = (self.section == Section::Inicio)
                        .then(|| self.list_state.selected())
                        .flatten()
                        .and_then(|i| i.checked_sub(recent_len))
                        .and_then(|i| self.home_item_at(i))
                        .map(|p| p.browse_id.clone());
                    self.home = sections;
                    if self.section == Section::Inicio {
                        let count = self.home_total_count();
                        let new_index = previous_id
                            .and_then(|id| self.home_flat_index_of(&id))
                            .map(|i| i + recent_len)
                            .or(if was_empty {
                                Some(0)
                            } else {
                                self.list_state.selected()
                            })
                            .map(|i| i.min(count.saturating_sub(1)));
                        self.list_state
                            .select((count > 0).then_some(new_index).flatten());
                    }
                }
                Msg::RadioTracks(tracks) => {
                    if tracks.is_empty() {
                        self.player.stop();
                        self.current = None;
                        self.clear_artwork();
                        self.loading_audio = false;
                        self.status = "Fila concluída.".to_string();
                    } else {
                        let start = self.queue.len();
                        self.queue.extend(tracks);
                        self.queue_index = Some(start);
                        self.status = "📻 Rádio iniciada.".to_string();
                        self.start_current();
                    }
                }
                Msg::AccountName(name) => {
                    self.finish_task();
                    if let Some(n) = name {
                        if self.account_name.is_none() {
                            self.account_name = Some(n);
                        }
                    }
                }
                Msg::SessionExpired => {
                    self.finish_task();
                    self.authentication = AuthenticationState::Expired;
                    self.library.clear();
                    self.account_name = None;
                    self.status = "Session expired. Press g to sign in again from your \
                                   browser (music.youtube.com must be signed in there)."
                        .to_string();
                }
                Msg::PlaylistTracks { title, tracks } => {
                    self.finish_task();
                    self.songs = tracks;
                    self.songs_title = title;
                    // Uma lista concreta de faixas substitui a visão mista da
                    // busca; a próxima busca a reativa.
                    self.search_mixed = false;
                    self.section = Section::Buscar;
                    self.sidebar_index = 0;
                    self.list_state.select(Some(0));
                    self.status = format!("{} faixas carregadas.", self.songs.len());
                }
                Msg::Lyrics { video_id, lyrics } => {
                    // A slow fetch for a track the user has since skipped
                    // past must not overwrite the current track's lyrics.
                    if self.is_current_track(&video_id) {
                        use crate::lyrics::LyricsState;
                        use crate::ytmusic::Lyrics;
                        self.lyrics = match lyrics {
                            Some(Lyrics::Synced(lines)) => LyricsState::Synced {
                                lines,
                                active: None,
                            },
                            Some(Lyrics::Plain(text)) => LyricsState::Plain(text),
                            None => LyricsState::NotAvailable,
                        };
                    }
                }
                Msg::ArtworkBytes { video_id, bytes } => {
                    if self.is_current_track(&video_id) {
                        // Decode the cover and prepare it for the terminal's
                        // image protocol; without a picker no art is shown.
                        // The decoded image is kept so a terminal resize can
                        // re-transmit it (see `rebuild_artwork`).
                        let decoded = image::load_from_memory(&bytes).ok();
                        self.artwork = match (self.picker.as_mut(), decoded.clone()) {
                            (Some(picker), Some(img)) => Some(picker.new_resize_protocol(img)),
                            _ => None,
                        };
                        self.artwork_source = decoded;
                    }
                }
                Msg::AudioReady { video_id, path } => {
                    // A slow download for a track the user has since skipped
                    // past must never start playing over the current one.
                    if self.is_current_track(&video_id) {
                        self.loading_audio = false;
                        if let Some(t) = &self.current {
                            self.status = format!("▶ Tocando: {} — {}", t.title, t.artist);
                        }
                        self.player.play_file(path);
                    }
                }
                Msg::CookiesImported { path, browser } => {
                    self.finish_task();
                    self.signing_in = false;
                    match YtMusicClient::with_cookies(&path) {
                        Ok(client) => {
                            self.client = client;
                            self.cookies = Some(path);
                            self.authentication = AuthenticationState::Authenticated;
                            self.account_name = None;
                            let name = browser.split(':').next().unwrap_or(&browser);
                            self.status =
                                format!("✔ Conectado via {name}. Carregando suas músicas…");
                            self.load_account();
                            self.load_home();
                            self.load_library();
                        }
                        Err(e) => {
                            self.authentication = AuthenticationState::InvalidCookies;
                            self.status = format!("⚠ Cookies importados são inválidos: {e}");
                        }
                    }
                }
                Msg::RelatedTracks { seed, tracks } => {
                    let added = self.append_related(&seed, tracks);
                    if added > 0 {
                        self.status = format!("📻 +{added} músicas semelhantes na fila.");
                        if let Some(n) = self.next_index {
                            self.prefetch(n);
                        }
                    }
                }
                Msg::Status(s) => self.status = s,
                Msg::Error(e) => {
                    self.loading_audio = false;
                    self.finish_task();
                    // Um sign-in que falhou termina aqui; libera o `g`.
                    self.signing_in = false;
                    self.status = format!("⚠ {e}");
                }
            }
        }
    }

    /// Chamado a cada tick para tarefas periódicas (auto-avanço de faixa).
    pub fn tick(&mut self) {
        if self.is_loading() {
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
        }
        if self.player.take_finished() && !self.loading_audio {
            self.advance_auto();
        }

        // Spectrum analysis only matters while it's visible (Home) and
        // audible (a track is loaded and not paused); elsewhere tapped
        // chunks are simply left to be dropped by the tap's backpressure,
        // and the bars settle toward zero instead of freezing.
        if self.section == Section::Inicio {
            let audible = self.current.is_some() && !self.player.is_paused();
            if audible {
                for chunk in self.player.drain_sample_chunks() {
                    self.visualizer.push_samples(&chunk);
                }
            } else {
                self.visualizer.decay_idle();
            }
        }

        // Advances the synced-lyrics active line every tick regardless of
        // section: this is a cheap O(1)/O(log n) index bump (unlike the
        // visualizer's per-chunk FFT work above), so the Lyrics section is
        // already showing the right line the instant the user switches to
        // it mid-song instead of needing one extra tick to catch up.
        if let crate::lyrics::LyricsState::Synced { lines, active } = &mut self.lyrics {
            let position_ms = self.player.position().as_millis() as u64;
            *active = crate::lyrics::advance_active_line(lines, *active, position_ms);
        }

        if self.last_synced.elapsed() >= self.sync_interval {
            self.last_synced = std::time::Instant::now();
            self.sync_home_and_library();
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
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            running: true,
            client: YtMusicClient::new(),
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
            busy_tasks: 0,
            signing_in: false,
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

        app.begin_task();
        assert!(app.needs_animation(), "loading shows the spinner");
        app.finish_task();

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
    fn stop_clears_the_now_playing_state_but_keeps_the_queue() {
        let mut app = App::new_for_tests();
        app.current = Some(Track::default());
        app.loading_audio = true;
        app.lyrics = crate::lyrics::LyricsState::Plain("la la".to_string());
        app.artwork_source = Some(image::DynamicImage::ImageRgb8(
            image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3])),
        ));
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
