//! Renderização da interface (Ratatui).

mod main_panel;
mod player;
mod sidebar;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Desenha toda a interface em um frame.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Divisão vertical: corpo + player.
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(8)])
        .split(area);

    // Corpo: barra lateral + área direita.
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(0)])
        .split(root[0]);

    // Área direita: barra de busca + conteúdo.
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(body[1]);

    sidebar::draw(f, app, body[0]);
    draw_search_bar(f, app, right[0]);
    main_panel::draw(f, app, right[1]);
    player::draw(f, app, root[1]);
}

/// Barra de busca no topo.
fn draw_search_bar(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (border_color, hint) = if app.input_mode {
        (Color::Yellow, "  (Enter: buscar | Esc: cancelar)")
    } else {
        (Color::DarkGray, "  (pressione '/' para buscar)")
    };

    let content = if app.input_mode {
        Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(Color::Yellow)),
            Span::raw(&app.query),
            Span::styled("▏", Style::default().fg(Color::Yellow)),
        ])
    } else if app.query.is_empty() {
        Line::from(Span::styled(
            format!("Digite para buscar músicas, artistas ou playlists{hint}"),
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.query),
        ])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Buscar ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(Paragraph::new(content).block(block), area);
}
