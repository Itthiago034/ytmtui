//! The Now Playing section: the current track, given the whole panel.
//!
//! The sidebar's album art is a thumbnail squeezed under the menu; this is
//! the screen where the cover is the point. Three layouts, picked by how
//! much room there is:
//!
//! - **wide** — cover on the left, metadata/progress/spectrum/lyric on the
//!   right;
//! - **stacked** — cover on top, the rest centered underneath;
//! - **bare** — no room for a cover, so just the text.
//!
//! Nothing here fetches: the cover is whatever `App::artwork` already holds
//! for the current track, so an unsupported terminal or a still-downloading
//! cover simply falls through to the layout without one.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;
use ratatui_image::StatefulImage;

use crate::app::App;
use crate::config::VisualizerStyle;
use crate::theme::ThemeColors;

/// Narrowest panel that still fits a cover beside the metadata.
const WIDE_MIN_WIDTH: u16 = 74;
/// Shortest panel that still fits the side-by-side layout comfortably.
const WIDE_MIN_HEIGHT: u16 = 16;
/// Shortest panel that can stack a cover above the metadata.
const STACKED_MIN_HEIGHT: u16 = 18;
/// Terminal cells are roughly twice as tall as they are wide, so a square
/// cover needs about twice as many columns as rows.
const CELL_ASPECT: u16 = 2;

/// Which arrangement fits `area`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout2 {
    /// Cover beside the metadata.
    Wide,
    /// Cover above the metadata.
    Stacked,
    /// Metadata only.
    Bare,
}

/// Picks the layout for a panel of this size. Pure, so every breakpoint is
/// testable without rendering.
pub(super) fn layout_for(width: u16, height: u16, has_artwork: bool) -> Layout2 {
    if !has_artwork {
        return Layout2::Bare;
    }
    if width >= WIDE_MIN_WIDTH && height >= WIDE_MIN_HEIGHT {
        return Layout2::Wide;
    }
    if height >= STACKED_MIN_HEIGHT {
        return Layout2::Stacked;
    }
    Layout2::Bare
}

/// The largest square (in terminal cells) that fits `area`, accounting for
/// cells being taller than they are wide. Zero-sized when nothing fits.
pub(super) fn square_in(area: Rect) -> Rect {
    let by_width = area.width / CELL_ASPECT;
    let side_rows = by_width.min(area.height);
    if side_rows == 0 {
        return Rect {
            x: area.x,
            y: area.y,
            width: 0,
            height: 0,
        };
    }
    let side_cols = side_rows * CELL_ASPECT;
    Rect {
        x: area.x + (area.width - side_cols) / 2,
        y: area.y + (area.height - side_rows) / 2,
        width: side_cols,
        height: side_rows,
    }
}

/// Draws the Now Playing screen inside `block`.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect, block: Block) {
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if app.current.is_none() {
        let theme = app.theme();
        let msg = Paragraph::new(vec![
            Line::from(Span::styled("◉", Style::default().fg(theme.border))),
            Line::from(""),
            Line::from(Span::styled(
                "Nothing playing — press Enter on a track.",
                Style::default().fg(theme.muted),
            )),
        ])
        .alignment(Alignment::Center);
        let y = inner.y + inner.height.saturating_sub(3) / 2;
        f.render_widget(
            msg,
            Rect {
                y,
                height: inner.height.min(3),
                ..inner
            },
        );
        return;
    }

    match layout_for(inner.width, inner.height, app.artwork.is_some()) {
        Layout2::Wide => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Min(1)])
                .split(inner);
            draw_cover(f, app, columns[0]);
            draw_details(f, app, columns[1]);
        }
        Layout2::Stacked => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(55), Constraint::Min(1)])
                .split(inner);
            draw_cover(f, app, rows[0]);
            draw_details(f, app, rows[1]);
        }
        Layout2::Bare => draw_details(f, app, inner),
    }
}

/// The cover, centered and kept square so it is never stretched.
fn draw_cover(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(protocol) = app.artwork.as_mut() else {
        return;
    };
    let square = square_in(area);
    if square.width == 0 || square.height == 0 {
        return;
    }
    f.render_stateful_widget(StatefulImage::new(None), square, protocol);
}

/// Title, artist, album, progress, spectrum and the current lyric line.
fn draw_details(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let Some(track) = app.current.as_ref() else {
        return;
    };

    // The title fades in on a track change, the same two-stage reveal the
    // compact now-playing line uses.
    let elapsed_ms = app.ui.anim.since_track_change_ms();
    let title_fg =
        match super::now_playing::metadata_stage(elapsed_ms, app.ui.anim.reduced_motion()) {
            super::now_playing::MetadataStage::Fading => theme.subtext,
            super::now_playing::MetadataStage::Final => theme.text,
        };

    let width = area.width as usize;
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            super::truncate_chars(&track.title, width),
            Style::default().fg(title_fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            super::truncate_chars(&track.artist, width),
            Style::default().fg(theme.secondary),
        )),
    ];
    if !track.album.is_empty() {
        lines.push(Line::from(Span::styled(
            super::truncate_chars(&track.album, width),
            Style::default().fg(theme.muted),
        )));
    }
    if app.liked.contains(&track.video_id) {
        lines.push(Line::from(Span::styled(
            "♥ liked",
            Style::default().fg(theme.player),
        )));
    }
    lines.push(Line::from(""));
    lines.push(progress_line(app, track, width, theme));

    let lyric = active_lyric_line(app, theme);

    // Size the optional blocks first, then center the whole stack. Laying
    // the text out from the top instead would leave a track sitting under a
    // tall column of empty panel, which reads as a rendering bug rather
    // than as a screen with room to breathe.
    let text_height = lines.len() as u16;
    let lyric_height = if lyric.is_some() { 2 } else { 0 };
    let bars_height = if app.visualizer_style == VisualizerStyle::Off {
        0
    } else {
        // One blank spacer row plus the bars themselves, out of whatever is
        // left once the text and lyric have their rows.
        let spare = area
            .height
            .saturating_sub(text_height + lyric_height)
            .saturating_sub(1);
        if spare >= 3 {
            spare.min(6) + 1
        } else {
            0
        }
    };

    let total = text_height + bars_height + lyric_height;
    let mut y = area.y + area.height.saturating_sub(total) / 2;

    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        Rect {
            y,
            height: text_height.min(area.height),
            ..area
        },
    );
    y += text_height;

    if bars_height > 0 {
        // `draw_bars` fills from the left edge of whatever rect it gets, so
        // centering means handing it a rect the bars exactly fill rather
        // than the full panel width — otherwise the spectrum hugs the left
        // while the text above it is centered.
        let bars_width = (crate::visualizer::BAR_COUNT as u16 * 2).min(area.width);
        super::main_panel::draw_bars(
            f,
            app.visualizer.bars(),
            app.visualizer.peaks(),
            Rect {
                x: area.x + (area.width - bars_width) / 2,
                y: y + 1,
                width: bars_width,
                height: bars_height - 1,
            },
            theme,
            app.visualizer_style,
        );
        y += bars_height;
    }

    // The active lyric line last, as the closing note of the screen.
    if let Some(line) = lyric {
        f.render_widget(
            Paragraph::new(line)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            Rect {
                y: y + 1,
                height: 1,
                ..area
            },
        );
    }
}

/// `1:23 ━━━━━●──────── 3:22`, the same visual language as the compact
/// player's progress bar and the volume slider.
fn progress_line(
    app: &App,
    track: &crate::models::Track,
    width: usize,
    theme: ThemeColors,
) -> Line<'static> {
    let position = app.player.position().as_secs();
    let duration = track.duration_secs;
    let fmt = |secs: u64| format!("{}:{:02}", secs / 60, secs % 60);
    let left = fmt(position);
    let right = if duration > 0 {
        fmt(duration)
    } else {
        "-:--".to_string()
    };

    let bar_width = width.saturating_sub(left.len() + right.len() + 2);
    if bar_width < 3 {
        return Line::from(Span::styled(
            format!("{left} / {right}"),
            Style::default().fg(theme.subtext),
        ));
    }
    let ratio = if duration > 0 {
        (position as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = ((ratio * (bar_width - 1) as f64).round() as usize).min(bar_width - 1);
    Line::from(vec![
        Span::styled(left, Style::default().fg(theme.subtext)),
        Span::raw(" "),
        Span::styled("━".repeat(filled), Style::default().fg(theme.player)),
        Span::styled(
            "●",
            Style::default()
                .fg(theme.player)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "─".repeat(bar_width - 1 - filled),
            Style::default().fg(theme.border),
        ),
        Span::raw(" "),
        Span::styled(right, Style::default().fg(theme.subtext)),
    ])
}

/// The lyric line being sung right now, with the same karaoke wipe the
/// Lyrics section uses. `None` when there are no synced lyrics.
fn active_lyric_line(app: &App, theme: ThemeColors) -> Option<Line<'static>> {
    let crate::lyrics::LyricsState::Synced { lines, active } = &app.lyrics else {
        return None;
    };
    let active = (*active)?;
    let line = lines.get(active)?;
    Some(super::main_panel::karaoke_line(
        line,
        app.player.position().as_millis() as u64,
        theme,
        app.ui.anim.reduced_motion(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_cover_always_falls_back_to_text_only() {
        // An unsupported terminal or a cover that hasn't downloaded yet must
        // never leave a hole where the art would be.
        assert_eq!(layout_for(200, 60, false), Layout2::Bare);
    }

    #[test]
    fn the_layout_degrades_with_the_available_room() {
        assert_eq!(layout_for(120, 40, true), Layout2::Wide);
        // Too narrow for a side-by-side cover, but tall enough to stack.
        assert_eq!(layout_for(60, 30, true), Layout2::Stacked);
        // A short terminal keeps the text and drops the cover.
        assert_eq!(layout_for(120, 10, true), Layout2::Bare);
    }

    #[test]
    fn the_cover_stays_square_in_cells_not_in_rows() {
        // Cells are about twice as tall as wide, so a square cover must be
        // twice as many columns as rows or it renders as a tall rectangle.
        let square = square_in(Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 30,
        });
        assert_eq!(square.width, square.height * CELL_ASPECT);
    }

    #[test]
    fn the_cover_is_centered_in_its_area() {
        let area = Rect {
            x: 10,
            y: 5,
            width: 40,
            height: 30,
        };
        let square = square_in(area);
        let left = square.x - area.x;
        let right = (area.x + area.width) - (square.x + square.width);
        assert!(left.abs_diff(right) <= 1, "left {left} vs right {right}");
    }

    #[test]
    fn an_area_too_small_for_any_square_yields_nothing_to_draw() {
        let square = square_in(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 10,
        });
        assert_eq!((square.width, square.height), (0, 0));
    }
}
