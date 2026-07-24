//! Search, and opening the things search returns.
//!
//! Mixed results (songs, artists, albums, playlists) share one flat
//! selectable list. The flattened index order here is the contract with
//! `ui::main_panel::draw_search_mixed`, which counts only real rows — never
//! the group headers it draws between them.

use super::*;

impl App {
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
    pub(super) fn load_artist(&mut self, artist: Artist) {
        if artist.browse_id.is_empty() {
            self.status = "Artista sem página disponível.".to_string();
            return;
        }
        self.status = format!("Carregando artista \"{}\"...", artist.name);
        self.begin_task();
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        let title = format!("Artist: {}", artist.name);
        tokio::spawn(async move {
            match provider.artist_tracks(&artist.browse_id).await {
                Ok(tracks) => {
                    let _ = tx.send(Msg::PlaylistTracks { title, tracks });
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("Erro ao abrir artista: {e}")));
                }
            }
        });
    }

    /// Dispara uma busca assíncrona com a query atual.
    pub fn do_search(&mut self) {
        let q = self.query.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.status = format!("Buscando por \"{q}\"...");
        self.begin_task();
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match provider.search(&q).await {
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
    pub(super) fn load_playlist(&mut self, pl: Playlist) {
        self.load_browse(pl, "Playlist");
    }

    /// Dispara o carregamento das faixas de uma playlist ou álbum; `kind`
    /// rotula o painel de resultados ("Playlist"/"Album").
    pub(super) fn load_browse(&mut self, pl: Playlist, kind: &str) {
        self.status = format!("Carregando \"{}\"...", pl.title);
        self.begin_task();
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        let title = format!("{kind}: {}", pl.title);
        tokio::spawn(async move {
            match provider.playlist_tracks(&pl.browse_id).await {
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
        if !self.provider.capabilities().radio {
            return;
        }
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(tracks) = provider.radio(&seed).await {
                let _ = tx.send(Msg::RelatedTracks { seed, tracks });
            }
        });
    }

    /// Anexa as faixas semelhantes ao fim da fila, sem duplicar o que já
    /// está nela e só enquanto a `seed` ainda é a faixa atual (resultados
    /// atrasados de uma faixa já pulada são descartados). Retorna quantas
    /// entraram. Separado do handler para ser testável sem runtime.
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
            self.recompute_next();
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::app::testing::*;

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
}
