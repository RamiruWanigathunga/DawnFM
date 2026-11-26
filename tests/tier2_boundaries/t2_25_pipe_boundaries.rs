//! Tier 2: F25 - Windows Named Pipe Single Instance Boundaries
//!
//! Validates immediate 0-byte client disconnect, zero-length payload rejection,
//! partial read stream reassembly, pipe name collision sanitization,
//! and invalid UTF-8 failure handling per ORIGINAL_REQUEST §R6.

pub const MAX_PIPE_PACKET_SIZE: usize = 64 * 1024;

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

#[test]
fn test_pipe_immediate_client_disconnect() {
    let empty_payload: &[u8] = &[];
    let res = deserialize_pipe_message(empty_payload);
    assert_eq!(res, Err("Empty message"));
}

#[test]
fn test_pipe_zero_length_payload_rejection() {
    let newlines_only = b"\r\n\r\n";
    let res = deserialize_pipe_message(newlines_only);
    assert_eq!(res, Err("Empty message"));

    let spaces_only = b"    \t  \n";
    let res_spaces = deserialize_pipe_message(spaces_only);
    assert_eq!(res_spaces, Err("Empty message"));
}

#[test]
fn test_pipe_framing_partial_reads() {
    // Simulate streaming pipe receiver buffer
    let mut stream_buffer = Vec::new();
    let partial_chunks: [&[u8]; 3] = [
        b"dawnfm",
        b"://jam/SESSION",
        b"_456\nsecond_ignored_line\n",
    ];

    let mut parsed_message = None;
    for chunk in partial_chunks {
        stream_buffer.extend_from_slice(chunk);
        if let Some(pos) = stream_buffer.iter().position(|&b| b == b'\n') {
            let line_bytes = &stream_buffer[..=pos];
            if let Ok(msg) = deserialize_pipe_message(line_bytes) {
                parsed_message = Some(msg);
                break;
            }
        }
    }

    assert_eq!(
        parsed_message,
        Some("dawnfm://jam/SESSION_456".to_string()),
        "Pipe buffer must assemble partial chunks and extract first newline-delimited message"
    );
}

#[test]
fn test_pipe_instance_collision_handling() {
    let collision_user = r"user\name:with*forbidden?chars<and>quotes|and/slashes";
    let pipe_name = format_pipe_name("dawnfm", collision_user, None);

    // Reserved Windows pipe characters (\, :, *, ?, ", <, >, |, /) in username must be sanitized
    let user_part = pipe_name.trim_start_matches(r"\\.\pipe\dawnfm-jam-");
    assert!(!user_part.contains('\\'));
    assert!(!user_part.contains(':'));
    assert!(!user_part.contains('*'));
    assert!(!user_part.contains('?'));
    assert!(!user_part.contains('"'));
    assert!(!user_part.contains('<'));
    assert!(!user_part.contains('>'));
    assert!(!user_part.contains('|'));
    assert!(!user_part.contains('/'));
}

#[test]
fn test_pipe_utf8_validation_failure_handling() {
    let invalid_utf8_payload: [u8; 4] = [0xFF, 0xFE, 0xFD, 0xFC];
    let res = deserialize_pipe_message(&invalid_utf8_payload);
    assert_eq!(res, Err("Invalid UTF-8"));
}
