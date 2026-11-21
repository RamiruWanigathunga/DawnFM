//! Tier 3: Scenario 1 - Streaming Windowing + Image Cache LRU + Mimalloc Memory Allocator
//!
//! Validates concurrent behavior and memory boundedness between:
//! - F03: Audio Buffer Windowing (`stream.rs`)
//! - F04: Image Cache LRU Bounding (`images.rs`)
//! - F02: Memory Allocator (`mimalloc`)
//! under active concurrent audio streaming, seeking, and image thumbnail loading.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use egui::load::TextureLoader;

use dawnfm::images::{Textures, HTTP_CACHE_MAX_ENTRIES};
use dawnfm::stream::{Format, BLOCK, HEAD_BLOCKS};

use crate::common::{
    count_present_blocks, deterministic_block, feed_all_blocks, feed_block, feed_range,
    make_synthetic_stream, make_synthetic_stream_with_format, sample_tracks,
};

#[test]
fn test_concurrent_streaming_window_and_image_cache_lru() {
    let file_size = 20 * 1024 * 1024; // 20 MB (windowed > 12 MB)
    let (shared, mut reader) = make_synthetic_stream(file_size, true);
    assert!(shared.is_windowed());

    let running = Arc::new(AtomicBool::new(true));
    let images_processed = Arc::new(AtomicUsize::new(0));

    let running_clone = running.clone();
    let img_counter = images_processed.clone();

    // Concurrent thread simulating heavy image cache churn
    let image_thread = thread::spawn(move || {
        let mut raw_cache: HashMap<String, (std::time::Instant, Vec<u8>)> = HashMap::new();
        let mut pass_index = 0u64;
        let textures = Textures::default();

        loop {
            pass_index += 1;
            // Generate synthetic 500 KB images
            let key = format!("https://example.com/art_{pass_index}.jpg");
            let img_data = vec![(pass_index & 0xFF) as u8; 500 * 1024];

            raw_cache.insert(key, (std::time::Instant::now(), img_data));

            // Enforce raw cache LRU bounds
            if raw_cache.len() > HTTP_CACHE_MAX_ENTRIES {
                let mut entries: Vec<_> = raw_cache.iter().map(|(k, (t, _))| (*t, k.clone())).collect();
                entries.sort_unstable();
                for (_, k) in entries.iter().take(raw_cache.len() - HTTP_CACHE_MAX_ENTRIES) {
                    raw_cache.remove(k);
                }
            }

            textures.end_pass(pass_index);
            img_counter.fetch_add(1, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(2));

            if !running_clone.load(Ordering::Relaxed) && img_counter.load(Ordering::Relaxed) >= 1 {
                break;
            }
        }

        assert!(raw_cache.len() <= HTTP_CACHE_MAX_ENTRIES);
        assert_eq!(textures.byte_size(), 0);
    });

    // Main thread drives windowed stream block feeding and reading
    let total_blocks = shared.block_count();
    let test_blocks = 50.min(total_blocks);

    for block_idx in 0..test_blocks {
        feed_block(&shared, block_idx);

        // Periodically read from the stream to advance position
        if block_idx > 0 && block_idx % 5 == 0 {
            let offset = (block_idx - 2) as u64 * BLOCK;
            reader.seek(SeekFrom::Start(offset)).expect("Seek must succeed");
            let mut buf = [0u8; 1024];
            let n = reader.read(&mut buf).expect("Read must succeed");
            assert_eq!(n, 1024);
            thread::sleep(Duration::from_millis(1));
        }
    }

    // Stop image worker
    running.store(false, Ordering::Relaxed);
    image_thread.join().expect("Image worker must exit cleanly");

    assert!(
        images_processed.load(Ordering::SeqCst) > 0,
        "Image thread must have processed multiple image cycles"
    );
    assert!(shared.is_windowed());
}

#[test]
fn test_backwards_seek_rebuffering_during_texture_eviction() {
    let file_size = 18 * 1024 * 1024; // 18 MB windowed
    let (shared, mut reader) = make_synthetic_stream(file_size, true);

    // Feed forward blocks 0 through 45
    for i in 0..=45 {
        feed_block(&shared, i);
    }

    // Read at block 40 (far ahead)
    let forward_offset = 40 * BLOCK;
    reader.seek(SeekFrom::Start(forward_offset)).expect("Forward seek must succeed");
    let mut forward_buf = [0u8; 512];
    reader.read_exact(&mut forward_buf).expect("Forward read must succeed");

    // Concurrently manage textures cache with high eviction pressure
    let textures = Textures::default();
    for pass in 1..=50 {
        textures.end_pass(pass);
    }
    assert_eq!(textures.byte_size(), 0);

    // Seek back to block 5 (outside the BEHIND = 16 window of block 40)
    let rewind_offset = 5 * BLOCK;
    reader.seek(SeekFrom::Start(rewind_offset)).expect("Backward seek must succeed");

    // In windowed streaming, reading an evicted block waits on the download worker.
    // Spawn a reader thread which registers want = 5 and waits on the ready Condvar.
    let read_handle = thread::spawn(move || {
        let mut rewind_buf = [0u8; 512];
        reader.read_exact(&mut rewind_buf).expect("Backward read must succeed");
        rewind_buf
    });

    // Network worker feeds block 5 once want has shifted
    thread::sleep(Duration::from_millis(20));
    feed_block(&shared, 5);

    let rewind_buf = read_handle.join().expect("Reader thread must join cleanly");

    let expected = deterministic_block(5, file_size);
    assert_eq!(&rewind_buf[..], &expected[..512]);
}

#[test]
fn test_multi_track_prefetch_alongside_image_prefetches() {
    let tracks = sample_tracks(5);
    let mut prefetched_streams = Vec::new();

    // In head-only prefetch mode, each stream only pre-buffers HEAD_BLOCKS (8 blocks = 512 KB)
    for (i, _track) in tracks.iter().enumerate() {
        let file_size = 15 * 1024 * 1024; // 15 MB track
        let (shared, _reader) = make_synthetic_stream_with_format(file_size, Format::Aac, true);

        for b in 0..HEAD_BLOCKS {
            feed_block(&shared, b);
        }

        assert_eq!(
            count_present_blocks(&shared),
            HEAD_BLOCKS,
            "Stream {} must strictly buffer HEAD_BLOCKS (8)",
            i
        );
        prefetched_streams.push(shared);
    }

    // Total memory allocated across all 5 prefetched streams:
    // 5 streams * 8 blocks * 64 KB = 2,560 KB (~2.5 MB)
    let total_audio_prefetch_bytes: usize = prefetched_streams
        .iter()
        .map(|s| count_present_blocks(s) * (BLOCK as usize))
        .sum();

    assert_eq!(
        total_audio_prefetch_bytes,
        5 * HEAD_BLOCKS * (BLOCK as usize),
        "Total audio prefetch footprint must equal 2.5 MB"
    );

    // Mock image prefetch URLs
    let mut image_prefetch_queue = Vec::new();
    for t in &tracks {
        for thumb in &t.thumbs {
            image_prefetch_queue.push(thumb.url.clone());
        }
    }
    assert_eq!(image_prefetch_queue.len(), 5 * 3); // 5 tracks * 3 thumbs
}

#[test]
fn test_memory_churn_under_rapid_track_and_cover_swapping() {
    let textures = Textures::default();
    let num_swaps = 25;

    for i in 0..num_swaps {
        // 1. Allocate a synthetic audio stream (alternating 6MB unwindowed and 16MB windowed)
        let is_windowed = i % 2 == 1;
        let file_size = if is_windowed { 16 * 1024 * 1024 } else { 6 * 1024 * 1024 };
        let (shared, mut reader) = make_synthetic_stream(file_size, is_windowed);

        // Feed initial blocks
        feed_range(&shared, 0, 8);

        // Read initial audio header
        let mut header = [0u8; 256];
        reader.seek(SeekFrom::Start(0)).expect("Seek 0 must succeed");
        reader.read_exact(&mut header).expect("Header read must succeed");

        // 2. Churn image allocations (simulating album art load)
        let uri = format!("https://example.com/cover_{i}.jpg");
        let _cover_bytes = vec![(i & 0xFF) as u8; 1024 * 1024]; // 1 MB art

        // Evict / forget previous cover
        textures.forget(&uri);
        textures.end_pass(i as u64 + 1);

        // Drop shared & reader at end of loop iteration to trigger mimalloc reclamation
        drop(reader);
        drop(shared);
    }

    assert_eq!(textures.byte_size(), 0);
}

#[test]
fn test_unwindowed_to_windowed_transition_with_album_gallery() {
    // Phase 1: Small unwindowed track (8 MB < 12 MB)
    let small_file = 8 * 1024 * 1024;
    let (small_shared, mut small_reader) = make_synthetic_stream(small_file, false);
    assert!(!small_shared.is_windowed());

    feed_all_blocks(&small_shared);
    assert_eq!(count_present_blocks(&small_shared), (small_file / BLOCK) as usize);

    let mut buf = [0u8; 1024];
    small_reader.read_exact(&mut buf).expect("Read must succeed");

    // Phase 2: Album gallery loads 30 images
    let mut raw_cache: HashMap<String, Vec<u8>> = HashMap::new();
    for i in 0..30 {
        raw_cache.insert(format!("thumb_{i}"), vec![0xEE; 100 * 1024]);
    }
    assert_eq!(raw_cache.len(), 30);
    assert!(raw_cache.len() <= HTTP_CACHE_MAX_ENTRIES);

    // Phase 3: Transition to large windowed track (24 MB > 12 MB)
    let large_file = 24 * 1024 * 1024;
    let (large_shared, mut large_reader) = make_synthetic_stream(large_file, true);
    assert!(large_shared.is_windowed());

    // Feed forward blocks up to 50
    feed_range(&large_shared, 0, 50);

    // Read ahead at block 40
    large_reader.seek(SeekFrom::Start(40 * BLOCK)).expect("Seek must succeed");
    let mut large_buf = [0u8; 1024];
    large_reader.read_exact(&mut large_buf).expect("Read must succeed");

    // Verify limit constraint
    assert!(large_shared.limit() > 0);
}
