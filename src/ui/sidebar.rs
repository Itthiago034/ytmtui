//! Barra lateral: logo, conta e navegação por seções.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, Focus, Section};

/// Desenha a barra lateral (cabeçalho com logo/conta + menu de seções).
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(area);

    draw_header(f, app, rows[0]);
    draw_menu(f, app, rows[1]);
}

/// Cabeçalho: logo em ASCII + linha da conta logada.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let width = area.width as usize;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" ♫ ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(
            "ytmtui",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(theme.accent),
    )));

    // Linha da conta.
    if app.logged_in {
        let name = app.account_name.clone().unwrap_or_else(|| "conectado".to_string());
        let initial = name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "♥".to_string());
        // Trunca o nome para caber na coluna.
        let max_name = width.saturating_sub(6);
        let shown: String = name.chars().take(max_name).collect();
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!(" {initial} "),
                Style::default()
                    .fg(theme.accent_fg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(shown, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("○ ", Style::default().fg(Color::DarkGray)),
            Span::styled("não conectado", Style::default().fg(Color::DarkGray)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), area);
}

/// Menu de seções.
fn draw_menu(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let focused = app.focus == Focus::Sidebar;
    let border_color = if focused { theme.accent } else { Color::DarkGray };

    let items: Vec<ListItem> = Section::ALL
        .iter()
        .map(|s| {
            let selected = *s == app.section;
            let style = if selected {
                Style::default()
                    .fg(theme.accent_fg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(Span::styled(format!(" {}", s.label()), style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Menu ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));

    let list = List::new(items).block(block).highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.sidebar_index));
    f.render_stateful_widget(list, area, &mut state);
}
