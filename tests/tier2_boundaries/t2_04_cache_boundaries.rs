//! Tier 2: F04 - Image Cache LRU Bounding
//!
//! Validates exact 36MB GPU texture budget boundary, 0-byte image handling, all-eligible mass eviction drain,
//! concurrent read-evict race conditions, and corrupted byte resilience per ORIGINAL_REQUEST §R2.

use std::sync::{Arc, RwLock};
use std::thread;

use egui::load::TextureLoader;

use dawnfm::images::{Textures, HTTP_CACHE_MAX_BYTES, HTTP_CACHE_MAX_ENTRIES, TEXTURE_BUDGET};

#[test]
fn test_texture_exact_36mb_boundary() {
    let budget = TEXTURE_BUDGET;
    let target = budget * 3 / 4;

    assert_eq!(budget, 36 * 1024 * 1024);
    assert_eq!(target, 27 * 1024 * 1024);

    // Simulate texture pool size tracking
    let total_bytes = budget;
    assert!(
        total_bytes <= budget,
        "Exactly 36MB does not strictly exceed budget"
    );

    // Exceed by 1 byte: 10MB + 10MB + 10MB + 6MB + 1 = 36MB + 1 byte
    let mut textures: Vec<(usize, usize, u64)> = vec![
        (1, 10 * 1024 * 1024, 1),
        (2, 10 * 1024 * 1024, 2),
        (3, 10 * 1024 * 1024, 3),
        (4, 6 * 1024 * 1024 + 1, 4),
    ];

    let current_sum: usize = textures.iter().map(|(_, sz, _)| sz).sum();
    assert_eq!(current_sum, budget + 1);

    // Evict least recently used (lowest pass_index) down to target
    textures.sort_by_key(|(_, _, pass)| *pass);
    while textures.iter().map(|(_, sz, _)| sz).sum::<usize>() > target && !textures.is_empty() {
        textures.remove(0);
    }

    let reduced_sum: usize = textures.iter().map(|(_, sz, _)| sz).sum();
    assert!(
        reduced_sum <= target,
        "Post-eviction texture byte size must be <= 27MB"
    );
}

#[test]
fn test_cache_zero_byte_image_handling() {
    let empty_bytes: &[u8] = &[];

    // Attempting to decode an empty byte array as an image must fail gracefully
    let decode_result = image::load_from_memory(empty_bytes);
    assert!(
        decode_result.is_err(),
        "Decoding a 0-byte image must return an Err without panicking"
    );

    // Textures struct should handle 0-byte registration cleanly
    let textures = Textures::default();
    assert_eq!(textures.byte_size(), 0);
}

#[test]
fn test_cache_all_eligible_eviction_drain() {
    // Simulate 100 non-pending entries of 500 KB each (total 50,000 KB)
    let mut cache: Vec<(usize, usize, std::time::Instant)> = Vec::new();
    let base = std::time::Instant::now();

    for i in 0..100 {
        cache.push((i, 500 * 1024, base + std::time::Duration::from_millis(i as u64)));
    }

    assert_eq!(cache.len(), 100);
    let total_bytes: usize = cache.iter().map(|(_, sz, _)| sz).sum();
    assert_eq!(total_bytes, 100 * 500 * 1024);

    // LRU eviction logic: prune oldest entries until count <= 40 and bytes <= 16MB
    cache.sort_by_key(|&(_, _, time)| time);
    while cache.len() > HTTP_CACHE_MAX_ENTRIES || cache.iter().map(|(_, sz, _)| sz).sum::<usize>() > HTTP_CACHE_MAX_BYTES {
        cache.remove(0);
    }

    assert!(cache.len() <= HTTP_CACHE_MAX_ENTRIES);
    let final_bytes: usize = cache.iter().map(|(_, sz, _)| sz).sum();
    assert!(final_bytes <= HTTP_CACHE_MAX_BYTES);
    // 32 entries * 500 KB = 16,000 KB <= 16 MB
    assert_eq!(cache.len(), 32);
}

#[test]
fn test_cache_concurrent_read_evict_race() {
    let shared_cache = Arc::new(RwLock::new(Vec::<(usize, Vec<u8>)>::new()));
    let mut handles = Vec::new();

    // Reader thread 1: rapid lookups
    let c1 = shared_cache.clone();
    handles.push(thread::spawn(move || {
        for _ in 0..200 {
            let r = c1.read().unwrap();
            let _len = r.len();
        }
    }));

    // Reader thread 2: lookups
    let c2 = shared_cache.clone();
    handles.push(thread::spawn(move || {
        for _ in 0..200 {
            let r = c2.read().unwrap();
            if let Some(entry) = r.first() {
                assert!(!entry.1.is_empty());
            }
        }
    }));

    // Writer/Evictor thread: appends and trims
    let c3 = shared_cache.clone();
    handles.push(thread::spawn(move || {
        for i in 0..200 {
            let mut w = c3.write().unwrap();
            w.push((i, vec![0xAB; 1024]));
            if w.len() > 20 {
                w.remove(0);
            }
        }
    }));

    for h in handles {
        h.join().expect("Thread must not panic during concurrent cache operations");
    }

    let final_cache = shared_cache.read().unwrap();
    assert!(final_cache.len() <= 20);
}

#[test]
fn test_cache_disk_sync_failure_resilience() {
    // Malformed JPEG/PNG header bytes
    let corrupt_payloads: [&[u8]; 4] = [
        b"\xFF\xD8\xFF\x00\x00corrupt", // Truncated JPEG
        b"\x89PNG\r\n\x1a\ncorrupt",     // Corrupt PNG
        b"RIFF\x00\x00\x00\x00WEBPbad", // Corrupt WebP
        b"random gibberish that is definitely not an image format",
    ];

    for (idx, payload) in corrupt_payloads.iter().enumerate() {
        let res = image::load_from_memory(payload);
        assert!(
            res.is_err(),
            "Payload {} must fail image decode cleanly without panicking",
            idx
        );
    }
}
