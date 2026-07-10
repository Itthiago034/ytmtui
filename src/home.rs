use crate::models::{CollectionKind, HomeSection, Playlist, Track};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HomeKey(String);

impl HomeKey {
    pub fn new(provider: &str, kind: &str, id: &str) -> Self {
        Self(format!("{provider}:{kind}:{id}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeCardKind {
    Track,
    Album,
    Playlist,
}

#[derive(Debug, Clone)]
pub enum HomeCardPayload {
    Track(Track),
    Collection(Playlist),
}

#[derive(Debug, Clone)]
pub struct HomeCard {
    pub key: HomeKey,
    pub kind: HomeCardKind,
    pub title: String,
    pub subtitle: String,
    pub duration: String,
    pub artwork_url: Option<String>,
    pub provider: String,
    pub payload: HomeCardPayload,
}

#[derive(Debug, Clone)]
pub struct HomeShelf {
    pub title: String,
    pub cards: Vec<HomeCard>,
}

#[derive(Debug, Clone, Default)]
pub struct HomeView {
    pub shelves: Vec<HomeShelf>,
}

impl HomeView {
    pub fn project(provider: &str, recent: &[Track], sections: &[HomeSection]) -> Self {
        let mut shelves = Vec::new();
        if !recent.is_empty() {
            shelves.push(HomeShelf {
                title: "Continue listening".into(),
                cards: recent
                    .iter()
                    .cloned()
                    .map(|track| HomeCard {
                        key: HomeKey::new("local", "track", &track.video_id),
                        kind: HomeCardKind::Track,
                        title: track.title.clone(),
                        subtitle: track.artist.clone(),
                        duration: track.duration.clone(),
                        artwork_url: track.thumbnail.clone(),
                        provider: provider.into(),
                        payload: HomeCardPayload::Track(track),
                    })
                    .collect(),
            });
        }
        shelves.extend(sections.iter().map(|section| {
            HomeShelf {
                title: section.title.clone(),
                cards: section
                    .items
                    .iter()
                    .cloned()
                    .map(|item| HomeCard {
                        key: HomeKey::new(provider, "collection", &item.browse_id),
                        kind: match item.kind {
                            CollectionKind::Album => HomeCardKind::Album,
                            CollectionKind::Playlist => HomeCardKind::Playlist,
                        },
                        title: item.title.clone(),
                        subtitle: item.subtitle.clone(),
                        duration: String::new(),
                        artwork_url: item.thumbnail.clone(),
                        provider: provider.into(),
                        payload: HomeCardPayload::Collection(item),
                    })
                    .collect(),
            }
        }));
        Self { shelves }
    }

    pub fn len(&self) -> usize {
        self.shelves.iter().map(|shelf| shelf.cards.len()).sum()
    }

    pub fn flat_card(&self, index: usize) -> Option<&HomeCard> {
        self.shelves
            .iter()
            .flat_map(|shelf| &shelf.cards)
            .nth(index)
    }

    pub fn flat_index_of(&self, key: &HomeKey) -> Option<usize> {
        self.shelves
            .iter()
            .flat_map(|shelf| &shelf.cards)
            .position(|card| &card.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{HomeSection, Playlist, Track};

    fn recent_tracks() -> Vec<Track> {
        vec![
            Track {
                video_id: "t1".into(),
                title: "First track".into(),
                ..Default::default()
            },
            Track {
                video_id: "t2".into(),
                title: "Second track".into(),
                ..Default::default()
            },
        ]
    }

    fn provider_sections() -> Vec<HomeSection> {
        vec![
            HomeSection {
                title: "Quick picks".into(),
                items: vec![
                    Playlist {
                        browse_id: "p1".into(),
                        title: "First collection".into(),
                        ..Default::default()
                    },
                    Playlist {
                        browse_id: "p2".into(),
                        title: "Second collection".into(),
                        ..Default::default()
                    },
                ],
            },
            HomeSection {
                title: "Made for you".into(),
                items: vec![Playlist {
                    browse_id: "p3".into(),
                    title: "Third collection".into(),
                    ..Default::default()
                }],
            },
        ]
    }

    #[test]
    fn projection_puts_recent_history_first_and_keeps_provider_order() {
        let view = HomeView::project("ytmusic", &recent_tracks(), &provider_sections());
        assert_eq!(view.shelves[0].title, "Continue listening");
        assert_eq!(view.shelves[1].title, "Quick picks");
        assert_eq!(view.shelves[2].title, "Made for you");
        assert_eq!(view.len(), 5);
        assert!(matches!(
            view.flat_card(0).unwrap().payload,
            HomeCardPayload::Track(_)
        ));
        assert!(matches!(
            view.flat_card(2).unwrap().payload,
            HomeCardPayload::Collection(_)
        ));
    }

    #[test]
    fn stable_keys_distinguish_recent_tracks_from_provider_collections() {
        let view = HomeView::project("ytmusic", &recent_tracks(), &provider_sections());
        assert_eq!(
            view.flat_card(0).unwrap().key,
            HomeKey::new("local", "track", "t1")
        );
        assert_eq!(
            view.flat_card(2).unwrap().key,
            HomeKey::new("ytmusic", "collection", "p1")
        );
        assert_eq!(
            view.flat_index_of(&HomeKey::new("ytmusic", "collection", "p2")),
            Some(3)
        );
    }
}
