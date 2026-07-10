//! Rendering tests for the balanced layout.
//!
//! Every test drives the full `ui::draw` entry point through a Ratatui
//! `TestBackend`, asserting on the produced buffer. Rendering must stay
//! side-effect-free, so these tests never touch the network or the disk.

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

use crate::app::{App, Focus, Section};
use crate::models::Track;

/// Renders one frame at the given size and returns the resulting buffer.
fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| super::draw(frame, app))
        .expect("draw should never fail");
    terminal.backend().buffer().clone()
}

/// Buffer content as one string per row.
fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

/// Full buffer content as a single newline-joined string.
fn text(buffer: &Buffer) -> String {
    rows(buffer).join("\n")
}

fn track(title: &str, artist: &str, duration: &str, duration_secs: u64, id: &str) -> Track {
    Track {
        video_id: id.to_string(),
        title: title.to_string(),
        artist: artist.to_string(),
        album: "Parachutes".to_string(),
        duration: duration.to_string(),
        duration_secs,
        thumbnail: None,
    }
}

fn playing_app() -> App {
    let mut app = App::new_for_tests();
    let t = track("Yellow", "Coldplay", "4:27", 267, "vid1");
    app.queue = vec![t.clone()];
    app.queue_index = Some(0);
    app.current = Some(t);
    app
}

#[test]
fn wide_layout_shows_navigation_content_playback_and_shortcuts() {
    let mut app = App::new_for_tests();
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);

    // Persistent navigation with concise English labels.
    assert!(content.contains("ytmtui"), "app name in nav:\n{content}");
    assert!(
        content.contains("Library"),
        "nav lists sections:\n{content}"
    );
    assert!(content.contains("Queue"), "nav lists sections:\n{content}");
    // Compact Now Playing summary is always present in tall terminals.
    assert!(
        content.contains("Nothing playing"),
        "idle playback summary:\n{content}"
    );
    assert!(content.contains("80%"), "volume indicator:\n{content}");
    // Contextual shortcut bar.
    assert!(content.contains("q quit"), "shortcut bar:\n{content}");
}

#[test]
fn narrow_layout_uses_a_single_column_with_a_header_row() {
    let mut app = App::new_for_tests();
    let buffer = render(&mut app, 50, 20);
    let lines = rows(&buffer);

    // The first row is a compact navigation header, not a sidebar column.
    assert!(lines[0].contains("Home"), "header row: {:?}", lines[0]);
    assert!(lines[0].contains("1/8"), "section position: {:?}", lines[0]);
    // No wide-mode nav column: "Library" only exists in the sidebar list.
    let body = lines[1..].join("\n");
    assert!(
        !body.contains("Library"),
        "no sidebar in narrow mode:\n{body}"
    );
}

#[test]
fn very_small_terminals_never_panic() {
    for width in [0u16, 1, 2, 3, 7, 10, 15, 25, 69, 80] {
        for height in [0u16, 1, 2, 3, 4, 5, 8] {
            let mut idle = App::new_for_tests();
            render(&mut idle, width, height);

            let mut playing = playing_app();
            playing.begin_task();
            playing.input_mode = true;
            playing.query = "x".repeat(200);
            render(&mut playing, width, height);
        }
    }
}

#[test]
fn short_terminal_drops_the_playback_rows_safely() {
    let mut app = playing_app();
    let buffer = render(&mut app, 80, 6);
    let content = text(&buffer);

    // Content survives; the two-line playback summary is dropped.
    assert!(
        !content.contains("/ 4:27"),
        "no progress gauge on short terminals:\n{content}"
    );
}

#[test]
fn playing_state_shows_track_progress_and_state_glyph() {
    let mut app = playing_app();
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);

    assert!(content.contains("Yellow"), "track title:\n{content}");
    assert!(content.contains("Coldplay"), "artist:\n{content}");
    assert!(content.contains("4:27"), "duration label:\n{content}");
    assert!(content.contains("▶"), "playing glyph:\n{content}");
}

#[test]
fn long_titles_are_truncated_with_an_ellipsis() {
    let mut app = playing_app();
    let long_title = "Supercalifragilistic ".repeat(12);
    if let Some(t) = app.current.as_mut() {
        t.title = long_title.clone();
    }
    let buffer = render(&mut app, 80, 24);
    let content = text(&buffer);

    assert!(content.contains('…'), "ellipsis for long title:\n{content}");
    assert!(
        !content.contains(&long_title),
        "full long title must not be rendered"
    );
}

#[test]
fn long_status_messages_are_truncated() {
    let mut app = App::new_for_tests();
    app.status = "status ".repeat(60);
    let buffer = render(&mut app, 80, 24);
    let content = text(&buffer);

    assert!(
        content.contains('…'),
        "ellipsis for long status:\n{content}"
    );
}

#[test]
fn empty_search_section_shows_an_english_hint() {
    let mut app = App::new_for_tests();
    app.section = Section::Buscar;
    app.sidebar_index = Section::Buscar.index();
    app.focus = Focus::Main;
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);

    assert!(
        content.contains("Press / to search"),
        "empty-state hint:\n{content}"
    );
}

#[test]
fn loading_state_shows_spinner_and_message() {
    let mut app = App::new_for_tests();
    app.begin_task();
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);

    assert!(
        content.contains("Loading recommendations"),
        "loading message:\n{content}"
    );
    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    assert!(
        content.chars().any(|c| spinner_frames.contains(&c)),
        "spinner glyph while loading:\n{content}"
    );
}

#[test]
fn selection_highlight_and_scrollbar_stay_visible() {
    let mut app = App::new_for_tests();
    app.songs = (0..40)
        .map(|i| {
            track(
                &format!("Track {i}"),
                "Artist",
                "3:00",
                180,
                &format!("v{i}"),
            )
        })
        .collect();
    app.section = Section::Buscar;
    app.sidebar_index = Section::Buscar.index();
    app.focus = Focus::Main;
    app.list_state.select(Some(30));

    let buffer = render(&mut app, 90, 15);
    let content = text(&buffer);
    let highlight = app.theme().highlight_bg;

    // The selected row is scrolled into view and highlighted.
    assert!(
        content.contains("Track 30"),
        "selected row visible:\n{content}"
    );
    let highlighted = (0..buffer.area.height)
        .any(|y| (0..buffer.area.width).any(|x| buffer[(x, y)].bg == highlight));
    assert!(highlighted, "selection highlight present:\n{content}");
    // Scrollbar markers survive the redesign.
    assert!(content.contains('█'), "scrollbar thumb:\n{content}");
    assert!(content.contains('▲'), "scrollbar begin arrow:\n{content}");
}

#[test]
fn home_shows_recent_tracks_group_and_search_shows_mixed_groups() {
    use crate::models::{Artist, Playlist};

    // Home: local history renders as the first group.
    let mut app = App::new_for_tests();
    app.focus = Focus::Main;
    app.recent = vec![track("Yellow", "Coldplay", "4:27", 267, "vid1")];
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);
    assert!(
        content.contains("Recently played"),
        "recent group header:\n{content}"
    );
    assert!(content.contains("Yellow"), "recent track row:\n{content}");

    // Search: mixed results render one header per non-empty type group.
    let mut app = App::new_for_tests();
    app.focus = Focus::Main;
    app.section = Section::Buscar;
    app.sidebar_index = Section::Buscar.index();
    app.search_mixed = true;
    app.songs = vec![track("Fix You", "Coldplay", "4:54", 294, "vid2")];
    app.artists = vec![Artist {
        browse_id: "UC1".to_string(),
        name: "Coldplay".to_string(),
        subtitle: "Artist".to_string(),
        thumbnail: None,
    }];
    app.albums = vec![Playlist {
        browse_id: "MPRE1".to_string(),
        title: "X&Y".to_string(),
        subtitle: "Album • Coldplay".to_string(),
        thumbnail: None,
        ..Default::default()
    }];
    app.playlists = Vec::new();
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);
    for needle in ["Songs", "Artists", "Albums", "Fix You", "X&Y"] {
        assert!(content.contains(needle), "'{needle}' visible:\n{content}");
    }
    assert!(
        !content.contains("Playlists ─"),
        "empty groups render no header:\n{content}"
    );
}

#[test]
fn home_section_highlight_lands_on_the_right_item_despite_header_rows() {
    use crate::models::{HomeSection, Playlist};

    let mut app = App::new_for_tests();
    app.focus = Focus::Main;
    app.home = vec![
        HomeSection {
            title: "Quick picks".to_string(),
            items: vec![Playlist {
                browse_id: "VL1".to_string(),
                title: "First pick".to_string(),
                subtitle: "Some artist".to_string(),
                thumbnail: None,
                ..Default::default()
            }],
        },
        HomeSection {
            title: "Mixed for you".to_string(),
            items: vec![Playlist {
                browse_id: "VL2".to_string(),
                title: "Second pick".to_string(),
                subtitle: "Another artist".to_string(),
                thumbnail: None,
                ..Default::default()
            }],
        },
    ];
    // Flattened index 1: the *second* section's only item — the header rows
    // in between must not throw off which rendered row gets highlighted.
    app.list_state.select(Some(1));

    let buffer = render(&mut app, 100, 30);
    let highlight = app.theme().highlight_bg;

    let row_of = |needle: &str| -> u16 {
        let buffer_rows = rows(&buffer);
        buffer_rows
            .iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("'{needle}' not found:\n{}", buffer_rows.join("\n")))
            as u16
    };

    let second_pick_row = row_of("Second pick");
    let first_pick_row = row_of("First pick");

    let row_is_highlighted =
        |y: u16| -> bool { (0..buffer.area.width).any(|x| buffer[(x, y)].bg == highlight) };

    assert!(
        row_is_highlighted(second_pick_row),
        "the selected item's row should be highlighted"
    );
    assert!(
        !row_is_highlighted(first_pick_row),
        "the non-selected item's row should not be highlighted"
    );
}

#[test]
fn search_input_line_appears_while_typing() {
    let mut app = App::new_for_tests();
    app.input_mode = true;
    app.section = Section::Buscar;
    app.query = "coldplay yellow".to_string();
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);

    assert!(content.contains("coldplay yellow"), "query:\n{content}");
    assert!(content.contains('▏'), "input cursor:\n{content}");
    assert!(
        content.contains("Esc cancel"),
        "contextual shortcuts while typing:\n{content}"
    );
}

#[test]
fn scrollbar_is_hidden_when_the_list_fits() {
    let mut app = App::new_for_tests();
    app.songs = (0..5)
        .map(|i| {
            track(
                &format!("Track {i}"),
                "Artist",
                "3:00",
                180,
                &format!("v{i}"),
            )
        })
        .collect();
    app.section = Section::Buscar;
    app.sidebar_index = Section::Buscar.index();
    app.focus = Focus::Main;

    let buffer = render(&mut app, 90, 30);
    let content = text(&buffer);

    assert!(
        !content.contains('▲'),
        "no scrollbar when everything fits:\n{content}"
    );
}

#[test]
fn volume_slider_is_visible_in_the_playback_summary() {
    let mut app = App::new_for_tests();
    let buffer = render(&mut app, 100, 30);
    let content = text(&buffer);

    // Default volume is 0.8 → eight filled segments, knob, two empty ones.
    assert!(
        content.contains("━━━━━━━━●──"),
        "volume slider with knob:\n{content}"
    );
    assert!(content.contains("80%"), "volume percentage:\n{content}");
}

#[test]
fn nav_column_shows_album_art_when_available() {
    let mut app = playing_app();
    // The half-block fallback protocol renders into plain buffer cells, so
    // it works without a real terminal. Kitty/Sixel need a live terminal.
    let mut picker = ratatui_image::picker::Picker::from_fontsize((8, 16));
    let cover = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        64,
        64,
        image::Rgb([200, 40, 40]),
    ));
    app.artwork = Some(picker.new_resize_protocol(cover));
    app.picker = Some(picker);

    let buffer = render(&mut app, 100, 30);

    // Half-block glyphs appear inside the nav column (first 18 columns).
    let found = (0..buffer.area.height).any(|y| (0..18u16).any(|x| buffer[(x, y)].symbol() == "▀"));
    assert!(found, "album art half-blocks in the nav column");
}

#[test]
fn display_width_counts_wide_characters_as_two_columns() {
    // "ミク" is two CJK characters, 2 columns each: 4 columns, not 2.
    assert_eq!(super::display_width("ミク"), 4);
    assert_eq!(super::display_width("abc"), 3);
}

#[test]
fn truncate_chars_never_splits_a_wide_character() {
    // Each character is 2 columns wide; a budget of 5 only fits two of them
    // plus the 1-column ellipsis (2 + 2 + 1 = 5), not a third half-rendered
    // character.
    let out = super::truncate_chars("初音ミク", 5);
    assert_eq!(out, "初音…");
    assert!(super::display_width(&out) <= 5);
}

#[test]
fn take_width_hard_truncates_by_display_width_without_an_ellipsis() {
    let out = super::take_width("初音ミク", 5);
    assert_eq!(out, "初音");
    assert!(super::display_width(&out) <= 5);
}

#[test]
fn karaoke_wipe_splits_the_active_line_by_elapsed_time() {
    let theme = crate::theme::get(0);
    let line = crate::models::LyricLine {
        text: "abcdefghij".to_string(), // 10 columns
        start_ms: 1000,
        end_ms: 2000,
    };
    // Halfway through the window: 5 sung columns, 5 waiting.
    let rendered = super::main_panel::karaoke_line(&line, 1500, theme);
    assert_eq!(rendered.spans.len(), 2);
    assert_eq!(rendered.spans[0].content.as_ref(), "abcde");
    assert_eq!(rendered.spans[0].style.fg, Some(theme.accent));
    assert_eq!(rendered.spans[1].content.as_ref(), "fghij");
    assert_eq!(rendered.spans[1].style.fg, Some(theme.text));

    // Before the window: nothing sung yet; after: everything sung.
    let before = super::main_panel::karaoke_line(&line, 0, theme);
    assert_eq!(before.spans[0].content.as_ref(), "");
    let after = super::main_panel::karaoke_line(&line, 9000, theme);
    assert_eq!(after.spans[0].content.as_ref(), "abcdefghij");
}

#[test]
fn synced_lyrics_highlight_only_the_active_line() {
    let mut app = playing_app();
    app.section = Section::Letra;
    app.focus = Focus::Main;
    let theme = app.theme();
    // New spotlight design: the active line's unsung text is bright
    // (theme.text, bold); neighbors one line away fade to subtext.
    let (active_color, neighbor_color) = (theme.text, theme.subtext);
    app.lyrics = crate::lyrics::LyricsState::Synced {
        lines: vec![
            crate::models::LyricLine {
                text: "First line".to_string(),
                start_ms: 0,
                end_ms: 1000,
            },
            crate::models::LyricLine {
                text: "Second line".to_string(),
                start_ms: 1000,
                end_ms: 2000,
            },
            crate::models::LyricLine {
                text: "Third line".to_string(),
                start_ms: 2000,
                end_ms: 3000,
            },
        ],
        active: Some(1),
    };

    let buffer = render(&mut app, 100, 30);

    // Finds the (x, y) of the first cell where `needle` starts, by matching
    // consecutive cells directly (not byte-offsets into a joined String,
    // which would misalign with columns whenever a preceding cell is
    // multi-byte, and not just the first character, which could
    // false-positive against unrelated text like the sidebar's "Search").
    let find_cell = |needle: &str| -> (u16, u16) {
        let chars: Vec<char> = needle.chars().collect();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let matches = chars.iter().enumerate().all(|(i, c)| {
                    let cx = x + i as u16;
                    cx < buffer.area.width && buffer[(cx, y)].symbol() == c.to_string()
                });
                if matches {
                    return (x, y);
                }
            }
        }
        panic!("'{needle}' not found in rendered buffer");
    };

    let (x, y) = find_cell("Second line");
    assert_eq!(
        buffer[(x, y)].fg,
        active_color,
        "the active line's waiting text is bright"
    );
    assert!(
        buffer[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD),
        "the active line is bold"
    );
    let (x, y) = find_cell("First line");
    assert_eq!(
        buffer[(x, y)].fg,
        neighbor_color,
        "lines one step away fade to subtext"
    );
}

#[test]
fn marquee_slides_one_column_per_step_and_wraps_with_a_gap() {
    use ratatui::style::Style;
    let bold = Style::default().add_modifier(ratatui::style::Modifier::BOLD);
    let dim = Style::default();
    let parts = [("ABCDE", bold), ("xy", dim)];
    let text_at = |step: usize| -> String {
        super::marquee_spans(&parts, 5, step)
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    };
    // Ciclo completo: "ABCDExy" + 3 colunas de respiro = 10 passos.
    assert_eq!(text_at(0), "ABCDE");
    assert_eq!(text_at(1), "BCDEx");
    assert_eq!(text_at(2), "CDExy");
    assert_eq!(text_at(3), "DExy ");
    assert_eq!(text_at(7), "   AB", "wraps around through the gap");
    assert_eq!(text_at(8), "  ABC");
    assert_eq!(text_at(10), "ABCDE", "cycle length is text + gap");

    // Estilos preservados por trecho: no passo 1, "BCDE" em bold e "x" dim.
    let spans = super::marquee_spans(&parts, 5, 1);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].content.as_ref(), "BCDE");
    assert_eq!(spans[0].style, bold);
    assert_eq!(spans[1].content.as_ref(), "x");

    // Caractere largo (2 colunas) nunca é cortado ao meio: a coluna órfã
    // vira espaço.
    let wide = [("A漢B", dim)];
    let at = |step: usize| -> String {
        super::marquee_spans(&wide, 3, step)
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    };
    assert_eq!(at(0), "A漢");
    assert_eq!(at(2), " B ", "half of a wide char becomes a space");
}
