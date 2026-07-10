//! Integração MPRIS (`org.mpris.MediaPlayer2.ytmtui`).
//!
//! Registra o player no D-Bus da sessão para que o widget de mídia do
//! desktop (Plasma, GNOME), o `playerctl` e as teclas multimídia do teclado
//! controlem a reprodução e exibam título/artista/capa.
//!
//! Os comandos chegam pelo callback da `souvlaki` (em uma thread própria) e
//! são reenviados como [`Msg::Media`] pelo canal já existente, então toda a
//! mutação de estado continua acontecendo no loop principal. Na direção
//! oposta, [`Mpris::sync`] espelha o estado do app no D-Bus emitindo apenas
//! diferenças, para não inundar o barramento com sinais idênticos.

use std::time::{Duration, Instant};

use souvlaki::{MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::{App, Msg};

/// Com que frequência a posição de reprodução é reenviada enquanto toca.
/// O applet de mídia interpola entre leituras, então 1s é suficiente para a
/// barra de progresso dele acompanhar sem encher o D-Bus de sinais.
const POSITION_REFRESH: Duration = Duration::from_secs(1);

/// Estado (status + posição) já publicado, para emitir apenas diffs.
#[derive(PartialEq, Clone, Copy)]
enum PublishedPlayback {
    Stopped,
    Paused,
    Playing,
}

pub struct Mpris {
    controls: MediaControls,
    /// `video_id` da última faixa cujos metadados foram publicados.
    last_track: Option<String>,
    last_playback: PublishedPlayback,
    last_volume: f32,
    last_position_push: Instant,
}

impl Mpris {
    /// Registra o player no D-Bus da sessão. Retorna `None` em ambientes sem
    /// D-Bus (TTY puro, containers) — o app segue funcionando sem MPRIS.
    pub fn new(tx: UnboundedSender<Msg>) -> Option<Self> {
        let mut controls = MediaControls::new(PlatformConfig {
            display_name: "ytmtui",
            dbus_name: "ytmtui",
            hwnd: None,
        })
        .ok()?;
        controls
            .attach(move |event| {
                let _ = tx.send(Msg::Media(event));
            })
            .ok()?;
        Some(Self {
            controls,
            last_track: None,
            last_playback: PublishedPlayback::Stopped,
            last_volume: -1.0,
            last_position_push: Instant::now(),
        })
    }

    /// Espelha o estado atual do app no D-Bus. Chamado a cada volta do loop
    /// principal; barato quando nada mudou (só comparações locais).
    pub fn sync(&mut self, app: &App) {
        self.sync_metadata(app);
        self.sync_playback(app);
        self.sync_volume(app);
    }

    fn sync_metadata(&mut self, app: &App) {
        let track_id = app.current.as_ref().map(|t| t.video_id.clone());
        if track_id == self.last_track {
            return;
        }
        let metadata = match &app.current {
            Some(t) => MediaMetadata {
                title: Some(&t.title),
                artist: Some(&t.artist),
                album: (!t.album.is_empty()).then_some(t.album.as_str()),
                // A miniatura é uma URL https; applets (Plasma incluso) a
                // baixam sozinhos para exibir a capa.
                cover_url: t.thumbnail.as_deref(),
                duration: (t.duration_secs > 0).then(|| Duration::from_secs(t.duration_secs)),
            },
            None => MediaMetadata::default(),
        };
        let _ = self.controls.set_metadata(metadata);
        self.last_track = track_id;
    }

    fn sync_playback(&mut self, app: &App) {
        let state = if app.current.is_none() {
            PublishedPlayback::Stopped
        } else if app.player.is_paused() {
            PublishedPlayback::Paused
        } else {
            PublishedPlayback::Playing
        };
        // Reenvia enquanto toca para a posição não ficar estagnada no
        // applet; em pausa/parado ela não anda, então só o diff interessa.
        let refresh_position = state == PublishedPlayback::Playing
            && self.last_position_push.elapsed() >= POSITION_REFRESH;
        if state == self.last_playback && !refresh_position {
            return;
        }
        let progress = Some(MediaPosition(app.player.position()));
        let playback = match state {
            PublishedPlayback::Stopped => MediaPlayback::Stopped,
            PublishedPlayback::Paused => MediaPlayback::Paused { progress },
            PublishedPlayback::Playing => MediaPlayback::Playing { progress },
        };
        let _ = self.controls.set_playback(playback);
        self.last_playback = state;
        self.last_position_push = Instant::now();
    }

    fn sync_volume(&mut self, app: &App) {
        let volume = app.player.volume();
        if (volume - self.last_volume).abs() < f32::EPSILON {
            return;
        }
        let _ = self.controls.set_volume(volume as f64);
        self.last_volume = volume;
    }
}
