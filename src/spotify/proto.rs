//! Minimal lossless Protobuf wire encoder/decoder and Spotify URI/GID helpers,
//! matching the contracts used by Melo's `Proto.kt` and `SpotifyCatalog.kt`.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;

const BASE62: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Default, Clone)]
pub struct ProtoWriter {
    buf: Vec<u8>,
}

impl ProtoWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn write_varint_raw(&mut self, mut v: u64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                break;
            }
            self.buf.push(byte | 0x80);
        }
    }

    fn write_tag(&mut self, field: u32, wire_type: u32) {
        self.write_varint_raw(((field as u64) << 3) | (wire_type as u64));
    }

    pub fn write_uint32(&mut self, field: u32, val: u32) {
        self.write_tag(field, 0);
        self.write_varint_raw(val as u64);
    }

    pub fn write_int32(&mut self, field: u32, val: i32) {
        self.write_tag(field, 0);
        self.write_varint_raw(val as i64 as u64);
    }

    pub fn write_int64(&mut self, field: u32, val: i64) {
        self.write_tag(field, 0);
        self.write_varint_raw(val as u64);
    }

    pub fn write_enum(&mut self, field: u32, val: i32) {
        self.write_int32(field, val);
    }

    pub fn write_bool(&mut self, field: u32, val: bool) {
        self.write_tag(field, 0);
        self.write_varint_raw(if val { 1 } else { 0 });
    }

    pub fn write_bytes(&mut self, field: u32, bytes: &[u8]) {
        self.write_tag(field, 2);
        self.write_varint_raw(bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    pub fn write_str(&mut self, field: u32, s: &str) {
        self.write_bytes(field, s.as_bytes());
    }

    pub fn write_msg(&mut self, field: u32, build: impl FnOnce(&mut ProtoWriter)) {
        let mut inner = ProtoWriter::new();
        build(&mut inner);
        self.write_bytes(field, &inner.buf);
    }
}

pub fn encode(build: impl FnOnce(&mut ProtoWriter)) -> Vec<u8> {
    let mut w = ProtoWriter::new();
    build(&mut w);
    w.into_bytes()
}

pub struct ProtoReader {
    delimited: HashMap<u32, Vec<Vec<u8>>>,
    varints: HashMap<u32, i64>,
    fixed32: HashMap<u32, u32>,
}

impl ProtoReader {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut delimited: HashMap<u32, Vec<Vec<u8>>> = HashMap::new();
        let mut varints: HashMap<u32, i64> = HashMap::new();
        let mut fixed32: HashMap<u32, u32> = HashMap::new();
        let mut pos = 0usize;

        while pos < bytes.len() {
            let tag = read_varint(bytes, &mut pos)?;
            let field = (tag >> 3) as u32;
            let wire = (tag & 0x7) as u32;
            match wire {
                0 => {
                    let v = read_varint(bytes, &mut pos)?;
                    varints.insert(field, v as i64);
                }
                1 => {
                    if pos + 8 > bytes.len() {
                        bail!("Truncated fixed64 in Protobuf response");
                    }
                    pos += 8;
                }
                2 => {
                    let len = read_varint(bytes, &mut pos)? as usize;
                    if pos.saturating_add(len) > bytes.len() {
                        bail!("Truncated length-delimited field in Protobuf response");
                    }
                    delimited.entry(field).or_default().push(bytes[pos..pos + len].to_vec());
                    pos += len;
                }
                5 => {
                    if pos + 4 > bytes.len() {
                        bail!("Truncated fixed32 in Protobuf response");
                    }
                    let v = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
                    fixed32.insert(field, v);
                    pos += 4;
                }
                _ => bail!("Unsupported wire type {wire} in Protobuf response"),
            }
        }
        Ok(Self { delimited, varints, fixed32 })
    }

    pub fn bytes(&self, field: u32) -> Option<&[u8]> {
        self.delimited.get(&field).and_then(|v| v.last()).map(Vec::as_slice)
    }

    pub fn text(&self, field: u32) -> Option<String> {
        self.bytes(field).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    pub fn texts(&self, field: u32) -> Vec<String> {
        self.delimited
            .get(&field)
            .map(|v| v.iter().map(|b| String::from_utf8_lossy(b).into_owned()).collect())
            .unwrap_or_default()
    }

    pub fn number(&self, field: u32) -> Option<i32> {
        self.varints.get(&field).map(|&v| v as i32)
    }

    pub fn long(&self, field: u32) -> Option<i64> {
        self.varints.get(&field).copied()
    }

    pub fn bool_val(&self, field: u32) -> Option<bool> {
        self.varints.get(&field).map(|&v| v != 0)
    }

    pub fn float(&self, field: u32) -> Option<f32> {
        self.fixed32.get(&field).map(|&bits| f32::from_le_bytes(bits.to_le_bytes()))
    }

    pub fn message(&self, field: u32) -> Option<ProtoReader> {
        self.bytes(field).and_then(|b| ProtoReader::parse(b).ok())
    }

    pub fn messages(&self, field: u32) -> Vec<ProtoReader> {
        self.delimited
            .get(&field)
            .map(|list| list.iter().filter_map(|b| ProtoReader::parse(b).ok()).collect())
            .unwrap_or_default()
    }
}

fn read_varint(bytes: &[u8], pos: &mut usize) -> Result<u64> {
    let mut val = 0u64;
    let mut shift = 0u32;
    for _ in 0..10 {
        let b = *bytes.get(*pos).context("Unexpected EOF reading varint")?;
        *pos += 1;
        val |= ((b & 0x7f) as u64) << shift;
        if (b & 0x80) == 0 {
            return Ok(val);
        }
        shift += 7;
    }
    bail!("Malformed varint in Protobuf")
}

pub fn unzigzag(n: i32) -> i32 {
    ((n as u32) >> 1) as i32 ^ -(n & 1)
}

/// 16-byte binary GID → canonical `spotify:{kind}:{22 base62 chars}`.
pub fn gid_uri(kind: &str, gid: &[u8]) -> Option<String> {
    if gid.len() != 16 {
        return None;
    }
    let mut value = u128::from_be_bytes(gid.try_into().ok()?);
    let mut out = [b'0'; 22];
    for i in (0..22).rev() {
        out[i] = BASE62[(value % 62) as usize];
        value /= 62;
    }
    let id = std::str::from_utf8(&out).ok()?;
    Some(format!("spotify:{kind}:{id}"))
}

/// Normalize Spotify URIs like `spotify:user:foo:playlist:BAR` or `spotify:track:BAR`.
pub fn canonical_uri(value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split(':').collect();
    let (kind, id) = match parts.as_slice() {
        ["spotify", k, id] => (*k, *id),
        ["spotify", "user", _, "playlist", id] => ("playlist", *id),
        _ => return None,
    };
    if !matches!(kind, "track" | "album" | "artist" | "playlist") {
        return None;
    }
    if !(21..=22).contains(&id.len()) || !id.bytes().all(|b| BASE62.contains(&b)) {
        return None;
    }
    Some(format!("spotify:{kind}:{id}"))
}

pub fn uri_id(uri: &str) -> Option<&str> {
    let (_, id) = uri.rsplit_once(':')?;
    (!id.is_empty()).then_some(id)
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks_exact(2) {
        let pair = std::str::from_utf8(chunk).ok()?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_proto_and_gid() {
        let raw = encode(|w| {
            w.write_uint32(1, 42);
            w.write_str(2, "hello");
            w.write_msg(3, |inner| {
                inner.write_bool(1, true);
                inner.write_int64(2, 1_700_000_000);
            });
        });
        let r = ProtoReader::parse(&raw).unwrap();
        assert_eq!(r.number(1), Some(42));
        assert_eq!(r.text(2).as_deref(), Some("hello"));
        let inner = r.message(3).unwrap();
        assert_eq!(inner.bool_val(1), Some(true));
        assert_eq!(inner.long(2), Some(1_700_000_000));

        // 0x00..00 -> 22 zeroes
        assert_eq!(gid_uri("track", &[0u8; 16]).as_deref(), Some("spotify:track:0000000000000000000000"));
        assert_eq!(
            canonical_uri("spotify:user:test:playlist:37i9dQZF1DXcBWIGoYBM5M").as_deref(),
            Some("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M")
        );
        assert_eq!(unzigzag(0), 0);
        assert_eq!(unzigzag(1), -1);
        assert_eq!(unzigzag(2), 1);
    }
}
