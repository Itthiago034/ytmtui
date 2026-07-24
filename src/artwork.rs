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

// --- on-disk cache -------------------------------------------------------

use std::path::{Path, PathBuf};

/// Ceiling for the cover cache. Covers are small (tens of KB), so this holds
/// a few hundred of them — well past a listening session's worth — while
/// staying a rounding error on any disk.
const CACHE_MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Companion ceiling on file count, so a run of unusually small covers
/// cannot leave tens of thousands of entries behind.
const CACHE_MAX_FILES: usize = 400;

/// Directory holding cached covers.
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("ytmtui").join("artwork"))
}

/// Cache file name for `url`.
///
/// SHA-1 of the URL: the URL itself contains slashes and query strings, and
/// is far longer than most filesystems allow in one component. `sha1` is
/// already a dependency (the auth header needs it), and this is a cache key,
/// not a security boundary.
fn cache_key(url: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(url.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Reads `url`'s cover from `dir`, if it was cached.
pub fn read_cached(dir: &Path, url: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(dir.join(cache_key(url))).ok()?;
    // A zero-length file is a write that was interrupted; treat it as a
    // miss so the cover is fetched again rather than decoded as garbage.
    (!bytes.is_empty()).then_some(bytes)
}

/// Stores `bytes` as `url`'s cover in `dir`. Failures are silent: a cache
/// that cannot be written is a slower app, not a broken one.
pub fn write_cached(dir: &Path, url: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join(cache_key(url)), bytes);
}

/// Deletes the oldest covers until the cache is back under both ceilings.
///
/// Oldest by modification time, which for a cache written once per cover
/// approximates least-recently-used closely enough.
pub fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();

    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    let mut count = files.len();
    if total <= CACHE_MAX_BYTES && count <= CACHE_MAX_FILES {
        return;
    }

    // Oldest first, so the loop below removes in the order it should.
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, size, path) in files {
        if total <= CACHE_MAX_BYTES && count <= CACHE_MAX_FILES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            count -= 1;
        }
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn a_cover_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_cached(dir.path(), "https://x/cover.jpg"), None);

        write_cached(dir.path(), "https://x/cover.jpg", b"cover bytes");

        assert_eq!(
            read_cached(dir.path(), "https://x/cover.jpg").as_deref(),
            Some(&b"cover bytes"[..])
        );
    }

    #[test]
    fn different_urls_do_not_collide() {
        let dir = tempfile::tempdir().unwrap();
        write_cached(dir.path(), "https://x/a.jpg", b"aaa");
        write_cached(dir.path(), "https://x/b.jpg", b"bbb");
        assert_eq!(read_cached(dir.path(), "https://x/a.jpg").unwrap(), b"aaa");
        assert_eq!(read_cached(dir.path(), "https://x/b.jpg").unwrap(), b"bbb");
    }

    #[test]
    fn a_url_with_slashes_and_a_query_still_produces_one_path_component() {
        // The raw URL could never be a filename; the key must be flat.
        let key = cache_key("https://lh3.googleusercontent.com/a/b?sz=544&v=2");
        assert!(!key.contains('/') && !key.contains('?'));
        assert_eq!(key.len(), 40, "sha-1 hex");
    }

    #[test]
    fn an_interrupted_write_reads_as_a_miss() {
        // A zero-length file would otherwise be handed to the decoder.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(cache_key("https://x/c.jpg")), b"").unwrap();
        assert_eq!(read_cached(dir.path(), "https://x/c.jpg"), None);
    }

    #[test]
    fn pruning_leaves_the_cache_under_the_file_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..CACHE_MAX_FILES + 25 {
            write_cached(dir.path(), &format!("https://x/{i}.jpg"), b"x");
        }
        assert!(std::fs::read_dir(dir.path()).unwrap().count() > CACHE_MAX_FILES);

        prune(dir.path());

        assert!(std::fs::read_dir(dir.path()).unwrap().count() <= CACHE_MAX_FILES);
    }

    #[test]
    fn pruning_a_cache_under_both_ceilings_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write_cached(dir.path(), "https://x/keep.jpg", b"still here");

        prune(dir.path());

        assert_eq!(
            read_cached(dir.path(), "https://x/keep.jpg").as_deref(),
            Some(&b"still here"[..])
        );
    }

    #[test]
    fn pruning_a_missing_directory_is_not_an_error() {
        prune(Path::new("/nonexistent/ytmtui/artwork"));
    }
}
