//! Album art plumbing: deciding how (and whether) this terminal can show a
//! cover.
//!
//! Lives outside `main.rs` because the picker is no longer built once at
//! startup — changing the artwork mode in Settings rebuilds it in place.

use crate::config::ArtworkMode;

/// Whether the environment identifies a terminal known to answer image
/// protocol and font-size queries (Kitty, Ghostty, WezTerm, iTerm2, foot,
/// Konsole). Unknown terminals must not be queried: one that never answers
/// leaves ratatui-image's reader thread blocked on stdin, where it steals
/// key presses from the event loop.
pub fn env_reports_image_support(
    term: Option<&str>,
    term_program: Option<&str>,
    has_kitty_window: bool,
    has_konsole_version: bool,
) -> bool {
    if has_kitty_window || has_konsole_version {
        return true;
    }
    let term = term.unwrap_or_default().to_ascii_lowercase();
    let program = term_program.unwrap_or_default().to_ascii_lowercase();
    term.contains("kitty")
        || term.contains("ghostty")
        || term.contains("foot")
        || program.contains("wezterm")
        || program.contains("iterm")
        || program.contains("ghostty")
}

/// Whether `mode` wants a picker built at all. `Off` means no cover art is
/// ever downloaded or drawn, so `build_picker` short-circuits before
/// touching the terminal.
pub fn wants_picker(mode: ArtworkMode) -> bool {
    mode != ArtworkMode::Off
}

/// Whether `build_picker` should query the terminal for its real image
/// protocol. Only `Auto` does — and only when the environment identifies a
/// terminal known to answer the query; `HalfBlocks` always skips it (even
/// on a capable terminal) to force the Unicode fallback.
pub fn should_query_protocol(mode: ArtworkMode, env_supported: bool) -> bool {
    mode == ArtworkMode::Auto && env_supported
}

/// Builds the album-art picker according to `mode`: `Auto` queries capable
/// terminals for their real protocol (Kitty graphics, Sixel, iTerm2) and
/// falls back to half-blocks otherwise; `HalfBlocks` always uses half-blocks
/// (skipping the query entirely); `Off` returns `None`, so no picker is
/// created and no cover art is ever drawn.
pub fn build_picker(mode: ArtworkMode) -> Option<ratatui_image::picker::Picker> {
    use ratatui_image::picker::Picker;

    if !wants_picker(mode) {
        return None;
    }

    let env_supported = env_reports_image_support(
        std::env::var("TERM").ok().as_deref(),
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var_os("KITTY_WINDOW_ID").is_some(),
        std::env::var_os("KONSOLE_VERSION").is_some(),
    );
    if should_query_protocol(mode, env_supported) {
        if let Ok(picker) = Picker::from_query_stdio() {
            return Some(picker);
        }
    }
    let font_size = crossterm::terminal::window_size()
        .ok()
        .and_then(|s| cell_size_from(s.columns, s.rows, s.width, s.height))
        .unwrap_or((8, 16));
    Some(Picker::from_fontsize(font_size))
}

/// Cell size in pixels derived from the reported window size; `None` when
/// the terminal does not report usable pixel dimensions. A zero-sized cell
/// must never reach the picker: it would break image scaling.
pub fn cell_size_from(columns: u16, rows: u16, width: u16, height: u16) -> Option<(u16, u16)> {
    if columns == 0 || rows == 0 {
        return None;
    }
    let cell = (width / columns, height / rows);
    (cell.0 > 0 && cell.1 > 0).then_some(cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_size_requires_sane_pixel_reports() {
        assert_eq!(cell_size_from(80, 24, 640, 384), Some((8, 16)));
        // Missing or nonsensical pixel reports must fall back, never
        // produce a zero-sized font that breaks image scaling.
        assert_eq!(cell_size_from(80, 24, 0, 0), None);
        assert_eq!(cell_size_from(80, 24, 40, 384), None);
        assert_eq!(cell_size_from(0, 0, 640, 384), None);
    }

    #[test]
    fn image_protocol_query_is_gated_by_terminal_identity() {
        assert!(env_reports_image_support(
            Some("xterm-kitty"),
            None,
            true,
            false
        ));
        assert!(env_reports_image_support(
            Some("xterm-256color"),
            Some("WezTerm"),
            false,
            false
        ));
        assert!(env_reports_image_support(
            Some("xterm-256color"),
            None,
            false,
            true
        ));
        // Unknown terminals must not be queried: a terminal that never
        // answers would leave a reader thread stealing key presses.
        assert!(!env_reports_image_support(
            Some("xterm-256color"),
            None,
            false,
            false
        ));
        assert!(!env_reports_image_support(None, None, false, false));
    }

    #[test]
    fn artwork_mode_off_never_wants_a_picker() {
        assert!(!wants_picker(ArtworkMode::Off));
        assert!(wants_picker(ArtworkMode::Auto));
        assert!(wants_picker(ArtworkMode::HalfBlocks));
    }

    #[test]
    fn only_auto_mode_queries_the_real_protocol() {
        // Auto follows the terminal's reported support either way.
        assert!(should_query_protocol(ArtworkMode::Auto, true));
        assert!(!should_query_protocol(ArtworkMode::Auto, false));
        // HalfBlocks always skips the query, even on a capable terminal.
        assert!(!should_query_protocol(ArtworkMode::HalfBlocks, true));
        assert!(!should_query_protocol(ArtworkMode::HalfBlocks, false));
        // Off is moot (build_picker never gets this far), but stays false.
        assert!(!should_query_protocol(ArtworkMode::Off, true));
    }
}
