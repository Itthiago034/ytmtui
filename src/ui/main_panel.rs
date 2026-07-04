//! Painel principal: exibe músicas, playlists, artistas, fila, letra ou ajuda.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
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
        Section::Inicio => "Início · recomendados".to_string(),
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
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));

    match app.section {
        Section::Inicio => draw_home(f, app, area, block),
        Section::Buscar => draw_songs(f, app, area, block, &app.songs.clone()),
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
        let msg = Paragraph::new("Nenhuma música. Use '/' para buscar.")
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
        let msg = Paragraph::new("A fila está vazia. Toque uma música para preenchê-la.")
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
        let msg = Paragraph::new("Nenhuma playlist. Busque algo para ver playlists.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let accent = app.theme().accent;
    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled("🎵 ", Style::default().fg(accent)),
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
            format!("{} Carregando recomendações...", app.spinner())
        } else if app.logged_in {
            "Sem recomendações no momento. Pressione '/' para buscar.".to_string()
        } else {
            "Faça login para ver recomendações. Pressione '?' para instruções.".to_string()
        };
        let msg = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let accent = app.theme().accent;
    let items: Vec<ListItem> = app
        .home
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled("★ ", Style::default().fg(accent)),
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
    if !app.logged_in {
        let msg = Paragraph::new(
            "Você não está conectado.\n\n\
             Para ver suas playlists, salve os cookies do YouTube Music \
             (formato Netscape) em:\n\n\
             ~/.config/ytmtui/cookies.txt\n\n\
             O app detecta o arquivo automaticamente na próxima vez que abrir.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(block)
        .wrap(Wrap { trim: false });
        f.render_widget(msg, area);
        return;
    }
    if app.library.is_empty() {
        let text = if app.busy {
            format!("{} Carregando sua biblioteca...", app.spinner())
        } else {
            "Nenhuma playlist na biblioteca.".to_string()
        };
        let msg = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let accent = app.theme().accent;
    let items: Vec<ListItem> = app
        .library
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled("📚 ", Style::default().fg(accent)),
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
        let msg = Paragraph::new("Nenhum artista. Busque algo para ver artistas.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
        return;
    }
    let secondary = app.theme().secondary;
    let items: Vec<ListItem> = app
        .artists
        .iter()
        .map(|a| {
            ListItem::new(Line::from(vec![
                Span::styled("👤 ", Style::default().fg(secondary)),
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

fn draw_help(f: &mut Frame, area: Rect, block: Block, accent: Color, secondary: Color) {
    let rows = [
        ("Navegação", ""),
        ("  ↑/↓  ou  k/j", "mover seleção"),
        ("  ←/→  ou  h/l", "alternar entre menu e lista"),
        ("  Tab", "alternar foco menu/lista"),
        ("  Enter", "tocar / abrir playlist / abrir artista"),
        ("  a", "adicionar faixa à fila"),
        ("", ""),
        ("Busca", ""),
        ("  /", "abrir campo de busca"),
        ("  Esc", "cancelar busca"),
        ("", ""),
        ("Conta / Biblioteca", ""),
        ("  📚 Biblioteca", "suas playlists da conta conectada"),
        ("  cookies.txt", "em ~/.config/ytmtui/ (login automático)"),
        ("", ""),
        ("Reprodução", ""),
        ("  Espaço", "play / pause"),
        ("  n / p", "próxima / anterior"),
        ("  [ / ]", "retroceder / avançar 5s"),
        ("  s", "parar"),
        ("  + / -", "volume"),
        ("  z", "alternar aleatório (shuffle)"),
        ("  r", "modo de repetição (off/todos/1)"),
        ("  f", "curtir / descurtir a faixa atual"),
        ("", ""),
        ("Aparência", ""),
        ("  t", "trocar tema de cores"),
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

/// Renderiza uma lista com estado (seleção destacada).
fn render_list(f: &mut Frame, app: &App, area: Rect, block: Block, items: Vec<ListItem>) {
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
}
