//! Moving the selection: within a list, between sections, and across the
//! sidebar.
//!
//! Every mutation clamps to the current list length, so a selection left
//! over from a longer list can never point past the end of a shorter one.

use super::*;

impl App {
    /// Número de itens na lista principal da seção atual.
    pub fn main_len(&self) -> usize {
        match self.section {
            Section::Inicio => self.home_total_count(),
            Section::Buscar if self.search_mixed => self.search_item_count(),
            Section::Buscar => self.songs.len(),
            Section::Biblioteca => self.library.len(),
            Section::Playlists => self.playlists.len(),
            Section::Artistas => self.artists.len(),
            Section::Fila => self.queue.len(),
            _ => 0,
        }
    }

    /// Move a seleção da lista principal (com wrap nas pontas).
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.main_len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.list_state.select(Some(next));
        if self.section == Section::Inicio {
            self.ui.anim.mark_selection_changed();
        }
    }

    /// Salta a seleção em `delta` itens, saturando nas pontas — para
    /// PageUp/PageDown e scroll do mouse, onde o wrap da navegação linha a
    /// linha seria desorientador.
    pub fn page_selection(&mut self, delta: isize) {
        let len = self.main_len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        self.list_state.select(Some(next));
        if self.section == Section::Inicio {
            self.ui.anim.mark_selection_changed();
        }
    }

    /// Seleciona o primeiro item da lista principal (tecla Home).
    pub fn select_first(&mut self) {
        if self.main_len() > 0 {
            self.list_state.select(Some(0));
            if self.section == Section::Inicio {
                self.ui.anim.mark_selection_changed();
            }
        }
    }

    /// Seleciona o último item da lista principal (tecla End).
    pub fn select_last(&mut self) {
        let len = self.main_len();
        if len > 0 {
            self.list_state.select(Some(len - 1));
            if self.section == Section::Inicio {
                self.ui.anim.mark_selection_changed();
            }
        }
    }

    /// Abre diretamente a seção de índice `index` (teclas 1–8), movendo o
    /// foco para o painel principal.
    pub fn jump_to_section(&mut self, index: usize) {
        if index >= Section::ALL.len() {
            return;
        }
        self.sidebar_index = index;
        self.section = Section::ALL[index];
        self.focus = Focus::Main;
        self.list_state.select(Some(0));
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::app::testing::*;

    #[test]
    fn number_keys_jump_to_sections() {
        let mut app = App::new_for_tests();
        app.jump_to_section(5);
        assert_eq!(app.section, Section::Fila);
        assert_eq!(app.sidebar_index, 5);
        assert_eq!(app.focus, Focus::Main);
        // Out of range is a no-op.
        app.jump_to_section(99);
        assert_eq!(app.section, Section::Fila);
    }
    #[test]
    fn page_selection_saturates_at_the_list_edges() {
        let mut app = App::new_for_tests();
        app.section = Section::Fila;
        app.queue = vec![track("a"), track("b"), track("c")];
        app.list_state.select(Some(1));
        app.page_selection(10);
        assert_eq!(app.list_state.selected(), Some(2), "clamps to the end");
        app.page_selection(-10);
        assert_eq!(app.list_state.selected(), Some(0), "clamps to the start");
    }
    #[test]
    fn move_selection_marks_the_change_only_in_the_home_section() {
        let mut app = App::new_for_tests();
        app.section = Section::Fila;
        app.queue = vec![track("a"), track("b")];
        app.move_selection(1);
        assert!(
            !app.ui.anim.selection_ever_changed(),
            "the queue section has no card-reveal transition to drive"
        );
    }
}
