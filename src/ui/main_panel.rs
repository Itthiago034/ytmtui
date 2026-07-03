//! Painel principal: exibe músicas, playlists, artistas, fila, letra ou ajuda.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Section};

/// Desenha o conteúdo do painel principal de acordo com a seção ativa.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Main;
    let border_color = if focused { Color::Cyan } else { Color::DarkGray };

    let title = match app.section {
        Section::Buscar => app.songs_title.clone(),
        Section::Biblioteca => "Minhas playlists".to_string(),
        Section::Playlists => "Playlists".to_string(),
        Section::Artistas => "Artistas".to_string(),
        Section::Fila => "Fila de reprodução".to_string(),
        Section::Letra => "Letra".to_string(),
        Section::Ajuda => "Ajuda".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));

    match app.section {
        Section::Buscar => draw_songs(f, app, area, block, &app.songs.clone()),
        Section::Fila => draw_queue(f, app, area, block),
        Section::Biblioteca => draw_library(f, app, area, block),
        Section::Playlists => draw_playlists(f, app, area, block),
        Section::Artistas => draw_artists(f, app, area, block),
        Section::Letra => draw_lyrics(f, app, area, block),
        Section::Ajuda => draw_help(f, area, block),
    }
}

/// Formata uma linha de faixa: "01  Título  —  Artista        3:45".
fn track_line(index: usize, t: &crate::ytmusic::Track, width: usize, playing: bool) -> Line<'static> {
    let num = format!("{:>2}  ", index + 1);
    let dur = if t.duration.is_empty() { String::new() } else { t.duration.clone() };
    // Espaço reservado para número + duração + margens.
    let avail = width.saturating_sub(num.len() + dur.len() + 6);
    let main = format!("{} — {}", t.title, t.artist);
    let main: String = main.chars().take(avail).collect();
    let pad = " ".repeat(avail.saturating_sub(main.chars().count()) + 2);

    let marker_style = if playing {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Line::from(vec![
        Span::styled(if playing { "▶ " } else { "  " }, marker_style),
        Span::styled(num, Style::default().fg(Color::DarkGray)),
        Span::styled(main, Style::default().fg(if playing { Color::Green } else { Color::White })),
        Span::raw(pad),
        Span::styled(dur, Style::default().fg(Color::DarkGray)),
    ])
}

fn draw_songs(f: &mut Frame, app: &App, area: Rect, block: Block, songs: &[crate::ytmusic::Track]) {
    if songs.is_empty() {
        let msg = Paragraph::new("Nenhuma música. Use '/' para buscar.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let width = area.width.saturating_sub(4) as usize;
    let current_id = app.current.as_ref().map(|t| t.video_id.clone());
    let items: Vec<ListItem> = songs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = current_id.as_deref() == Some(t.video_id.as_str());
            ListItem::new(track_line(i, t, width, playing))
        })
        .collect();

    render_list(f, app, area, block, items);
}

fn draw_queue(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.queue.is_empty() {
        let msg = Paragraph::new("A fila está vazia. Toque uma música para preenchê-la.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let playing = app.queue_index == Some(i);
            ListItem::new(track_line(i, t, width, playing))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_playlists(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.playlists.is_empty() {
        let msg = Paragraph::new("Nenhuma playlist. Busque algo para ver playlists.")
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
                Span::styled("🎵 ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    p.title.clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  ·  {}", p.subtitle), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_library(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if !app.logged_in {
        let msg = Paragraph::new(
            "Você não está logado.\n\n\
             Para ver suas playlists, exporte os cookies do YouTube Music \
             (formato Netscape) e inicie o app com:\n\n\
             export YTM_COOKIES=\"/caminho/para/cookies.txt\"\n\
             ./target/release/ytmtui",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(block)
        .wrap(Wrap { trim: false });
        f.render_widget(msg, area);
        return;
    }
    if app.library.is_empty() {
        let msg = Paragraph::new("Carregando sua biblioteca...")
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
                Span::styled("📚 ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    p.title.clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  ·  {}", p.subtitle), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_artists(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if app.artists.is_empty() {
        let msg = Paragraph::new("Nenhum artista. Busque algo para ver artistas.")
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
                Span::styled("👤 ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    a.name.clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  ·  {}", a.subtitle), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    render_list(f, app, area, block, items);
}

fn draw_lyrics(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let text = match &app.lyrics {
        Some(l) if !l.is_empty() => l.clone(),
        Some(_) => "Letra indisponível para esta música.".to_string(),
        None => {
            if app.current.is_some() {
                "Buscando letra...".to_string()
            } else {
                "Toque uma música para ver a letra.".to_string()
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

fn draw_help(f: &mut Frame, area: Rect, block: Block) {
    let rows = [
        ("Navegação", ""),
        ("  ↑/↓  ou  k/j", "mover seleção"),
        ("  ←/→  ou  h/l", "alternar entre menu e lista"),
        ("  Tab", "alternar foco menu/lista"),
        ("  Enter", "tocar música / abrir playlist"),
        ("", ""),
        ("Busca", ""),
        ("  /", "abrir campo de busca"),
        ("  Esc", "cancelar busca"),
        ("", ""),
        ("Conta / Biblioteca", ""),
        ("  📚 Biblioteca", "suas playlists (requer login por cookies)"),
        ("  YTM_COOKIES", "variável com o caminho do cookies.txt"),
        ("", ""),
        ("Reprodução", ""),
        ("  Espaço", "play / pause"),
        ("  n / p", "próxima / anterior"),
        ("  [ / ]", "retroceder / avançar 5s"),
        ("  s", "parar"),
        ("  + / -", "volume"),
        ("  z", "alternar aleatório (shuffle)"),
        ("  r", "modo de repetição (off/todos/1)"),
        ("", ""),
        ("Geral", ""),
        ("  ?", "esta ajuda"),
        ("  q  ou  Ctrl+C", "sair"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                Line::from(Span::styled(
                    k.to_string(),
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(format!("{k:<18}"), Style::default().fg(Color::Cyan)),
                    Span::styled(v.to_string(), Style::default().fg(Color::Gray)),
                ])
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Renderiza uma lista com estado (seleção destacada).
fn render_list(f: &mut Frame, app: &App, area: Rect, block: Block, items: Vec<ListItem>) {
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    let mut state = app.list_state.clone();
    f.render_stateful_widget(list, area, &mut state);
}
