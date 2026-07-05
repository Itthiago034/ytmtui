//! Navigation column for the wide layout: app identity, account state and
//! the section list. Borderless by design; hierarchy comes from color and
//! the selection bar.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use ratatui_image::StatefulImage;

use crate::app::{App, AuthenticationState, Focus, Section};

/// Rows taken by the header (blank, title, account, blank) plus the menu.
const MENU_ROWS: u16 = 4 + Section::ALL.len() as u16;

/// Draws the navigation column.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = app.theme();
    let width = area.width as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(Section::ALL.len() + 4);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " ♫ ytmtui",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(account_line(app, width));
    lines.push(Line::from(""));

    let focused = app.focus == Focus::Sidebar;
    for (index, section) in Section::ALL.iter().enumerate() {
        lines.push(section_line(section, index, app, focused, width));
    }

    f.render_widget(Paragraph::new(lines), area);
    draw_artwork(f, app, area);
}

/// Album art at the bottom of the column, below the menu, when a cover has
/// been prepared and the terminal is tall enough.
fn draw_artwork(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(protocol) = app.artwork.as_mut() else {
        return;
    };
    if area.height <= MENU_ROWS + 4 || area.width <= 4 {
        return;
    }
    let height = (area.height - MENU_ROWS - 2).min(9);
    let art = Rect {
        x: area.x + 1,
        y: area.y + area.height - height - 1,
        width: area.width - 2,
        height,
    };
    f.render_stateful_widget(StatefulImage::new(None), art, protocol);
}

/// Account line: signed-in name, expired-session warning, or anonymous.
fn account_line(app: &App, width: usize) -> Line<'static> {
    let max = width.saturating_sub(4);
    match app.authentication {
        AuthenticationState::Authenticated => {
            let name = app
                .account_name
                .clone()
                .unwrap_or_else(|| "signed in".to_string());
            let style = Style::default().fg(app.theme().secondary);
            Line::from(vec![
                Span::styled(" ● ", style),
                Span::styled(crate::ui::truncate_chars(&name, max), style),
            ])
        }
        AuthenticationState::Expired => {
            let style = Style::default().fg(Color::Yellow);
            Line::from(vec![
                Span::styled(" ● ", style),
                Span::styled("session expired".to_string(), style),
            ])
        }
        AuthenticationState::Anonymous | AuthenticationState::InvalidCookies => {
            let style = Style::default().fg(Color::DarkGray);
            Line::from(vec![
                Span::styled(" ○ ", style),
                Span::styled("not signed in".to_string(), style),
            ])
        }
    }
}

/// One menu row. The selected section carries an accent bar; the background
/// fill additionally shows whether the menu has keyboard focus.
fn section_line(
    section: &Section,
    index: usize,
    app: &App,
    focused: bool,
    width: usize,
) -> Line<'static> {
    let theme = app.theme();
    let selected = index == app.sidebar_index;
    if selected {
        let style = if focused {
            Style::default()
                .fg(theme.accent_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        };
        let label = format!(" {:<w$}", section.label(), w = width.saturating_sub(2));
        Line::from(vec![
            Span::styled("▍", Style::default().fg(theme.accent)),
            Span::styled(label, style),
        ])
    } else {
        Line::from(Span::styled(
            format!("  {}", section.label()),
            Style::default().fg(Color::Gray),
        ))
    }
}
