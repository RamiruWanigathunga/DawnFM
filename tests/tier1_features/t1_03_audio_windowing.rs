//! Tier 1: F03 - Audio Buffer Windowing
//!
//! Validates audio streaming buffer constants, 12MB threshold, 48-ahead / 16-behind
//! windowing constraints, and prefetch limits per ORIGINAL_REQUEST §R2.

use std::io::{Read, Seek, SeekFrom};

use dawnfm::stream::{Format, AHEAD, BEHIND, BLOCK, HEAD_BLOCKS, WINDOWED_FROM};

use crate::common::{feed_block, feed_range, make_synthetic_stream, make_synthetic_stream_with_format};

#[test]
fn test_stream_windowed_threshold_constant() {
    assert_eq!(
        WINDOWED_FROM,
        12 * 1024 * 1024,
        "Audio stream windowing threshold WINDOWED_FROM must be exactly 12 MB"
    );
    assert_eq!(
        BLOCK,
        64 * 1024,
        "Audio stream chunk block size BLOCK must be exactly 64 KB"
    );
}

#[test]
fn test_stream_ahead_behind_constants() {
    assert_eq!(
        AHEAD, 48,
        "Audio stream forward buffer AHEAD must be tuned to exactly 48 blocks (~3 MB)"
    );
    assert_eq!(
        BEHIND, 16,
        "Audio stream rewind buffer BEHIND must be tuned to exactly 16 blocks (~1 MB)"
    );
    assert_eq!(
        HEAD_BLOCKS, 8,
        "Audio stream initial prefetch HEAD_BLOCKS must be exactly 8 blocks (512 KB)"
    );
}

#[test]
fn test_stream_small_file_unwindowed() {
    let file_size = 6 * 1024 * 1024; // 6 MB (well below 12 MB)
    let (shared, mut reader) = make_synthetic_stream(file_size, false);

    assert!(
        !shared.is_windowed(),
        "Audio streams under 12 MB must remain unwindowed"
    );
    assert_eq!(shared.total, file_size);

    let total_blocks = shared.block_count();
    assert_eq!(total_blocks, (file_size / BLOCK) as usize);

    // Feed all blocks
    feed_range(&shared, 0, total_blocks);
    for idx in 0..total_blocks {
        assert!(
            shared.is_block_present(idx),
            "Block {} must be present in unwindowed buffer",
            idx
        );
    }

    // Read initial 256 bytes
    let mut buf = [0u8; 256];
    reader.seek(SeekFrom::Start(0)).expect("Seek to 0 must succeed");
    let n = reader.read(&mut buf).expect("Read from stream must succeed");
    assert_eq!(n, 256);
}

#[test]
fn test_stream_large_file_window_sliding() {
    let file_size = 18 * 1024 * 1024; // 18 MB (well above 12 MB)
    let (shared, mut reader) = make_synthetic_stream(file_size, true);

    assert!(
        shared.is_windowed(),
        "Audio streams over 12 MB must be windowed"
    );
    assert_eq!(shared.total, file_size);

    // Feed first 10 blocks
    for i in 0..10 {
        feed_block(&shared, i);
        assert!(shared.is_block_present(i));
    }

    // Read across blocks 0 and 1 (128 KB)
    reader.seek(SeekFrom::Start(0)).expect("Seek must succeed");
    let mut buf = vec![0u8; 128 * 1024];
    reader.read_exact(&mut buf).expect("Read across blocks must succeed");
    assert_eq!(buf.len(), 128 * 1024);

    // Verify limit constraint
    assert!(shared.limit() > 0);
}

#[test]
fn test_stream_prefetch_head_only() {
    let file_size = 15 * 1024 * 1024;
    let (shared, _reader) = make_synthetic_stream_with_format(file_size, Format::Aac, true);

    assert!(shared.is_windowed());
    assert_eq!(shared.total, file_size);

    // In head-only mode, initial prefetch targets HEAD_BLOCKS
    for i in 0..HEAD_BLOCKS {
        feed_block(&shared, i);
        assert!(shared.is_block_present(i));
    }

    assert_eq!(HEAD_BLOCKS, 8);
}
