//! Fila de reprodução, transporte (play/pause/seek), shuffle/repeat, curtidas,
//! prefetch e artwork.

use super::*;

impl App {
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
            let _ = crate::fs_util::atomic_write(&path, json.as_bytes());
        }
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
    ///
    /// `pub(super)`: também chamado por `messages::drain_messages`.
    pub(super) fn prefetch(&self, idx: usize) {
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
    ///
    /// `pub(super)`: também chamado por `messages::drain_messages` e pelos
    /// testes de `app`.
    pub(super) fn append_related(&mut self, seed: &str, tracks: Vec<Track>) -> usize {
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
    ///
    /// `pub(super)`: também chamado pelos testes de `app`.
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
    ///
    /// `pub(super)`: também chamado por `messages::drain_messages`.
    pub(super) fn is_current_track(&self, video_id: &str) -> bool {
        self.current
            .as_ref()
            .is_some_and(|t| t.video_id == video_id)
    }

    /// Clears the current album art and flags the terminal for a full clear
    /// on the next draw, so Kitty/Sixel graphics left over by the previous
    /// cover don't linger behind whatever gets drawn next.
    ///
    /// `pub(super)`: também chamado por `messages::drain_messages`.
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
    ///
    /// `pub(super)`: também chamado por `search::open_selected_home` e por
    /// `messages::drain_messages`.
    pub(super) fn start_current(&mut self) {
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
    ///
    /// `pub(super)`: também chamado por `messages::tick`.
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
}
