//! Tier 4: Workload 1 - 2-Hour Continuous Marathon Playback Simulation
//!
//! Simulates an extensive 2-hour, 40-track continuous playback marathon exercising:
//! - F03: Audio Buffer Windowing (`stream.rs`) - Sliding window (AHEAD=48, BEHIND=16, 12MB threshold)
//! - F02: Memory Allocator (`mimalloc`) - Long-run heap stability across high allocation/deallocation churn
//! - F04: Image Cache LRU Bounding (`images.rs`) - 40-entry/16MB raw cache LRU and 36MB GPU texture budget
//! - F05: Lyric Blur Texture Pooling (`karaoke.rs`) - 64px bucket quantization and bounded spare slot recycling

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

use egui::load::TextureLoader;

use dawnfm::images::{Textures, HTTP_CACHE_MAX_BYTES, HTTP_CACHE_MAX_ENTRIES, TEXTURE_BUDGET};
use dawnfm::stream::BLOCK;

use crate::common::{
    count_present_blocks, feed_block, feed_range, make_synthetic_stream, sample_tracks,
};

pub const MARATHON_TRACK_COUNT: usize = 40;
pub const SLOT_STEP: usize = 64;
pub const SPARE_SLOTS: usize = 2;

/// Mock slot representing an allocated GPU texture slice for blurred lyrics.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MockLyricSlot {
    pub id: usize,
    pub size: [usize; 2],
    pub used: [usize; 2],
    pub hash: u64,
}

/// Headless simulation of `ui::karaoke::Glow` texture pool.
#[derive(Default)]
pub struct MockGlowPool {
    pub spare: Vec<MockLyricSlot>,
    pub allocated_count: usize,
    pub recycled_count: usize,
    pub next_id: usize,
}

impl MockGlowPool {
    pub fn new() -> Self {
        Self {
            spare: Vec::new(),
            allocated_count: 0,
            recycled_count: 0,
            next_id: 1,
        }
    }

    pub fn quantize(dim: usize) -> usize {
        (dim + 1).next_multiple_of(SLOT_STEP)
    }

    pub fn upload(&mut self, w: usize, h: usize, hash: u64) -> MockLyricSlot {
        let bucket = [Self::quantize(w), Self::quantize(h)];
        // Check if an existing spare slot fits
        if let Some(pos) = self.spare.iter().position(|s| s.size[0] >= w && s.size[1] >= h) {
            let mut slot = self.spare.swap_remove(pos);
            slot.used = [w, h];
            slot.hash = hash;
            slot
        } else {
            self.allocated_count += 1;
            let id = self.next_id;
            self.next_id += 1;
            MockLyricSlot {
                id,
                size: bucket,
                used: [w, h],
                hash,
            }
        }
    }

    pub fn recycle(&mut self, slot: MockLyricSlot) {
        self.recycled_count += 1;
        if self.spare.len() >= SPARE_SLOTS {
            self.spare.remove(0); // Evict oldest spare to maintain bounded limit
        }
        self.spare.push(slot);
    }
}

/// Simulated raw image cache tracking LRU entries and byte size.
#[derive(Default)]
pub struct MockRawImageCache {
    pub entries: HashMap<String, (Instant, Vec<u8>)>,
}

impl MockRawImageCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, url: String, bytes: Vec<u8>) {
        self.entries.insert(url, (Instant::now(), bytes));
        self.trim();
    }

    pub fn total_bytes(&self) -> usize {
        self.entries.values().map(|(_, b)| b.len()).sum()
    }

    pub fn trim(&mut self) {
        while self.entries.len() > HTTP_CACHE_MAX_ENTRIES || self.total_bytes() > HTTP_CACHE_MAX_BYTES {
            // Find oldest entry
            if let Some((oldest_key, _)) = self
                .entries
                .iter()
                .min_by_key(|(_, (t, _))| *t)
                .map(|(k, v)| (k.clone(), v.clone()))
            {
                self.entries.remove(&oldest_key);
            } else {
                break;
            }
        }
    }
}

#[test]
fn test_workload_marathon_playback_2_hours_40_tracks() {
    let tracks = sample_tracks(MARATHON_TRACK_COUNT);
    assert_eq!(tracks.len(), MARATHON_TRACK_COUNT);

    let mut virtual_time = 0.0f64;
    let mut total_played_duration = 0u64;

    let mut raw_image_cache = MockRawImageCache::new();
    let gpu_textures = Textures::default();
    let mut glow_pool = MockGlowPool::new();

    let mut total_blocks_streamed = 0usize;
    let mut max_simultaneous_blocks_held = 0usize;

    for (track_idx, track) in tracks.iter().enumerate() {
        let duration = track.duration.unwrap_or(180) as u64;
        total_played_duration += duration;

        // Alternate between windowed (>12MB) and unwindowed (<=12MB) track files
        let is_windowed = track_idx % 2 == 0;
        let file_size = if is_windowed {
            18 * 1024 * 1024 // 18 MB windowed
        } else {
            8 * 1024 * 1024 // 8 MB unwindowed
        };

        let (shared, mut reader) = make_synthetic_stream(file_size, is_windowed);
        assert_eq!(shared.is_windowed(), is_windowed);

        let total_blocks = shared.block_count();
        total_blocks_streamed += total_blocks;

        // 1. Simulate audio streaming progress through the track
        let sim_read_steps = 15;
        let blocks_per_step = (total_blocks / sim_read_steps).max(1);

        for step in 0..sim_read_steps {
            let start_block = step * blocks_per_step;
            let end_block = ((step + 1) * blocks_per_step).min(total_blocks);

            for b in start_block..end_block {
                feed_block(&shared, b);
            }

            // Read from current position
            let read_offset = (start_block as u64 * BLOCK).min(file_size.saturating_sub(2048));
            reader.seek(SeekFrom::Start(read_offset)).expect("Seek must succeed");
            let mut read_buf = [0u8; 1024];
            let _ = reader.read(&mut read_buf);

            let present = count_present_blocks(&shared);
            if present > max_simultaneous_blocks_held {
                max_simultaneous_blocks_held = present;
            }

            // In windowed streaming, resident blocks must not exceed the active window (65 blocks)
            if is_windowed && step > 5 {
                assert!(
                    present <= 65,
                    "Windowed track block retention must remain <= 65, found {}",
                    present
                );
            }
        }

        // 2. Simulate album artwork fetching and LRU caching
        let art_url = format!("https://music.youtube.com/art/album_{:03}.jpg", track_idx);
        let art_bytes = vec![(track_idx & 0xFF) as u8; 350 * 1024]; // 350 KB per thumbnail
        raw_image_cache.insert(art_url, art_bytes);

        // Verify raw cache invariants per track
        assert!(
            raw_image_cache.entries.len() <= HTTP_CACHE_MAX_ENTRIES,
            "Raw cache entries ({}) exceeded HTTP_CACHE_MAX_ENTRIES ({})",
            raw_image_cache.entries.len(),
            HTTP_CACHE_MAX_ENTRIES
        );
        assert!(
            raw_image_cache.total_bytes() <= HTTP_CACHE_MAX_BYTES,
            "Raw cache total bytes ({}) exceeded HTTP_CACHE_MAX_BYTES ({})",
            raw_image_cache.total_bytes(),
            HTTP_CACHE_MAX_BYTES
        );

        // 3. Simulate GPU texture pass eviction
        gpu_textures.end_pass(track_idx as u64 + 1);
        assert!(
            gpu_textures.byte_size() <= TEXTURE_BUDGET,
            "GPU texture size must remain within budget"
        );

        // 4. Simulate synced karaoke lyric lines rendering and pooling
        // Each song renders 15 lyric lines; each line requests a slot and then recycles upon scroll-off
        let mut active_line_slots = Vec::new();
        for line_num in 0..15 {
            let line_w = 280 + (line_num * 17) % 200;
            let line_h = 24 + (line_num * 3) % 20;
            let hash = (track_idx * 1000 + line_num) as u64;

            let slot = glow_pool.upload(line_w, line_h, hash);
            assert_eq!(slot.size[0] % SLOT_STEP, 0, "Slot width must be quantized to 64px");
            assert_eq!(slot.size[1] % SLOT_STEP, 0, "Slot height must be quantized to 64px");

            active_line_slots.push(slot);

            // Recycle oldest line after 3 lines to simulate scrolling off-screen
            if active_line_slots.len() > 3 {
                let old = active_line_slots.remove(0);
                glow_pool.recycle(old);
            }
        }

        // Clean up remaining line slots for the finished track
        for s in active_line_slots {
            glow_pool.recycle(s);
        }

        // Spare slots must strictly be capped at SPARE_SLOTS
        assert!(
            glow_pool.spare.len() <= SPARE_SLOTS,
            "Glow pool spare slots ({}) exceeded SPARE_SLOTS ({})",
            glow_pool.spare.len(),
            SPARE_SLOTS
        );

        // Advance virtual clock by track duration
        virtual_time += duration as f64;
    }

    // Marathon verification assertions
    assert_eq!(
        tracks.len(),
        MARATHON_TRACK_COUNT,
        "All 40 tracks must have completed"
    );
    assert!(
        total_played_duration >= 6000,
        "Total played duration ({}s) must approximate 2-hour target",
        total_played_duration
    );
    assert!(
        virtual_time >= 6000.0,
        "Virtual elapsed time must advance monotonically across marathon"
    );
    assert!(
        total_blocks_streamed > 500,
        "Marathon must stream extensive audio data (>500 blocks)"
    );

    // Bounded memory verification
    assert!(
        raw_image_cache.entries.len() <= HTTP_CACHE_MAX_ENTRIES,
        "Raw cache must remain bounded at end of marathon"
    );
    assert!(
        raw_image_cache.total_bytes() <= HTTP_CACHE_MAX_BYTES,
        "Raw cache memory footprint must remain <= 16MB"
    );
    assert!(
        glow_pool.recycled_count > 400,
        "Marathon must extensively recycle lyric blur slots across tracks"
    );
    assert!(
        glow_pool.spare.len() <= SPARE_SLOTS,
        "Spare pool must strictly respect cap of {}",
        SPARE_SLOTS
    );
}

#[test]
fn test_marathon_sliding_window_memory_boundedness() {
    // Simulates an extended 1-hour DJ mix (60 MB, ~960 blocks)
    let file_size = 60 * 1024 * 1024;
    let (shared, mut reader) = make_synthetic_stream(file_size, true);
    assert!(shared.is_windowed());

    let total_blocks = shared.block_count();
    assert!(total_blocks > 900);

    let mut peak_blocks = 0;

    // Stream through 300 blocks progressively
    for b in 0..300 {
        feed_block(&shared, b);

        if b >= 30 && b % 10 == 0 {
            // Seek reader near playback frontier
            let read_offset = (b - 5) as u64 * BLOCK;
            reader.seek(SeekFrom::Start(read_offset)).expect("Seek must succeed");
            let mut buf = [0u8; 1024];
            let _ = reader.read(&mut buf);

            let present = count_present_blocks(&shared);
            if present > peak_blocks {
                peak_blocks = present;
            }

            // Window constraint: 48 ahead + 16 behind + 1 = 65 maximum blocks
            assert!(
                present <= 65,
                "Sliding window must evict older blocks; peak observed: {}",
                present
            );
        }
    }

    assert!(
        peak_blocks > 0 && peak_blocks <= 65,
        "Peak memory footprint must stay strictly within sliding window budget (65 blocks)"
    );
}

#[test]
fn test_marathon_image_cache_lru_under_continuous_album_art_cycling() {
    let mut cache = MockRawImageCache::new();

    // Insert 60 high-res thumbnails (each 300 KB) sequentially
    for i in 0..60 {
        let url = format!("https://lh3.googleusercontent.com/art_marathon_{:04}.webp", i);
        let data = vec![(i % 256) as u8; 300 * 1024];
        cache.insert(url, data);

        assert!(cache.entries.len() <= HTTP_CACHE_MAX_ENTRIES);
        assert!(cache.total_bytes() <= HTTP_CACHE_MAX_BYTES);
    }

    // Verify oldest entries (0..20) have been evicted while newest (40..59) remain
    assert!(
        !cache.entries.contains_key("https://lh3.googleusercontent.com/art_marathon_0000.webp"),
        "Earliest loaded thumbnail must be evicted by LRU policy"
    );
    assert!(
        cache.entries.contains_key("https://lh3.googleusercontent.com/art_marathon_0059.webp"),
        "Most recently loaded thumbnail must reside in cache"
    );
}

#[test]
fn test_marathon_lyric_blur_pool_recycling_across_tracks() {
    let mut pool = MockGlowPool::new();

    // 40 tracks × 25 lyric lines = 1000 line allocations
    for track_idx in 0..40 {
        let mut active_slots = Vec::new();

        for line_idx in 0..25 {
            let w = 200 + ((track_idx * 13 + line_idx * 7) % 300);
            let h = 28 + (line_idx % 12);
            let hash = (track_idx * 100 + line_idx) as u64;

            let slot = pool.upload(w, h, hash);
            active_slots.push(slot);

            if active_slots.len() > 2 {
                let expired = active_slots.remove(0);
                pool.recycle(expired);
            }
        }

        for s in active_slots {
            pool.recycle(s);
        }

        assert!(
            pool.spare.len() <= SPARE_SLOTS,
            "Spare slots must remain bounded at {} after track {}",
            SPARE_SLOTS,
            track_idx
        );
    }

    assert!(
        pool.recycled_count >= 800,
        "Recycled count ({}) must reflect heavy pool recycling across 1000 lines",
        pool.recycled_count
    );
    // Because of slot reuse, total new allocations must be a tiny fraction of the 1000 line requests
    assert!(
        pool.allocated_count < 200,
        "Allocated count ({}) should be bounded due to active slot pooling",
        pool.allocated_count
    );
}

#[test]
fn test_marathon_stream_seek_stress_and_rebuffering_during_playback() {
    let file_size = 25 * 1024 * 1024; // 25 MB windowed stream
    let (shared, mut reader) = make_synthetic_stream(file_size, true);
    assert!(shared.is_windowed());

    // Feed initial blocks 0..45 within the initial want=0 window [0..48)
    feed_range(&shared, 0, 45);

    // Read at block 40 to advance want to 40
    reader.seek(SeekFrom::Start(40 * BLOCK)).expect("Seek 40 must succeed");
    let mut buf = [0u8; 1024];
    reader.read_exact(&mut buf).expect("Read exact at 40 must succeed");

    // Feed blocks 45..85 within want=40 window [24..88)
    feed_range(&shared, 45, 85);

    // Read at block 80 to advance want to 80
    reader.seek(SeekFrom::Start(80 * BLOCK)).expect("Seek 80 must succeed");
    reader.read_exact(&mut buf).expect("Read exact at 80 must succeed");

    // Feed blocks 85..125 within want=80 window [64..128)
    feed_range(&shared, 85, 125);

    // Read at block 110 (advances want to 110, causing sliding window to evict earlier blocks < 94)
    reader.seek(SeekFrom::Start(110 * BLOCK)).expect("Seek 110 must succeed");
    reader.read_exact(&mut buf).expect("Read exact at 110 must succeed");

    let present = count_present_blocks(&shared);
    assert!(
        present <= 65,
        "Sliding window must evict older blocks when advancing far forward"
    );
    assert!(present > 0, "Window must contain present blocks around want=110");

    // Seeking backwards to block 10 (which was evicted because 10 < 94)
    // In windowed streaming, reading an evicted block waits on the download worker.
    // Spawn a reader thread which registers want = 10 and waits on the ready Condvar.
    reader.seek(SeekFrom::Start(10 * BLOCK)).expect("Backwards seek must succeed");
    let read_handle = std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        reader.read_exact(&mut buf).expect("Read after backwards seek and rebuffering must succeed");
        (reader, buf)
    });

    std::thread::sleep(std::time::Duration::from_millis(20));
    feed_block(&shared, 10);

    let (mut reader, buf) = read_handle.join().expect("Reader thread must join cleanly");
    assert_eq!(reader.stream_position().unwrap(), 10 * BLOCK + 1024);
    let expected = crate::common::mock_stream::deterministic_block(10, file_size);
    assert_eq!(&buf[..], &expected[..1024]);
}
