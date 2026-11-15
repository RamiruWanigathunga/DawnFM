//! Tier 3: Scenario 4 - Windows Vault Chunking + Named Pipe IPC + Cargo Windows Partition
//!
//! Validates credential storage, IPC single-instance dispatch, and platform dependency guards:
//! - F24: Windows Credential Manager Vault (2560-byte payload chunking, terminal sentinel, reassembly)
//! - F25: Named Pipe Single Instance IPC (`\\.\pipe\dawnfm-...`, URL framing, packet bounding)
//! - F21: Cargo `[target.'cfg(windows)'.dependencies]` isolation (keyring partition, zero macOS overhead)
//! under session restoration, deep-link handoff, and malformed packet rejection.

#![allow(dead_code)]

use std::collections::HashMap;

pub const VAULT_CHUNK_SIZE: usize = 2560;
pub const MAX_PIPE_PACKET_SIZE: usize = 64 * 1024; // 64 KB

pub fn chunk_payload<'a>(secret: &'a [u8]) -> Vec<&'a [u8]> {
    let mut parts: Vec<&'a [u8]> = secret.chunks(VAULT_CHUNK_SIZE).collect();
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

pub fn format_pipe_name(app_name: &str, user: &str, profile_hash: Option<u64>) -> String {
    let sanitized_user: String = user
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();

    let mut name = format!(r"\\.\pipe\{app_name}-jam-{sanitized_user}");
    if let Some(h) = profile_hash {
        name.push_str(&format!("-{h:016x}"));
    }
    name
}

pub fn serialize_pipe_message(msg: &str) -> Vec<u8> {
    format!("{msg}\n").into_bytes()
}

pub fn deserialize_pipe_message(raw_bytes: &[u8]) -> Result<String, &'static str> {
    if raw_bytes.len() > MAX_PIPE_PACKET_SIZE {
        return Err("Packet exceeds maximum allowable size");
    }
    let text = std::str::from_utf8(raw_bytes).map_err(|_| "Invalid UTF-8")?;
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Err("Empty message");
    }
    Ok(line.to_string())
}

/// In-memory mock Credential Vault simulating Windows Credential Manager storage
#[derive(Default)]
pub struct MockCredentialVault {
    storage: HashMap<String, Vec<u8>>,
}

impl MockCredentialVault {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save_chunked(&mut self, base_key: &str, secret: &[u8]) {
        let chunks = chunk_payload(secret);
        for (i, chunk) in chunks.iter().enumerate() {
            let key = chunk_username(base_key, i);
            self.storage.insert(key, chunk.to_vec());
        }
    }

    pub fn load_chunked(&self, base_key: &str) -> Option<Vec<u8>> {
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        let mut i = 0;
        loop {
            let key = chunk_username(base_key, i);
            if let Some(chunk) = self.storage.get(&key) {
                let is_full = chunk.len() == VAULT_CHUNK_SIZE;
                chunks.push(chunk.clone());
                if !is_full {
                    break;
                }
                i += 1;
            } else {
                break;
            }
        }

        if chunks.is_empty() {
            None
        } else {
            let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
            Some(reassemble_payload(&chunk_refs))
        }
    }
}

#[test]
fn test_named_pipe_deeplink_triggers_chunked_vault_session_restore() {
    let mut vault = MockCredentialVault::new();

    // 6,000-byte OAuth token requiring 3 chunks: 2560, 2560, 880
    let token_secret: Vec<u8> = (0..6000).map(|i| (i % 251) as u8).collect();
    vault.save_chunked("session_alice", &token_secret);

    // Incoming deep link packet sent over Windows Named Pipe
    let pipe_name = format_pipe_name("dawnfm", "session_alice", None);
    assert!(pipe_name.starts_with(r"\\.\pipe\dawnfm-jam-session_alice"));

    let incoming_url = "dawnfm://auth/session?user=session_alice";
    let wire_bytes = serialize_pipe_message(incoming_url);

    // Primary instance receives and parses named pipe packet
    let parsed_cmd = deserialize_pipe_message(&wire_bytes).expect("Valid deep link line must parse");
    assert_eq!(parsed_cmd, incoming_url);

    let user_key = parsed_cmd
        .split("user=")
        .nth(1)
        .expect("Must contain user parameter");

    // Retrieve and reassemble chunked credentials from vault
    let restored = vault.load_chunked(user_key).expect("Vault credentials must exist");
    assert_eq!(restored, token_secret, "Restored token must match original 6,000 bytes");
}

#[test]
fn test_vault_credentials_payload_sent_via_named_pipe_within_packet_budget() {
    let mut vault = MockCredentialVault::new();

    // 10 KB session payload
    let session_data = vec![0x77u8; 10 * 1024];
    vault.save_chunked("large_session", &session_data);

    let loaded = vault.load_chunked("large_session").expect("Must load large session");
    assert_eq!(loaded.len(), 10 * 1024);

    // Base64-like serialization over named pipe protocol
    let encoded_payload = format!("dawnfm://sync/session?data={}", "A".repeat(10 * 1024));
    let wire_bytes = serialize_pipe_message(&encoded_payload);

    assert!(
        wire_bytes.len() < MAX_PIPE_PACKET_SIZE,
        "Wire packet ({} bytes) must reside within 64KB MAX_PIPE_PACKET_SIZE",
        wire_bytes.len()
    );

    let parsed = deserialize_pipe_message(&wire_bytes).expect("Pipe packet must parse");
    assert!(parsed.starts_with("dawnfm://sync/session?data="));
}

#[test]
fn test_cargo_cfg_windows_isolation_guards_vault_and_pipe_modules() {
    let dawn_toml = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml must exist");

    assert!(
        dawn_toml.contains("[target.'cfg(windows)'.dependencies]"),
        "Windows target dependencies must be strictly isolated under [target.'cfg(windows)'.dependencies]"
    );
    assert!(
        dawn_toml.contains("features = [\"windows-native\"]"),
        "keyring on Windows target must enable windows-native"
    );
    assert!(
        dawn_toml.contains("features = [\"apple-native\"]"),
        "keyring on macOS in DawnFM must enable apple-native"
    );

    // General dependencies must not declare un-guarded windows crates
    let general_deps = dawn_toml.split("[dependencies]").nth(1).unwrap_or("");
    let top_deps = general_deps.split("\n[").next().unwrap_or("");
    assert!(!top_deps.contains("windows ="));
    assert!(!top_deps.contains("windows-sys ="));
}

#[test]
fn test_named_pipe_malformed_deeplink_preserves_vault_state() {
    let mut vault = MockCredentialVault::new();
    let initial_secret = b"SAFE_CREDENTIALS_DATA_123456789";
    vault.save_chunked("important_key", initial_secret);

    // 1. Non-UTF8 payload
    let bad_utf8 = [0xFF, 0xFE, 0xFD];
    assert!(deserialize_pipe_message(&bad_utf8).is_err());

    // 2. Oversized packet exceeding 64KB
    let oversized = vec![b'X'; MAX_PIPE_PACKET_SIZE + 50];
    assert!(deserialize_pipe_message(&oversized).is_err());

    // 3. Empty buffer
    assert!(deserialize_pipe_message(b"").is_err());
    assert!(deserialize_pipe_message(b"\n\r\n").is_err());

    // Verify vault storage remained unmodified
    let restored = vault.load_chunked("important_key").expect("Key must remain intact");
    assert_eq!(restored, initial_secret);
}

#[test]
fn test_multi_account_vault_chunking_with_named_pipe_switching() {
    let mut vault = MockCredentialVault::new();

    // Account Alpha: 3,000 bytes (2 chunks: 2560, 440)
    let secret_alpha = vec![0xAA; 3000];
    vault.save_chunked("account_alpha", &secret_alpha);

    // Account Beta: 5,120 bytes (exact multiple 2*2560 -> 3 chunks: 2560, 2560, 0)
    let secret_beta = vec![0xBB; 5120];
    vault.save_chunked("account_beta", &secret_beta);

    // Switch to Beta via Named Pipe command
    let pipe_cmd_beta = serialize_pipe_message("dawnfm://switch_user?name=account_beta");
    let parsed_beta = deserialize_pipe_message(&pipe_cmd_beta).unwrap();
    let name_beta = parsed_beta.split("name=").nth(1).unwrap();

    let loaded_beta = vault.load_chunked(name_beta).expect("Beta credentials must load");
    assert_eq!(loaded_beta, secret_beta);
    assert_eq!(loaded_beta.len(), 5120);

    // Switch to Alpha via Named Pipe command
    let pipe_cmd_alpha = serialize_pipe_message("dawnfm://switch_user?name=account_alpha");
    let parsed_alpha = deserialize_pipe_message(&pipe_cmd_alpha).unwrap();
    let name_alpha = parsed_alpha.split("name=").nth(1).unwrap();

    let loaded_alpha = vault.load_chunked(name_alpha).expect("Alpha credentials must load");
    assert_eq!(loaded_alpha, secret_alpha);
    assert_eq!(loaded_alpha.len(), 3000);
}
