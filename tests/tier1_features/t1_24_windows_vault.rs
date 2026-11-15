//! Tier 1: F24 - Windows Credential Manager Vault Chunking
//!
//! Validates 2560-byte chunk size limit, exact-multiple terminal sentinel chunking,
//! remainder splitting, lossless reassembly roundtrip, and empty secret handling
//! per ORIGINAL_REQUEST §R6 and BitChord reference implementation.

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

pub fn chunk_username(base_key: &str, part: usize) -> String {
    if part == 0 {
        base_key.to_owned()
    } else {
        format!("{base_key} #{}", part + 1)
    }
}

#[test]
fn test_vault_chunk_size_threshold() {
    assert_eq!(
        VAULT_CHUNK_SIZE, 2560,
        "Windows Credential Manager payload chunk limit must be exactly 2560 bytes"
    );
}

#[test]
fn test_vault_payload_splitting_exact() {
    // 5120 bytes = exactly 2 * 2560 bytes
    let payload = vec![0x42u8; 5120];
    let chunks = chunk_payload(&payload);

    assert_eq!(chunks.len(), 3, "Exact multiple requires terminal empty sentinel chunk");
    assert_eq!(chunks[0].len(), 2560);
    assert_eq!(chunks[1].len(), 2560);
    assert_eq!(chunks[2].len(), 0);

    assert_eq!(chunk_username("token", 0), "token");
    assert_eq!(chunk_username("token", 1), "token #2");
    assert_eq!(chunk_username("token", 2), "token #3");
}

#[test]
fn test_vault_payload_splitting_remainder() {
    // 3000 bytes = 2560 + 440 bytes
    let payload = vec![0x99u8; 3000];
    let chunks = chunk_payload(&payload);

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].len(), 2560);
    assert_eq!(chunks[1].len(), 440);
}

#[test]
fn test_vault_payload_reassembly_roundtrip() {
    for size in [1, 100, 2559, 2560, 2561, 5120, 7000] {
        let original: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let chunks = chunk_payload(&original);
        let restored = reassemble_payload(&chunks);
        assert_eq!(restored, original, "Reassembly roundtrip failed for size {}", size);
    }
}

#[test]
fn test_vault_empty_secret_handling() {
    let empty = b"";
    let chunks = chunk_payload(empty);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 0);

    let restored = reassemble_payload(&chunks);
    assert!(restored.is_empty());
}
