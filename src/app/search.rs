//! Busca, navegação de Início/Artistas/Playlists/Biblioteca e sincronização
//! em segundo plano.

use super::*;

impl App {
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
    ///
    /// `pub(super)`: também chamado por `playback::prepare_selection_for_playback`.
    pub(super) fn load_artist(&mut self, artist: Artist) {
        if artist.browse_id.is_empty() {
            self.status = "Artista sem página disponível.".to_string();
            return;
        }
        self.status = format!("Carregando artista \"{}\"...", artist.name);
        self.busy = true;
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
    ///
    /// `pub(super)`: também chamado por `playback::prepare_selection_for_playback`.
    pub(super) fn load_playlist(&mut self, pl: Playlist) {
        self.load_browse(pl, "Playlist");
    }

    /// Dispara o carregamento das faixas de uma playlist ou álbum; `kind`
    /// rotula o painel de resultados ("Playlist"/"Album").
    ///
    /// `pub(super)`: também chamado por `playback::prepare_selection_for_playback`.
    pub(super) fn load_browse(&mut self, pl: Playlist, kind: &str) {
        self.status = format!("Carregando \"{}\"...", pl.title);
        self.busy = true;
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
}
