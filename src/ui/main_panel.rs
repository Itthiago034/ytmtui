//! Main content panel: tracks, playlists, artists, queue, lyrics or help.
//! This is the only panel that keeps a rounded border and a scrollbar; both
//! aid orientation in long lists.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Section};

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
    let main: String = main.chars().take(avail).collect();
    let pad = " ".repeat(avail.saturating_sub(main.chars().count()) + 2);

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

fn draw_home(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.home.is_empty() {
        let text = if app.busy {
            format!("{} Loading recommendations…", app.spinner())
        } else if app.is_authenticated() {
            "No recommendations are available. Press / to search.".to_string()
        } else {
            "Sign in to see recommendations. Press ? for instructions.".to_string()
        };
        let msg = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let items: Vec<ListItem> = app
        .home
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
    let text = match &app.lyrics {
        Some(l) if !l.is_empty() => l.clone(),
        Some(_) => "Lyrics are not available for this track.".to_string(),
        None => {
            if app.current.is_some() {
                "Fetching lyrics…".to_string()
            } else {
                "Play a track to see its lyrics.".to_string()
            }
        }
    };
    let p = Paragraph::new(text)
        .style(Style::default().fg(Color::Gray))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.lyrics_scroll, 0));
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
