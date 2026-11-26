//! Tier 1: F25 - Windows Named Pipe Single Instance IPC
//!
//! Validates `\\.\pipe\dawnfm-...` naming format, newline framing serialization,
//! message parsing, malformed packet rejection, and oversized frame bounding
//! per ORIGINAL_REQUEST §R6 and BitChord reference implementation.

pub const MAX_PIPE_PACKET_SIZE: usize = 64 * 1024; // 64 KB

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

#[test]
fn test_named_pipe_pipe_name_format() {
    let name = format_pipe_name("dawnfm", "test.user@domain", None);
    assert!(name.starts_with(r"\\.\pipe\dawnfm-jam-"));
    // '.' and '@' sanitized to '_'
    assert!(name.contains("test_user_domain"));

    let dev_name = format_pipe_name("dawnfm", "user", Some(0xDEADBEEFCAFE1234));
    assert!(dev_name.ends_with("-deadbeefcafe1234"));
}

#[test]
fn test_named_pipe_message_serialization() {
    let msg = "dawnfm://jam/XYZ789";
    let bytes = serialize_pipe_message(msg);
    assert_eq!(bytes, b"dawnfm://jam/XYZ789\n");
}

#[test]
fn test_named_pipe_message_deserialization() {
    let raw = b"dawnfm://jam/ABC123\n";
    let parsed = deserialize_pipe_message(raw).expect("Valid line must parse");
    assert_eq!(parsed, "dawnfm://jam/ABC123");

    let show_raw = b"show\r\n";
    let show_parsed = deserialize_pipe_message(show_raw).expect("Show command must parse");
    assert_eq!(show_parsed, "show");
}

#[test]
fn test_named_pipe_malformed_packet_rejection() {
    // Non-UTF8 payload
    let bad_utf8 = [0xFF, 0xFE, 0xFD];
    assert!(deserialize_pipe_message(&bad_utf8).is_err());

    // Empty buffer
    assert!(deserialize_pipe_message(b"").is_err());
    assert!(deserialize_pipe_message(b"\n\r\n").is_err());
}

#[test]
fn test_named_pipe_oversized_packet_rejection() {
    let oversized = vec![b'A'; MAX_PIPE_PACKET_SIZE + 1];
    let res = deserialize_pipe_message(&oversized);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Packet exceeds maximum allowable size");
}
