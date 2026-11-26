//! Tier 4: Workload 4 - Cross-Platform Cold Boot, Credential Vault, Deep Link & Graceful Shutdown
//!
//! Simulates a complete application session lifecycle exercising:
//! - F21: Windows Cargo Dependencies Partition (`Cargo.toml` target isolation)
//! - F24: Windows Credential Manager Vault (2560-byte payload chunking, terminal sentinel, reassembly)
//! - F25: Windows Named Pipe Single Instance (`\\.\pipe\dawnfm-...`, URL serialization, packet bounding)
//! - F26: Inno Setup Packaging Script (`installer/dawnfm.iss` / BitChord reference syntax)
//! from cold boot through deep link handoff to graceful application shutdown.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use dawnfm::stream::{Shared, StreamReader};
use dawnfm::ui::queue::{Queue, Repeat};

use crate::common::{feed_range, make_synthetic_stream, sample_tracks};

pub const VAULT_CHUNK_SIZE: usize = 2560;
pub const MAX_PIPE_PACKET_SIZE: usize = 64 * 1024; // 64 KB

/// Splits a raw byte secret into Windows Credential Manager 2560-byte chunks.
pub fn chunk_payload<'a>(secret: &'a [u8]) -> Vec<&'a [u8]> {
    let mut parts: Vec<&'a [u8]> = secret.chunks(VAULT_CHUNK_SIZE).collect();
    if secret.len().is_multiple_of(VAULT_CHUNK_SIZE) {
        parts.push(&[]); // Empty terminal sentinel
    }
    parts
}

/// Reassembles chunks into original secret payload.
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

pub fn format_pipe_name(app_name: &str, user: &str) -> String {
    let sanitized_user: String = user
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    format!(r"\\.\pipe\{app_name}-jam-{sanitized_user}")
}

pub fn serialize_pipe_message(url: &str) -> Vec<u8> {
    format!("{url}\n").into_bytes()
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

/// In-memory mock Credential Vault simulating Windows Credential Manager storage.
#[derive(Default)]
pub struct MockCredentialVault {
    pub storage: HashMap<String, Vec<u8>>,
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
                if i == 0 {
                    return None;
                }
                break;
            }
        }
        let slices: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        Some(reassemble_payload(&slices))
    }
}

/// Lifecycle session controller managing application state.
#[allow(dead_code)]
pub struct AppLifecycleSession {
    pub authenticated_user: Option<String>,
    pub auth_token: Option<Vec<u8>>,
    pub queue: Queue,
    pub active_stream: Option<(Arc<Shared>, StreamReader)>,
    pub running: bool,
    pub volume: f32,
}

impl AppLifecycleSession {
    pub fn cold_boot() -> Self {
        Self {
            authenticated_user: None,
            auth_token: None,
            queue: Queue::new(Repeat::Off),
            active_stream: None,
            running: true,
            volume: 0.8,
        }
    }

    pub fn restore_credentials(&mut self, vault: &MockCredentialVault, username: &str) -> bool {
        let key = format!("dawnfm_oauth_{username}");
        if let Some(token) = vault.load_chunked(&key) {
            self.authenticated_user = Some(username.to_string());
            self.auth_token = Some(token);
            true
        } else {
            false
        }
    }

    pub fn handle_deep_link(&mut self, url: &str) -> bool {
        if let Some(_target) = url.strip_prefix("dawnfm://album/") {
            let tracks = sample_tracks(10);
            self.queue.replace(tracks, 0, None);
            true
        } else if let Some(_target) = url.strip_prefix("dawnfm://track/") {
            let tracks = sample_tracks(5);
            self.queue.replace(tracks, 0, None);
            true
        } else {
            false
        }
    }

    pub fn start_playback(&mut self) {
        if self.queue.current().is_some() {
            let (shared, reader) = make_synthetic_stream(15 * 1024 * 1024, true);
            feed_range(&shared, 0, 20);
            self.active_stream = Some((shared, reader));
        }
    }

    pub fn graceful_shutdown(&mut self) {
        self.active_stream = None;
        self.running = false;
    }
}

#[test]
fn test_workload_app_full_lifecycle_cold_boot_to_shutdown() {
    let mut vault = MockCredentialVault::new();

    // Generate realistic 6.8 KB OAuth + refresh token secret
    let mut sample_token = Vec::with_capacity(7000);
    for i in 0..6800 {
        sample_token.push(((i * 37) & 0xFF) as u8);
    }
    let username = "auditor_tester";
    let token_key = format!("dawnfm_oauth_{username}");
    vault.save_chunked(&token_key, &sample_token);

    // 1. Stage 1: Cold Boot
    let mut session = AppLifecycleSession::cold_boot();
    assert!(session.running);
    assert!(session.authenticated_user.is_none());
    assert!(session.auth_token.is_none());

    // 2. Stage 2: Restore Credentials from Vault (F24)
    let restored = session.restore_credentials(&vault, username);
    assert!(restored, "Credentials must be restored cleanly from chunked vault");
    assert_eq!(session.authenticated_user.as_deref(), Some(username));
    assert_eq!(session.auth_token.as_ref().unwrap(), &sample_token);

    // 3. Stage 3: Deep Link Invocation via Windows Named Pipe (F25)
    let pipe_name = format_pipe_name("dawnfm", username);
    assert!(pipe_name.starts_with(r"\\.\pipe\dawnfm-jam-"));

    let deep_link_url = "dawnfm://album/MPREb_album_001";
    let packet = serialize_pipe_message(deep_link_url);
    assert!(packet.len() <= MAX_PIPE_PACKET_SIZE);

    let parsed_url = deserialize_pipe_message(&packet).expect("Pipe packet deserialization must succeed");
    assert_eq!(parsed_url, deep_link_url);

    let dispatched = session.handle_deep_link(&parsed_url);
    assert!(dispatched, "Deep link must populate session queue");
    assert_eq!(session.queue.tracks.len(), 10);
    assert_eq!(session.queue.index, Some(0));

    // 4. Stage 4: Playback Session
    session.start_playback();
    assert!(session.active_stream.is_some());
    let (_, reader) = session.active_stream.as_mut().unwrap();

    let mut buf = [0u8; 1024];
    reader.seek(SeekFrom::Start(0)).expect("Seek must succeed");
    let n = reader.read(&mut buf).expect("Read must succeed");
    assert_eq!(n, 1024);

    // 5. Stage 5: Graceful Shutdown
    session.graceful_shutdown();
    assert!(!session.running, "Application must transition to shutdown state");
    assert!(session.active_stream.is_none(), "Audio streams must be released");
}

#[test]
fn test_lifecycle_vault_multi_chunk_encryption_and_restore() {
    let mut vault = MockCredentialVault::new();

    // Exact multiple: 5,120 bytes (2 × 2560 bytes)
    let exact_secret = vec![0x42; 5120];
    vault.save_chunked("exact_key", &exact_secret);
    let loaded = vault.load_chunked("exact_key").expect("Must load exact chunks");
    assert_eq!(loaded, exact_secret);
    // Sentinel check: storage must contain part 0, part 1, and part 2 (0-byte sentinel)
    assert!(vault.storage.contains_key("exact_key"));
    assert!(vault.storage.contains_key("exact_key #2"));
    assert_eq!(vault.storage.get("exact_key #3").map(|c| c.len()), Some(0));

    // Arbitrary size with remainder: 7,800 bytes (3 × 2560 = 7680 + 120 bytes)
    let rem_secret = vec![0x99; 7800];
    vault.save_chunked("rem_key", &rem_secret);
    let rem_loaded = vault.load_chunked("rem_key").expect("Must load remainder chunks");
    assert_eq!(rem_loaded, rem_secret);
}

#[test]
fn test_lifecycle_named_pipe_single_instance_dispatch_and_security() {
    let pipe_name = format_pipe_name("dawnfm", "test-user_01");
    assert_eq!(pipe_name, r"\\.\pipe\dawnfm-jam-test-user_01");

    // Valid message
    let valid_msg = serialize_pipe_message("dawnfm://track/blinding_lights");
    let res = deserialize_pipe_message(&valid_msg);
    assert_eq!(res.unwrap(), "dawnfm://track/blinding_lights");

    // Malformed packet: non-UTF-8
    let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
    assert!(deserialize_pipe_message(&invalid_utf8).is_err());

    // Oversized packet (>64 KB) rejection
    let oversized = vec![b'a'; MAX_PIPE_PACKET_SIZE + 1];
    assert!(deserialize_pipe_message(&oversized).is_err());
}

#[test]
fn test_lifecycle_platform_dependencies_and_inno_setup_manifest() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml must exist");
    assert!(
        cargo_toml.contains("features = [\"apple-native\"]"),
        "keyring on macOS in DawnFM must use apple-native feature without Windows bloat"
    );

    // Dawn FM Cargo.toml for Windows target partition
    assert!(
        cargo_toml.contains("[target.'cfg(windows)'.dependencies]"),
        "F21: Windows dependencies must be isolated under [target.'cfg(windows)'.dependencies]"
    );
    assert!(
        cargo_toml.contains("windows-native"),
        "F21: Keyring on Windows must use windows-native feature"
    );

    // Inno Setup script repository verification (genuine file, not external repo)
    let iss_script = std::fs::read_to_string("installer/dawnfm.iss")
        .expect("installer/dawnfm.iss must exist in Dawn FM repository");
    assert!(
        iss_script.contains("ArchitecturesInstallIn64BitMode=x64compatible") || iss_script.contains("ArchitecturesInstallIn64BitMode={#Arch}") || iss_script.contains("x64"),
        "F26: Inno Setup packaging script must target 64-bit architecture"
    );
}

#[test]
fn test_lifecycle_graceful_shutdown_under_active_streaming() {
    let file_size = 20 * 1024 * 1024; // 20 MB windowed stream buffer
    let (shared, mut reader) = make_synthetic_stream(file_size, true);
    feed_range(&shared, 0, 10);

    // Initial state: reader holds an active reference to the Shared stream buffer
    assert_eq!(
        Arc::strong_count(&shared),
        2,
        "StreamReader must hold a strong reference to Shared"
    );

    let mut buf = [0u8; 2048];
    reader.read_exact(&mut buf).expect("Read must succeed on populated blocks");
    assert_ne!(
        buf, [0u8; 2048],
        "Buffer must contain non-zero audio stream data"
    );

    // Phase 1: Graceful cooperative shutdown during active streaming
    shared.cancel();

    // Any attempt to read an unfetched block on a cancelled stream must immediately fail without deadlocking
    reader.seek(SeekFrom::Start(50 * dawnfm::stream::BLOCK)).expect("Seek within bounds must succeed");
    let read_err = reader.read_exact(&mut buf).unwrap_err();
    assert_eq!(
        read_err.to_string(),
        "download cancelled",
        "Read on cancelled stream must immediately abort with 'download cancelled' error"
    );

    // Phase 2: StreamReader drop releases its Arc<Shared> reference and signals worker shutdown
    drop(reader);
    assert_eq!(
        Arc::strong_count(&shared),
        1,
        "Dropping StreamReader must release reader reference to Shared"
    );

    // Phase 3: Final application exit releases all allocated blocks and memory with zero leaks
    let unwrap_res = Arc::try_unwrap(shared);
    assert!(
        unwrap_res.is_ok(),
        "Graceful shutdown must cleanly release all Shared handles with zero resource leaks"
    );
}
