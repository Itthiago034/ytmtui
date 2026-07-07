//! Main content panel: tracks, playlists, artists, queue, lyrics or help.
//! This is the only panel that keeps a rounded border and a scrollbar; both
//! aid orientation in long lists.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Section};
use crate::theme::Theme;

/// Desenha o conteúdo do painel principal de acordo com a seção ativa.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let focused = app.focus == Focus::Main;
    let border_color = if focused {
        theme.accent
    } else {
        Color::DarkGray
    };

    let title = match app.section {
        Section::Inicio => "Home".to_string(),
        Section::Buscar => app.songs_title.clone(),
        Section::Biblioteca => "Library".to_string(),
        Section::Playlists => "Playlists".to_string(),
        Section::Artistas => "Artists".to_string(),
        Section::Fila => "Queue".to_string(),
        Section::Letra => "Lyrics".to_string(),
        Section::Ajuda => "Help".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));

    match app.section {
        Section::Inicio => draw_home(f, app, area, block),
        Section::Buscar => draw_songs(f, app, area, block, &app.songs),
        Section::Fila => draw_queue(f, app, area, block),
        Section::Biblioteca => draw_library(f, app, area, block),
        Section::Playlists => draw_playlists(f, app, area, block),
        Section::Artistas => draw_artists(f, app, area, block),
        Section::Letra => draw_lyrics(f, app, area, block),
        Section::Ajuda => draw_help(f, area, block, app.theme().accent, app.theme().secondary),
    }
}

/// Formata uma linha de faixa: "01  Título  —  Artista        3:45".
fn track_line(
    index: usize,
    t: &crate::ytmusic::Track,
    width: usize,
    playing: bool,
    accent: Color,
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
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Line::from(vec![
        Span::styled(if playing { "▶ " } else { "  " }, marker_style),
        Span::styled(num, Style::default().fg(Color::DarkGray)),
        Span::styled(
            main,
            Style::default().fg(if playing { accent } else { Color::White }),
        ),
        Span::raw(pad),
        Span::styled(dur, Style::default().fg(Color::DarkGray)),
    ])
}

fn draw_songs(f: &mut Frame, app: &App, area: Rect, block: Block, songs: &[crate::ytmusic::Track]) {
    if songs.is_empty() {
        let msg = Paragraph::new("No tracks yet. Press / to search.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let width = area.width.saturating_sub(4) as usize;
    let accent = app.theme().player;
    let current_id = app.current.as_ref().map(|t| t.video_id.clone());
    let items: Vec<ListItem> = songs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = current_id.as_deref() == Some(t.video_id.as_str());
            ListItem::new(track_line(i, t, width, playing, accent))
        })
        .collect();

    render_list(f, app, area, block, items);
}

fn draw_queue(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.queue.is_empty() {
        let msg = Paragraph::new("The queue is empty. Play a track to fill it.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let width = area.width.saturating_sub(4) as usize;
    let accent = app.theme().player;
    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = app.queue_index == Some(i);
            ListItem::new(track_line(i, t, width, playing, accent))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_playlists(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.playlists.is_empty() {
        let msg = Paragraph::new("No playlists yet. Search to find some.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    p.title.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}", p.subtitle),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
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
fn draw_home(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < PLAYER_PANEL_HEIGHT + 3 {
        draw_home_sections(f, app, inner);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(PLAYER_PANEL_HEIGHT), Constraint::Min(1)])
        .split(inner);
    draw_player_panel(f, app, rows[0]);
    draw_home_sections(f, app, rows[1]);
}

/// The recommendations list (or its empty/loading message), without a
/// border of its own — the border now belongs to the whole Home area.
/// Sections are shown as non-selectable header rows interleaved with their
/// items in one flat scrollable list (v1 scope: full section contents, no
/// per-section cap/"show more" — overflow uses the existing scrollbar,
/// exactly like the old flat list did).
fn draw_home_sections(f: &mut Frame, app: &App, area: Rect) {
    if app.home.is_empty() {
        let text = if app.busy {
            format!("{} Loading recommendations…", app.spinner())
        } else if app.is_authenticated() {
            "No recommendations are available. Press / to search.".to_string()
        } else {
            "Sign in to see recommendations. Press ? for instructions.".to_string()
        };
        let msg = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, area);
        return;
    }

    let selected = app.list_state.selected();
    let mut items: Vec<ListItem> = Vec::new();
    let mut shadow_selected: Option<usize> = None;
    let mut flat_idx = 0usize;

    for section in &app.home {
        items.push(ListItem::new(Line::from(Span::styled(
            section.title.clone(),
            Style::default()
                .fg(app.theme().accent)
                .add_modifier(Modifier::BOLD),
        ))));
        for p in &section.items {
            if selected == Some(flat_idx) {
                shadow_selected = Some(items.len());
            }
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    p.title.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}", p.subtitle),
                    Style::default().fg(Color::DarkGray),
                ),
            ])));
            flat_idx += 1;
        }
    }

    // The real selection index (over selectable items only) doesn't match
    // this list's row index once section headers are interleaved in — a
    // "shadow" ListState remaps it to the right row before rendering.
    let mut shadow_state = app.list_state.clone();
    shadow_state.select(shadow_selected);
    render_list_borderless(f, app, area, items, &shadow_state);
}

/// Track title + real-time spectrum bars, or a placeholder when nothing is
/// loaded. Paused playback keeps rendering bars — they just settle toward
/// zero via `SpectrumAnalyzer::decay_idle`, so there's no separate branch.
fn draw_player_panel(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    if app.current.is_none() {
        let msg = Paragraph::new("Nothing playing — pick something below.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    draw_panel_title(f, app, rows[0], theme);
    draw_bars(f, app.visualizer.bars(), rows[1], theme);
}

/// Compact "▶ Title — Artist" line above the bars.
fn draw_panel_title(f: &mut Frame, app: &App, area: Rect, theme: &'static Theme) {
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
/// eighth-block glyphs sized to that bar's smoothed height. Colored by the
/// bar's own current height using the theme's existing palette (no new
/// gradient helper) so quiet bars read as calmer and loud ones as louder.
// Precomputed "glyph + trailing space" static slices: with the Home screen's
// fast (~60ms) redraw tier, this function runs often, so each cell is a
// `&'static str` lookup rather than a fresh `format!()` allocation.
const BAR_GLYPHS: [&str; 9] = ["  ", "▁ ", "▂ ", "▃ ", "▄ ", "▅ ", "▆ ", "▇ ", "█ "];

fn draw_bars(f: &mut Frame, bars: &[f32], area: Rect, theme: &'static Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let max_bars = (area.width / 2).max(1) as usize;
    let visible = bars.len().min(max_bars);
    let rows = area.height;

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(rows as usize);
    for row in 0..rows {
        let row_from_bottom = (rows - 1 - row) as f32;
        let mut spans = Vec::with_capacity(visible);
        for &height in &bars[..visible] {
            let filled_rows = height * rows as f32;
            let glyph = if row_from_bottom + 1.0 <= filled_rows {
                BAR_GLYPHS[8]
            } else if row_from_bottom < filled_rows {
                let idx = ((filled_rows - row_from_bottom) * 8.0) as usize;
                BAR_GLYPHS[idx.min(8)]
            } else {
                BAR_GLYPHS[0]
            };
            let color = if height > 0.66 {
                theme.accent
            } else if height > 0.33 {
                theme.secondary
            } else {
                theme.player
            };
            spans.push(Span::styled(glyph, Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_library(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if !app.is_authenticated() {
        let msg = Paragraph::new(
            "You are not signed in.\n\n\
             Save a Netscape cookie file to:\n\n\
             ~/.config/ytmtui/cookies.txt\n\n\
             Restart ytmtui after refreshing the file.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(block)
        .wrap(Wrap { trim: false });
        f.render_widget(msg, area);
        return;
    }
    if app.library.is_empty() {
        let text = if app.busy {
            format!("{} Loading your library…", app.spinner())
        } else {
            "No playlists in your library.".to_string()
        };
        let msg = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let items: Vec<ListItem> = app
        .library
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    p.title.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}", p.subtitle),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_artists(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.artists.is_empty() {
        let msg = Paragraph::new("No artists yet. Search to find some.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let items: Vec<ListItem> = app
        .artists
        .iter()
        .map(|a| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    a.name.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}", a.subtitle),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_lyrics(f: &mut Frame, app: &App, area: Rect, block: Block) {
    match &app.lyrics {
        crate::lyrics::LyricsState::Synced { lines, active } => {
            draw_synced_lyrics(f, app, area, block, lines, *active)
        }
        crate::lyrics::LyricsState::Plain(text) => draw_plain_lyrics(f, app, area, block, text),
        crate::lyrics::LyricsState::NotAvailable => {
            let p = Paragraph::new("Lyrics are not available for this track.")
                .style(Style::default().fg(Color::Gray))
                .block(block)
                .wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }
        crate::lyrics::LyricsState::None => {
            let text = if app.current.is_some() {
                "Fetching lyrics…"
            } else {
                "Play a track to see its lyrics."
            };
            let p = Paragraph::new(text)
                .style(Style::default().fg(Color::Gray))
                .block(block)
                .wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }
    }
}

/// Plain-text lyrics (Musixmatch fallback, no timestamps): manual scroll via
/// `app.lyrics_scroll`, exactly as before this section supported synced
/// lyrics.
fn draw_plain_lyrics(f: &mut Frame, app: &App, area: Rect, block: Block, text: &str) {
    let p = Paragraph::new(text.to_string())
        .style(Style::default().fg(Color::Gray))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.lyrics_scroll, 0));
    f.render_widget(p, area);
}

/// Karaoke-style synced lyrics: the active line is highlighted in the
/// theme's accent color, and the view auto-scrolls to keep it roughly
/// centered (approximate — a single logical line that wraps to 2+ terminal
/// rows will throw off exact centering, which is an acceptable v1 tradeoff).
fn draw_synced_lyrics(
    f: &mut Frame,
    app: &App,
    area: Rect,
    block: Block,
    lines: &[crate::ytmusic::LyricLine],
    active: Option<usize>,
) {
    let theme = app.theme();
    let rendered: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let style = if Some(i) == active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.secondary)
            };
            Line::from(Span::styled(l.text.clone(), style))
        })
        .collect();

    let visible_rows = area.height.saturating_sub(2) as usize;
    let scroll = active
        .map(|i| {
            let half = visible_rows / 2;
            i.saturating_sub(half)
                .min(lines.len().saturating_sub(visible_rows.max(1)))
        })
        .unwrap_or(0) as u16;

    let p = Paragraph::new(rendered)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(p, area);
}

fn draw_help(f: &mut Frame, area: Rect, block: Block, accent: Color, secondary: Color) {
    let rows = [
        ("Navigation", ""),
        ("  ↑/↓  or  k/j", "move selection"),
        ("  ←/→  or  h/l", "switch between menu and list"),
        ("  Tab", "toggle focus menu/list"),
        ("  Enter", "play / open playlist / open artist"),
        ("  a", "add track to the queue"),
        ("", ""),
        ("Search", ""),
        ("  /", "open the search input"),
        ("  Esc", "cancel the search"),
        ("", ""),
        ("Account / Library", ""),
        ("  Library", "playlists from your signed-in account"),
        ("  cookies.txt", "in ~/.config/ytmtui/ (automatic sign-in)"),
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
        ("Appearance", ""),
        ("  t", "cycle the color theme"),
        ("", ""),
        ("General", ""),
        ("  ?", "this help"),
        ("  q  or  Ctrl+C", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                Line::from(Span::styled(
                    k.to_string(),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(format!("{k:<18}"), Style::default().fg(secondary)),
                    Span::styled(v.to_string(), Style::default().fg(Color::Gray)),
                ])
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_list(f: &mut Frame, app: &App, area: Rect, block: Block, items: Vec<ListItem>) {
    let item_count = items.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(app.theme().highlight_bg)
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

        let scrollbar = ratatui::widgets::Scrollbar::default()
            .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .thumb_symbol("█")
            .track_symbol(Some("│"));

        // inner margin to avoid overwriting the block border
        let scroll_area = area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 0,
        });
        f.render_stateful_widget(scrollbar, scroll_area, &mut scrollbar_state);
    }
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
    let item_count = items.len();
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(app.theme().highlight_bg)
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

        let scrollbar = ratatui::widgets::Scrollbar::default()
            .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .thumb_symbol("█")
            .track_symbol(Some("│"));
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}
