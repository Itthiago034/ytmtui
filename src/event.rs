//! Tratamento de eventos de teclado.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Focus, Section};

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
            app.sidebar_index = 0;
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
        KeyCode::Char('s') => app.player.stop(),
        KeyCode::Char('+') | KeyCode::Char('=') => app.player.volume_up(),
        KeyCode::Char('-') | KeyCode::Char('_') => app.player.volume_down(),
        // Seek dentro da faixa.
        KeyCode::Char('[') => app.seek_backward(),
        KeyCode::Char(']') => app.seek_forward(),
        // Modos de reprodução.
        KeyCode::Char('z') => app.toggle_shuffle(),
        KeyCode::Char('r') => app.cycle_repeat(),
        // Adiciona a faixa selecionada à fila.
        KeyCode::Char('a') => app.enqueue_selected(),
        // Curte / descurte a faixa atual.
        KeyCode::Char('f') => app.like_current(),
        // Alterna o tema de cores.
        KeyCode::Char('t') => app.cycle_theme(),

        // Alterna o foco entre a barra lateral e o painel principal.
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::Sidebar => Focus::Main,
                Focus::Main => Focus::Sidebar,
            };
        }
        KeyCode::Left | KeyCode::Char('h') => app.focus = Focus::Sidebar,
        KeyCode::Right | KeyCode::Char('l') => app.focus = Focus::Main,

        // Navegação vertical (depende do foco).
        KeyCode::Down | KeyCode::Char('j') => navigate(app, 1),
        KeyCode::Up | KeyCode::Char('k') => navigate(app, -1),

        // Ação principal.
        KeyCode::Enter => activate(app),
        _ => {}
    }
}

/// Navega para cima/baixo no componente com foco.
fn navigate(app: &mut App, delta: isize) {
    match app.focus {
        Focus::Sidebar => app.move_sidebar(delta),
        Focus::Main => {
            if app.section == Section::Letra {
                // Rola a letra.
                if delta > 0 {
                    app.lyrics_scroll = app.lyrics_scroll.saturating_add(1);
                } else {
                    app.lyrics_scroll = app.lyrics_scroll.saturating_sub(1);
                }
            } else {
                app.move_selection(delta);
            }
        }
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
