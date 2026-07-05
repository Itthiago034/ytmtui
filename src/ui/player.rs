//! Painel inferior do player: capa, faixa atual, barra de progresso e volume.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ascii_art;

/// Formata segundos como "m:ss".
fn fmt(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Desenha o painel do player.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let player_color = app.theme().player;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(player_color))
        .title(Span::styled(
            " ▶ Player ",
            Style::default()
                .fg(player_color)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Divide em: capa (esquerda) + informações (direita).
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(14), Constraint::Min(0)])
        .split(inner);

    draw_artwork(f, app, cols[0]);
    draw_info(f, app, cols[1]);
}

/// Desenha a capa (arte em meio-blocos) com cache por tamanho.
fn draw_artwork(f: &mut Frame, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (w, h) = (area.width, area.height);
    let accent = app.theme().accent;

    let lines: Vec<Line<'static>> = if let Some(bytes) = &app.artwork_bytes {
        // Usa o cache se o tamanho não mudou.
        let cached = app
            .artwork_cache
            .as_ref()
            .filter(|(cw, ch, _)| *cw == w && *ch == h)
            .map(|(_, _, l)| l.clone());
        match cached {
            Some(l) => l,
            None => {
                let l = ascii_art::image_to_lines(bytes, w, h)
                    .unwrap_or_else(|| ascii_art::placeholder(h, accent));
                app.artwork_cache = Some((w, h, l.clone()));
                l
            }
        }
    } else {
        ascii_art::placeholder(h, accent)
    };

    f.render_widget(Paragraph::new(lines), area);
}

/// Desenha título, artista, barra de progresso e volume.
fn draw_info(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // título
            Constraint::Length(1), // artista / álbum
            Constraint::Length(1), // progresso
            Constraint::Length(1), // volume + estado
            Constraint::Min(0),    // status / atalhos
        ])
        .split(area);

    let (title, artist, album, dur_secs) = match &app.current {
        Some(t) => (
            t.title.clone(),
            t.artist.clone(),
            t.album.clone(),
            t.duration_secs,
        ),
        None => (
            "Nada tocando".to_string(),
            "—".to_string(),
            String::new(),
            0,
        ),
    };

    // Título (com coração se a faixa atual estiver curtida).
    let liked = app
        .current
        .as_ref()
        .map(|t| app.liked.contains(&t.video_id))
        .unwrap_or(false);
    let mut title_spans = vec![Span::styled(
        title,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )];
    if liked {
        title_spans.push(Span::styled("  💚", Style::default().fg(theme.player)));
    }
    f.render_widget(Paragraph::new(Line::from(title_spans)), layout[0]);

    // Artista • Álbum.
    let sub = if album.is_empty() {
        artist
    } else {
        format!("{artist}  •  {album}")
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sub,
            Style::default().fg(theme.secondary),
        ))),
        layout[1],
    );

    // Barra de progresso.
    let pos = app.player.position().as_secs();
    let ratio = if dur_secs > 0 {
        (pos as f64 / dur_secs as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = if dur_secs > 0 {
        format!("{} / {}", fmt(pos), fmt(dur_secs))
    } else if app.current.is_some() {
        fmt(pos)
    } else {
        "--:-- / --:--".to_string()
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(theme.player).bg(Color::Rgb(30, 30, 30)))
        .ratio(ratio)
        .use_unicode(true)
        .label(Span::styled(label, Style::default().fg(Color::White)));
    f.render_widget(gauge, layout[2]);

    // Estado + volume.
    let state_icon = if app.loading_audio {
        Span::styled("⏳ carregando", Style::default().fg(Color::Yellow))
    } else if app.current.is_none() {
        Span::styled("⏹ parado", Style::default().fg(Color::DarkGray))
    } else if app.player.is_paused() {
        Span::styled("⏸ pausado", Style::default().fg(Color::Yellow))
    } else {
        Span::styled("▶ tocando", Style::default().fg(theme.player))
    };
    let vol = (app.player.volume() * 100.0).round() as u32;
    let vol_blocks = (app.player.volume() * 10.0).round() as usize;
    let vol_bar = format!(
        "{}{}{}",
        "━".repeat(vol_blocks),
        "●",
        "─".repeat(10usize.saturating_sub(vol_blocks))
    );

    // Indicadores de shuffle e repeat.
    let shuffle_style = if app.shuffle {
        Style::default()
            .fg(theme.player)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let repeat_style = if app.repeat != crate::app::RepeatMode::Off {
        Style::default()
            .fg(theme.player)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            state_icon,
            Span::raw("    "),
            Span::styled("🔊 ", Style::default().fg(Color::White)),
            Span::styled(vol_bar, Style::default().fg(theme.player)),
            Span::styled(format!(" {vol}%"), Style::default().fg(Color::Gray)),
            Span::raw("    "),
            Span::styled("🔀", shuffle_style),
            Span::styled(if app.shuffle { " on" } else { " off" }, shuffle_style),
            Span::raw("  "),
            Span::styled("🔁", repeat_style),
            Span::styled(format!(" {}", app.repeat.label()), repeat_style),
            Span::raw("    "),
            Span::styled("🎨 ", Style::default().fg(theme.accent)),
            Span::styled(
                theme.name,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[3],
    );

    // Barra de status / atalhos.
    let help = "Espaço play/pause  n/p próx/ant  a fila  f curtir  z shuffle  r repeat  t tema  +/- vol  / buscar  ? ajuda  q sair";
    let mut status_spans = Vec::new();
    if app.is_loading() {
        status_spans.push(Span::styled(
            format!("{} ", app.spinner()),
            Style::default().fg(theme.player),
        ));
    }
    status_spans.push(Span::styled(
        app.status.clone(),
        Style::default().fg(Color::Yellow),
    ));
    let status_line = Line::from(status_spans);
    let mut lines = vec![status_line];
    lines.push(Line::from(Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(Paragraph::new(lines), layout[4]);
}
