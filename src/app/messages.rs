//! Processamento das mensagens vindas das tasks assíncronas e o tick
//! periódico do loop principal.

use super::*;

impl App {
    /// Processa mensagens recebidas das tasks assíncronas.
    pub fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::SearchResults(res) => {
                    self.busy = false;
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
                    self.busy = false;
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
                    self.status = format!(
                        "Library loaded: {} playlist(s). Open Library in the menu.",
                        self.library.len()
                    );
                }
                Msg::HomeSections(sections) => {
                    self.busy = false;
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
                    self.status = "Session expired. Press g to sign in again from your \
                                   browser (music.youtube.com must be signed in there)."
                        .to_string();
                }
                Msg::PlaylistTracks { title, tracks } => {
                    self.busy = false;
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
                Msg::CookiesImported {
                    path,
                    browser,
                    warning,
                } => {
                    self.busy = false;
                    match YtMusicClient::with_cookies(&path) {
                        Ok(client) => {
                            self.client = client;
                            self.cookies = Some(path);
                            self.authentication = AuthenticationState::Authenticated;
                            self.account_name = None;
                            let name = browser.split(':').next().unwrap_or(&browser);
                            self.status = match warning {
                                Some(w) => format!(
                                    "✔ Conectado via {name} (⚠ {w}). Carregando suas músicas…"
                                ),
                                None => {
                                    format!("✔ Conectado via {name}. Carregando suas músicas…")
                                }
                            };
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
