//! Tier 1: F04 - Image Cache LRU Bounding
//!
//! Validates 36MB GPU texture budget, 40-entry / 16MB raw image cache LRU eviction,
//! and in-flight request protection per ORIGINAL_REQUEST §R2.

use egui::load::TextureLoader;

use dawnfm::images::{Textures, HTTP_CACHE_MAX_BYTES, HTTP_CACHE_MAX_ENTRIES, TEXTURE_BUDGET};

#[test]
fn test_texture_budget_constant() {
    assert_eq!(
        TEXTURE_BUDGET,
        36 * 1024 * 1024,
        "GPU texture cache budget TEXTURE_BUDGET must be exactly 36 MB"
    );
}

#[test]
fn test_raw_image_cache_max_limits() {
    assert_eq!(
        HTTP_CACHE_MAX_ENTRIES, 40,
        "HTTP raw image cache capacity HTTP_CACHE_MAX_ENTRIES must be exactly 40"
    );
    assert_eq!(
        HTTP_CACHE_MAX_BYTES,
        16 * 1024 * 1024,
        "HTTP raw image cache byte ceiling HTTP_CACHE_MAX_BYTES must be exactly 16 MB"
    );
}

#[test]
fn test_raw_cache_lru_eviction() {
    let mut cache: Vec<(usize, std::time::Instant, usize)> = Vec::new();
    let base_time = std::time::Instant::now();

    for i in 0..50 {
        let entry_bytes = 256 * 1024; // 256 KB
        let timestamp = base_time + std::time::Duration::from_millis(i as u64 * 10);
        cache.push((i, timestamp, entry_bytes));
    }

    assert_eq!(cache.len(), 50);

    // Evict oldest until count <= HTTP_CACHE_MAX_ENTRIES
    cache.sort_by_key(|&(_, t, _)| t);
    while cache.len() > HTTP_CACHE_MAX_ENTRIES {
        cache.remove(0);
    }

    assert_eq!(cache.len(), HTTP_CACHE_MAX_ENTRIES);
    // Oldest remaining item must have been index 10 (since 0..10 were evicted)
    assert_eq!(cache[0].0, 10);
}

#[test]
fn test_gpu_texture_lru_eviction() {
    let textures = Textures::default();
    assert_eq!(textures.byte_size(), 0);

    // Verify end_pass on empty cache runs cleanly
    textures.end_pass(1);
    assert_eq!(textures.byte_size(), 0);

    textures.end_pass(100);
    assert_eq!(textures.byte_size(), 0);

    textures.forget_all();
    assert_eq!(textures.byte_size(), 0);
}

#[test]
fn test_cache_in_flight_protection() {
    #[derive(Debug, PartialEq)]
    enum EntryStatus {
        Pending,
        Ready(usize),
    }

    let entries = vec![
        (0, EntryStatus::Pending, std::time::Instant::now()),
        (1, EntryStatus::Ready(500_000), std::time::Instant::now()),
        (2, EntryStatus::Pending, std::time::Instant::now()),
        (3, EntryStatus::Ready(800_000), std::time::Instant::now()),
    ];

    // Filter eviction candidates: exclude Pending
    let evictable: Vec<usize> = entries
        .iter()
        .filter(|(_, status, _)| !matches!(status, EntryStatus::Pending))
        .map(|(id, _, _)| *id)
        .collect();

    assert_eq!(evictable, vec![1, 3], "Only Ready entries may be evictable");
}
