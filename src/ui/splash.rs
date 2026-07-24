//! The entry animation: the wordmark assembling itself before the app
//! appears.
//!
//! Three phases over one window (see [`SPLASH_MS`]):
//!
//! 1. **Wipe** — the wordmark is revealed column by column from the left,
//!    each column warming from `muted` to `accent` as it lands.
//! 2. **Tagline** — the tagline fades up underneath.
//! 3. **Handoff** — the real UI draws underneath while the wordmark travels
//!    from the center to where the sidebar wordmark lives, fading out.
//!
//! The splash is skipped entirely under reduced motion or when the user
//! turns it off, and any key press cancels it mid-flight — a startup
//! animation that cannot be dismissed is a startup animation that gets
//! resented.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::{self, ThemeColors};
use crate::ui::state::progress;

/// Block-glyph wordmark. 23 columns wide, built only from full/half blocks
/// so it renders on any monospace font — no Nerd Font, no emoji.
pub(super) const LOGO: [&str; 3] = [
    "█ █ ▀█▀ █▀▄▀█ ▀█▀ █ █ █",
    "▀█▀  █  █   █  █  █ █ █",
    " ▀   ▀  ▀   ▀  ▀  ▀▀▀ ▀",
];
pub(super) const TAGLINE: &str = "YouTube Music in your terminal";

/// Total length of the entry animation, in animation-time milliseconds
/// (already speed-scaled by `AnimationClock::since_boot_ms`).
pub const SPLASH_MS: u128 = 1100;

/// Fraction of the window each phase ends at.
const WIPE_END: f32 = 0.40;
const TAGLINE_END: f32 = 0.70;

/// Which phase the animation is in at `elapsed_ms`.
///
/// Pure — takes elapsed time rather than reading a clock, so the phase
/// boundaries are testable without sleeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Wordmark revealing left to right; `columns` of it are visible.
    Wipe { columns: usize },
    /// Wordmark complete, tagline fading up.
    Tagline { fade: u8 },
    /// Real UI underneath, wordmark travelling out.
    Handoff { travel: u8 },
    /// Animation over; the caller should drop the splash entirely.
    Done,
}

/// Resolves the phase at `elapsed_ms`. `fade`/`travel` are 0..=100 rather
/// than `f32` so the phase compares exactly in tests.
pub fn phase_at(elapsed_ms: u128) -> Phase {
    let t = progress(elapsed_ms, SPLASH_MS);
    if t >= 1.0 {
        return Phase::Done;
    }
    let width = LOGO[0].chars().count();
    if t < WIPE_END {
        let local = t / WIPE_END;
        // At least one column from the very first frame: `ceil` alone still
        // yields zero at exactly t=0, and a blank opening frame reads as a
        // hang rather than as an animation about to start.
        return Phase::Wipe {
            columns: ((local * width as f32).ceil() as usize).clamp(1, width),
        };
    }
    if t < TAGLINE_END {
        let local = (t - WIPE_END) / (TAGLINE_END - WIPE_END);
        return Phase::Tagline {
            fade: (local * 100.0).round() as u8,
        };
    }
    let local = (t - TAGLINE_END) / (1.0 - TAGLINE_END);
    Phase::Handoff {
        travel: (local * 100.0).round() as u8,
    }
}

/// Whether the real UI should be drawn underneath this phase. Only the
/// handoff hands over.
pub fn shows_app_underneath(phase: Phase) -> bool {
    matches!(phase, Phase::Handoff { .. } | Phase::Done)
}

/// Draws the splash for `phase` over `area`.
pub fn draw(f: &mut Frame, area: Rect, phase: Phase, theme: ThemeColors) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match phase {
        Phase::Wipe { columns } => draw_centered(f, area, wipe_lines(columns, theme), 0.5),
        Phase::Tagline { fade } => {
            let mut lines = wipe_lines(LOGO[0].chars().count(), theme);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                TAGLINE,
                Style::default().fg(theme::mix(
                    theme.muted,
                    theme.secondary,
                    fade as f32 / 100.0,
                )),
            )));
            draw_centered(f, area, lines, 0.5);
        }
        Phase::Handoff { travel } => {
            let t = travel as f32 / 100.0;
            // Travel from the vertical middle up to the top, where the
            // sidebar wordmark sits, fading into the background as it goes.
            let anchor = 0.5 - 0.5 * t;
            let color = theme::mix(theme.accent, theme.border, t);
            let lines = LOGO
                .iter()
                .map(|row| Line::from(Span::styled(*row, Style::default().fg(color))))
                .collect();
            draw_centered(f, area, lines, anchor);
        }
        Phase::Done => {}
    }
}

/// The wordmark with only its first `columns` columns revealed. Columns that
/// have not landed yet are blank, so the glyph assembles rather than slides.
fn wipe_lines(columns: usize, theme: ThemeColors) -> Vec<Line<'static>> {
    LOGO.iter()
        .map(|row| {
            let shown: String = row.chars().take(columns).collect();
            Line::from(Span::styled(
                shown,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

/// Renders `lines` centered horizontally, with their vertical middle at
/// `anchor` (0.0 = top of `area`, 1.0 = bottom).
fn draw_centered(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>, anchor: f32) {
    let height = lines.len() as u16;
    if height > area.height {
        return;
    }
    let center = (area.height as f32 * anchor.clamp(0.0, 1.0)) as u16;
    let y = center
        .saturating_sub(height / 2)
        .min(area.height - height)
        .saturating_add(area.y);
    let rect = Rect {
        x: area.x,
        y,
        width: area.width,
        height,
    };
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 23; // LOGO[0] is 23 columns.

    #[test]
    fn the_first_frame_already_shows_something() {
        // Starting from a blank screen reads as a hang rather than an
        // animation, so column 1 must be visible immediately.
        match phase_at(0) {
            Phase::Wipe { columns } => assert!(columns >= 1, "got {columns} columns at t=0"),
            other => panic!("expected the wipe at t=0, got {other:?}"),
        }
    }

    #[test]
    fn the_wipe_completes_the_wordmark_before_the_tagline_starts() {
        let last_wipe = (SPLASH_MS as f32 * WIPE_END) as u128 - 1;
        match phase_at(last_wipe) {
            Phase::Wipe { columns } => assert_eq!(columns, WIDTH, "the wipe must finish the glyph"),
            other => panic!("expected the wipe, got {other:?}"),
        }
    }

    #[test]
    fn phases_run_in_order_and_end_at_done() {
        assert!(matches!(phase_at(0), Phase::Wipe { .. }));
        assert!(matches!(phase_at(500), Phase::Tagline { .. }));
        assert!(matches!(phase_at(900), Phase::Handoff { .. }));
        assert_eq!(phase_at(SPLASH_MS), Phase::Done);
        assert_eq!(phase_at(SPLASH_MS * 10), Phase::Done, "and stays done");
    }

    #[test]
    fn each_phase_fills_its_own_range_end_to_end() {
        // The fade and travel fractions must sweep 0..100 within their
        // phase, not stall partway through it.
        let tagline_start = (SPLASH_MS as f32 * WIPE_END) as u128 + 1;
        assert!(
            matches!(phase_at(tagline_start), Phase::Tagline { fade } if fade <= 2),
            "the tagline starts invisible"
        );
        let handoff_start = (SPLASH_MS as f32 * TAGLINE_END) as u128 + 1;
        assert!(
            matches!(phase_at(handoff_start), Phase::Handoff { travel } if travel <= 2),
            "the handoff starts at the center"
        );
        assert!(
            matches!(phase_at(SPLASH_MS - 1), Phase::Handoff { travel } if travel >= 98),
            "the handoff ends fully travelled"
        );
    }

    #[test]
    fn only_the_handoff_reveals_the_app_underneath() {
        assert!(!shows_app_underneath(Phase::Wipe { columns: 3 }));
        assert!(!shows_app_underneath(Phase::Tagline { fade: 50 }));
        assert!(shows_app_underneath(Phase::Handoff { travel: 0 }));
        assert!(shows_app_underneath(Phase::Done));
    }
}
