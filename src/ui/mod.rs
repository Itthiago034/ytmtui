//! Interface rendering (Ratatui).
//!
//! Rendering is side-effect-free: every function here reads application
//! state and draws widgets. No network, disk, or domain mutation happens
//! while drawing.
//!
//! Layout overview:
//! - wide terminals: navigation column + content panel, followed by a
//!   two-line playback summary and a one-line status/shortcut bar;
//! - narrow terminals: single column with a compact one-line header;
//! - short terminals: the playback and status rows are dropped first so the
//!   content always keeps the remaining space and nothing panics.

mod main_panel;
mod nav;
mod now_playing;

#[cfg(test)]
mod tests;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Focus, Section};

/// Terminals narrower than this use the single-column fallback layout.
const NARROW_WIDTH: u16 = 70;
/// Width of the navigation column in the wide layout.
const NAV_WIDTH: u16 = 18;
/// Minimum height that still fits the playback summary and status rows.
const MIN_FULL_HEIGHT: u16 = 9;

/// Draws the whole interface for one frame.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    if area.width < NARROW_WIDTH {
        draw_narrow(f, app, area);
    } else {
        draw_wide(f, app, area);
    }
}

/// Wide layout: navigation + content columns above playback and status rows.
fn draw_wide(f: &mut Frame, app: &mut App, area: Rect) {
    let (body, playback, status) = split_main_rows(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(NAV_WIDTH), Constraint::Min(1)])
        .split(body);
    nav::draw(f, app, columns[0]);
    draw_content(f, app, columns[1]);

    if let Some(rect) = playback {
        now_playing::draw(f, app, rect);
    }
    if let Some(rect) = status {
        draw_status_bar(f, app, rect);
    }
}

/// Narrow layout: a one-line header above a single content column.
fn draw_narrow(f: &mut Frame, app: &mut App, area: Rect) {
    let (header, body, playback, status) = split_narrow_rows(area);

    if let Some(rect) = header {
        if app.input_mode {
            draw_search_input(f, app, rect);
        } else {
            draw_narrow_header(f, app, rect);
        }
    }
    main_panel::draw(f, app, body);

    if let Some(rect) = playback {
        now_playing::draw(f, app, rect);
    }
    if let Some(rect) = status {
        draw_status_bar(f, app, rect);
    }
}

/// Splits the frame into body, optional playback rows and optional status
/// row, dropping the optional rows first when the terminal is short.
fn split_main_rows(area: Rect) -> (Rect, Option<Rect>, Option<Rect>) {
    if area.height >= MIN_FULL_HEIGHT {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(area);
        (rows[0], Some(rows[1]), Some(rows[2]))
    } else if area.height >= 4 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        (rows[0], None, Some(rows[1]))
    } else {
        (area, None, None)
    }
}

/// Same as [`split_main_rows`] plus the one-line narrow header on top.
fn split_narrow_rows(area: Rect) -> (Option<Rect>, Rect, Option<Rect>, Option<Rect>) {
    if area.height > MIN_FULL_HEIGHT {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(area);
        (Some(rows[0]), rows[1], Some(rows[2]), Some(rows[3]))
    } else if area.height >= 5 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);
        (Some(rows[0]), rows[1], None, Some(rows[2]))
    } else if area.height >= 2 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        (Some(rows[0]), rows[1], None, None)
    } else {
        (None, area, None, None)
    }
}

/// Content column: the main panel, with a one-line search input above it
/// while the user is typing a query.
fn draw_content(f: &mut Frame, app: &mut App, area: Rect) {
    if app.input_mode && area.height > 1 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        draw_search_input(f, app, rows[0]);
        main_panel::draw(f, app, rows[1]);
    } else {
        main_panel::draw(f, app, area);
    }
}

/// One-line search input with a visible cursor. Long queries keep their end
/// (next to the cursor) visible.
fn draw_search_input(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let avail = (area.width as usize).saturating_sub(4);
    let shown = tail_chars(&app.query, avail);
    let line = Line::from(vec![
        Span::styled(
            " / ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(shown),
        Span::styled("▏", Style::default().fg(theme.accent)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

/// Narrow-layout header: current section and its position in the menu.
fn draw_narrow_header(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let index = app.sidebar_index.min(Section::ALL.len() - 1);
    let section = Section::ALL[index];
    let line = Line::from(vec![
        Span::styled(
            " ♫ ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            section.label().to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}/{}", index + 1, Section::ALL.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

/// One-line status bar: transient status message on the left, contextual
/// shortcuts on the right. Shortcuts are dropped first when space is tight.
fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let width = area.width as usize;
    let shortcuts = contextual_shortcuts(app);
    let shortcuts_len = shortcuts.chars().count();

    let mut spans: Vec<Span> = Vec::new();
    if app.is_loading() {
        spans.push(Span::styled(
            format!(" {} ", app.spinner()),
            Style::default().fg(theme.accent),
        ));
    } else {
        spans.push(Span::raw(" "));
    }
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();

    let show_shortcuts = width > used + shortcuts_len + 12;
    let status_room = if show_shortcuts {
        width.saturating_sub(used + shortcuts_len + 2)
    } else {
        width.saturating_sub(used + 1)
    };
    let status = truncate_chars(&app.status, status_room);
    let status_len = status.chars().count();
    spans.push(Span::styled(status, Style::default().fg(Color::Gray)));

    if show_shortcuts {
        let pad = width.saturating_sub(used + status_len + shortcuts_len + 1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(
            shortcuts,
            Style::default().fg(Color::DarkGray),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Shortcuts that make sense for the current focus and section.
fn contextual_shortcuts(app: &App) -> &'static str {
    if app.input_mode {
        return "Enter search · Esc cancel";
    }
    if app.focus == Focus::Sidebar {
        return "↑↓ section · Enter open · / search · q quit";
    }
    match app.section {
        Section::Buscar | Section::Fila => "Enter play · a queue · Space pause · / search · ? help",
        Section::Letra => "↑↓ scroll · / search · ? help · q quit",
        Section::Ajuda => "/ search · q quit",
        _ => "Enter open · / search · ? help · q quit",
    }
}

/// Truncates `s` to at most `max` characters, ending with an ellipsis when
/// anything was cut.
pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Keeps the last `max` characters of `s` (used so the search cursor and the
/// end of long queries stay visible).
fn tail_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        s.chars().skip(count - max).collect()
    }
}
