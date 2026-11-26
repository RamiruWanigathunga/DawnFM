//! Tier 2: F24 - Windows Credential Manager Vault Chunking Boundaries
//!
//! Validates exact 2560-byte payload chunking with terminal sentinel, 2561-byte remainder splitting,
//! 1-byte minimal chunking, non-UTF8 arbitrary binary payload preservation,
//! and multi-threaded key churn per ORIGINAL_REQUEST §R6.

use std::thread;

pub const VAULT_CHUNK_SIZE: usize = 2560;

pub fn chunk_payload<'a>(secret: &'a [u8]) -> Vec<&'a [u8]> {
    let mut parts: Vec<&'a [u8]> = secret.chunks(VAULT_CHUNK_SIZE).collect();
    // A secret of an exact multiple of CHUNK ends with an empty part, so reading knows where it stops
    if secret.len().is_multiple_of(VAULT_CHUNK_SIZE) {
        parts.push(&[]);
    }
    parts
}

pub fn reassemble_payload(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        let full = part.len() == VAULT_CHUNK_SIZE;
        out.extend_from_slice(part);
        if !full {
            break;
        }
    }
    out
}

#[test]
fn test_vault_exact_2560_bytes_chunking() {
    let payload = vec![0x33u8; VAULT_CHUNK_SIZE];
    let chunks = chunk_payload(&payload);

    assert_eq!(chunks.len(), 2, "Exact 2560-byte payload must include terminal empty chunk");
    assert_eq!(chunks[0].len(), VAULT_CHUNK_SIZE);
    assert_eq!(chunks[1].len(), 0);

    let reassembled = reassemble_payload(&chunks);
    assert_eq!(reassembled, payload);
}

#[test]
fn test_vault_2561_bytes_two_chunks() {
    let mut payload = vec![0x44u8; VAULT_CHUNK_SIZE];
    payload.push(0x55); // 2561 bytes total

    let chunks = chunk_payload(&payload);
    assert_eq!(chunks.len(), 2, "2561 bytes must split into 2560 and 1-byte remainder");
    assert_eq!(chunks[0].len(), VAULT_CHUNK_SIZE);
    assert_eq!(chunks[1].len(), 1);
    assert_eq!(chunks[1][0], 0x55);

    let reassembled = reassemble_payload(&chunks);
    assert_eq!(reassembled, payload);
}

#[test]
fn test_vault_single_byte_chunking() {
    let payload = vec![0xAAu8];
    let chunks = chunk_payload(&payload);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 1);

    let reassembled = reassemble_payload(&chunks);
    assert_eq!(reassembled, payload);
}

#[test]
fn test_vault_non_utf8_binary_payload() {
    // Binary sequence containing 0x00 null bytes and invalid UTF-8 sequences (0xFF, 0xFE, 0x80)
    let mut binary_data = Vec::with_capacity(3000);
    for i in 0..3000 {
        binary_data.push((i * 13) as u8);
    }
    assert!(std::str::from_utf8(&binary_data).is_err(), "Must be invalid UTF-8");

    let chunks = chunk_payload(&binary_data);
    let reassembled = reassemble_payload(&chunks);

    assert_eq!(reassembled.len(), 3000);
    assert_eq!(reassembled, binary_data, "Binary payload must be preserved byte-for-byte");
}

#[test]
fn test_vault_concurrent_key_churn() {
    let mut handles = Vec::new();

    for t_idx in 0..8 {
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let size = 100 + (t_idx * 300) + i;
                let payload = vec![(t_idx ^ i) as u8; size];
                let chunks = chunk_payload(&payload);
                let reassembled = reassemble_payload(&chunks);
                assert_eq!(reassembled, payload);
            }
        }));
    }

    for h in handles {
        h.join().expect("Concurrent vault thread must not panic");
    }
}
