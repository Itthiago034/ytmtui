//! The Home screen's data: recommendation shelves, the user's library, and
//! the local recently-played history.
//!
//! Note the neighbour: `crate::home` is the provider-neutral *projection*
//! (`HomeView`, `HomeCard`) that the UI navigates; this module holds the
//! `App` commands that load and move through it.

use super::*;

impl App {
    /// Carrega (em background) as playlists da biblioteca, se autenticado.
    pub fn load_library(&mut self) {
        if !self.provider.capabilities().library || !self.is_authenticated() {
            return;
        }
        self.begin_task();
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        let session_generation = self.session_generation;
        tokio::spawn(async move {
            match provider.library_playlists().await {
                Ok(pls) => {
                    let _ = tx.send(Msg::LibraryPlaylistsForSession {
                        session_generation,
                        playlists: pls,
                    });
                }
                Err(ProviderError::SessionExpired) => {
                    let _ = tx.send(Msg::SessionExpiredForSession { session_generation });
                }
                Err(error) => {
                    let _ = tx.send(Msg::Error(format!("Could not load library: {error}")));
                }
            }
        });
    }

    /// Carrega (em background) as recomendações da tela inicial.
    pub fn load_home(&mut self) {
        if !self.provider.capabilities().home {
            return;
        }
        // A new attempt supersedes whatever error the last one left behind:
        // the loading state itself is the feedback while it's in flight.
        self.home_error = None;
        self.begin_task();
        let provider = Arc::clone(&self.provider);
        let tx = self.tx.clone();
        let session_generation = self.session_generation;
        tokio::spawn(async move {
            match provider.home().await {
                Ok(sections) => {
                    let _ = tx.send(Msg::HomeSectionsForSession {
                        session_generation,
                        sections,
                    });
                }
                Err(ProviderError::SessionExpired) => {
                    let _ = tx.send(Msg::SessionExpiredForSession { session_generation });
                }
                Err(error) => {
                    let _ = tx.send(Msg::HomeFailedForSession {
                        session_generation,
                        message: format!("Could not load recommendations: {error}"),
                    });
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
    pub fn home_view(&self) -> HomeView {
        HomeView::project(self.provider.id(), &self.recent, &self.home)
    }

    pub fn home_total_count(&self) -> usize {
        self.home_view().len()
    }

    pub fn move_home(&mut self, direction: HomeDirection) {
        let current = self.list_state.selected().unwrap_or(0);
        let next = self
            .home_view()
            .move_index(current, direction, self.home_columns);
        self.list_state
            .select((self.home_total_count() > 0).then_some(next));
        self.mark_selection_changed();
    }

    /// Registra uma faixa no histórico local (topo da lista, sem duplicatas,
    /// limitado a [`RECENT_CAP`]) e persiste em `recent.json`. Persistência é
    /// melhor-esforço: falhas de disco nunca interrompem a reprodução.
    pub(super) fn remember_recent(&mut self, track: &Track) {
        self.recent.retain(|t| t.video_id != track.video_id);
        self.recent.insert(0, track.clone());
        self.recent.truncate(RECENT_CAP);
        // Persistência só quando o app foi construído a partir do disco
        // (`App::new`). `with_provider` (testes/injeção) fica em memória —
        // um teste que toca uma faixa não pode escrever no recent.json
        // real do usuário.
        if !self.persist_recent {
            return;
        }
        let Some(path) = recent_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.recent) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Abre o item selecionado na tela inicial: faixas do histórico recente
    /// tocam na hora (a fila vira o próprio histórico); recomendações abrem
    /// como playlist.
    pub fn open_selected_home(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let Some(card) = self.home_view().flat_card(idx).cloned() else {
            return;
        };
        match card.payload {
            HomeCardPayload::Track(track) => {
                let recent_index = self
                    .recent
                    .iter()
                    .position(|candidate| candidate.video_id == track.video_id)
                    .unwrap_or(0);
                self.queue = self.recent.clone();
                self.queue_index = Some(recent_index);
                self.shuffle_played.clear();
                self.start_current();
            }
            HomeCardPayload::Collection(collection) => self.load_playlist(collection),
        }
    }
}
