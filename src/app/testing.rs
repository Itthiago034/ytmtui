//! Shared fixtures for the submodule test suites.
//!
//! These build `App` values through [`App::new_for_tests`], so they never
//! read the user's real config, cookies, or play history.

use super::*;

pub(super) fn home_sections() -> Vec<crate::models::HomeSection> {
    vec![
        crate::models::HomeSection {
            title: "Quick picks".to_string(),
            items: vec![
                Playlist {
                    browse_id: "VL1".to_string(),
                    title: "First".to_string(),
                    ..Default::default()
                },
                Playlist {
                    browse_id: "VL2".to_string(),
                    title: "Second".to_string(),
                    ..Default::default()
                },
            ],
        },
        crate::models::HomeSection {
            title: "Mixed for you".to_string(),
            items: vec![Playlist {
                browse_id: "VL3".to_string(),
                title: "Third".to_string(),
                ..Default::default()
            }],
        },
    ]
}
pub(super) fn mixed_search_app() -> App {
    let mut app = App::new_for_tests();
    app.search_mixed = true;
    app.songs = vec![
        Track {
            video_id: "s1".to_string(),
            title: "Song one".to_string(),
            ..Default::default()
        },
        Track {
            video_id: "s2".to_string(),
            title: "Song two".to_string(),
            ..Default::default()
        },
    ];
    app.artists = vec![crate::models::Artist {
        browse_id: "UC1".to_string(),
        name: "Artist one".to_string(),
        ..Default::default()
    }];
    app.albums = vec![Playlist {
        browse_id: "MPRE1".to_string(),
        title: "Album one".to_string(),
        ..Default::default()
    }];
    app.playlists = vec![Playlist {
        browse_id: "VLPL1".to_string(),
        title: "Playlist one".to_string(),
        ..Default::default()
    }];
    app
}
pub(super) fn track(id: &str) -> Track {
    Track {
        video_id: id.to_string(),
        title: format!("Track {id}"),
        ..Default::default()
    }
}
pub(super) fn queue_app() -> App {
    let mut app = App::new_for_tests();
    app.section = Section::Fila;
    app.queue = vec![track("a"), track("b"), track("c"), track("d")];
    app.queue_index = Some(1);
    app.current = Some(track("b"));
    app
}
