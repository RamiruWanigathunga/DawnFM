//! Tier 2: F02 - Memory Allocator Boundary Limits
//!
//! Validates 0-byte allocations, excessive allocation rejection, rapid realloc oscillation,
//! power-of-two alignment boundaries, and heap recovery post-spike per ORIGINAL_REQUEST §R2.

use std::alloc::{alloc, dealloc, Layout};

#[test]
fn test_mimalloc_zero_size_allocation() {
    // 1. Zero-capacity Vec
    let v: Vec<u8> = Vec::with_capacity(0);
    assert_eq!(v.capacity(), 0);
    assert!(v.is_empty());

    // 2. Zero-length slice box
    let b: Box<[u8]> = vec![].into_boxed_slice();
    assert_eq!(b.len(), 0);

    // 3. Raw Layout with size 0
    let layout = Layout::from_size_align(0, 8).expect("Valid layout");
    unsafe {
        let ptr = alloc(layout);
        // Under mimalloc / standard Rust allocators, size 0 may return a non-null or dangling sentinel
        // but must be safely deallocatable
        dealloc(ptr, layout);
    }
}

#[test]
fn test_mimalloc_excessive_allocation_rejection() {
    // Test that extreme allocation sizes return TryReserveErrorKind::CapacityOverflow or AllocError
    let mut v: Vec<u8> = Vec::new();

    // 1. usize::MAX
    let res_max = v.try_reserve(usize::MAX);
    assert!(
        res_max.is_err(),
        "try_reserve(usize::MAX) must fail gracefully with error"
    );

    // 2. isize::MAX as usize + 1 (exceeds Rust addressable object limit)
    let res_isize = v.try_reserve((isize::MAX as usize).saturating_add(1));
    assert!(
        res_isize.is_err(),
        "try_reserve(isize::MAX + 1) must fail gracefully"
    );

    // 3. 1 Petabyte (unreachable virtual address space on desktop OS)
    // Prevent LLVM from eliding the allocation in release builds by observing the buffer pointer
    let petabyte = 1024usize * 1024 * 1024 * 1024 * 1024;
    let res_peta = v.try_reserve(petabyte);
    std::hint::black_box(v.as_ptr());
    assert!(
        res_peta.is_err(),
        "try_reserve(1PB) must fail gracefully with AllocError"
    );

    // 4. Raw allocator rejection for excessive layout
    let layout = Layout::from_size_align(petabyte, 1).expect("Valid 1PB layout");
    unsafe {
        let ptr = std::hint::black_box(alloc(layout));
        assert!(ptr.is_null(), "alloc(1PB) must return null pointer");
    }
}

#[test]
fn test_mimalloc_rapid_realloc_oscillation() {
    let mut v: Vec<u8> = Vec::new();

    // Rapidly oscillate between 16 bytes and 2 MB across 200 cycles
    for i in 0..200 {
        if i % 2 == 0 {
            v.resize(16, 0x11);
            assert_eq!(v.len(), 16);
            assert_eq!(v[0], 0x11);
            assert_eq!(v[15], 0x11);
        } else {
            v.resize(2 * 1024 * 1024, 0x22);
            assert_eq!(v.len(), 2 * 1024 * 1024);
            assert_eq!(v[0], 0x11); // Retains existing elements
            assert_eq!(v[16], 0x22); // New elements initialized
            assert_eq!(v[v.len() - 1], 0x22);
        }
    }
    v.clear();
    v.shrink_to_fit();
    assert_eq!(v.capacity(), 0);
}

#[test]
fn test_mimalloc_alignment_boundary_limits() {
    // Test various power-of-two alignments from 1B up to 64KB page-level boundaries
    let alignments = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536];

    for &align in &alignments {
        let size = align * 2;
        let layout = Layout::from_size_align(size, align)
            .unwrap_or_else(|e| panic!("Layout creation failed for align {}: {:?}", align, e));

        unsafe {
            let ptr = alloc(layout);
            assert!(!ptr.is_null(), "alloc failed for align {}", align);
            let addr = ptr as usize;
            assert_eq!(
                addr % align,
                0,
                "Pointer address {:#x} was not aligned to {:#x}",
                addr,
                align
            );

            // Write test bytes at boundaries
            std::ptr::write_bytes(ptr, 0xAA, size);
            assert_eq!(*ptr, 0xAA);
            assert_eq!(*ptr.add(size - 1), 0xAA);

            dealloc(ptr, layout);
        }
    }
}

#[test]
fn test_mimalloc_heap_recovery_post_spike() {
    // 1. Allocate a temporary 32MB spike
    let spike_chunks = 16;
    let chunk_size = 2 * 1024 * 1024;
    let mut spike: Vec<Vec<u8>> = Vec::with_capacity(spike_chunks);

    for idx in 0..spike_chunks {
        let mut chunk = vec![0u8; chunk_size];
        chunk[0] = (idx & 0xFF) as u8;
        chunk[chunk_size - 1] = (idx & 0xFF) as u8;
        spike.push(chunk);
    }
    assert_eq!(spike.len(), spike_chunks);

    // 2. Drop the spike to release memory back to mimalloc arenas
    drop(spike);

    // 3. Immediately churn 5,000 small objects to verify heap recovery and no fragmentation panic
    let mut small_objects: Vec<Box<[u64; 16]>> = Vec::with_capacity(5_000);
    for i in 0..5_000 {
        let mut b = Box::new([0u64; 16]);
        b[0] = i as u64;
        b[15] = (i * 7) as u64;
        small_objects.push(b);
    }

    for (i, b) in small_objects.iter().enumerate() {
        assert_eq!(b[0], i as u64);
        assert_eq!(b[15], (i * 7) as u64);
    }

    drop(small_objects);
}

#[test]
fn test_req_channel_message_envelope_bounded() {
    let req_size = std::mem::size_of::<dawnfm::backend::Req>();
    assert!(
        req_size <= 224,
        "Req channel message size must remain compact (<= 224 bytes, reduced from 625 bytes), was: {}",
        req_size
    );
}
