//! Estado central da aplicação e lógica de coordenação.

mod authentication;

use authentication::resolve_cookie_path;
pub use authentication::AuthenticationState;

use std::path::PathBuf;

use ratatui::text::Line;
use ratatui::widgets::ListState;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::config::Config;
use crate::player::{self, AudioPlayer};
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

    /// Rótulo exibido na barra lateral.
    pub fn label(&self) -> &str {
        match self {
            Section::Inicio => "🏠 Início",
            Section::Buscar => "🔎 Buscar",
            Section::Biblioteca => "📚 Biblioteca",
            Section::Playlists => "💿 Playlists",
            Section::Artistas => "🎤 Artistas",
            Section::Fila => "🎶 Fila Atual",
            Section::Letra => "📖 Letras",
            Section::Ajuda => "💡 Ajuda",
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
    HomePlaylists(Vec<Playlist>),
    RadioTracks(Vec<Track>),
    AccountName(Option<String>),
    PlaylistTracks {
        title: String,
        tracks: Vec<Track>,
    },
    Lyrics(Option<String>),
    ArtworkBytes(Vec<u8>),
    AudioReady(PathBuf),
    Status(String),
    Error(String),
    /// Cookies are present, but the API session is no longer valid.
    SessionExpired,
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
    /// Playlists da biblioteca do usuário logado.
    pub library: Vec<Playlist>,
    /// Recomendações da tela inicial (playlists/álbuns).
    pub home: Vec<Playlist>,
    /// videoIds curtidos nesta sessão (para alternar curtir/descurtir).
    pub liked: std::collections::HashSet<String>,
    /// Autoplay: continuar com uma rádio quando a fila termina.
    pub autoplay: bool,
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
    pub lyrics: Option<String>,
    pub lyrics_scroll: u16,
    pub artwork_bytes: Option<Vec<u8>>,
    /// Cache da arte já convertida: (largura, altura, linhas).
    pub artwork_cache: Option<(u16, u16, Vec<Line<'static>>)>,

    pub status: String,
    /// Caminho opcional para arquivo de cookies do yt-dlp.
    pub cookies: Option<String>,
    /// Um download de áudio está em andamento.
    pub loading_audio: bool,
    /// Uma tarefa de carregamento (busca/playlist/artista/biblioteca) está ativa.
    pub busy: bool,
    /// Quadro atual do spinner de carregamento (avança a cada tick).
    pub spinner_frame: usize,
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
                "Cookie file is invalid. Refresh it with ./scripts/refresh-cookies.sh.".to_string()
            }
            AuthenticationState::Anonymous => match resolution.missing_requested_path.as_deref() {
                Some(path) => format!("Configured cookie file does not exist: {path}"),
                None => "Welcome to ytmtui. Press / to search or ? for help.".to_string(),
            },
            AuthenticationState::Expired => {
                "Session expired. Refresh browser cookies and restart ytmtui.".to_string()
            }
        };

        Ok(Self {
            running: true,
            client,
            player,
            tx,
            rx,
            focus: Focus::Sidebar,
            section: Section::Inicio,
            sidebar_index: 0,
            input_mode: false,
            query: String::new(),
            songs: Vec::new(),
            songs_title: "Resultados da busca".to_string(),
            playlists: Vec::new(),
            artists: Vec::new(),
            library: Vec::new(),
            home: Vec::new(),
            liked: std::collections::HashSet::new(),
            autoplay: true,
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
            lyrics: None,
            lyrics_scroll: 0,
            artwork_bytes: None,
            artwork_cache: None,
            status,
            cookies,
            loading_audio: false,
            busy: false,
            spinner_frame: 0,
        })
    }

    pub fn is_authenticated(&self) -> bool {
        self.authentication.is_authenticated()
    }

    /// Há alguma tarefa de carregamento em andamento (rede ou áudio)?
    pub fn is_loading(&self) -> bool {
        self.busy || self.loading_audio
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
        self.busy = true;
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
        self.busy = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(pls) = client.get_home().await {
                let _ = tx.send(Msg::HomePlaylists(pls));
            }
        });
    }

    /// Abre a recomendação selecionada na tela inicial.
    pub fn open_selected_home(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let Some(pl) = self.home.get(idx).cloned() else {
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
        if artist.browse_id.is_empty() {
            self.status = "Artista sem página disponível.".to_string();
            return;
        }
        self.status = format!("Carregando artista \"{}\"...", artist.name);
        self.busy = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let title = format!("Artista: {}", artist.name);
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
        self.artwork_cache = None; // recolore o placeholder na próxima renderização
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
            Section::Inicio => self.home.len(),
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
        self.busy = true;
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
        self.status = format!("Carregando playlist \"{}\"...", pl.title);
        self.busy = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let title = pl.title.clone();
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
        match self.section {
            Section::Buscar => {
                if self.songs.is_empty() {
                    return;
                }
                let idx = self.list_state.selected().unwrap_or(0);
                self.queue = self.songs.clone();
                self.queue_index = Some(idx);
            }
            Section::Fila => {
                if self.queue.is_empty() {
                    return;
                }
                let idx = self.list_state.selected().unwrap_or(0);
                self.queue_index = Some(idx);
            }
            _ => return,
        }
        self.start_current();
    }

    /// Inicia a reprodução da faixa apontada por `queue_index`.
    fn start_current(&mut self) {
        let Some(idx) = self.queue_index else { return };
        let Some(track) = self.queue.get(idx).cloned() else {
            return;
        };
        self.current = Some(track.clone());
        self.lyrics = None;
        self.lyrics_scroll = 0;
        self.artwork_bytes = None;
        self.artwork_cache = None;
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
                    let _ = tx.send(Msg::AudioReady(path));
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
                let _ = tx2.send(Msg::Lyrics(lyr));
            }
        });

        // 3) Capa (artwork).
        if let Some(url) = track.thumbnail.clone() {
            let tx3 = self.tx.clone();
            let http = self.client.clone();
            tokio::spawn(async move {
                if let Ok(bytes) = http.fetch_bytes(&url).await {
                    let _ = tx3.send(Msg::ArtworkBytes(bytes));
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
                                if let Ok(tracks) = client.get_radio(&seed).await {
                                    let _ = tx.send(Msg::RadioTracks(tracks));
                                }
                            });
                            return;
                        }
                    }
                }
                // Sem autoplay/semente: encerra a reprodução.
                self.player.stop();
                self.current = None;
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
                    self.busy = false;
                    self.songs = res.songs;
                    self.songs_title = "Resultados da busca".to_string();
                    self.playlists = res.playlists;
                    self.artists = res.artists;
                    self.status = format!(
                        "{} músicas, {} playlists, {} artistas encontrados.",
                        self.songs.len(),
                        self.playlists.len(),
                        self.artists.len()
                    );
                    if self.section == Section::Buscar {
                        self.list_state.select(Some(0));
                    }
                }
                Msg::LibraryPlaylists(pls) => {
                    self.busy = false;
                    self.library = pls;
                    self.status = format!(
                        "📚 Biblioteca: {} playlist(s). Acesse '📚 Biblioteca' no menu.",
                        self.library.len()
                    );
                }
                Msg::HomePlaylists(pls) => {
                    self.busy = false;
                    self.home = pls;
                    if self.section == Section::Inicio {
                        self.list_state.select(Some(0));
                    }
                }
                Msg::RadioTracks(tracks) => {
                    if tracks.is_empty() {
                        self.player.stop();
                        self.current = None;
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
                    if let Some(n) = name {
                        if self.account_name.is_none() {
                            self.account_name = Some(n);
                        }
                    }
                }
                Msg::SessionExpired => {
                    self.busy = false;
                    self.authentication = AuthenticationState::Expired;
                    self.library.clear();
                    self.account_name = None;
                    self.status = "Session expired. Run ./scripts/refresh-cookies.sh with \
                                   music.youtube.com signed in, then restart ytmtui."
                        .to_string();
                }
                Msg::PlaylistTracks { title, tracks } => {
                    self.busy = false;
                    self.songs = tracks;
                    self.songs_title = format!("Playlist: {title}");
                    self.section = Section::Buscar;
                    self.sidebar_index = 0;
                    self.list_state.select(Some(0));
                    self.status = format!("{} faixas carregadas.", self.songs.len());
                }
                Msg::Lyrics(lyr) => {
                    self.lyrics = lyr;
                }
                Msg::ArtworkBytes(bytes) => {
                    self.artwork_bytes = Some(bytes);
                    self.artwork_cache = None;
                }
                Msg::AudioReady(path) => {
                    self.loading_audio = false;
                    if let Some(t) = &self.current {
                        self.status = format!("▶ Tocando: {} — {}", t.title, t.artist);
                    }
                    self.player.play_file(path);
                }
                Msg::Status(s) => self.status = s,
                Msg::Error(e) => {
                    self.loading_audio = false;
                    self.busy = false;
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ytmusic::YtMusicError;
    use reqwest::StatusCode;

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
