//! Tratamento de eventos de teclado.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Focus, Section};
use crate::home::HomeDirection;

/// Processa uma tecla pressionada, atualizando o estado da aplicação.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    // ------- Modo de digitação da busca -------
    if app.input_mode {
        match key.code {
            KeyCode::Enter => {
                app.input_mode = false;
                app.focus = Focus::Main;
                app.do_search();
            }
            KeyCode::Esc => {
                app.input_mode = false;
                app.query.clear();
            }
            KeyCode::Backspace => {
                app.query.pop();
            }
            KeyCode::Char(c) => {
                app.query.push(c);
            }
            _ => {}
        }
        return;
    }

    // ------- Atalhos globais -------
    match key.code {
        KeyCode::Char('q') => app.running = false,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.running = false;
        }
        KeyCode::Char('/') => {
            // Entra no modo de busca.
            app.input_mode = true;
            app.query.clear();
            app.section = Section::Buscar;
            // Must match Buscar's real position, or a later sidebar move
            // (k/j) computes its next index from the wrong base and jumps
            // to an unrelated section instead of the adjacent one.
            app.sidebar_index = Section::Buscar.index();
            app.focus = Focus::Main;
            // Otherwise a selection left over from whatever list was open
            // before (e.g. index 4 of a 5-item queue) stays applied to the
            // new, possibly shorter, search results list.
            app.list_state.select(Some(0));
        }
        KeyCode::Char('?') => {
            app.section = Section::Ajuda;
            app.sidebar_index = Section::Ajuda.index();
        }
        // Controles de reprodução (globais).
        KeyCode::Char(' ') => app.player.toggle_pause(),
        KeyCode::Char('n') => app.next_track(),
        KeyCode::Char('p') => app.prev_track(),
        KeyCode::Char('s') => app.stop_playback(),
        KeyCode::Char('+') | KeyCode::Char('=') => app.player.volume_up(),
        KeyCode::Char('-') | KeyCode::Char('_') => app.player.volume_down(),
        // Seek dentro da faixa.
        KeyCode::Char('[') => app.seek_backward(),
        KeyCode::Char(']') => app.seek_forward(),
        // Modos de reprodução.
        KeyCode::Char('z') => app.toggle_shuffle(),
        KeyCode::Char('r') => app.cycle_repeat(),
        // Recarrega Home + Biblioteca manualmente (mesmo caminho do sync de
        // fundo periódico) — sobretudo o jeito de sair do banner de erro da
        // Home sem esperar o próximo ciclo automático.
        KeyCode::Char('R') => {
            app.sync_home_and_library();
            app.status = "Atualizando recomendações e biblioteca…".to_string();
        }
        // Adiciona a faixa selecionada à fila.
        KeyCode::Char('a') => app.enqueue_selected(),
        // Gerência da fila (apenas com a seção Fila aberta e em foco).
        KeyCode::Char('d') | KeyCode::Delete
            if app.section == Section::Fila && app.focus == Focus::Main =>
        {
            app.queue_remove_selected()
        }
        KeyCode::Char('J') if app.section == Section::Fila && app.focus == Focus::Main => {
            app.queue_move_selected(1)
        }
        KeyCode::Char('K') if app.section == Section::Fila && app.focus == Focus::Main => {
            app.queue_move_selected(-1)
        }
        KeyCode::Char('c') if app.section == Section::Fila && app.focus == Focus::Main => {
            app.queue_clear()
        }
        // Salto direto de seção: 1 = Início … 8 = Ajuda.
        KeyCode::Char(c @ '1'..='8') => {
            app.jump_to_section(c as usize - '1' as usize);
        }
        // Curte / descurte a faixa atual.
        KeyCode::Char('f') => app.like_current(),
        // Alterna o tema de cores.
        KeyCode::Char('t') => app.cycle_theme(),
        // Conecta a conta importando cookies do navegador (ou renova a sessão).
        KeyCode::Char('g') => app.sign_in(),

        // Alterna o foco entre a barra lateral e o painel principal.
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::Sidebar => Focus::Main,
                Focus::Main => Focus::Sidebar,
            };
        }
        KeyCode::Left | KeyCode::Char('h')
            if app.section == Section::Inicio && app.focus == Focus::Main =>
        {
            app.move_home(HomeDirection::Left)
        }
        KeyCode::Right | KeyCode::Char('l')
            if app.section == Section::Inicio && app.focus == Focus::Main =>
        {
            app.move_home(HomeDirection::Right)
        }
        KeyCode::Left | KeyCode::Char('h') => app.focus = Focus::Sidebar,
        KeyCode::Right | KeyCode::Char('l') => app.focus = Focus::Main,

        // Navegação vertical (depende do foco).
        KeyCode::Down | KeyCode::Char('j') => navigate(app, 1),
        KeyCode::Up | KeyCode::Char('k') => navigate(app, -1),

        // Saltos maiores na lista principal (sem wrap: paginar através da
        // "costura" fim→início desorienta).
        KeyCode::PageDown => page(app, PAGE_JUMP),
        KeyCode::PageUp => page(app, -PAGE_JUMP),
        KeyCode::Home => {
            if app.focus == Focus::Main {
                app.select_first();
            }
        }
        KeyCode::End => {
            if app.focus == Focus::Main {
                app.select_last();
            }
        }

        // Ação principal.
        KeyCode::Enter => activate(app),
        _ => {}
    }
}

/// Quantos itens PageUp/PageDown saltam por vez.
const PAGE_JUMP: isize = 10;

/// Rolagem do mouse: mesma semântica de um salto pequeno e saturado na
/// lista principal (ou na letra), independentemente do foco — a roda age
/// sobre o conteúdo, não sobre a barra lateral.
pub fn handle_scroll(app: &mut App, delta: isize) {
    match app.section {
        Section::Letra => scroll_lyrics(app, delta),
        Section::Ajuda => scroll_help(app, delta),
        _ => app.page_selection(delta),
    }
}

/// Salto de página no componente com foco; em Letra/Ajuda, rola o texto.
fn page(app: &mut App, delta: isize) {
    match app.focus {
        Focus::Sidebar => {}
        Focus::Main => match app.section {
            Section::Letra => scroll_lyrics(app, delta),
            Section::Ajuda => scroll_help(app, delta),
            _ => app.page_selection(delta),
        },
    }
}

/// Rola a tela de Ajuda (o limite inferior é clampado na renderização, que
/// conhece a altura real do painel).
fn scroll_help(app: &mut App, delta: isize) {
    if delta > 0 {
        app.help_scroll = app.help_scroll.saturating_add(delta as u16);
    } else {
        app.help_scroll = app.help_scroll.saturating_sub((-delta) as u16);
    }
}

/// Rola a letra em texto plano; letras sincronizadas seguem a reprodução e
/// ignoram rolagem manual.
fn scroll_lyrics(app: &mut App, delta: isize) {
    if matches!(app.lyrics, crate::lyrics::LyricsState::Plain(_)) {
        if delta > 0 {
            app.lyrics_scroll = app.lyrics_scroll.saturating_add(delta as u16);
        } else {
            app.lyrics_scroll = app.lyrics_scroll.saturating_sub((-delta) as u16);
        }
    }
}

/// Navega para cima/baixo no componente com foco.
fn navigate(app: &mut App, delta: isize) {
    match app.focus {
        Focus::Sidebar => app.move_sidebar(delta),
        Focus::Main => match app.section {
            Section::Inicio => app.move_home(if delta < 0 {
                HomeDirection::Up
            } else {
                HomeDirection::Down
            }),
            Section::Letra => scroll_lyrics(app, delta),
            Section::Ajuda => scroll_help(app, delta),
            _ => app.move_selection(delta),
        },
    }
}

/// Executa a ação de "Enter" conforme o contexto atual.
fn activate(app: &mut App) {
    match app.focus {
        Focus::Sidebar => {
            // Confirmar uma seção move o foco para o painel principal.
            app.focus = Focus::Main;
        }
        Focus::Main => match app.section {
            Section::Buscar | Section::Fila => app.play_selected(),
            Section::Inicio => app.open_selected_home(),
            Section::Playlists => app.open_selected_playlist(),
            Section::Biblioteca => app.open_selected_library_playlist(),
            Section::Artistas => app.open_selected_artist(),
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn slash_keeps_sidebar_index_in_sync_with_the_search_section() {
        let mut app = App::new_for_tests();
        // Start somewhere else in the sidebar, as if the user had been
        // browsing the Queue before pressing "/".
        app.sidebar_index = Section::Fila.index();
        app.section = Section::Fila;

        handle_key(&mut app, key(KeyCode::Char('/')));
        assert_eq!(app.section, Section::Buscar);
        assert_eq!(app.sidebar_index, Section::Buscar.index());

        // Confirm the search, then move the sidebar selection: it must land
        // on the section adjacent to Search, not jump somewhere unrelated
        // because sidebar_index was left stale.
        app.input_mode = false;
        app.focus = Focus::Sidebar;
        handle_key(&mut app, key(KeyCode::Char('k'))); // up: Search -> Home
        assert_eq!(app.section, Section::Inicio);

        app.focus = Focus::Sidebar;
        handle_key(&mut app, key(KeyCode::Char('j'))); // down: Home -> Search
        handle_key(&mut app, key(KeyCode::Char('j'))); // down: Search -> Library
        assert_eq!(app.section, Section::Biblioteca);
    }

    #[test]
    fn home_right_moves_cards_while_search_right_only_moves_focus() {
        let mut home = App::new_for_tests();
        home.section = Section::Inicio;
        home.focus = Focus::Main;
        home.home_columns = 3;
        home.recent = (1..=3)
            .map(|i| crate::models::Track {
                video_id: format!("t{i}"),
                ..Default::default()
            })
            .collect();
        home.home = vec![crate::models::HomeSection {
            title: "Next shelf".into(),
            items: (1..=2)
                .map(|i| crate::models::Playlist {
                    browse_id: format!("p{i}"),
                    ..Default::default()
                })
                .collect(),
        }];
        home.list_state.select(Some(1));

        handle_key(&mut home, key(KeyCode::Right));
        assert_eq!(home.focus, Focus::Main);
        assert_eq!(home.list_state.selected(), Some(2));
        handle_key(&mut home, key(KeyCode::Down));
        assert_eq!(home.list_state.selected(), Some(4));

        let mut search = App::new_for_tests();
        search.section = Section::Buscar;
        search.focus = Focus::Sidebar;
        search.list_state.select(Some(1));

        handle_key(&mut search, key(KeyCode::Right));
        assert_eq!(search.focus, Focus::Main);
        assert_eq!(search.list_state.selected(), Some(1));
    }

    #[tokio::test]
    async fn shift_r_triggers_a_home_and_library_reload() {
        let mut app = App::new_for_tests();
        assert!(!app.is_loading(), "idle before the key is pressed");

        handle_key(&mut app, key(KeyCode::Char('R')));

        assert!(
            app.is_loading(),
            "R kicks off the same reload as the background sync"
        );
        assert!(app.status.contains("Atualizando"));
    }
}
