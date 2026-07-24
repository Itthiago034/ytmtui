//! Compact two-line playback summary: track line plus progress bar.

use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, RepeatMode};

/// Formats seconds as "m:ss".
fn fmt(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Stage of the now-playing title's metadata fade-in after a track change,
/// driven by elapsed time since the current track started. Two stages only
/// (unlike the Home card's three-stage [`super::main_panel::RevealStage`]):
/// there's just the one style property (color) fading in here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetadataStage {
    /// The title is still fading in: rendered in the neutral `subtext`
    /// color rather than its final style.
    Fading,
    /// The fade finished (or was skipped): final title style applies.
    Final,
}

/// Pure decision of which [`MetadataStage`] applies `elapsed_ms` after the
/// track changed. Free of `Instant`/`App`, like
/// `main_panel::reveal_stage`, so it's directly testable with explicit
/// elapsed values. `reduced_motion` always short-circuits to `Final`.
pub(super) fn metadata_stage(elapsed_ms: u128, reduced_motion: bool) -> MetadataStage {
    if reduced_motion || elapsed_ms >= 150 {
        MetadataStage::Final
    } else {
        MetadataStage::Fading
    }
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
        ("⏹ ".to_string(), Style::default().fg(theme.muted))
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
        // Just after a track change, the title fades in from a neutral
        // color instead of snapping straight to its final style — the
        // "metadata reveal" for the now-playing summary. `reduced_motion`
        // skips this via `metadata_stage`.
        let elapsed_ms = app.ui.anim.since_track_change_ms();
        let fg = match metadata_stage(elapsed_ms, app.ui.anim.reduced_motion()) {
            MetadataStage::Fading => theme.subtext,
            MetadataStage::Final => theme.text,
        };
        Style::default().fg(fg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
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

    let subtitle_style = Style::default().fg(theme.secondary);
    let needed = crate::ui::display_width(&title) + crate::ui::display_width(&subtitle);
    // Título que não cabe vira um marquee deslizando com o relógio da faixa
    // (e congelando em pausa); com espaço de sobra, texto estático normal.
    // `reduced_motion` desativa esse deslizamento (volta ao truncamento com
    // '…' abaixo); `animation_speed` ajusta o passo via `marquee_interval`.
    let mut middle =
        if needed > avail && avail >= 8 && app.current.is_some() && !app.ui.anim.reduced_motion() {
            let interval = crate::ui::marquee_interval(app.ui.anim.speed());
            let step = (app.player.position().as_millis() / interval) as usize;
            crate::ui::marquee_spans(
                &[
                    (title.as_str(), title_style),
                    (subtitle.as_str(), subtitle_style),
                ],
                avail,
                step,
            )
        } else {
            let title_shown = crate::ui::truncate_chars(&title, avail);
            let remaining = avail.saturating_sub(crate::ui::display_width(&title_shown));
            let subtitle_shown = if remaining >= 4 {
                crate::ui::truncate_chars(&subtitle, remaining)
            } else {
                String::new()
            };
            vec![
                Span::styled(title_shown, title_style),
                Span::styled(subtitle_shown, subtitle_style),
            ]
        };
    let used: usize = middle
        .iter()
        .map(|s| crate::ui::display_width(&s.content))
        .sum();
    let pad = avail.saturating_sub(used) + 1;

    let mut spans = vec![Span::raw(" "), Span::styled(glyph, glyph_style)];
    spans.append(&mut middle);
    if liked {
        spans.push(Span::styled(" ♥", Style::default().fg(theme.player)));
    }
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(slider, Style::default().fg(theme.player)));
    spans.push(Span::styled(modes, Style::default().fg(theme.muted)));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Progress line: `0:42 ━━━●──────── 4:27`. Same visual language as the
/// volume slider (filled track, knob, empty track) so the two read as one
/// family of controls. Idle and loading states degrade gracefully.
fn draw_progress(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    if inner.width == 0 {
        return;
    }

    if app.loading_audio {
        let line = Line::from(Span::styled(
            format!("{} loading…", app.spinner()),
            Style::default().fg(theme.subtext),
        ));
        f.render_widget(Paragraph::new(line), inner);
        return;
    }

    let (position, duration) = match &app.current {
        Some(t) => (app.player.position().as_secs(), t.duration_secs),
        None => (0, 0),
    };
    let ratio = if duration > 0 {
        (position as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let left = if app.current.is_some() {
        fmt(position)
    } else {
        "-:--".to_string()
    };
    let right = if duration > 0 {
        fmt(duration)
    } else {
        "-:--".to_string()
    };

    let time_style = Style::default().fg(theme.subtext);
    let width = inner.width as usize;
    let bar_width = width.saturating_sub(left.len() + right.len() + 2);
    if bar_width < 3 {
        // Too narrow for a bar: show what fits of the times alone.
        let text = crate::ui::truncate_chars(&format!("{left} / {right}"), width);
        f.render_widget(Paragraph::new(Span::styled(text, time_style)), inner);
        return;
    }

    // Knob occupies one cell; the filled track grows to its left. Idle
    // playback renders a flat dim track with no knob at all.
    let mut spans = vec![Span::styled(left, time_style), Span::raw(" ")];
    if app.current.is_some() {
        let filled = ((ratio * (bar_width - 1) as f64).round() as usize).min(bar_width - 1);
        let empty = bar_width - 1 - filled;
        spans.push(Span::styled(
            "━".repeat(filled),
            Style::default().fg(theme.player),
        ));
        spans.push(Span::styled(
            "●",
            Style::default()
                .fg(theme.player)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "─".repeat(empty),
            Style::default().fg(theme.border),
        ));
    } else {
        spans.push(Span::styled(
            "─".repeat(bar_width),
            Style::default().fg(theme.border),
        ));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(right, time_style));
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}
