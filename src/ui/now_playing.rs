//! Compact two-line playback summary: track line plus progress gauge.

use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph};
use ratatui::Frame;

use crate::app::{App, RepeatMode};

/// Formats seconds as "m:ss".
fn fmt(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Draws the playback summary.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    draw_track_line(f, app, rows[0]);
    if rows[1].height > 0 {
        draw_progress(f, app, rows[1]);
    }
}

/// Track line: state glyph, title and artist on the left; volume and active
/// playback modes on the right.
fn draw_track_line(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();

    let (glyph, glyph_style) = if app.loading_audio {
        (
            format!("{} ", app.spinner()),
            Style::default().fg(Color::Yellow),
        )
    } else if app.current.is_none() {
        ("⏹ ".to_string(), Style::default().fg(Color::DarkGray))
    } else if app.player.is_paused() {
        ("⏸ ".to_string(), Style::default().fg(Color::Yellow))
    } else {
        ("▶ ".to_string(), Style::default().fg(theme.player))
    };

    let (title, subtitle) = match &app.current {
        Some(t) if t.album.is_empty() => (t.title.clone(), format!(" — {}", t.artist)),
        Some(t) => (t.title.clone(), format!(" — {} · {}", t.artist, t.album)),
        None => ("Nothing playing".to_string(), String::new()),
    };
    let title_style = if app.current.is_some() {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let liked = app
        .current
        .as_ref()
        .map(|t| app.liked.contains(&t.video_id))
        .unwrap_or(false);

    // Right-hand indicators: volume slider and percentage always, playback
    // modes only when active.
    let volume = (app.player.volume() * 100.0).round() as u32;
    let filled = ((app.player.volume() * 10.0).round() as usize).min(10);
    let slider = format!("{}●{}", "━".repeat(filled), "─".repeat(10 - filled));
    let mut modes = format!(" {volume}%");
    if app.shuffle {
        modes.push_str(" · shuffle");
    }
    match app.repeat {
        RepeatMode::All => modes.push_str(" · repeat all"),
        RepeatMode::One => modes.push_str(" · repeat one"),
        RepeatMode::Off => {}
    }

    let width = area.width as usize;
    let fixed = 1 + glyph.chars().count() + if liked { 2 } else { 0 };
    let right = slider.chars().count() + modes.chars().count() + 2;
    let avail = width.saturating_sub(fixed + right);

    let title_shown = crate::ui::truncate_chars(&title, avail);
    let remaining = avail.saturating_sub(title_shown.chars().count());
    let subtitle_shown = if remaining >= 4 {
        crate::ui::truncate_chars(&subtitle, remaining)
    } else {
        String::new()
    };
    let used = title_shown.chars().count() + subtitle_shown.chars().count();
    let pad = avail.saturating_sub(used) + 1;

    let mut spans = vec![
        Span::raw(" "),
        Span::styled(glyph, glyph_style),
        Span::styled(title_shown, title_style),
        Span::styled(subtitle_shown, Style::default().fg(theme.secondary)),
    ];
    if liked {
        spans.push(Span::styled(" ♥", Style::default().fg(theme.player)));
    }
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(slider, Style::default().fg(theme.player)));
    spans.push(Span::styled(modes, Style::default().fg(Color::DarkGray)));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Progress gauge with the elapsed/total time label.
fn draw_progress(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let (position, duration) = match &app.current {
        Some(t) => (app.player.position().as_secs(), t.duration_secs),
        None => (0, 0),
    };
    let ratio = if duration > 0 {
        (position as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = if app.loading_audio {
        format!("{} loading…", app.spinner())
    } else if duration > 0 {
        format!("{} / {}", fmt(position), fmt(duration))
    } else if app.current.is_some() {
        fmt(position)
    } else {
        "--:-- / --:--".to_string()
    };

    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    if inner.width == 0 {
        return;
    }
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(theme.player).bg(Color::Rgb(30, 30, 30)))
        .ratio(ratio)
        .use_unicode(true)
        .label(Span::styled(label, Style::default().fg(Color::White)));
    f.render_widget(gauge, inner);
}
