//! Async task bookkeeping and the periodic tick.
//!
//! Everything that crosses the boundary between background work and app
//! state lives here: the in-flight task counter behind the spinner, the
//! `Msg` drain that applies completed work, MPRIS media-key events, and the
//! per-frame `tick` that advances playback progress, synced lyrics, queue
//! auto-advance, and background Home/Library sync.

use super::*;

impl App {
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
    pub(super) fn finish_task(&mut self) {
        self.busy_tasks = self.busy_tasks.saturating_sub(1);
    }

    /// Há alguma tarefa de carregamento em andamento (rede ou áudio)?
    pub fn is_loading(&self) -> bool {
        self.busy() || self.loading_audio
    }

    /// Glifo atual do spinner de carregamento (braille animado).
    pub fn spinner(&self) -> char {
        const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        FRAMES[self.spinner_frame % FRAMES.len()]
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
                    // Results always land on the section that can show them,
                    // even if the user navigated elsewhere while the request
                    // was in flight. `sidebar_index` must move with it — a
                    // later `j`/`k` computes its next index from that base,
                    // so leaving it behind jumps to an unrelated section
                    // (same reasoning as the `/` handler in `event.rs`).
                    self.set_section(Section::Buscar);
                    self.sidebar_index = Section::Buscar.index();
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
                Msg::LibraryPlaylistsForSession {
                    session_generation,
                    playlists,
                } => {
                    if session_generation == self.session_generation {
                        let _ = self.tx.send(Msg::LibraryPlaylists(playlists));
                    } else {
                        self.finish_task();
                    }
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
                Msg::HomeSectionsForSession {
                    session_generation,
                    sections,
                } => {
                    if session_generation == self.session_generation {
                        let _ = self.tx.send(Msg::HomeSections(sections));
                    } else {
                        self.finish_task();
                    }
                }
                Msg::HomeSections(sections) => {
                    self.finish_task();
                    self.home_error = None;
                    let was_empty = self.home.is_empty();
                    let previous_key = (self.section == Section::Inicio)
                        .then(|| self.list_state.selected())
                        .flatten()
                        .and_then(|i| self.home_view().flat_card(i).map(|card| card.key.clone()));
                    self.home = sections;
                    if self.section == Section::Inicio {
                        let view = self.home_view();
                        let count = view.len();
                        let new_index = previous_key
                            .and_then(|key| view.flat_index_of(&key))
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
                Msg::HomeFailedForSession {
                    session_generation,
                    message,
                } => {
                    if session_generation == self.session_generation {
                        let _ = self.tx.send(Msg::HomeFailed(message));
                    } else {
                        self.finish_task();
                    }
                }
                Msg::HomeFailed(message) => {
                    self.finish_task();
                    self.home_error = Some(message);
                    // `self.home`/`self.recent` are deliberately left alone:
                    // whatever shelves were already cached stay on screen,
                    // per the empty-state/banner split in `draw_home_sections`.
                    self.status = "⚠ Falha ao carregar recomendações — R recarrega.".to_string();
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
                Msg::AccountNameForSession {
                    session_generation,
                    name,
                } => {
                    if session_generation == self.session_generation {
                        let _ = self.tx.send(Msg::AccountName(name));
                    } else {
                        self.finish_task();
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
                Msg::SessionExpiredForSession { session_generation } => {
                    if session_generation == self.session_generation {
                        let _ = self.tx.send(Msg::SessionExpired);
                    } else {
                        self.finish_task();
                    }
                }
                Msg::SessionExpired => {
                    self.finish_task();
                    self.authentication = AuthState::Expired;
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
                    self.set_section(Section::Buscar);
                    self.sidebar_index = 0;
                    self.list_state.select(Some(0));
                    self.status = format!("{} faixas carregadas.", self.songs.len());
                }
                Msg::Lyrics { video_id, lyrics } => {
                    // A slow fetch for a track the user has since skipped
                    // past must not overwrite the current track's lyrics.
                    if self.is_current_track(&video_id) {
                        use crate::lyrics::LyricsState;
                        use crate::models::Lyrics;
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
                Msg::SignInPrepared {
                    operation_id,
                    preview,
                } => self.handle_sign_in_prepared(operation_id, preview),
                Msg::SignedIn {
                    operation_id,
                    preview_id,
                    method,
                    credentials_path,
                    account_name,
                } => self.handle_signed_in(
                    operation_id,
                    preview_id,
                    method,
                    credentials_path,
                    account_name,
                ),
                Msg::SignInFailed {
                    operation_id,
                    message,
                    preview_id,
                } => self.handle_sign_in_failed(operation_id, message, preview_id),
                Msg::RelatedTracks { seed, tracks } => {
                    let added = self.append_related(&seed, tracks);
                    if added > 0 {
                        self.status = format!("📻 +{added} músicas semelhantes na fila.");
                        if let Some(n) = self.next_index {
                            self.prefetch(n);
                        }
                    }
                }
                Msg::Media(event) => self.handle_media_event(event),
                Msg::Status(s) => self.status = s,
                Msg::Error(e) => {
                    self.loading_audio = false;
                    self.finish_task();
                    self.status = format!("⚠ {e}");
                }
            }
        }
    }

    /// Aplica um comando de mídia vindo do desktop (MPRIS): os mesmos
    /// caminhos dos atalhos de teclado, então o comportamento é idêntico.
    fn handle_media_event(&mut self, event: souvlaki::MediaControlEvent) {
        use souvlaki::{MediaControlEvent as E, SeekDirection};
        match event {
            E::Play => {
                if self.player.is_paused() {
                    self.player.toggle_pause();
                }
            }
            E::Pause => {
                if !self.player.is_paused() {
                    self.player.toggle_pause();
                }
            }
            E::Toggle => self.player.toggle_pause(),
            E::Next => self.next_track(),
            E::Previous => self.prev_track(),
            E::Stop => self.stop_playback(),
            E::Seek(SeekDirection::Forward) => self.seek_forward(),
            E::Seek(SeekDirection::Backward) => self.seek_backward(),
            E::SeekBy(direction, amount) => {
                let secs = amount.as_secs().max(1);
                if self.current.is_some() {
                    match direction {
                        SeekDirection::Forward => self.player.seek_forward(secs),
                        SeekDirection::Backward => self.player.seek_backward(secs),
                    }
                }
            }
            E::SetPosition(souvlaki::MediaPosition(position)) => {
                if self.current.is_some() {
                    self.player.seek_to(position);
                }
            }
            E::SetVolume(volume) => {
                self.player.set_volume(volume.clamp(0.0, 1.0) as f32);
            }
            E::Quit => self.request_quit(),
            // Uma TUI não tem janela própria para trazer à frente, e abrir
            // URIs externas não faz sentido aqui.
            E::Raise | E::OpenUri(_) => {}
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
                // Todos os chunks acumulados entram na janela, mas a FFT
                // roda uma única vez por tick: só o frame final é desenhado.
                let mut fed = false;
                for chunk in self.player.drain_sample_chunks() {
                    self.visualizer.push_samples(&chunk);
                    fed = true;
                }
                if fed {
                    self.visualizer.compute_frame();
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
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::app::testing::*;

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
        let message = client_error_message("Could not load library", ProviderError::SessionExpired);
        assert!(matches!(message, Msg::SessionExpired));
    }
}
