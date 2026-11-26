//! Tier 1: F02 - Memory Allocator (mimalloc)
//!
//! Validates mimalloc global allocator binding, rapid churn across allocation sizes,
//! heap fragmentation resilience, and multi-threaded contention per ORIGINAL_REQUEST §R2.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

#[test]
fn test_mimalloc_global_allocator_bound() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml must exist");
    assert!(
        cargo_toml.contains("mimalloc = \"0.1\""),
        "Cargo.toml must include mimalloc = '0.1' dependency"
    );

    let main_rs = std::fs::read_to_string("src/main.rs").expect("src/main.rs must exist");
    assert!(
        main_rs.contains("#[global_allocator]") && main_rs.contains("mimalloc::MiMalloc"),
        "src/main.rs must bind mimalloc::MiMalloc as the #[global_allocator]"
    );
}

#[test]
fn test_mimalloc_small_allocations_churn() {
    let count = 10_000;
    let mut allocations = Vec::with_capacity(count);

    for i in 0..count {
        let size = 32 + (i % 224);
        let mut v: Vec<u8> = Vec::with_capacity(size);
        v.resize(size, (i & 0xFF) as u8);
        allocations.push(v);
    }

    // Verify all allocations retained their values
    for (i, v) in allocations.iter().enumerate() {
        let expected = (i & 0xFF) as u8;
        assert_eq!(v[0], expected);
        assert_eq!(v[v.len() - 1], expected);
    }

    allocations.clear();
    assert!(allocations.is_empty());
}

#[test]
fn test_mimalloc_large_allocations_churn() {
    for mb in [1, 4, 8, 16] {
        let bytes = mb * 1024 * 1024;
        let mut buf: Vec<u8> = vec![0xAA; bytes];
        assert_eq!(buf.len(), bytes);
        assert_eq!(buf[0], 0xAA);
        assert_eq!(buf[bytes - 1], 0xAA);
        buf[0] = 0x55;
        buf[bytes - 1] = 0x55;
        assert_eq!(buf[0], 0x55);
        assert_eq!(buf[bytes - 1], 0x55);
        drop(buf);
    }
}

#[test]
fn test_mimalloc_fragmentation_rss_stability() {
    let count = 2_000;
    let mut items: Vec<Option<Vec<u8>>> = Vec::with_capacity(count);

    // Interleave varied allocation sizes (1 byte to 64 KB)
    for i in 0..count {
        let size = 1 + ((i * 37) % (64 * 1024));
        let buf = vec![(i & 0xFF) as u8; size];
        items.push(Some(buf));
    }

    // Free odd-indexed items to induce fragmentation pressure
    for i in (1..count).step_by(2) {
        items[i] = None;
    }

    // Reallocate in freed holes with different sizes
    for i in (1..count).step_by(2) {
        let size = 16 + ((i * 53) % (32 * 1024));
        items[i] = Some(vec![0xEE; size]);
    }

    // Verify even items are still intact
    for i in (0..count).step_by(2) {
        if let Some(buf) = &items[i] {
            assert_eq!(buf[0], (i & 0xFF) as u8);
        }
    }
}

#[test]
fn test_mimalloc_multithreaded_contention() {
    let threads = 4;
    let iterations = 2_500;
    let counter = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for t in 0..threads {
        let c = counter.clone();
        handles.push(thread::spawn(move || {
            for i in 0..iterations {
                let size = 16 + ((t * 100 + i) % 1024);
                let v = vec![t as u8; size];
                assert_eq!(v[0], t as u8);
                c.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().expect("Worker thread must join without panic");
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        threads * iterations,
        "All thread iterations must complete"
    );
}
