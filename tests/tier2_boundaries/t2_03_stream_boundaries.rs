//! Tier 2: F03 - Audio Buffer Windowing Boundaries
//!
//! Validates exact 12MB threshold boundary, off-by-one byte boundaries, 0-byte stream handling,
//! 100 rapid alternating seeks, and EOF missing block recovery per ORIGINAL_REQUEST §R2.

use std::io::{Read, Seek, SeekFrom};

use dawnfm::stream::{Format, Shared, StreamReader, BLOCK, WINDOWED_FROM};

use crate::common::{feed_block, feed_range};

#[test]
fn test_stream_exact_12mb_boundary() {
    let exact_12mb = WINDOWED_FROM; // 12 * 1024 * 1024 = 12,582,912 bytes
    let shared = Shared::new(exact_12mb, 180_000, Some(-14.0), Format::Aac, None, false);

    // Contract: total > WINDOWED_FROM determines windowing
    assert!(
        !shared.is_windowed(),
        "At exactly 12MB (WINDOWED_FROM), stream must NOT be windowed (windowed requires total > 12MB)"
    );
    assert_eq!(shared.total, exact_12mb);

    let expected_blocks = (exact_12mb / BLOCK) as usize;
    assert_eq!(expected_blocks, 192);
    assert_eq!(shared.block_count(), expected_blocks);
}

#[test]
fn test_stream_off_by_one_byte_boundaries() {
    // 1. One byte below threshold (12MB - 1 byte = 12,582,911 bytes)
    let below_12mb = WINDOWED_FROM - 1;
    let shared_below = Shared::new(below_12mb, 180_000, Some(-14.0), Format::Aac, None, false);
    assert!(
        !shared_below.is_windowed(),
        "12MB - 1 byte must NOT be windowed"
    );
    assert_eq!(shared_below.block_count(), 192);

    // 2. One byte above threshold (12MB + 1 byte = 12,582,913 bytes)
    let above_12mb = WINDOWED_FROM + 1;
    let shared_above = Shared::new(above_12mb, 180_000, Some(-14.0), Format::Aac, None, false);
    assert!(
        shared_above.is_windowed(),
        "12MB + 1 byte MUST be windowed (total > WINDOWED_FROM)"
    );
    assert_eq!(
        shared_above.block_count(),
        193,
        "12MB + 1 byte spans 193 blocks (partial final block)"
    );
}

#[test]
fn test_stream_zero_byte_stream_error() {
    let zero_shared = Shared::new(0, 0, None, Format::Aac, None, false);
    assert_eq!(zero_shared.total, 0);
    assert_eq!(zero_shared.block_count(), 0);
    assert!(!zero_shared.is_windowed());

    let mut reader = StreamReader::new(zero_shared);
    let mut buf = [0u8; 64];

    // Read on a 0-byte stream must immediately return Ok(0) (EOF)
    let read_res = reader.read(&mut buf);
    assert!(read_res.is_ok());
    assert_eq!(read_res.unwrap(), 0);

    // Seek on a 0-byte stream to start(0) must succeed at 0
    let seek_res = reader.seek(SeekFrom::Start(0));
    assert!(seek_res.is_ok());
    assert_eq!(seek_res.unwrap(), 0);
}

#[test]
fn test_stream_100_rapid_back_forth_seeks() {
    let total_bytes = 14 * 1024 * 1024; // 14 MB (windowed)
    let shared = Shared::new(total_bytes, 240_000, Some(-14.0), Format::Aac, None, false);
    let mut reader = StreamReader::new(shared.clone());

    // Populate a few blocks
    feed_range(&shared, 0, 10);
    feed_range(&shared, 100, 110);
    feed_range(&shared, shared.block_count() - 2, shared.block_count());

    // Perform 100 alternating seek operations across start, middle, end, relative
    for i in 0..100 {
        let target = match i % 4 {
            0 => SeekFrom::Start(0),
            1 => SeekFrom::Start(total_bytes / 2),
            2 => SeekFrom::End(0),
            _ => SeekFrom::Current(-1024),
        };

        let pos = reader.seek(target);
        assert!(pos.is_ok(), "Seek iteration {} failed: {:?}", i, target);
        let current_pos = pos.unwrap();
        assert!(
            current_pos <= total_bytes,
            "Seek position {} exceeded stream total {}",
            current_pos,
            total_bytes
        );
    }
}

#[test]
fn test_stream_eof_missing_block_recovery() {
    let total_bytes = BLOCK * 3; // 3 blocks: 0, 1, 2
    let shared = Shared::new(total_bytes, 60_000, None, Format::Aac, None, false);
    let mut reader = StreamReader::new(shared.clone());

    // Feed block 0 and block 1 only
    feed_block(&shared, 0);
    feed_block(&shared, 1);

    // Read through block 0 and 1
    let mut chunk = vec![0u8; (BLOCK * 2) as usize];
    let n = reader.read_exact(&mut chunk);
    assert!(n.is_ok(), "Must read present blocks 0 and 1");

    // Block 2 is currently missing; verify is_block_present is false
    assert!(!shared.is_block_present(2));

    // Now feed block 2
    feed_block(&shared, 2);
    assert!(shared.is_block_present(2));

    // Read remaining block 2
    let mut final_block = vec![0u8; BLOCK as usize];
    let n2 = reader.read_exact(&mut final_block);
    assert!(n2.is_ok(), "Must successfully read newly fed block 2");

    // Reader should now be at EOF
    let mut eof_buf = [0u8; 16];
    let n_eof = reader.read(&mut eof_buf).expect("Read at EOF succeeds");
    assert_eq!(n_eof, 0, "Read at end of stream must yield 0 bytes (EOF)");
}
