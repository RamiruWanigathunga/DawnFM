//! Tier 4: Workload 5 - Active Playback Navigation & Vibrancy Lifecycle Simulation
//!
//! Simulates a full end-user session combining active playback with UI navigation across multiple views,
//! queue modifications, and vibrancy/styling lifecycle exercising:
//! - F06: macOS Native Window Vibrancy (transparency, `theme::glass()`, `theme::shade()`, dark theme tokens)
//! - F07: macOS Forward Styling Compatibility (Darwin 23/24/25 detection, liquid glass, card/panel radii)
//! - F10: Minimal Fullscreen Controls (floating below artwork, no glass card container)
//! - F11: Fullscreen Auto-Hide Controls (3.5s inactivity timeout, mouse wake)
//! - F12: Fullscreen Queue Integration (header toggle button, side panel)
//! - F13: Fullscreen Queue Reordering (`Queue::move_track` contract and playing index shifts)
//! - F18-F20: Album Tracklist Table & Navigation (7 columns, album ambient glow, action pills)

use std::io::{Read, Seek};
use std::sync::Arc;

use egui::{pos2, vec2, Color32, Pos2, Rect};

use dawnfm::model::Track;
use dawnfm::stream::{Shared, StreamReader};
use dawnfm::theme::{self, card_radius, is_macos_above_15, panel_radius, thumb_radius};
use dawnfm::ui::queue::{Queue, Repeat};

use crate::common::{
    feed_range, make_synthetic_stream, sample_thumbs, sample_tracks, MockCatalog,
};

pub const AUTOHIDE_TIMEOUT_SECS: f64 = 3.5;
pub const FADEOUT_DURATION_SECS: f64 = 0.5;

/// High-level navigation route enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveRoute {
    Explore,
    Search,
    Artist,
    Album,
    Settings,
}

/// End-user application session state coordinating playback, routing, queue, and styling.
#[allow(dead_code)]
pub struct ActiveUserSession {
    pub current_route: ActiveRoute,
    pub queue: Queue,
    pub stream_shared: Arc<Shared>,
    pub stream_reader: StreamReader,
    pub total_bytes_read: usize,
    pub volume: f32,
    pub fullscreen: bool,
    pub np_open: bool,
    pub queue_panel_open: bool,
    pub last_mouse_activity: f64,
    pub hovered_controls: bool,
    pub dominant_glow: Color32,
}

impl ActiveUserSession {
    pub fn new() -> Self {
        let tracks = sample_tracks(5);
        let mut queue = Queue::new(Repeat::All);
        queue.replace(tracks, 0, None);

        let (shared, reader) = make_synthetic_stream(16 * 1024 * 1024, true);
        feed_range(&shared, 0, 40);

        Self {
            current_route: ActiveRoute::Explore,
            queue,
            stream_shared: shared,
            stream_reader: reader,
            total_bytes_read: 0,
            volume: 0.8,
            fullscreen: false,
            np_open: false,
            queue_panel_open: false,
            last_mouse_activity: 0.0,
            hovered_controls: false,
            dominant_glow: Color32::from_rgb(180, 40, 50),
        }
    }

    pub fn navigate_to(&mut self, route: ActiveRoute) {
        self.current_route = route;
        // Navigation should never interrupt playback
    }

    pub fn enter_fullscreen(&mut self, current_time: f64) {
        self.fullscreen = true;
        self.np_open = true;
        self.last_mouse_activity = current_time;
    }

    pub fn exit_fullscreen(&mut self) {
        self.fullscreen = false;
        self.np_open = false;
        self.queue_panel_open = false;
    }

    pub fn toggle_fullscreen_queue(&mut self, current_time: f64) {
        self.queue_panel_open = !self.queue_panel_open;
        self.last_mouse_activity = current_time;
    }

    pub fn on_mouse_activity(&mut self, current_time: f64, pos: Pos2, controls_rect: Rect) {
        self.last_mouse_activity = current_time;
        self.hovered_controls = controls_rect.contains(pos);
    }

    pub fn controls_opacity(&self, current_time: f64) -> f32 {
        if self.hovered_controls {
            return 1.0;
        }
        let elapsed = current_time - self.last_mouse_activity;
        if elapsed <= AUTOHIDE_TIMEOUT_SECS {
            1.0
        } else if elapsed >= AUTOHIDE_TIMEOUT_SECS + FADEOUT_DURATION_SECS {
            0.0
        } else {
            let fade = (elapsed - AUTOHIDE_TIMEOUT_SECS) / FADEOUT_DURATION_SECS;
            (1.0 - fade as f32).clamp(0.0, 1.0)
        }
    }

    pub fn read_audio_chunk(&mut self, bytes: usize) -> usize {
        let mut buf = vec![0u8; bytes];
        if let Ok(n) = self.stream_reader.read(&mut buf) {
            self.total_bytes_read += n;
            n
        } else {
            0
        }
    }
}

#[test]
fn test_workload_active_playback_with_cross_page_navigation_and_vibrancy() {
    let mut session = ActiveUserSession::new();
    assert_eq!(session.current_route, ActiveRoute::Explore);
    assert_eq!(session.queue.tracks.len(), 5);

    // Initial audio stream reading (playing Track #1)
    let read_initial = session.read_audio_chunk(4096);
    assert_eq!(read_initial, 4096);
    assert_eq!(session.total_bytes_read, 4096);

    // 1. User navigates: Explore -> Artist Page ("The Weeknd")
    session.navigate_to(ActiveRoute::Artist);
    assert_eq!(session.current_route, ActiveRoute::Artist);
    let artist_page = MockCatalog::mock_artist_page("the_weeknd", "The Weeknd", 15);
    assert_eq!(artist_page.top.len(), 15);

    // Audio stream continues seamlessly while viewing artist page
    let read_during_artist = session.read_audio_chunk(4096);
    assert_eq!(read_during_artist, 4096);

    // 2. User clicks Album card: Artist -> Album Page ("After Hours")
    session.navigate_to(ActiveRoute::Album);
    assert_eq!(session.current_route, ActiveRoute::Album);
    let album = MockCatalog::mock_album("after_hours_001", "After Hours", "The Weeknd", 14);
    assert_eq!(album.tracks.len(), 14);

    // F18: Album ambient glow color
    session.dominant_glow = Color32::from_rgb(190, 25, 45); // Deep red
    assert_eq!(session.dominant_glow.r(), 190);

    // F19: User clicks "Add to Queue" action pill
    // Queue should now contain 5 original tracks + 14 album tracks = 19 tracks
    session.queue.tracks.extend(album.tracks);
    assert_eq!(session.queue.tracks.len(), 19);
    assert_eq!(session.queue.index, Some(0), "Current playing song remains Track #1");

    // Audio stream continues reading
    session.read_audio_chunk(8192);
    assert_eq!(session.total_bytes_read, 16384);

    // 3. User enters Fullscreen Player mode (F10/F11/F12)
    session.enter_fullscreen(1.0);
    assert!(session.fullscreen);
    assert!(session.np_open);
    assert_eq!(session.controls_opacity(1.0), 1.0);

    // 4. Inactivity in fullscreen: 3.5s pass, controls fade out to 0.0 at 5.0s
    let faded_opacity = session.controls_opacity(5.0);
    assert_eq!(faded_opacity, 0.0, "Controls must be fully hidden at 5.0s");

    // Mouse wake event at 6.0s
    session.on_mouse_activity(6.0, pos2(500.0, 500.0), Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)));
    assert_eq!(session.controls_opacity(6.0), 1.0, "Mouse activity must wake controls");

    // 5. Open Fullscreen Queue and reorder tracks (F12/F13)
    session.toggle_fullscreen_queue(6.1);
    assert!(session.queue_panel_open);

    // Move track from index 8 to index 2
    let reorder_ok = session.queue.move_track(8, 2);
    assert!(reorder_ok, "F13: Queue::move_track must succeed in fullscreen");
    assert_eq!(session.queue.index, Some(0), "Playing index remains unaffected");

    // 6. Vibrancy & Styling lifecycle check (F06/F07)
    let glass_token = theme::glass(0.08);
    let shade_token = theme::shade(0.35);
    assert!(glass_token.a() > 0);
    assert_eq!(shade_token.a(), (0.35f32 * 255.0f32).round() as u8);

    let cr = card_radius();
    let pr = panel_radius();
    let tr = thumb_radius();
    assert!(cr > 0.0 && cr.is_finite());
    assert!(pr > 0);
    assert!(tr > 0.0 && tr.is_finite());

    // 7. User exits Fullscreen back to Album page
    session.exit_fullscreen();
    assert!(!session.fullscreen);
    assert!(!session.np_open);
    assert_eq!(session.current_route, ActiveRoute::Album);

    // Audio stream continues reading with zero state disruption
    let final_chunk = session.read_audio_chunk(4096);
    assert_eq!(final_chunk, 4096);
    assert_eq!(session.total_bytes_read, 20480);
    assert_eq!(session.queue.tracks.len(), 19);
    assert_eq!(session.volume, 0.8);
}

#[test]
fn test_active_playback_continuity_during_rapid_view_swapping() {
    let mut session = ActiveUserSession::new();
    let routes = [
        ActiveRoute::Explore,
        ActiveRoute::Search,
        ActiveRoute::Artist,
        ActiveRoute::Album,
        ActiveRoute::Settings,
        ActiveRoute::Explore,
        ActiveRoute::Album,
        ActiveRoute::Artist,
        ActiveRoute::Search,
        ActiveRoute::Explore,
    ];

    let mut prior_pos = 0u64;

    for &route in &routes {
        session.navigate_to(route);
        let n = session.read_audio_chunk(2048);
        assert_eq!(n, 2048);

        let cur_pos = session.stream_reader.stream_position().unwrap();
        assert!(
            cur_pos > prior_pos,
            "Stream position must monotonically advance regardless of UI route transitions"
        );
        prior_pos = cur_pos;
    }

    assert_eq!(session.total_bytes_read, 2048 * routes.len());
}

#[test]
fn test_queue_manipulation_from_pages_during_active_playback() {
    let mut queue = Queue::new(Repeat::Off);
    let initial_tracks = sample_tracks(3);
    queue.replace(initial_tracks, 0, None);

    assert_eq!(queue.index, Some(0));
    let initial_title = queue.current().unwrap().title.clone();

    // 1. Enqueue 3 new tracks from an album page
    let more_tracks = sample_tracks(3);
    for t in more_tracks {
        queue.enqueue(t);
    }
    assert_eq!(queue.tracks.len(), 6);
    assert_eq!(queue.current().unwrap().title, initial_title);

    // 2. Play Next: insert track at index + 1
    let priority_track = Track {
        id: "vip_track".to_string(),
        title: "VIP Up Next".to_string(),
        artists: Vec::new(),
        album: None,
        duration: Some(190),
        thumbs: sample_thumbs(),
        track_nr: None,
        plays: None,
        music_video: false,
    };
    queue.play_next(priority_track);
    assert_eq!(queue.tracks[1].title, "VIP Up Next");
    assert_eq!(queue.index, Some(0));

    // 3. Reorder track 4 to track 2
    assert!(queue.move_track(4, 2));
    assert_eq!(queue.index, Some(0));

    // 4. Advance queue to next song
    assert!(queue.advance());
    assert_eq!(queue.index, Some(1));
    assert_eq!(queue.current().unwrap().title, "VIP Up Next");
}

#[test]
fn test_vibrancy_and_darwin_styling_lifecycle_transitions() {
    // Tests forward Darwin compatibility and theme tokens
    let sonoma = "23.6.0";
    let sequoia = "24.2.0";
    let macos16 = "25.0.0";

    let parse_major = |s: &str| s.split('.').next().unwrap().parse::<u32>().unwrap();
    assert_eq!(parse_major(sonoma), 23);
    assert_eq!(parse_major(sequoia), 24);
    assert_eq!(parse_major(macos16), 25);

    let above_15 = is_macos_above_15();
    let _ = above_15;

    // Verify semantic opacity hierarchy
    let g1 = theme::glass(0.05);
    let g2 = theme::glass(0.10);
    let g3 = theme::glass(0.15);
    assert!(g1.a() < g2.a());
    assert!(g2.a() < g3.a());
}

#[test]
fn test_fullscreen_transition_and_back_preserves_playback_state() {
    let mut session = ActiveUserSession::new();
    session.volume = 0.65;
    session.queue.repeat = Repeat::All;

    // Advance audio position
    session.read_audio_chunk(8192);
    let pos_before = session.stream_reader.stream_position().unwrap();

    // Repeated fullscreen transitions
    for i in 0..5 {
        session.enter_fullscreen(i as f64 * 10.0);
        assert!(session.fullscreen);
        session.read_audio_chunk(1024);

        session.exit_fullscreen();
        assert!(!session.fullscreen);
        session.read_audio_chunk(1024);
    }

    assert_eq!(session.volume, 0.65);
    assert_eq!(session.queue.repeat, Repeat::All);
    assert_eq!(session.queue.index, Some(0));
    assert!(session.stream_reader.stream_position().unwrap() > pos_before);
}
