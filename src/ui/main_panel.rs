//! Main content panel: tracks, playlists, artists, queue, lyrics or help.
//! This is the only panel that keeps a rounded border and a scrollbar; both
//! aid orientation in long lists.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Section};
use crate::config::{HomeDensity, VisualizerStyle};
use crate::home::{HomeCard, HomeCardKind, HomeShelf};
use crate::theme::ThemeColors;

/// Desenha o conteúdo do painel principal de acordo com a seção ativa.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let focused = app.focus == Focus::Main;
    let border_color = if focused { theme.accent } else { theme.border };

    let title = match app.section {
        Section::Inicio => "Home".to_string(),
        Section::Buscar => app.songs_title.clone(),
        Section::Biblioteca => "Library".to_string(),
        Section::Playlists => "Playlists".to_string(),
        Section::Artistas => "Artists".to_string(),
        Section::Fila => "Queue".to_string(),
        Section::Tocando => "Now Playing".to_string(),
        // The Lyrics panel names the track it's showing lyrics for, plus
        // any non-default sync correction and whether auto-follow is
        // currently paused — both are modes the user needs to see to
        // understand what the panel is doing.
        Section::Letra => {
            let mut title = match &app.current {
                Some(t) => format!("Lyrics — {}", t.title),
                None => "Lyrics".to_string(),
            };
            let offset = app.ui.lyrics.offset_ms();
            if offset != 0 {
                title.push_str(&format!("  [{:+.2}s]", offset as f64 / 1000.0));
            }
            if !app.ui.lyrics.following() {
                title.push_str("  [browsing · Home to follow]");
            }
            crate::ui::truncate_chars(&title, (area.width as usize).saturating_sub(12))
        }
        Section::Ajustes => "Settings".to_string(),
        Section::Ajuda => "Help".to_string(),
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {} {} ", app.section.icon(), title),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));

    // Item count badge in the bottom-right corner of list sections.
    let count = match app.section {
        Section::Buscar if app.search_mixed => app.search_item_count(),
        Section::Buscar => app.songs.len(),
        Section::Fila => app.queue.len(),
        Section::Biblioteca => app.library.len(),
        Section::Playlists => app.playlists.len(),
        Section::Artistas => app.artists.len(),
        _ => 0,
    };
    if count > 0 {
        let noun = if count == 1 { "item" } else { "items" };
        block = block.title_bottom(
            Line::from(Span::styled(
                format!(" {count} {noun} "),
                Style::default().fg(theme.muted),
            ))
            .right_aligned(),
        );
    }

    match app.section {
        Section::Inicio => draw_home(f, app, area, block),
        Section::Buscar if app.search_mixed && app.search_item_count() > 0 => {
            draw_search_mixed(f, app, area, block)
        }
        Section::Buscar => draw_songs(f, app, area, block, &app.songs),
        Section::Fila => draw_queue(f, app, area, block),
        Section::Biblioteca => draw_library(f, app, area, block),
        Section::Playlists => draw_playlists(f, app, area, block),
        Section::Artistas => draw_artists(f, app, area, block),
        Section::Tocando => super::now_playing_screen::draw(f, app, area, block),
        Section::Letra => draw_lyrics(f, app, area, block),
        Section::Ajustes => super::settings::draw(f, app, area, block),
        Section::Ajuda => draw_help(f, app, area, block),
    }
}

/// Centered empty-state: a large dim glyph above a short hint. Keeps every
/// message text intact; only presentation changes. Falls back to the plain
/// message when the panel is too short for the decoration.
fn draw_empty_state(
    f: &mut Frame,
    area: Rect,
    block: Block,
    icon: &str,
    message: &str,
    theme: ThemeColors,
) {
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    if inner.height >= 7 {
        let pad = (inner.height as usize - 4) / 3;
        for _ in 0..pad {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            icon.to_string(),
            Style::default().fg(theme.border),
        )));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        message.to_string(),
        Style::default().fg(theme.muted),
    )));
    let p = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    f.render_widget(p, inner);
}

/// Formata uma linha de faixa: "01  Título  —  Artista        3:45".
fn track_line(
    index: usize,
    t: &crate::models::Track,
    width: usize,
    playing: bool,
    theme: ThemeColors,
) -> Line<'static> {
    let num = format!("{:>2}  ", index + 1);
    let dur = if t.duration.is_empty() {
        String::new()
    } else {
        t.duration.clone()
    };
    // Espaço reservado para número + duração + margens.
    let avail = width.saturating_sub(num.len() + dur.len() + 6);
    let main = format!("{} — {}", t.title, t.artist);
    let main = crate::ui::take_width(&main, avail);
    let pad = " ".repeat(avail.saturating_sub(crate::ui::display_width(&main)) + 2);

    let marker_style = if playing {
        Style::default()
            .fg(theme.player)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };
    Line::from(vec![
        Span::styled(if playing { "▶ " } else { "  " }, marker_style),
        Span::styled(num, Style::default().fg(theme.muted)),
        Span::styled(
            main,
            Style::default().fg(if playing { theme.player } else { theme.text }),
        ),
        Span::raw(pad),
        Span::styled(dur, Style::default().fg(theme.muted)),
    ])
}

/// One playlist/artist-style row: dim icon, bold title, dim subtitle.
fn entry_line(icon: &str, title: &str, subtitle: &str, theme: ThemeColors) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {icon} "), Style::default().fg(theme.muted)),
        Span::styled(
            title.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  ·  {subtitle}"), Style::default().fg(theme.muted)),
    ])
}

fn draw_songs(f: &mut Frame, app: &App, area: Rect, block: Block, songs: &[crate::models::Track]) {
    let theme = app.theme();
    if songs.is_empty() {
        draw_empty_state(
            f,
            area,
            block,
            "⌕",
            "No tracks yet. Press / to search.",
            theme,
        );
        return;
    }
    let width = area.width.saturating_sub(4) as usize;
    let current_id = app.current.as_ref().map(|t| t.video_id.clone());
    let items: Vec<ListItem> = songs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = current_id.as_deref() == Some(t.video_id.as_str());
            ListItem::new(track_line(i, t, width, playing, theme))
        })
        .collect();

    render_list(f, app, area, block, items);
}

/// Mixed search results, grouped by type with the same header-plus-rule rows
/// the Home screen uses. All groups share one flat selectable list; the
/// flattened index order (songs, artists, albums, playlists) must match
/// `App::search_hit_at`, which resolves Enter/queue actions from it.
fn draw_search_mixed(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let selected = app.list_state.selected();
    let track_width = inner.width.saturating_sub(2) as usize;
    let header_width = inner.width as usize;
    let current_id = app.current.as_ref().map(|t| t.video_id.clone());

    let mut items: Vec<ListItem> = Vec::new();
    let mut shadow_selected: Option<usize> = None;
    let mut flat_idx = 0usize;

    if !app.songs.is_empty() {
        items.push(ListItem::new(section_header("Songs", header_width, theme)));
        for (i, t) in app.songs.iter().enumerate() {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            let playing = current_id.as_deref() == Some(t.video_id.as_str());
            items.push(ListItem::new(track_line(i, t, track_width, playing, theme)));
            flat_idx += 1;
        }
    }
    if !app.artists.is_empty() {
        items.push(ListItem::new(section_header(
            "Artists",
            header_width,
            theme,
        )));
        for a in &app.artists {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            items.push(ListItem::new(entry_line("◆", &a.name, &a.subtitle, theme)));
            flat_idx += 1;
        }
    }
    if !app.albums.is_empty() {
        items.push(ListItem::new(section_header("Albums", header_width, theme)));
        for al in &app.albums {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            items.push(ListItem::new(entry_line(
                "◈",
                &al.title,
                &al.subtitle,
                theme,
            )));
            flat_idx += 1;
        }
    }
    if !app.playlists.is_empty() {
        items.push(ListItem::new(section_header(
            "Playlists",
            header_width,
            theme,
        )));
        for p in &app.playlists {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            items.push(ListItem::new(entry_line("♫", &p.title, &p.subtitle, theme)));
            flat_idx += 1;
        }
    }

    // Same shadow-selection remapping as the Home screen: header rows are
    // not selectable, so the flat selection index must be shifted past them.
    let mut shadow_state = app.list_state.clone();
    shadow_state.select(shadow_selected);
    render_list_borderless(f, app, inner, items, &shadow_state);
}

fn draw_queue(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    if app.queue.is_empty() {
        draw_empty_state(
            f,
            area,
            block,
            "≡",
            "The queue is empty. Play a track to fill it.",
            theme,
        );
        return;
    }
    let width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = app.queue_index == Some(i);
            ListItem::new(track_line(i, t, width, playing, theme))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_playlists(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    if app.playlists.is_empty() {
        draw_empty_state(
            f,
            area,
            block,
            "≡",
            "No playlists yet. Search to find some.",
            theme,
        );
        return;
    }
    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .map(|p| ListItem::new(entry_line("≡", &p.title, &p.subtitle, theme)))
        .collect();
    render_list(f, app, area, block, items);
}

/// Height of the player panel above the recommendations list: 1 title row +
/// 5 bar rows + 1 blank spacer row.
const PLAYER_PANEL_HEIGHT: u16 = 7;

/// Home renders the outer border once around the whole area, then — when
/// there's enough height — splits the inside into a player panel (track
/// title + spectrum bars) above the usual recommendations list. Short
/// terminals degrade to just the list/message, exactly like before.
fn draw_home(f: &mut Frame, app: &mut App, area: Rect, block: Block) {
    let inner = block.inner(area);
    f.render_widget(block, area);

    // With nothing playing and nothing to list, the player panel's
    // placeholder would just crowd the wordmark — give it the whole panel.
    let idle_empty = app.current.is_none() && app.home.is_empty() && app.recent.is_empty();
    if idle_empty || inner.height < PLAYER_PANEL_HEIGHT + 4 {
        draw_home_sections(f, app, inner);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(PLAYER_PANEL_HEIGHT),
            Constraint::Min(1),
        ])
        .split(inner);
    draw_greeting(f, app, rows[0]);
    draw_player_panel(f, app, rows[1]);
    draw_home_sections(f, app, rows[2]);
}

/// Time-of-day greeting with the signed-in name on the left and the current
/// date on the right — the "you are home" row.
fn draw_greeting(f: &mut Frame, app: &App, area: Rect) {
    use chrono::{Datelike, Local, Timelike};
    let theme = app.theme();
    let now = Local::now();
    let salutation = match now.hour() {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    let greeting = match &app.account_name {
        Some(name) => format!(" {salutation}, {name}"),
        None => format!(" {salutation}"),
    };
    let weekday = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [now.weekday().num_days_from_monday() as usize];
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][now.month0() as usize];
    let date = format!("{weekday} · {month} {} ", now.day());

    let width = area.width as usize;
    let greeting_shown = crate::ui::truncate_chars(&greeting, width);
    let used = crate::ui::display_width(&greeting_shown);
    let date_len = crate::ui::display_width(&date);
    let mut spans = vec![Span::styled(
        greeting_shown,
        Style::default()
            .fg(theme.secondary)
            .add_modifier(Modifier::BOLD),
    )];
    if width > used + date_len {
        spans.push(Span::raw(" ".repeat(width - used - date_len)));
        spans.push(Span::styled(date, Style::default().fg(theme.muted)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// The empty Home screen and the entry animation show the same wordmark, so
// it is defined once, next to the animation that assembles it.
use super::splash::{LOGO, TAGLINE};

/// The recommendations area (or its empty/loading message), without a
/// border of its own — the border now belongs to the whole Home area. Wide
/// areas (`width >= GRID_MIN_WIDTH`) render a 2D grid of cards per shelf;
/// narrower ones degrade to the original flat list.
///
/// Below this width, the Home grid degrades to the flat list exactly as it
/// rendered before this feature. Evaluated against the width this function
/// actually receives (inside the Home border, after the nav column and any
/// player panel split), not the raw terminal width.
const GRID_MIN_WIDTH: u16 = 70;
/// Minimum width of one card column in grid mode; the real per-card width
/// stretches to `list_area.width / columns` so there's no ragged empty
/// margin on the right (see `draw_home_grid`).
const CARD_WIDTH: u16 = 24;
/// One blank terminal column between neighboring card frames. Without this
/// gutter, adjacent borders visually merge back into a table.
const CARD_GAP: u16 = 1;

/// Height (rows) of one card for the given Home density: "comfortable" is
/// title, subtitle, footer (3 rows); "compact" drops the subtitle row and
/// stays at 2 (title, footer) — navigation and card count are unaffected,
/// only the vertical footprint changes.
fn card_height(density: HomeDensity) -> u16 {
    match density {
        // Content rows plus the rounded top/bottom frame.
        HomeDensity::Comfortable => 5,
        HomeDensity::Compact => 4,
    }
}

/// Height of one shelf block in grid mode: a 1-row header plus one row of
/// cards sized per `card_height`. Vertical scrolling moves by whole
/// shelves, so this is also the scroll granularity.
fn shelf_height(density: HomeDensity) -> u16 {
    1 + card_height(density)
}

fn draw_home_sections(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    if app.home.is_empty() && app.recent.is_empty() {
        // Nothing to page through either way; keep the list-mode default.
        app.ui.home_columns = 1;
        draw_home_empty_state(f, app, area, theme);
        return;
    }

    // Cached shelves exist: a failed background refresh never replaces them
    // (see `Msg::HomeFailed`) — it only earns a small retryable banner above
    // the still-visible content, instead of blanking the whole Home screen.
    let list_area = if app.home_error.is_some() && area.height > 1 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        let banner = Paragraph::new(Span::styled(
            "⚠ Couldn't refresh recommendations — press R to retry",
            Style::default().fg(theme.player),
        ));
        f.render_widget(banner, rows[0]);
        rows[1]
    } else {
        area
    };

    if list_area.width < GRID_MIN_WIDTH {
        app.ui.home_columns = 1;
        draw_home_list(f, app, list_area, theme);
        return;
    }

    draw_home_grid(f, app, list_area, theme);
}

/// Loading/empty/error placeholder shown when there is nothing at all to
/// list yet — same message and wordmark regardless of grid vs. list mode.
fn draw_home_empty_state(f: &mut Frame, app: &App, area: Rect, theme: ThemeColors) {
    let text = if app.busy() {
        format!("{} Loading recommendations…", app.spinner())
    } else if let Some(err) = &app.home_error {
        // No cache to fall back on: the empty state itself carries the
        // error and the retry hint, in place of the generic messages
        // below.
        format!("{err} — Press R to retry.")
    } else if app.is_authenticated() {
        "No recommendations are available. Press / to search.".to_string()
    } else {
        "Sign in to see recommendations — press g.".to_string()
    };

    let mut lines: Vec<Line> = Vec::new();
    // Identity moment: the wordmark and tagline, when there's room.
    if area.width as usize >= LOGO[0].chars().count() + 2 && area.height >= 10 {
        let pad = (area.height as usize).saturating_sub(LOGO.len() + 4) / 3;
        for _ in 0..pad {
            lines.push(Line::from(""));
        }
        for row in LOGO {
            lines.push(Line::from(Span::styled(
                row,
                Style::default().fg(theme.accent),
            )));
        }
        lines.push(Line::from(Span::styled(
            TAGLINE,
            Style::default().fg(theme.secondary),
        )));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        text,
        Style::default().fg(theme.muted),
    )));
    let msg = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    f.render_widget(msg, area);
}

/// Narrow-terminal (and pre-grid) rendering: sections as non-selectable
/// header rows interleaved with one row per item in a single flat scrollable
/// list (v1 scope: full section contents, no per-section cap/"show more" —
/// overflow uses the existing scrollbar, exactly like the old flat list
/// did).
fn draw_home_list(f: &mut Frame, app: &App, list_area: Rect, theme: ThemeColors) {
    let selected = app.list_state.selected();
    let mut items: Vec<ListItem> = Vec::new();
    let mut shadow_selected: Option<usize> = None;
    let mut flat_idx = 0usize;

    // Local history first — flat indices here must line up with
    // `App::open_selected_home`, which plays indices below `recent.len()`.
    if !app.recent.is_empty() {
        items.push(ListItem::new(section_header(
            "Continue listening",
            list_area.width as usize,
            theme,
        )));
        let track_width = list_area.width.saturating_sub(2) as usize;
        let current_id = app.current.as_ref().map(|t| t.video_id.clone());
        for (i, t) in app.recent.iter().enumerate() {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            let playing = current_id.as_deref() == Some(t.video_id.as_str());
            items.push(ListItem::new(track_line(i, t, track_width, playing, theme)));
            flat_idx += 1;
        }
    }

    for section in &app.home {
        items.push(ListItem::new(section_header(
            &section.title,
            list_area.width as usize,
            theme,
        )));
        for p in &section.items {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            items.push(ListItem::new(entry_line("♪", &p.title, &p.subtitle, theme)));
            flat_idx += 1;
        }
    }

    // The real selection index (over selectable items only) doesn't match
    // this list's row index once section headers are interleaved in — a
    // "shadow" ListState remaps it to the right row before rendering.
    let mut shadow_state = app.list_state.clone();
    shadow_state.select(shadow_selected);
    render_list_borderless(f, app, list_area, items, &shadow_state);
}

/// Wide-terminal rendering: one shelf per vertical block, each a header row
/// followed by a single row of cards. Uses `App::home_view` (the same 2D
/// projection `move_home` navigates) so flattened selection indices always
/// line up with what's drawn here.
///
/// Scrolling: vertical scrolling moves by whole shelves (never splitting one
/// mid-row) to keep the selected shelf on screen; horizontal scrolling is
/// per-shelf and keeps the selected card on screen, showing the first
/// `columns` cards for every other shelf. Overflow in either direction is
/// signaled with a `‹`/`›` marker appended to that shelf's header, rather
/// than a scrollbar — there's no single flat viewport to attach one to.
fn draw_home_grid(f: &mut Frame, app: &mut App, area: Rect, theme: ThemeColors) {
    // The caller only takes this path once `area.width >= GRID_MIN_WIDTH`,
    // but a zero height is still possible on very short terminals.
    if area.width == 0 || area.height == 0 {
        app.ui.home_columns = 1;
        return;
    }
    let columns = (area.width / CARD_WIDTH).max(1) as usize;
    app.ui.home_columns = columns;
    let density = app.home_density;
    let shelf_h = shelf_height(density);
    // Only one card in the whole grid can be selected, so the reveal stage
    // is computed once up front and threaded down to that card.
    let reveal = current_reveal_stage(app);

    let view = app.home_view();
    if view.is_empty() {
        return;
    }
    let selected = app.list_state.selected();

    // Running flat-index base of each shelf, to map a shelf-local column
    // back to the flattened index `list_state` uses.
    let mut bases = Vec::with_capacity(view.shelves.len());
    let mut base = 0usize;
    for shelf in &view.shelves {
        bases.push(base);
        base += shelf.cards.len();
    }
    let selected_shelf = selected.and_then(|idx| {
        view.shelves.iter().enumerate().find_map(|(i, shelf)| {
            (idx >= bases[i] && idx < bases[i] + shelf.cards.len()).then_some(i)
        })
    });

    // Vertical window: scroll by whole shelves so the selected one is
    // visible, clamped so the view never scrolls past the last shelf.
    let total_shelves = view.shelves.len();
    let visible_shelf_count = (area.height / shelf_h).max(1) as usize;
    let max_scroll = total_shelves.saturating_sub(visible_shelf_count);
    let mut shelf_scroll = 0usize;
    if let Some(sel) = selected_shelf {
        if sel >= visible_shelf_count {
            shelf_scroll = sel - visible_shelf_count + 1;
        }
    }
    shelf_scroll = shelf_scroll.min(max_scroll);
    let shelf_end = (shelf_scroll + visible_shelf_count).min(total_shelves);

    let bottom = area.y.saturating_add(area.height);
    let mut y = area.y;
    let visible_shelves = view
        .shelves
        .iter()
        .zip(bases.iter().copied())
        .enumerate()
        .skip(shelf_scroll)
        .take(shelf_end - shelf_scroll);
    for (shelf_idx, (shelf, shelf_base)) in visible_shelves {
        if y >= bottom {
            break;
        }
        let this_height = shelf_h.min(bottom - y);
        let header_height = this_height.min(1);
        let cards_height = this_height.saturating_sub(header_height);

        // Per-shelf horizontal window: only the selected shelf scrolls past
        // its first `columns` cards, and only far enough to keep the
        // selection visible.
        let local_selected = (selected_shelf == Some(shelf_idx))
            .then(|| selected.map(|idx| idx - shelf_base))
            .flatten();
        let offset = horizontal_offset(local_selected, shelf.cards.len(), columns);
        let can_scroll_left = offset > 0;
        let can_scroll_right = offset + columns < shelf.cards.len();

        if header_height > 0 {
            let header_rect = Rect {
                x: area.x,
                y,
                width: area.width,
                height: header_height,
            };
            let marker = match (can_scroll_left, can_scroll_right) {
                (true, true) => " ‹›",
                (true, false) => " ‹",
                (false, true) => " ›",
                (false, false) => "",
            };
            let title = format!("{}{marker}", shelf.title);
            f.render_widget(
                Paragraph::new(section_header(&title, header_rect.width as usize, theme)),
                header_rect,
            );
        }
        y += header_height;

        if cards_height > 0 {
            let cards_rect = Rect {
                x: area.x,
                y,
                width: area.width,
                height: cards_height,
            };
            draw_shelf_cards(
                f, cards_rect, shelf, shelf_base, offset, columns, selected, theme, density, reveal,
            );
        }
        y += cards_height;
    }
}

/// Smallest per-shelf horizontal offset (in cards) that keeps
/// `local_selected` inside the visible `columns`-wide window. Shelves with
/// no selected card default to offset 0 — their first `columns` cards.
fn horizontal_offset(local_selected: Option<usize>, card_count: usize, columns: usize) -> usize {
    let Some(local) = local_selected else {
        return 0;
    };
    let max_offset = card_count.saturating_sub(columns);
    if local < columns {
        0
    } else {
        (local + 1 - columns).min(max_offset)
    }
}

/// One row of cards for a single shelf: up to `columns` cards starting at
/// `offset`, each stretched to `area.width / columns` so the row fills the
/// available width without a ragged margin.
#[allow(clippy::too_many_arguments)]
fn draw_shelf_cards(
    f: &mut Frame,
    area: Rect,
    shelf: &HomeShelf,
    shelf_base: usize,
    offset: usize,
    columns: usize,
    selected: Option<usize>,
    theme: ThemeColors,
    density: HomeDensity,
    reveal: RevealStage,
) {
    if area.width == 0 || area.height == 0 || columns == 0 {
        return;
    }
    let col_width = area.width / columns as u16;
    if col_width == 0 {
        return;
    }
    for slot in 0..columns {
        let card_index = offset + slot;
        let Some(card) = shelf.cards.get(card_index) else {
            break;
        };
        let card_rect = Rect {
            x: area.x + slot as u16 * col_width,
            y: area.y,
            width: col_width.saturating_sub(CARD_GAP),
            height: area.height,
        };
        let is_selected = selected == Some(shelf_base + card_index);
        draw_card(f, card_rect, card, is_selected, theme, density, reveal);
    }
}

/// Stage of the selected card's staged reveal, driven by the elapsed time
/// since the selection last moved (see [`reveal_stage`]). Non-selected
/// cards never consult this — they always render the plain, un-accented
/// look regardless of stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RevealStage {
    /// 0–80ms: only the `selected_card` background shows; title and footer
    /// stay in their normal (non-selected) colors, no badge.
    Background,
    /// 80–160ms: the title has switched to accent+bold; the badge is still
    /// absent.
    Title,
    /// Past 160ms, under `reduced_motion`, or with no transition in
    /// progress at all (the selection never moved): the complete
    /// final look, including the provider badge. This is the only stage
    /// buffer tests that never navigate the Home grid ever
    /// observe, so it must stay identical to the pre-Etapa-6 unconditional
    /// rendering.
    Full,
}

/// Pure decision of which [`RevealStage`] applies `elapsed_ms` after the
/// selection changed. Kept free of `Instant`/`App` so it's directly
/// testable with explicit elapsed values — `Instant` itself can't be
/// mocked. `reduced_motion` always short-circuits to `Full`: reduced motion
/// skips the staged reveal entirely rather than slowing it down.
pub(super) fn reveal_stage(elapsed_ms: u128, reduced_motion: bool) -> RevealStage {
    if reduced_motion {
        return RevealStage::Full;
    }
    if elapsed_ms < 80 {
        RevealStage::Background
    } else if elapsed_ms < 160 {
        RevealStage::Title
    } else {
        RevealStage::Full
    }
}

/// Resolves the current [`RevealStage`] from live `App` state. A selection
/// that never moved has no reveal to play, so it resolves to `Full`.
fn current_reveal_stage(app: &App) -> RevealStage {
    let Some(elapsed_ms) = app.ui.anim.since_selection_ms() else {
        return RevealStage::Full;
    };
    reveal_stage(elapsed_ms, app.ui.anim.reduced_motion())
}

/// One rounded card: title (bold), an optional subtitle (dropped in
/// "compact" density), and a footer with the item's type glyph and duration.
/// Every card has a surface and frame so the grid reads as a collection of
/// objects instead of a text table. The selected card gets the accent border
/// and `theme.selected_card` background; its provider badge is integrated
/// into the lower frame and phases in over `reveal` (see [`RevealStage`]).
fn draw_card(
    f: &mut Frame,
    area: Rect,
    card: &HomeCard,
    selected: bool,
    theme: ThemeColors,
    density: HomeDensity,
    reveal: RevealStage,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let card_bg = if selected {
        theme.selected_card
    } else {
        theme.surface
    };
    let border_style = if selected {
        Style::default()
            .fg(theme.accent)
            .bg(card_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border).bg(card_bg)
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::default().bg(card_bg));
    if selected && reveal == RevealStage::Full {
        let badge_width = (area.width as usize).saturating_sub(4);
        let badge = crate::ui::take_width(&format!(" ◆ {} ", card.provider), badge_width);
        block = block.title_bottom(
            Line::from(Span::styled(
                badge,
                Style::default()
                    .fg(theme.provider_badge)
                    .bg(card_bg)
                    .add_modifier(Modifier::BOLD),
            ))
            .right_aligned(),
        );
    }
    let inner = block.inner(area);
    f.render_widget(block, area);

    // One leading content column inside the frame, matching the rest of the
    // file's rows without crowding the left border.
    let inner_width = (inner.width as usize).saturating_sub(1);

    let title = crate::ui::truncate_chars(&card.title, inner_width);
    let subtitle = crate::ui::truncate_chars(&card.subtitle, inner_width);

    let glyph = match card.kind {
        HomeCardKind::Track => "♪",
        HomeCardKind::Album => "▤",
        HomeCardKind::Playlist => "♫",
    };
    let duration_part = if card.duration.is_empty() {
        String::new()
    } else {
        format!(" {}", card.duration)
    };
    let footer_main = crate::ui::truncate_chars(&format!("{glyph}{duration_part}"), inner_width);

    // The title only picks up the accent color from the `Title` stage
    // onward — during `Background` it still reads as a normal card, just
    // with the selection background already showing.
    let title_accented = selected && reveal != RevealStage::Background;
    let title_fg = if title_accented {
        theme.accent
    } else {
        theme.text
    };
    let mut lines = vec![Line::from(Span::styled(
        format!(" {title}"),
        Style::default().fg(title_fg).add_modifier(Modifier::BOLD),
    ))];
    // "compact" density drops the subtitle row entirely (2-line card:
    // título + rodapé) instead of just leaving it blank, so the shelf's
    // vertical footprint actually shrinks (see `card_height`).
    if density == HomeDensity::Comfortable {
        lines.push(Line::from(Span::styled(
            format!(" {subtitle}"),
            Style::default().fg(theme.muted),
        )));
    }

    let footer_spans = vec![Span::styled(
        format!(" {footer_main}"),
        Style::default().fg(theme.muted),
    )];
    lines.push(Line::from(footer_spans));

    // The selected background still appears from the first reveal stage;
    // non-selected cards keep the quieter theme surface.
    let paragraph = Paragraph::new(lines).style(Style::default().bg(card_bg));
    f.render_widget(paragraph, inner);
}

/// Section header row: accent title followed by a dim rule to the edge,
/// indented to line up with the item rows below it.
fn section_header(title: &str, width: usize, theme: ThemeColors) -> Line<'static> {
    let rule = "─".repeat(width.saturating_sub(crate::ui::display_width(title) + 4));
    Line::from(vec![
        Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(rule, Style::default().fg(theme.border)),
    ])
}

/// Track title + real-time spectrum bars, or a placeholder when nothing is
/// loaded. Paused playback keeps rendering bars — they just settle toward
/// zero via `SpectrumAnalyzer::decay_idle`, so there's no separate branch.
fn draw_player_panel(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    if app.current.is_none() {
        let msg = Paragraph::new("Nothing playing — pick something below.")
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    draw_panel_title(f, app, rows[0], theme);
    // "off" draws only the title row above — no bar glyphs at all — leaving
    // the rest of the panel blank instead of splitting it further.
    if app.visualizer_style != VisualizerStyle::Off {
        draw_bars(
            f,
            app.visualizer.bars(),
            app.visualizer.peaks(),
            rows[1],
            theme,
            app.visualizer_style,
        );
    }
}

/// Compact "▶ Title — Artist" line above the bars.
fn draw_panel_title(f: &mut Frame, app: &App, area: Rect, theme: ThemeColors) {
    let Some(track) = &app.current else { return };
    let glyph = if app.player.is_paused() { "⏸" } else { "▶" };
    let text = format!("{glyph} {} — {}", track.title, track.artist);
    let shown = crate::ui::truncate_chars(&text, area.width as usize);
    let style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(shown, style)), area);
}

/// Cava-style bars: one column per entry in `bars`, each a stack of Unicode
/// eighth-block glyphs sized to that bar's smoothed height, plus a slowly
/// falling "peak cap" marking each bar's recent maximum (Winamp-style).
/// In `Gradient` style, cells are colored by their own height in the panel —
/// quiet bars stay in the player color, tall bars grade through secondary
/// into accent — so every loud bar reads as a vertical gradient. `Mono`
/// keeps every filled cell in `theme.player` instead, no matter the row;
/// peak caps are unaffected either way. Never called with `Off` — the
/// caller (`draw_player_panel`) skips this function entirely in that case.
// Precomputed "glyph + trailing space" static slices: with the Home screen's
// fast (~60ms) redraw tier, this function runs often, so each cell is a
// `&'static str` lookup rather than a fresh `format!()` allocation.
const BAR_GLYPHS: [&str; 9] = ["  ", "▁ ", "▂ ", "▃ ", "▄ ", "▅ ", "▆ ", "▇ ", "█ "];
/// Glyph do peak cap (linha fina no alto da célula onde o pico está).
const PEAK_GLYPH: &str = "▔ ";

pub(super) fn draw_bars(
    f: &mut Frame,
    bars: &[f32],
    peaks: &[f32],
    area: Rect,
    theme: ThemeColors,
    style: VisualizerStyle,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let max_bars = (area.width / 2).max(1) as usize;
    let visible = bars.len().min(max_bars);
    let rows = area.height;

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(rows as usize);
    for row in 0..rows {
        let row_from_bottom = (rows - 1 - row) as f32;
        // Fração de altura desta linha no painel: gradiente por célula
        // (ignorada em `Mono`, que sempre usa `theme.player`).
        let fraction = (row_from_bottom + 1.0) / rows as f32;
        let color = if style == VisualizerStyle::Mono {
            theme.player
        } else if fraction > 0.66 {
            theme.accent
        } else if fraction > 0.33 {
            theme.secondary
        } else {
            theme.player
        };
        let mut spans = Vec::with_capacity(visible);
        for (&height, &peak) in bars[..visible].iter().zip(&peaks[..visible]) {
            let filled_rows = height * rows as f32;
            let glyph = if row_from_bottom + 1.0 <= filled_rows {
                BAR_GLYPHS[8]
            } else if row_from_bottom < filled_rows {
                let idx = ((filled_rows - row_from_bottom) * 8.0) as usize;
                BAR_GLYPHS[idx.min(8)]
            } else {
                BAR_GLYPHS[0]
            };
            // O cap só aparece em célula vazia acima da barra; quando o
            // pico coincide com o topo da barra, a própria barra o mostra.
            if glyph == BAR_GLYPHS[0] && peak > 0.04 {
                let peak_row = ((peak * rows as f32).ceil() - 1.0).max(0.0);
                if (peak_row - row_from_bottom).abs() < f32::EPSILON {
                    spans.push(Span::styled(PEAK_GLYPH, Style::default().fg(theme.text)));
                    continue;
                }
            }
            spans.push(Span::styled(glyph, Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_library(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    if !app.is_authenticated() {
        let msg = Paragraph::new(
            "You are not signed in.\n\n\
             Press g to sign in — ytmtui imports the session from your\n\
             browser (Brave, Chrome, Chromium, Edge, Vivaldi, Opera or\n\
             Firefox). Sign in to music.youtube.com there first.\n\n\
             Manual alternative: save a Netscape cookie file to\n\
             ~/.config/ytmtui/cookies.txt",
        )
        .style(Style::default().fg(theme.muted))
        .block(block)
        .wrap(Wrap { trim: false });
        f.render_widget(msg, area);
        return;
    }
    if app.library.is_empty() {
        let text = if app.busy() {
            format!("{} Loading your library…", app.spinner())
        } else {
            "No playlists in your library.".to_string()
        };
        draw_empty_state(f, area, block, "♪", &text, theme);
        return;
    }
    let items: Vec<ListItem> = app
        .library
        .iter()
        .map(|p| ListItem::new(entry_line("♪", &p.title, &p.subtitle, theme)))
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_artists(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    if app.artists.is_empty() {
        draw_empty_state(
            f,
            area,
            block,
            "◆",
            "No artists yet. Search to find some.",
            theme,
        );
        return;
    }
    let items: Vec<ListItem> = app
        .artists
        .iter()
        .map(|a| ListItem::new(entry_line("◆", &a.name, &a.subtitle, theme)))
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_lyrics(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    match &app.lyrics {
        crate::lyrics::LyricsState::Synced { lines, active } => {
            draw_synced_lyrics(f, app, area, block, lines, *active)
        }
        crate::lyrics::LyricsState::Plain(text) => draw_plain_lyrics(f, app, area, block, text),
        crate::lyrics::LyricsState::NotAvailable => {
            draw_empty_state(
                f,
                area,
                block,
                "¶",
                "Lyrics are not available for this track.",
                theme,
            );
        }
        crate::lyrics::LyricsState::None => {
            let text = if app.current.is_some() {
                "Fetching lyrics…"
            } else {
                "Play a track to see its lyrics."
            };
            draw_empty_state(f, area, block, "¶", text, theme);
        }
    }
}

/// Plain-text lyrics (Musixmatch fallback, no timestamps): manual scroll via
/// `app.ui.lyrics.scroll`, exactly as before this section supported synced
/// lyrics.
fn draw_plain_lyrics(f: &mut Frame, app: &App, area: Rect, block: Block, text: &str) {
    let p = Paragraph::new(text.to_string())
        .style(Style::default().fg(app.theme().subtext))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.ui.lyrics.scroll, 0));
    f.render_widget(p, area);
}

/// Karaoke-style synced lyrics, centered like a stage: the active line gets
/// a per-character wipe (sung part in accent, rest bright) driven by the
/// playback position within the line's [start_ms, end_ms] window, and the
/// surrounding lines fade with distance — a spotlight around the moment.
///
/// The view centers on whichever line has focus: the one being sung while
/// auto-follow is on, or the user's cursor while they browse. Centering is
/// approximate — a logical line that wraps to 2+ terminal rows throws it
/// off, which is an acceptable tradeoff.
fn draw_synced_lyrics(
    f: &mut Frame,
    app: &App,
    area: Rect,
    block: Block,
    lines: &[crate::models::LyricLine],
    active: Option<usize>,
) {
    let theme = app.theme();
    let position_ms = app
        .ui
        .lyrics
        .corrected_position_ms(app.player.position().as_millis() as u64);
    let focused = app.ui.lyrics.focused_line(active);
    let browsing = !app.ui.lyrics.following();

    let rendered: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            // While browsing, the cursor gets the highlight instead of the
            // line being sung: the user is reading ahead, so the spotlight
            // follows their eye, not the audio.
            if browsing && focused == Some(i) {
                return Line::from(Span::styled(
                    l.text.clone(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            }
            match active {
                Some(a) if i == a => karaoke_line_entering(
                    l,
                    position_ms,
                    theme,
                    app.ui.anim.reduced_motion(),
                    app.ui.lyrics.line_entry_progress(LINE_ENTRY_MS),
                ),
                Some(a) => {
                    let color = match a.abs_diff(i) {
                        1 => theme.subtext,
                        2 => theme.muted,
                        _ => theme.border,
                    };
                    Line::from(Span::styled(l.text.clone(), Style::default().fg(color)))
                }
                // Before the first line starts, everything waits at equal
                // volume.
                None => Line::from(Span::styled(
                    l.text.clone(),
                    Style::default().fg(theme.subtext),
                )),
            }
        })
        .collect();

    let visible_rows = area.height.saturating_sub(2) as usize;
    let scroll = focused
        .map(|i| {
            let half = visible_rows / 2;
            i.saturating_sub(half)
                .min(lines.len().saturating_sub(visible_rows.max(1)))
        })
        .unwrap_or(0) as u16;

    let p = Paragraph::new(rendered)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(p, area);
}

/// The active lyric line with its karaoke wipe: characters whose share of
/// the line's time window has already elapsed are sung (accent), the rest
/// waits in bright text. Both halves stay bold so the active line pops from
/// its dimmer neighbors even at the very start of the window.
///
/// Under `reduced_motion` the per-character wipe — a continuous animation —
/// is skipped entirely: the whole line renders already in the "sung" style,
/// the same economy trade-off `App::needs_fast_animation` makes for this
/// same driver (falls back to the 200ms redraw tier instead of 60ms).
pub(super) fn karaoke_line(
    l: &crate::models::LyricLine,
    position_ms: u64,
    theme: ThemeColors,
    reduced_motion: bool,
) -> Line<'static> {
    karaoke_line_entering(l, position_ms, theme, reduced_motion, 1.0)
}

/// Milliseconds a newly sung line takes to brighten to its full style.
pub(super) const LINE_ENTRY_MS: u128 = 180;

/// [`karaoke_line`] with control over the entry fade: `entry` is `0.0` the
/// instant the line becomes the sung one and `1.0` once it has settled.
///
/// The fade applies only to the *unsung* half. Fading the sung half too
/// would fight the wipe, which is already drawing attention across the line
/// from the left — two animations describing the same moment in different
/// ways read as a glitch.
pub(super) fn karaoke_line_entering(
    l: &crate::models::LyricLine,
    position_ms: u64,
    theme: ThemeColors,
    reduced_motion: bool,
    entry: f32,
) -> Line<'static> {
    if reduced_motion {
        return Line::from(Span::styled(
            l.text.clone(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let window = l.end_ms.saturating_sub(l.start_ms).max(1);
    let fraction = (position_ms.saturating_sub(l.start_ms) as f64 / window as f64).clamp(0.0, 1.0);
    let width = crate::ui::display_width(&l.text);
    let sung_cols = (fraction * width as f64).round() as usize;
    let sung = crate::ui::take_width(&l.text, sung_cols);
    let rest = l.text[sung.len()..].to_string();
    Line::from(vec![
        Span::styled(
            sung,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            rest,
            Style::default()
                .fg(crate::theme::mix(theme.subtext, theme.text, entry))
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn draw_help(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let theme = app.theme();
    let rows = [
        ("Navigation", ""),
        ("  ↑/↓  or  k/j", "move selection"),
        ("  PgUp/PgDn", "jump 10 items; Home/End first/last"),
        ("  mouse wheel", "scroll the list"),
        ("  1..9, 0", "jump straight to a section"),
        ("  ←/→  or  h/l", "switch between menu and list"),
        ("  Tab", "toggle focus menu/list"),
        ("  Enter", "play / open playlist / open artist"),
        ("  a", "add track to the queue"),
        ("", ""),
        ("Queue", ""),
        ("  d / Delete", "remove the selected track"),
        ("  Shift+J/K", "move the selected track down / up"),
        ("  c", "clear the queue (keeps what's playing)"),
        ("", ""),
        ("Search", ""),
        ("  /", "open the search input"),
        ("  Esc", "cancel the search"),
        ("", ""),
        ("Account / Library", ""),
        ("  g", "sign in (imports cookies from your browser)"),
        ("  Library", "playlists from your signed-in account"),
        ("  cookies.txt", "in ~/.config/ytmtui/ (manual alternative)"),
        ("", ""),
        ("Playback", ""),
        ("  Space", "play / pause"),
        ("  n / p", "next / previous"),
        ("  [ / ]", "seek back / forward 5s"),
        ("  s", "stop"),
        ("  + / -", "volume"),
        ("  z", "toggle shuffle"),
        ("  r", "repeat mode (off/all/one)"),
        ("  f", "like / unlike the current track"),
        ("", ""),
        ("Lyrics", ""),
        ("  ↑/↓", "browse lines (pauses auto-follow)"),
        ("  Enter", "jump playback to the selected line"),
        ("  Home", "resume following the song"),
        ("  < / >", "nudge lyric timing by 0.25s"),
        ("  w", "open Now Playing (big cover, lyric line)"),
        ("", ""),
        ("Appearance", ""),
        ("  ,", "open Settings (edit everything in place)"),
        ("  t", "cycle the color theme"),
        ("", ""),
        ("General", ""),
        ("  ?", "this help"),
        ("  R", "refresh Home and Library"),
        ("  q  or  Ctrl+C", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                Line::from(Span::styled(
                    k.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{k:<18}"),
                        Style::default()
                            .fg(theme.secondary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(v.to_string(), Style::default().fg(theme.subtext)),
                ])
            }
        })
        .collect();
    // A lista de atalhos é mais alta que terminais baixos; j/k/roda rolam.
    // Clampa aqui, onde a altura real do painel é conhecida, para a rolagem
    // parar na última linha em vez de sumir com o texto.
    let visible = area.height.saturating_sub(2); // bordas do block
    let max_scroll = (lines.len() as u16).saturating_sub(visible);
    let scroll = app.ui.help_scroll.min(max_scroll);
    f.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

/// Milliseconds each row waits behind the one above it when a section opens.
const STAGGER_STEP_MS: u128 = 14;
/// Rows past this point appear together. Without a cap, opening a long list
/// would take visibly longer than opening a short one, which reads as the
/// app being slow rather than as an animation.
const STAGGER_MAX_ROWS: usize = 8;

/// How many rows have arrived `elapsed_ms` into a section's reveal.
///
/// Pure, so the sequence is testable at exact instants. `None` (the section
/// never changed) means every row is already in place.
pub(super) fn stagger_rows(elapsed_ms: Option<u128>, reduced_motion: bool) -> Option<usize> {
    if reduced_motion {
        return None;
    }
    let elapsed_ms = elapsed_ms?;
    let arrived = (elapsed_ms / STAGGER_STEP_MS) as usize;
    (arrived < STAGGER_MAX_ROWS).then_some(arrived)
}

/// Blanks the rows that have not arrived yet, keeping the vector's length so
/// the selection index and scrollbar geometry stay exactly as they would be
/// without the animation — only the content appears progressively.
fn stagger<'a>(app: &App, items: Vec<ListItem<'a>>) -> Vec<ListItem<'a>> {
    let Some(arrived) = stagger_rows(
        app.ui.anim.since_section_change_ms(),
        app.ui.anim.reduced_motion(),
    ) else {
        return items;
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| if i < arrived { item } else { ListItem::new("") })
        .collect()
}

fn render_list(f: &mut Frame, app: &App, area: Rect, block: Block, items: Vec<ListItem>) {
    let theme = app.theme();
    let item_count = items.len();
    let list = List::new(stagger(app, items))
        .block(block)
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    let mut state = app.list_state.clone();
    f.render_stateful_widget(list, area, &mut state);

    // The scrollbar only appears when the list actually overflows the panel.
    let viewport_rows = area.height.saturating_sub(2) as usize;
    if item_count > viewport_rows && area.height > 2 {
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::default()
            .content_length(item_count)
            .viewport_content_length(viewport_rows);
        if let Some(selected) = state.selected() {
            scrollbar_state = scrollbar_state.position(selected);
        }

        let scrollbar = scrollbar_widget(theme);

        // inner margin to avoid overwriting the block border
        let scroll_area = area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 0,
        });
        f.render_stateful_widget(scrollbar, scroll_area, &mut scrollbar_state);
    }
}

/// Shared scrollbar look: dim track and arrows, muted thumb, so it recedes
/// behind the content while staying findable.
fn scrollbar_widget(theme: ThemeColors) -> ratatui::widgets::Scrollbar<'static> {
    ratatui::widgets::Scrollbar::default()
        .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("▲"))
        .end_symbol(Some("▼"))
        .thumb_symbol("█")
        .track_symbol(Some("│"))
        .begin_style(Style::default().fg(theme.border))
        .end_style(Style::default().fg(theme.border))
        .track_style(Style::default().fg(theme.border))
        .thumb_style(Style::default().fg(theme.muted))
}

/// Same as `render_list`, but for an area with no border of its own (used
/// only by the Home player panel's list, where the border already belongs
/// to the whole outer Home area). Kept separate rather than parameterizing
/// `render_list` with a has-border flag, since that helper is shared by five
/// other bordered call sites and this avoids touching their geometry math.
fn render_list_borderless(
    f: &mut Frame,
    app: &App,
    area: Rect,
    items: Vec<ListItem>,
    list_state: &ListState,
) {
    let theme = app.theme();
    let item_count = items.len();
    let list = List::new(stagger(app, items))
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    let mut state = list_state.clone();
    f.render_stateful_widget(list, area, &mut state);

    let viewport_rows = area.height as usize;
    if item_count > viewport_rows && area.height > 0 {
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::default()
            .content_length(item_count)
            .viewport_content_length(viewport_rows);
        if let Some(selected) = state.selected() {
            scrollbar_state = scrollbar_state.position(selected);
        }

        let scrollbar = scrollbar_widget(theme);
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}
