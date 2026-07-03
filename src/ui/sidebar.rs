//! Barra lateral de navegação.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, Focus, Section};

/// Desenha a barra lateral com as seções.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Sidebar;
    let border_color = if focused { Color::Magenta } else { Color::DarkGray };

    let items: Vec<ListItem> = Section::ALL
        .iter()
        .map(|s| {
            let selected = *s == app.section;
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(Span::styled(format!(" {}", s.label()), style)))
        })
        .collect();

    // Indicador persistente de login (● verde = logado).
    let mut title_spans = vec![Span::styled(
        " ytmtui ",
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    )];
    if app.logged_in {
        title_spans.push(Span::styled(
            "● ",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(title_spans));

    let list = List::new(items).block(block).highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.sidebar_index));
    f.render_stateful_widget(list, area, &mut state);
}
