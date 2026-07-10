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
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

/// Narrow-layout header: current section (with its icon) and its position
/// in the menu.
fn draw_narrow_header(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let index = app.sidebar_index.min(Section::ALL.len() - 1);
    let section = Section::ALL[index];
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", section.icon()),
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
            Style::default().fg(theme.muted),
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
    let status_len = display_width(&status);
    spans.push(Span::styled(status, Style::default().fg(theme.subtext)));

    if show_shortcuts {
        let pad = width.saturating_sub(used + status_len + shortcuts_len + 1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.extend(shortcut_spans(shortcuts, theme));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Renders a "key desc · key desc" shortcut string as styled spans: keys
/// pop slightly, descriptions and separators recede. Purely presentational —
/// the visible text (and thus its width) is exactly the input string.
fn shortcut_spans(
    shortcuts: &'static str,
    theme: &'static crate::theme::Theme,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, entry) in shortcuts.split(" · ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme.border)));
        }
        match entry.split_once(' ') {
            Some((key, desc)) => {
                spans.push(Span::styled(
                    key,
                    Style::default()
                        .fg(theme.subtext)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(desc, Style::default().fg(theme.muted)));
            }
            None => spans.push(Span::styled(entry, Style::default().fg(theme.muted))),
        }
    }
    spans
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
        Section::Fila => "Enter play · d remove · J/K move · c clear · ? help",
        Section::Buscar => "Enter play · a queue · Space pause · / search · ? help",
        Section::Biblioteca => "g sign in · Enter open · / search · ? help · q quit",
        Section::Letra => "↑↓ scroll · / search · ? help · q quit",
        Section::Ajuda => "↑↓ scroll · / search · q quit",
        _ => "Enter open · / search · ? help · q quit",
    }
}

/// Display width (terminal columns) of `s`. Unlike `.chars().count()`, this
/// accounts for wide characters (CJK, many emoji) that occupy two columns
/// each — track titles and artist names routinely contain these.
pub(crate) fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncates `s` to at most `max` display columns, ending with an ellipsis
/// (1 column) when anything was cut. Stops before a character that would
/// overflow the budget rather than splitting it, so wide characters are
/// never rendered partially.
pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1;
    let mut out = String::new();
    let mut width = 0;
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + w > budget {
            break;
        }
        width += w;
        out.push(c);
    }
    out.push('…');
    out
}

/// Hard-truncates `s` to at most `max` display columns, with no ellipsis
/// (used where the caller pads the remainder itself, e.g. a fixed-width
/// list column).
pub(crate) fn take_width(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut width = 0;
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + w > max {
            break;
        }
        width += w;
        out.push(c);
    }
    out
}

/// Marquee: recorta uma janela de `width` colunas sobre os trechos
/// estilizados de `parts`, deslizando uma coluna por unidade de `step` e
/// recomeçando após um respiro de 3 colunas — o clássico título rolante de
/// players de música. O chamador só usa quando o texto não cabe e deriva
/// `step` do relógio de reprodução, então o texto desliza enquanto toca e
/// congela em pausa. Caracteres largos (CJK/emoji) nunca são cortados ao
/// meio: a coluna órfã na borda vira um espaço.
pub(crate) fn marquee_spans(
    parts: &[(&str, Style)],
    width: usize,
    step: usize,
) -> Vec<Span<'static>> {
    const GAP: usize = 3;
    let cells: Vec<(char, Style)> = parts
        .iter()
        .flat_map(|(text, style)| text.chars().map(move |c| (c, *style)))
        .chain((0..GAP).map(|_| (' ', Style::default())))
        .collect();
    let total: usize = cells
        .iter()
        .map(|(c, _)| UnicodeWidthChar::width(*c).unwrap_or(0))
        .sum();
    if total == 0 || width == 0 {
        return Vec::new();
    }

    // Localiza a célula que cobre a coluna `offset`; se o offset cair no
    // meio de um caractere largo, começa na célula seguinte e compensa a
    // coluna órfã com um espaço à esquerda.
    let offset = step % total;
    let mut col = 0usize;
    let mut start = cells.len();
    for (i, (c, _)) in cells.iter().enumerate() {
        if col >= offset {
            start = i;
            break;
        }
        col += UnicodeWidthChar::width(*c).unwrap_or(0);
    }
    let mut out: Vec<(char, Style)> = Vec::new();
    let mut used = 0usize;
    for _ in 0..col.saturating_sub(offset).min(width) {
        out.push((' ', Style::default()));
        used += 1;
    }
    let mut i = start;
    while used < width {
        let (c, style) = cells[i % cells.len()];
        i += 1;
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if w == 0 {
            continue;
        }
        if used + w > width {
            break;
        }
        out.push((c, style));
        used += w;
    }

    // Mescla caracteres consecutivos de mesmo estilo em um span só.
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (c, style) in out {
        match spans.last_mut() {
            Some(last) if last.style == style => last.content.to_mut().push(c),
            _ => spans.push(Span::styled(c.to_string(), style)),
        }
    }
    spans
}

/// Keeps the last `max` display columns of `s` (used so the search cursor
/// and the end of long queries stay visible).
fn tail_chars(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    let mut reversed = String::new();
    let mut width = 0;
    for c in s.chars().rev() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + w > max {
            break;
        }
        width += w;
        reversed.push(c);
    }
    reversed.chars().rev().collect()
}
