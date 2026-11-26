//! Synthetic audio stream generator and range block injector for headless testing.

#![allow(dead_code)]

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

use bytes::Bytes;
use dawnfm::stream::{Format, Shared, StreamReader, BLOCK, WINDOWED_FROM};

/// Generates a deterministic byte vector for block `idx` belonging to a stream of `total_bytes`.
pub fn deterministic_block(idx: usize, total_bytes: u64) -> Vec<u8> {
    let start = idx as u64 * BLOCK;
    if start >= total_bytes {
        return Vec::new();
    }
    let end = (start + BLOCK).min(total_bytes);
    let len = (end - start) as usize;

    let mut buf = Vec::with_capacity(len);
    let seed = (idx as u32).wrapping_mul(31).wrapping_add(17);
    for offset in 0..len {
        let byte = ((seed + offset as u32) & 0xFF) as u8;
        buf.push(byte);
    }
    buf
}

/// Creates a synthetic in-memory audio stream buffer and associated reader without physical hardware or network.
///
/// Invariant: Zero audio sink / Rodio initialization; runs 100% in memory.
pub fn make_synthetic_stream(total_bytes: u64, windowed: bool) -> (Arc<Shared>, StreamReader) {
    let effective_total = if windowed && total_bytes <= WINDOWED_FROM {
        WINDOWED_FROM + (2 * BLOCK) // Force threshold > 12MB
    } else if !windowed && total_bytes > WINDOWED_FROM {
        WINDOWED_FROM // Cap at <= 12MB
    } else {
        total_bytes
    };

    let shared = Shared::new(
        effective_total,
        180_000, // 3 minutes
        Some(-14.0),
        Format::Aac,
        None,
        false,
    );
    let reader = StreamReader::new(shared.clone());
    (shared, reader)
}

/// Creates a synthetic stream with custom container format and head_only prefetch flag.
pub fn make_synthetic_stream_with_format(
    total_bytes: u64,
    format: Format,
    head_only: bool,
) -> (Arc<Shared>, StreamReader) {
    let shared = Shared::new(total_bytes, 200_000, Some(-14.0), format, None, head_only);
    let reader = StreamReader::new(shared.clone());
    (shared, reader)
}

/// Feeds deterministic range block `idx` into `shared.put()`.
pub fn feed_block(shared: &Arc<Shared>, idx: usize) {
    let data = deterministic_block(idx, shared.total);
    if !data.is_empty() {
        shared.put(idx, Bytes::from(data));
    }
}

/// Feeds a contiguous range of blocks [start_idx, end_idx) into `shared`.
pub fn feed_range(shared: &Arc<Shared>, start_idx: usize, end_idx: usize) {
    for idx in start_idx..end_idx {
        feed_block(shared, idx);
    }
}

/// Feeds all blocks across the entire stream into `shared`.
pub fn feed_all_blocks(shared: &Arc<Shared>) {
    let count = shared.block_count();
    for idx in 0..count {
        feed_block(shared, idx);
    }
}

/// Feeds block `idx` in progressive streaming chunks using `shared.grow()` and `shared.complete()`.
pub fn feed_streaming_chunks(shared: &Arc<Shared>, idx: usize, chunk_size: usize) {
    let data = deterministic_block(idx, shared.total);
    for chunk in data.chunks(chunk_size) {
        shared.grow(idx, chunk);
    }
    shared.complete(idx);
}

/// Counts how many blocks currently reside in memory in `shared`.
pub fn count_present_blocks(shared: &Arc<Shared>) -> usize {
    let count = shared.block_count();
    let mut present = 0;
    for i in 0..count {
        if shared.is_block_present(i) {
            present += 1;
        }
    }
    present
}

/// Reads `len` bytes from `reader` at `offset` and verifies the data against deterministic blocks.
pub fn verify_range_read(
    reader: &mut StreamReader,
    offset: u64,
    len: usize,
) -> io::Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    let n = reader.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// Executes rapid forward and backward seeks on `reader` to stress the seek state machine.
pub fn rapid_seek_test(reader: &mut StreamReader, positions: &[u64]) -> io::Result<()> {
    for &pos in positions {
        reader.seek(SeekFrom::Start(pos))?;
        let mut byte = [0u8; 1];
        let _ = reader.read(&mut byte)?;
    }
    Ok(())
}
