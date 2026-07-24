//! Driving the audio player: starting tracks, moving between them, and the
//! album art that follows the current track.
//!
//! The queue decides *what* plays (see [`super::queue`]); this module makes
//! it actually play — resolving audio, seeding lyrics/artwork fetches, and
//! handling natural track end through `advance_auto`.

use super::*;

impl App {
    /// Curte ou descurte a faixa atual (alterna com base no estado da sessão).
    pub fn like_current(&mut self) {
        let Some(track) = self.current.clone() else {
            self.status = "Nada tocando para curtir.".to_string();
            return;
        };
        if !self.provider.capabilities().likes {
            self.status = format!(
                "{} não suporta curtir faixas.",
                self.provider.display_name()
            );
            return;
        }
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
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Err(e) = provider.rate_track(&vid, like).await {
                let _ = tx.send(Msg::Error(format!("Não foi possível curtir: {e}")));
            }
        });
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
        self.ui.lyrics.reset();
        self.visualizer.reset();
        if had_track {
            self.clear_artwork();
            self.status = "⏹ Reprodução parada.".to_string();
        }
    }

    /// Avança 5s na faixa atual.
    /// Jumps playback to the start of the lyric line the user is looking
    /// at, and resumes auto-follow — the line they picked is about to
    /// become the line being sung, so browsing is over.
    ///
    /// A no-op without synced lyrics: plain text has no timestamps to seek
    /// to.
    pub fn seek_to_focused_lyric(&mut self) {
        let crate::lyrics::LyricsState::Synced { lines, active } = &self.lyrics else {
            return;
        };
        let Some(index) = self.ui.lyrics.focused_line(*active) else {
            return;
        };
        let Some(line) = lines.get(index) else {
            return;
        };
        // Seek to the *uncorrected* timestamp: the correction describes how
        // far the lyrics drift from the audio, so undoing it here lands the
        // audio where this line actually plays.
        let target_ms = (line.start_ms as i64 - self.ui.lyrics.offset_ms()).max(0) as u64;
        self.player
            .seek_to(std::time::Duration::from_millis(target_ms));
        self.ui.lyrics.follow_now();
        self.status = format!("↷ {}", line.text);
    }

    /// Nudges the lyrics timing correction and reports the new value.
    pub fn adjust_lyrics_offset(&mut self, delta_ms: i64) {
        self.ui.lyrics.adjust_offset(delta_ms);
        let offset = self.ui.lyrics.offset_ms();
        self.status = if offset == 0 {
            "Sincronia da letra: original.".to_string()
        } else {
            format!("Sincronia da letra: {:+.2}s", offset as f64 / 1000.0)
        };
        self.save_config();
    }

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

    /// Resolve o Enter da lista atual: monta a fila (retornando `true` para
    /// iniciar a reprodução) ou dispara o carregamento de artista/álbum/
    /// playlist (retornando `false`). Separado de [`Self::play_selected`]
    /// para ser testável sem um runtime tokio ativo.
    pub(super) fn prepare_selection_for_playback(&mut self) -> bool {
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
                        self.shuffle_played.clear();
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
                self.shuffle_played.clear();
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
    pub(super) fn is_current_track(&self, video_id: &str) -> bool {
        self.current
            .as_ref()
            .is_some_and(|t| t.video_id == video_id)
    }

    /// Clears the current album art and flags the terminal for a full clear
    /// on the next draw, so Kitty/Sixel graphics left over by the previous
    /// cover don't linger behind whatever gets drawn next.
    pub(super) fn clear_artwork(&mut self) {
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
    pub(super) fn start_current(&mut self) {
        let Some(idx) = self.queue_index else { return };
        let Some(track) = self.queue.get(idx).cloned() else {
            return;
        };
        self.current = Some(track.clone());
        self.ui.anim.mark_track_changed();
        if self.shuffle {
            self.shuffle_played.insert(track.video_id.clone());
        }
        self.remember_recent(&track);
        self.lyrics = crate::lyrics::LyricsState::None;
        self.ui.lyrics.reset();
        self.clear_artwork();
        self.visualizer.reset();
        self.loading_audio = true;
        self.status = format!("Baixando \"{}\"...", track.title);

        // 1) Resolução do áudio (bloqueante) em task dedicada, a cargo do
        // provedor (download/cache/remux ficam do lado de lá do contrato).
        let tx = self.tx.clone();
        let provider = Arc::clone(&self.provider);
        let provider_name = provider.display_name();
        let track_audio = track.clone();
        tokio::task::spawn_blocking(move || match provider.resolve_playable(&track_audio) {
            Ok(path) => {
                let _ = tx.send(Msg::AudioReady {
                    video_id: track_audio.video_id,
                    path,
                });
            }
            Err(e) => {
                let _ = tx.send(Msg::Error(format!(
                    "Falha ao obter áudio ({provider_name}): {e}",
                    provider_name = provider_name
                )));
            }
        });

        // Pré-calcula e pré-baixa a próxima faixa para transição mais suave.
        self.next_index = self.compute_next(idx, self.repeat != RepeatMode::Off);
        if let Some(n) = self.next_index {
            self.prefetch(n);
        }

        // 2) Letras (só quando o provedor as fornece).
        if self.provider.capabilities().lyrics {
            let provider = Arc::clone(&self.provider);
            let tx2 = self.tx.clone();
            let vid = track.video_id.clone();
            tokio::spawn(async move {
                if let Ok(lyr) = provider.lyrics(&vid).await {
                    let _ = tx2.send(Msg::Lyrics {
                        video_id: vid,
                        lyrics: lyr,
                    });
                }
            });
        }

        // 3) Capa (artwork).
        if let Some(url) = track.thumbnail.clone() {
            let tx3 = self.tx.clone();
            let provider = Arc::clone(&self.provider);
            let vid_art = track.video_id.clone();
            let cache = crate::artwork::cache_dir();
            tokio::spawn(async move {
                // Covers rarely change and are re-requested constantly —
                // every repeat, every `p`, every revisit to the same album.
                // Check the disk before the network.
                if let Some(bytes) = cache
                    .as_deref()
                    .and_then(|dir| crate::artwork::read_cached(dir, &url))
                {
                    let _ = tx3.send(Msg::ArtworkBytes {
                        video_id: vid_art,
                        bytes,
                    });
                    return;
                }
                if let Ok(bytes) = provider.fetch_artwork(&url).await {
                    if let Some(dir) = cache.as_deref() {
                        crate::artwork::write_cached(dir, &url, &bytes);
                    }
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
    pub(super) fn advance_auto(&mut self) {
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
                if self.autoplay && self.provider.capabilities().radio {
                    if let Some(seed) = self.current.as_ref().map(|t| t.video_id.clone()) {
                        if !seed.is_empty() {
                            self.status = "📻 Fila concluída — carregando rádio...".to_string();
                            let provider = Arc::clone(&self.provider);
                            let tx = self.tx.clone();
                            tokio::spawn(async move {
                                match provider.radio(&seed).await {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::app::testing::*;

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
}
