//! UI-facing lyrics state: what the Lyrics section renders, plus the logic
//! for tracking which synced line is currently active as playback advances.

use crate::ytmusic::LyricLine;

/// What the Lyrics section currently has to show.
#[derive(Debug, Clone, Default)]
pub enum LyricsState {
    /// No track playing, or the fetch for the current track hasn't
    /// completed yet. Reset here on every track change (see
    /// `App::start_current`).
    #[default]
    None,
    /// The fetch completed but no lyrics exist for this track at all.
    NotAvailable,
    /// Plain Musixmatch-sourced text (WEB_REMIX fallback), no timestamps.
    Plain(String),
    /// Per-line timed lyrics (ANDROID_MUSIC). `active` is the index of the
    /// currently active line; `None` before the first line's start time.
    /// Doubles as the scan cursor for `advance_active_line`, so ticks don't
    /// rescan from the start.
    Synced {
        lines: Vec<LyricLine>,
        active: Option<usize>,
    },
}

/// Advances (or rewinds) `cursor` to the line active at `position_ms`,
/// without rescanning from the beginning on the common case (monotonic
/// playback). Assumes `lines` is sorted ascending by `start_ms` (true of the
/// InnerTube response order).
///
/// - Forward case (normal playback): walks forward from `cursor` while the
///   *next* line's start has already passed — O(1) amortized per tick.
/// - Backward case (the user seeked back, or a repeated track restarted):
///   detected when `position_ms` is before the cursor's own start, handled
///   with a binary search (`partition_point`) instead of a linear rewind.
pub fn advance_active_line(
    lines: &[LyricLine],
    cursor: Option<usize>,
    position_ms: u64,
) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let idx = cursor.unwrap_or(0);
    if position_ms < lines[idx].start_ms {
        let pos = lines.partition_point(|l| l.start_ms <= position_ms);
        return (pos > 0).then(|| pos - 1);
    }
    let mut idx = idx;
    while idx + 1 < lines.len() && lines[idx + 1].start_ms <= position_ms {
        idx += 1;
    }
    Some(idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start_ms: u64) -> LyricLine {
        LyricLine {
            text: String::new(),
            start_ms,
            end_ms: start_ms + 1000,
        }
    }

    fn sample_lines() -> Vec<LyricLine> {
        vec![line(0), line(1000), line(2000), line(3000)]
    }

    #[test]
    fn empty_lines_never_have_an_active_line() {
        assert_eq!(advance_active_line(&[], None, 5000), None);
    }

    #[test]
    fn before_the_first_line_has_no_active_line() {
        let lines = sample_lines();
        assert_eq!(advance_active_line(&lines, None, 0), Some(0));
        // A position before line 0's start (impossible with start_ms=0, so
        // use a synthetic set starting later) yields None.
        let later_lines = vec![line(500), line(1500)];
        assert_eq!(advance_active_line(&later_lines, None, 100), None);
    }

    #[test]
    fn advances_forward_monotonically_without_rescanning() {
        let lines = sample_lines();
        let mut cursor = advance_active_line(&lines, None, 0);
        assert_eq!(cursor, Some(0));
        cursor = advance_active_line(&lines, cursor, 1500);
        assert_eq!(cursor, Some(1));
        cursor = advance_active_line(&lines, cursor, 3200);
        assert_eq!(cursor, Some(3));
    }

    #[test]
    fn past_the_last_line_stays_on_the_last_line() {
        let lines = sample_lines();
        assert_eq!(advance_active_line(&lines, Some(3), 999_999), Some(3));
    }

    #[test]
    fn seeking_backward_rewinds_via_binary_search() {
        let lines = sample_lines();
        let cursor = advance_active_line(&lines, Some(3), 500);
        assert_eq!(cursor, Some(0));
    }

    #[test]
    fn track_repeat_restart_resets_to_the_first_line() {
        let lines = sample_lines();
        // Cursor left at the last line from the previous play-through; the
        // track restarts at position 0.
        let cursor = advance_active_line(&lines, Some(3), 0);
        assert_eq!(cursor, Some(0));
    }
}
