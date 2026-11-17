//! In-memory mock backend data providers and synthetic catalog models.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use crossbeam_channel::{unbounded, Receiver, Sender};

use dawnfm::backend::{ArtistPage, Collection, Explore, Resp, SearchResults};
use dawnfm::model::{Card, Link, Target};
use super::fixtures::{sample_album_tracks, sample_thumbs, sample_tracks};

/// Synthetic catalog generators for testing pages and UI components.
pub struct MockCatalog;

impl MockCatalog {
    /// Generates an in-memory album `Collection` fixture with realistic metadata.
    pub fn mock_album(id: &str, title: &str, artist_name: &str, track_count: usize) -> Collection {
        let tracks = if track_count == 14 {
            sample_album_tracks()
        } else {
            sample_tracks(track_count)
        };

        Collection {
            id: id.to_string(),
            is_album: true,
            save_id: Some(format!("VL{}", id)),
            title: title.to_string(),
            subtitle: format!("Album • 2024 • {} songs", tracks.len()),
            artists: vec![Link {
                name: artist_name.to_string(),
                id: Some(format!("UC_{}", artist_name.replace(' ', "_"))),
            }],
            description: Some(format!("Sample description for album {}", title)),
            thumbs: sample_thumbs(),
            tracks,
            set_ids: Vec::new(),
            count: Some(track_count as u64),
            owned: false,
        }
    }

    /// Generates an in-memory playlist `Collection` fixture.
    pub fn mock_playlist(id: &str, title: &str, track_count: usize) -> Collection {
        let tracks = sample_tracks(track_count);
        let set_ids = (0..track_count).map(|i| Some(format!("set_video_{:04}", i))).collect();

        Collection {
            id: id.to_string(),
            is_album: false,
            save_id: None,
            title: title.to_string(),
            subtitle: format!("Playlist • {} songs", tracks.len()),
            artists: vec![Link {
                name: "Test Playlist Creator".to_string(),
                id: Some("UC_creator_001".to_string()),
            }],
            description: Some(format!("Description for playlist {}", title)),
            thumbs: sample_thumbs(),
            tracks,
            set_ids,
            count: Some(track_count as u64),
            owned: true,
        }
    }

    /// Generates an in-memory `ArtistPage` fixture with top songs, releases, and similar artists.
    pub fn mock_artist_page(id: &str, name: &str, top_song_count: usize) -> ArtistPage {
        ArtistPage {
            id: id.to_string(),
            name: name.to_string(),
            banner: sample_thumbs(),
            subscribers: Some(15_400_000),
            description: Some(format!("Biography and career overview of {}.", name)),
            top: sample_tracks(top_song_count),
            top_playlist: Some(format!("VL_top_{}", id)),
            albums: vec![
                Self::mock_card("Dawn FM", "Album • 2022"),
                Self::mock_card("After Hours", "Album • 2020"),
                Self::mock_card("Starboy", "Album • 2016"),
            ],
            playlists: vec![
                Self::mock_card("The Weeknd Essentials", "Playlist • 50 songs"),
                Self::mock_card("Next Wave", "Playlist • 30 songs"),
            ],
            similar: vec![
                Self::mock_card("Post Malone", "Artist • 24M subscribers"),
                Self::mock_card("Dua Lipa", "Artist • 22M subscribers"),
            ],
            photo: sample_thumbs(),
            following: None,
        }
    }

    /// Generates an in-memory `SearchResults` fixture for search page tests.
    pub fn mock_search_results(query: &str) -> SearchResults {
        SearchResults {
            query: query.to_string(),
            tracks: sample_tracks(10),
            albums: vec![
                Self::mock_card(&format!("{} Album Vol 1", query), "Album • 2023"),
                Self::mock_card(&format!("{} Deluxe Edition", query), "Album • 2024"),
            ],
            artists: vec![
                Self::mock_card(&format!("{} Official", query), "Artist • 1M subscribers"),
            ],
            playlists: vec![
                Self::mock_card(&format!("Best of {}", query), "Playlist • 25 songs"),
            ],
        }
    }

    /// Generates an in-memory `Explore` page fixture.
    pub fn mock_explore() -> Explore {
        Explore {
            new_albums: vec![
                Self::mock_card("New Release 1", "Album • 2024"),
                Self::mock_card("New Release 2", "Album • 2024"),
            ],
            charts: vec![
                Self::mock_card("Top 100 Global", "Chart • Updated today"),
                Self::mock_card("Top 100 USA", "Chart • Updated today"),
            ],
            top: sample_tracks(10),
        }
    }

    /// Helper to create a sample Card.
    pub fn mock_card(title: &str, subtitle: &str) -> Card {
        Card {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            target: Target::Album("MPREb_sample_target".to_string()),
            thumbs: sample_thumbs(),
            round: false,
            links: Vec::new(),
        }
    }

    /// Creates a headless `dawnfm::ui::App` instance with sample tracks loaded and `np_open` enabled.
    pub fn mock_headless_app() -> dawnfm::ui::App {
        let mut app = dawnfm::ui::App::mock_headless();
        let tracks = sample_tracks(5);
        app.queue.replace(tracks, 0, None);
        app.np_open = true;
        app.np_tab = dawnfm::ui::NpTab::Art;
        app
    }
}

/// Simulated mock backend message bus tracking requests and feeding responses.
pub struct MockBackendQueue {
    pub tx: Sender<Resp>,
    pub rx: Receiver<Resp>,
    pub recorded_requests: Arc<Mutex<Vec<String>>>,
}

impl MockBackendQueue {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            tx,
            rx,
            recorded_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Records the name or debug representation of a request.
    pub fn record_req(&self, name: &str) {
        self.recorded_requests.lock().unwrap().push(name.to_string());
    }

    /// Returns a snapshot of recorded requests.
    pub fn recorded_reqs(&self) -> Vec<String> {
        self.recorded_requests.lock().unwrap().clone()
    }

    /// Clears recorded requests.
    pub fn clear_recorded(&self) {
        self.recorded_requests.lock().unwrap().clear();
    }

    /// Sends a response into the receiver channel.
    pub fn send_resp(&self, resp: Resp) {
        let _ = self.tx.send(resp);
    }

    /// Drains all responses currently queued in the channel.
    pub fn drain_resp(&self) -> Vec<Resp> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx.try_recv() {
            out.push(r);
        }
        out
    }
}

impl Default for MockBackendQueue {
    fn default() -> Self {
        Self::new()
    }
}
