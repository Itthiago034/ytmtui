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
            .move_index(current, direction, self.ui.home_columns);
        self.list_state
            .select((self.home_total_count() > 0).then_some(next));
        self.ui.anim.mark_selection_changed();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::app::testing::*;

    #[test]
    fn background_home_refresh_preserves_selection_by_browse_id() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.home = vec![crate::models::HomeSection {
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
            .send(Msg::HomeSections(vec![crate::models::HomeSection {
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
        app.home = vec![crate::models::HomeSection {
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
            .send(Msg::HomeSections(vec![crate::models::HomeSection {
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
            .send(Msg::HomeSections(vec![crate::models::HomeSection {
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
    #[tokio::test]
    async fn entering_a_recent_home_card_preserves_history_order_and_selected_index() {
        let mut app = App::new_for_tests();
        app.section = Section::Inicio;
        app.recent = (1..=3)
            .map(|i| Track {
                video_id: format!("r{i}"),
                title: format!("Recent {i}"),
                ..Default::default()
            })
            .collect();
        app.list_state.select(Some(1));

        app.open_selected_home();

        assert_eq!(
            app.queue
                .iter()
                .map(|track| track.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["r1", "r2", "r3"]
        );
        assert_eq!(app.queue_index, Some(1));
        assert_eq!(
            app.current.as_ref().map(|track| track.video_id.as_str()),
            Some("r2")
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
    fn home_failed_preserves_cached_shelves_and_clears_the_spinner() {
        let mut app = App::new_for_tests();
        app.home = home_sections();
        app.begin_task();

        app.tx.send(Msg::HomeFailed("boom".to_string())).unwrap();
        app.drain_messages();

        assert_eq!(
            app.home.len(),
            2,
            "cached shelves survive a failed background refresh"
        );
        assert_eq!(app.home_error.as_deref(), Some("boom"));
        assert!(!app.is_loading(), "the spinner is released on failure");
        assert!(
            app.status.contains('R'),
            "status hints at the retry key: {}",
            app.status
        );
    }
    #[test]
    fn home_sections_success_clears_a_previous_error() {
        let mut app = App::new_for_tests();
        app.home_error = Some("boom".to_string());

        app.tx.send(Msg::HomeSections(Vec::new())).unwrap();
        app.drain_messages();

        assert!(
            app.home_error.is_none(),
            "a successful load clears the stale error"
        );
    }
    #[test]
    fn load_home_without_the_home_capability_creates_no_task() {
        let mut mock = crate::provider::mock::MockProvider::default();
        mock.capabilities.home = false;
        let mut app = App::with_provider(std::sync::Arc::new(mock));

        app.load_home();

        assert!(
            !app.is_loading(),
            "no capability means no task, hence no spinner"
        );
    }
}
