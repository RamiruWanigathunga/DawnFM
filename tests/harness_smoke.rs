//! Smoke test for Dawn FM test harness and dual-target crate configuration.

mod common;

use std::io::{Read, Seek, SeekFrom};

use dawnfm::images::{HTTP_CACHE_MAX_BYTES, HTTP_CACHE_MAX_ENTRIES, TEXTURE_BUDGET};
use dawnfm::model::pick_thumb;
use dawnfm::stream::{AHEAD, BEHIND, BLOCK, HEAD_BLOCKS, WINDOWED_FROM};
use dawnfm::{data_dir, DATA_NAME, DEFAULT_SIZE, MIN_SIZE};

use common::{
    assert_bounds_valid, assert_within_tolerance, feed_block, feed_streaming_chunks,
    make_synthetic_stream, rapid_seek_test, run_headless_egui_frame, sample_album_tracks,
    sample_byte_block, sample_music_video, sample_pcm_stereo_sine, sample_thumbs, sample_track,
    sample_tracks, verify_range_read, HeadlessRunner, MockBackendQueue, MockCatalog,
    SAMPLE_INNO_SETUP_SCRIPT, SAMPLE_TOKEN,
};

#[test]
fn test_crate_exports_and_constants_resolve() {
    // Verify core crate constants
    assert_eq!(DEFAULT_SIZE, [1200.0, 780.0]);
    assert_eq!(MIN_SIZE, [760.0, 480.0]);
    assert_eq!(DATA_NAME, "DawnFM");
    assert!(!data_dir().as_os_str().is_empty());

    // Verify stream module constants
    assert_eq!(WINDOWED_FROM, 12 * 1024 * 1024);
    assert_eq!(AHEAD, 48);
    assert_eq!(BEHIND, 16);
    assert_eq!(BLOCK, 64 * 1024);
    assert_eq!(HEAD_BLOCKS, 8);

    // Verify image cache constants
    assert_eq!(HTTP_CACHE_MAX_ENTRIES, 40);
    assert_eq!(HTTP_CACHE_MAX_BYTES, 16 * 1024 * 1024);
    assert_eq!(TEXTURE_BUDGET, 36 * 1024 * 1024);
}

#[test]
fn test_fixtures_generation() {
    let track = sample_track();
    assert_eq!(track.id, "test_track_001");
    assert_eq!(track.title, "Blinding Lights");
    assert_eq!(track.artist_line(), "The Weeknd");
    assert_eq!(track.duration, Some(200));

    let mv = sample_music_video();
    assert!(mv.music_video);

    let tracks = sample_tracks(20);
    assert_eq!(tracks.len(), 20);

    let album_tracks = sample_album_tracks();
    assert_eq!(album_tracks.len(), 14);

    let thumbs = sample_thumbs();
    assert_eq!(thumbs.len(), 3);
    assert_eq!(pick_thumb(&thumbs, 500).unwrap().width, 544);

    let sine = sample_pcm_stereo_sine(0.1, 44100, 440.0);
    assert_eq!(sine.len(), 4410 * 2); // 4410 stereo frames = 8820 samples

    let block = sample_byte_block(42, 1024);
    assert_eq!(block.len(), 1024);

    assert!(SAMPLE_INNO_SETUP_SCRIPT.contains("ArchitecturesAllowed=x64compatible"));
    assert_eq!(SAMPLE_TOKEN.starts_with("ya29"), true);
}

#[test]
fn test_headless_egui_frame_runner() {
    let mut executed = false;
    run_headless_egui_frame(|ctx| {
        ctx.root_ui(|ui| {
            ui.label("Dawn FM Headless Test Frame");
            executed = true;
        });
    });
    assert!(executed, "Headless egui frame closure should execute");
}

#[test]
fn test_headless_runner_stateful_and_input_stepper() {
    let mut runner = HeadlessRunner::new().with_size(1024.0, 768.0);
    assert_eq!(runner.current_time, 0.0);
    assert_eq!(runner.screen_size, egui::vec2(1024.0, 768.0));

    runner.step_time(1.5);
    assert_eq!(runner.current_time, 1.5);

    runner.move_mouse(egui::pos2(150.0, 200.0));
    runner.click_mouse(egui::pos2(150.0, 200.0));
    runner.press_key(egui::Key::Space);

    let (_clicked, _output) = runner.run_frame(|ctx| {
        let mut button_clicked = false;
        ctx.root_ui(|ui| {
            if ui.button("Play").clicked() {
                button_clicked = true;
            }
        });
        button_clicked
    });

    assert_bounds_valid(runner.context().input(|i| i.viewport_rect()));
    assert_within_tolerance(runner.current_time, 1.5, 1e-6, "Virtual time should match");

    // Run multi-frame sequence
    let mut frames_counted = 0;
    runner.run_frames(5, 0.1, |_ctx, _frame_idx| {
        frames_counted += 1;
    });
    assert_eq!(frames_counted, 5);
    assert_within_tolerance(runner.current_time, 2.0, 1e-6, "Time stepped by 5*0.1s");
}

#[test]
fn test_mock_stream_synthetic_generator_unwindowed() {
    // 4MB stream is below WINDOWED_FROM (12MB) -> unwindowed
    let (shared, mut reader) = make_synthetic_stream(4 * 1024 * 1024, false);
    assert!(!shared.is_windowed(), "4MB stream should not be windowed");
    assert_eq!(shared.total, 4 * 1024 * 1024);

    let block_count = shared.block_count();
    assert_eq!(block_count, 64); // 4MB / 64KB = 64 blocks

    // Feed first block and read from it
    feed_block(&shared, 0);
    assert!(shared.is_block_present(0));

    let data = verify_range_read(&mut reader, 0, 100).expect("Read range should succeed");
    assert_eq!(data.len(), 100);

    // Rapid seek test on block 0
    rapid_seek_test(&mut reader, &[0, 50, 10, 80, 0]).expect("Rapid seeks on block 0 should succeed");
}

#[test]
fn test_mock_stream_synthetic_generator_windowed() {
    // 15MB stream is above WINDOWED_FROM (12MB) -> windowed
    let (shared, mut reader) = make_synthetic_stream(15 * 1024 * 1024, true);
    assert!(shared.is_windowed(), "15MB stream must be windowed");
    assert_eq!(shared.total, 15 * 1024 * 1024);

    let total_blocks = shared.block_count();
    assert_eq!(total_blocks, 240); // 15MB / 64KB = 240 blocks

    // Feed first 10 blocks in chunks
    for i in 0..10 {
        feed_streaming_chunks(&shared, i, 16 * 1024);
        assert!(shared.is_block_present(i));
    }

    // Read across blocks
    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut buf = vec![0u8; 128 * 1024]; // 2 blocks
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf.len(), 128 * 1024);
}

#[test]
fn test_mock_backend_catalog_and_queue() {
    let album = MockCatalog::mock_album("album_123", "Dawn FM", "The Weeknd", 14);
    assert_eq!(album.id, "album_123");
    assert_eq!(album.title, "Dawn FM");
    assert_eq!(album.tracks.len(), 14);
    assert!(album.is_album);

    let artist = MockCatalog::mock_artist_page("artist_123", "The Weeknd", 5);
    assert_eq!(artist.name, "The Weeknd");
    assert_eq!(artist.top.len(), 5);
    assert_eq!(artist.albums.len(), 3);

    let search = MockCatalog::mock_search_results("Weeknd");
    assert_eq!(search.query, "Weeknd");
    assert_eq!(search.tracks.len(), 10);

    let queue = MockBackendQueue::new();
    queue.record_req("Search('Weeknd')");
    queue.record_req("Album('album_123')");
    assert_eq!(queue.recorded_reqs(), vec!["Search('Weeknd')", "Album('album_123')"]);

    queue.clear_recorded();
    assert!(queue.recorded_reqs().is_empty());

    // Send and drain response
    queue.send_resp(dawnfm::backend::Resp::Search(search));
    let drained = queue.drain_resp();
    assert_eq!(drained.len(), 1);
    match &drained[0] {
        dawnfm::backend::Resp::Search(res) => assert_eq!(res.query, "Weeknd"),
        _ => panic!("Expected Search response"),
    }
}
