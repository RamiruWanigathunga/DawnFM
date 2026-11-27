//! Lyrics types, recent cache, and lightweight Unison/LRCLib/lrc.red fallback.
//!
//! Primary lyrics always come from the active playback platform's official API
//! (`ytm::lyrics` for YouTube Music, `SpotifyClient::lyrics` for Spotify). When
//! neither platform has lyrics for a track (common for regional/Sinhala songs and
//! some catalog tracks), `Client::fetch_fallback` checks Unison, LRCLib, and lrc.red
//! silently without adding platform-picker clutter to the UI.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::sync::MutexExt;

const UNISON_API: &str = "https://unison.betterlyrics.org/lyrics";
const LRCLIB_API: &str = "https://lrclib.net/api/";
const LRCRED_API: &str = "https://lrc.red/";
const UA: &str = concat!("DawnFM/", env!("CARGO_PKG_VERSION"), " (+https://github.com/wenujacodes/DawnFM)");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Provider {
    YouTubeMusic,
    Spotify,
    Unison,
    Lrcred,
    Lrclib,
}

impl Provider {
    pub const ALL: [Self; 5] = [Self::YouTubeMusic, Self::Spotify, Self::Unison, Self::Lrcred, Self::Lrclib];

    pub const fn key(self) -> &'static str {
        match self {
            Self::YouTubeMusic => "youtube-music",
            Self::Spotify => "spotify",
            Self::Unison => "unison",
            Self::Lrcred => "lrcred",
            Self::Lrclib => "lrclib",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|p| p.key() == k)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::YouTubeMusic => "YouTube Music",
            Self::Spotify => "Spotify",
            Self::Unison => "Unison",
            Self::Lrcred => "lrc.red",
            Self::Lrclib => "LRCLib",
        }
    }
}

/// How a set of lines is timed, decided from the lines themselves.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Sync {
    None,
    Line,
    Word,
}

/// A timed piece of a line: a syllable, a word or several words.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub start: i32,
    pub dur: i32,
    pub text: String,
    /// Background vocals, shown on their own row under the line.
    pub bg: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Line {
    pub start: i32,
    pub dur: i32,
    pub text: String,
    /// Empty for line-synced and plain lyrics.
    pub parts: Vec<Part>,
    /// Singer: `v1`, `v2`, … or `v1000` for everyone.
    pub agent: Option<String>,
    /// A gap of more than five seconds nobody sings over.
    pub instrumental: bool,
    /// Translations shipped with the lyrics, by language.
    pub translations: Vec<(String, String)>,
    /// Romanization shipped with the lyrics, and its timing when it has some.
    pub romanization: Option<String>,
    pub timed_roman: Vec<Part>,
}

impl Line {
    pub fn end(&self) -> i32 {
        self.start + self.dur
    }
}

#[derive(Clone, Debug)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    pub sync: Sync,
    pub provider: Provider,
    /// Language the source says the lyrics are in.
    pub language: Option<String>,
    pub songwriters: Vec<String>,
}

impl Lyrics {
    pub fn from_raw(provider: Provider, lines: Vec<Line>, language: Option<String>, songwriters: Vec<String>) -> Option<Self> {
        if !lines.iter().any(|l| !l.instrumental && !l.text.trim().is_empty()) {
            return None;
        }
        let sync = derive_sync(&lines);
        Some(Self { lines, sync, provider, language, songwriters })
    }
}

fn derive_sync(lines: &[Line]) -> Sync {
    if lines.iter().any(|l| !l.instrumental && l.parts.iter().any(|p| p.dur != 0)) {
        Sync::Word
    } else if lines.iter().all(|l| l.start == 0) {
        Sync::None
    } else {
        Sync::Line
    }
}

/// Lyrics resolved for a song.
#[derive(Clone, Debug, Default)]
pub struct LyricsSet {
    pub sources: Vec<Lyrics>,
}

impl LyricsSet {
    pub fn get(&self, p: Provider) -> Option<&Lyrics> {
        self.sources.iter().find(|l| l.provider == p)
    }
}

const RECENT: usize = 128;

pub struct Client {
    http: reqwest::Client,
    /// The last few songs' lyrics, for replays and going back.
    recent: Mutex<VecDeque<(String, LyricsSet)>>,
}

impl Client {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, recent: Mutex::new(VecDeque::new()) }
    }

    /// Look up cached lyrics by track ID.
    pub fn get_recent(&self, video_id: &str) -> Option<LyricsSet> {
        self.recent.lock_unpoisoned().iter().find(|(id, _)| id == video_id).map(|(_, l)| l.clone())
    }

    /// Insert or update lyrics in recent cache.
    pub fn insert_recent(&self, video_id: String, set: LyricsSet) {
        let mut recent = self.recent.lock_unpoisoned();
        if let Some(pos) = recent.iter().position(|(id, _)| *id == video_id) {
            recent.remove(pos);
        } else if recent.len() >= RECENT {
            recent.pop_front();
        }
        recent.push_back((video_id, set));
    }

    /// Fetch fallback lyrics from Unison, lrc.red, and LRCLib when official platform sources have none.
    pub async fn fetch_fallback(&self, video_id: Option<&str>, song: &str, artist: &str, duration: Option<u32>) -> Option<Lyrics> {
        let clean_artist = strip_topic(artist);
        let (unison, lrcred, lrclib) = tokio::join!(
            self.fetch_unison(video_id, song, clean_artist, duration),
            self.fetch_lrcred(song, clean_artist, duration),
            self.fetch_lrclib(song, clean_artist, duration),
        );
        [unison, lrcred, lrclib]
            .into_iter()
            .flatten()
            .max_by_key(|l| match l.sync {
                Sync::Word => 3,
                Sync::Line => 2,
                Sync::None => 1,
            })
    }

    async fn fetch_unison(&self, video_id: Option<&str>, song: &str, artist: &str, duration: Option<u32>) -> Option<Lyrics> {
        if song.trim().is_empty() && video_id.is_none() {
            return None;
        }
        let clean_song = clean_title(song);
        let mut params = vec![
            ("song", if clean_song.is_empty() { song.to_owned() } else { clean_song }),
            ("artist", artist.to_owned()),
            ("duration", duration.unwrap_or(0).to_string()),
        ];
        if let Some(v) = video_id.filter(|v| !v.is_empty()) {
            params.push(("v", v.to_owned()));
        }
        let resp = self
            .http
            .get(UNISON_API)
            .query(&params)
            .header(reqwest::header::USER_AGENT, UA)
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: Value = resp.json().await.ok()?;
        let data = &v["data"];
        let text = data["lyrics"].as_str()?;
        let format = data["format"].as_str().unwrap_or("");
        let lang = data["language"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        let dur_ms = duration.unwrap_or(0) as i32 * 1000;
        let lines = match format {
            "lrc" => parse_lrc(text, dur_ms),
            "ttml" => parse_ttml_basic(text, dur_ms),
            "plain" => parse_plain(text),
            _ if looks_like_lrc(text) => parse_lrc(text, dur_ms),
            _ => parse_plain(text),
        };
        Lyrics::from_raw(Provider::Unison, lines, lang, Vec::new())
    }

    async fn fetch_lrcred(&self, song: &str, artist: &str, duration: Option<u32>) -> Option<Lyrics> {
        let base_title = clean_title(song);
        if base_title.is_empty() {
            return None;
        }
        let primary = primary_artist(artist);
        let query = if primary.is_empty() {
            base_title.clone()
        } else {
            format!("{base_title} {primary}")
        };
        let hits: Value = self
            .http
            .get(format!("{LRCRED_API}search.json"))
            .query(&[("q", query)])
            .header(reqwest::header::USER_AGENT, UA)
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()?;
        let hit = best_lrcred_hit(hits["hits"].as_array().map(Vec::as_slice).unwrap_or_default(), &base_title, artist, duration)?;
        let isrc = hit["isrc"]
            .as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric()))?;
        let text = self
            .http
            .get(format!("{LRCRED_API}s/{isrc}.ttml"))
            .header(reqwest::header::USER_AGENT, UA)
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .text()
            .await
            .ok()?;
        let lines = parse_ttml_basic(&text, duration.unwrap_or(0) as i32 * 1000);
        Lyrics::from_raw(Provider::Lrcred, lines, None, Vec::new())
    }

    async fn fetch_lrclib(&self, song: &str, artist: &str, duration: Option<u32>) -> Option<Lyrics> {
        let raw_title = song.trim();
        let base_title = clean_title(raw_title);
        if base_title.is_empty() && raw_title.is_empty() {
            return None;
        }
        let dur_ms = duration.unwrap_or(0) as i32 * 1000;
        let primary = primary_artist(artist);

        let titles: Vec<&str> = if base_title.is_empty() || base_title == raw_title {
            vec![raw_title]
        } else {
            vec![raw_title, &base_title]
        };
        for t in &titles {
            for art in [artist.trim(), primary] {
                if art.is_empty() {
                    continue;
                }
                let mut params = vec![("track_name", (*t).to_owned()), ("artist_name", art.to_owned())];
                if let Some(d) = duration.filter(|&d| d > 0) {
                    params.push(("duration", d.to_string()));
                }
                if let Ok(resp) = self
                    .http
                    .get(format!("{LRCLIB_API}get"))
                    .query(&params)
                    .header(reqwest::header::USER_AGENT, UA)
                    .timeout(Duration::from_secs(4))
                    .send()
                    .await
                    && resp.status().is_success()
                    && let Ok(v) = resp.json::<Value>().await
                    && let Some(l) = lrclib_item_to_lyrics(&v, dur_ms)
                {
                    return Some(l);
                }
                if art == primary {
                    break;
                }
            }
        }

        // Fall back to LRCLib search (handles "(feat. ...)", "- Remaster", and regional naming variants).
        let search_title = if base_title.is_empty() { raw_title } else { &base_title };
        let queries: Vec<String> = if primary.is_empty() {
            vec![search_title.to_owned()]
        } else {
            vec![format!("{search_title} {primary}"), search_title.to_owned()]
        };
        for q in queries {
            let Ok(resp) = self
                .http
                .get(format!("{LRCLIB_API}search"))
                .query(&[("q", q)])
                .header(reqwest::header::USER_AGENT, UA)
                .timeout(Duration::from_secs(4))
                .send()
                .await
            else {
                continue;
            };
            if !resp.status().is_success() {
                continue;
            }
            let Ok(Value::Array(hits)) = resp.json::<Value>().await else { continue };
            if let Some(best) = best_lrclib_hit(&hits, search_title, artist, duration)
                && let Some(l) = lrclib_item_to_lyrics(best, dur_ms)
            {
                return Some(l);
            }
        }
        None
    }
}

fn strip_topic(artist: &str) -> &str {
    artist.trim().strip_suffix(" - Topic").unwrap_or_else(|| artist.trim())
}

fn primary_artist(artist: &str) -> &str {
    strip_topic(artist).split([',', '&']).next().map(str::trim).unwrap_or("")
}

fn clean_title(s: &str) -> String {
    let s = s.split(" (").next().unwrap_or(s);
    let s = s.split(" [").next().unwrap_or(s);
    let s = s.split(" - ").next().unwrap_or(s);
    s.trim().to_owned()
}

fn fold(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn fold_title(s: &str) -> String {
    fold(&clean_title(s))
}

fn lrclib_item_to_lyrics(v: &Value, dur_ms: i32) -> Option<Lyrics> {
    if v["instrumental"].as_bool() == Some(true) {
        return None;
    }
    if let Some(synced) = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(synced, dur_ms);
        if let Some(l) = Lyrics::from_raw(Provider::Lrclib, lines, None, Vec::new()) {
            return Some(l);
        }
    }
    if let Some(plain) = v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_plain(plain);
        if let Some(l) = Lyrics::from_raw(Provider::Lrclib, lines, None, Vec::new()) {
            return Some(l);
        }
    }
    None
}

fn best_lrcred_hit<'a>(hits: &'a [Value], song: &str, artist: &str, duration: Option<u32>) -> Option<&'a Value> {
    let title = fold_title(song);
    let artist = fold(strip_topic(artist));
    let dur = duration.filter(|&d| d > 0).map(f64::from);
    let mut best: Option<(i32, &'a Value)> = None;
    for h in hits {
        let ht = fold_title(h["title"].as_str().unwrap_or_default());
        let ha = fold(h["artist"].as_str().unwrap_or_default());
        let title_ok = !title.is_empty() && (ht == title || ht.contains(&title) || title.contains(&ht));
        let artist_ok = artist.is_empty()
            || ha == artist
            || ha.contains(&artist)
            || artist.contains(&ha)
            || artist
                .split_whitespace()
                .filter(|w| w.len() >= 2)
                .any(|w| ha.split_whitespace().any(|hw| hw == w));
        if !title_ok || !artist_ok {
            continue;
        }
        let diff = match (dur, h["duration"].as_f64()) {
            (Some(a), Some(b)) => (a - b).abs(),
            _ => 0.0,
        };
        if diff > 20.0 {
            continue;
        }
        let base = if ht == title { 100 } else { 60 };
        let score = base + (20.0 - diff) as i32;
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, h));
        }
    }
    best.map(|(_, h)| h)
}

fn best_lrclib_hit<'a>(hits: &'a [Value], song: &str, artist: &str, duration: Option<u32>) -> Option<&'a Value> {
    let want_title = fold_title(song);
    let want_artist = fold(strip_topic(artist));
    let dur = duration.filter(|&d| d > 0).map(f64::from);
    let mut best: Option<(i32, &'a Value)> = None;

    for h in hits {
        if h["instrumental"].as_bool() == Some(true) {
            continue;
        }
        let has_synced = h["syncedLyrics"].as_str().is_some_and(|s| !s.trim().is_empty());
        let has_plain = h["plainLyrics"].as_str().is_some_and(|s| !s.trim().is_empty());
        if !has_synced && !has_plain {
            continue;
        }
        let got_title = fold_title(h["trackName"].as_str().or_else(|| h["name"].as_str()).unwrap_or(""));
        if got_title.is_empty() || (got_title != want_title && !got_title.contains(&want_title) && !want_title.contains(&got_title)) {
            continue;
        }
        let got_artist = fold(h["artistName"].as_str().unwrap_or(""));
        let artist_match = !want_artist.is_empty()
            && want_artist
                .split_whitespace()
                .filter(|w| w.len() >= 2)
                .any(|w| got_artist.split_whitespace().any(|gw| gw == w));
        let d_diff = match (dur, h["duration"].as_f64()) {
            (Some(want), Some(got)) => Some((want - got).abs()),
            _ => None,
        };
        if let Some(diff) = d_diff
            && diff > 15.0
        {
            continue;
        }
        if !artist_match && d_diff.is_none_or(|d| d > 3.0) {
            continue;
        }
        let mut score = 0;
        if got_title == want_title {
            score += 40;
        }
        if artist_match {
            score += 40;
        }
        if has_synced {
            score += 20;
        }
        if let Some(diff) = d_diff {
            score -= (diff * 2.0) as i32;
        }
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, h));
        }
    }
    best.map(|(_, h)| h)
}

fn looks_like_lrc(text: &str) -> bool {
    text.lines().take(8).any(|l| parse_lrc_stamp(l.trim_start()).is_some())
}

pub fn parse_plain(text: &str) -> Vec<Line> {
    let norm = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = norm.split('\n').map(|s| s.trim_end().to_owned()).collect();
    while lines.first().is_some_and(|s| s.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|s| s.trim().is_empty()) {
        lines.pop();
    }
    lines.into_iter().map(|text| Line { text, ..Default::default() }).collect()
}

fn parse_lrc_stamp(s: &str) -> Option<(i32, &str)> {
    let rest = s.strip_prefix('[')?;
    let end = rest.find(']')?;
    let tag = &rest[..end];
    let (mm, ss_part) = tag.split_once(':')?;
    let min: i32 = mm.trim().parse().ok()?;
    let (sec_s, frac_s) = match ss_part.split_once('.') {
        Some((a, b)) => (a, b),
        None => ss_part.split_once(':').unwrap_or((ss_part, "")),
    };
    let sec: i32 = sec_s.trim().parse().ok()?;
    let ms: i32 = if frac_s.is_empty() {
        0
    } else {
        let mut digits: String = frac_s.chars().take(3).collect();
        while digits.len() < 3 {
            digits.push('0');
        }
        digits.parse().ok()?
    };
    Some((min * 60_000 + sec * 1_000 + ms, &rest[end + 1..]))
}

fn parse_lrc(text: &str, song_ms: i32) -> Vec<Line> {
    let mut offset_ms = 0i32;
    let mut rows: Vec<(i32, String)> = Vec::new();
    for raw in text.lines() {
        let mut s = raw.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(val) = s.strip_prefix("[offset:").and_then(|x| x.strip_suffix(']')) {
            if let Ok(sec) = val.trim().parse::<f32>() {
                offset_ms = (sec * 1000.0).round() as i32;
            }
            continue;
        }
        let mut times = Vec::new();
        while let Some((t, rest)) = parse_lrc_stamp(s) {
            times.push(t);
            s = rest.trim_start();
        }
        if times.is_empty() || s.is_empty() {
            continue;
        }
        let clean = strip_angle_stamps(s);
        let clean = clean.trim();
        if clean.is_empty() {
            continue;
        }
        for t in times {
            rows.push(((t - offset_ms).max(0), clean.to_owned()));
        }
    }
    rows.sort_by_key(|(t, _)| *t);
    let mut out: Vec<Line> = rows
        .into_iter()
        .map(|(start, text)| Line { start, text, ..Default::default() })
        .collect();
    for i in 0..out.len() {
        let next_start = out.get(i + 1).map(|l| l.start).unwrap_or_else(|| {
            if song_ms > out[i].start { song_ms } else { out[i].start + 4000 }
        });
        out[i].dur = (next_start - out[i].start).max(0);
    }
    out
}

fn strip_angle_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        if let Some(j) = after.find('>') {
            rest = &after[j + 1..];
        } else {
            out.push('<');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn parse_ttml_time(s: &str) -> Option<i32> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("ms") {
        return v.trim().parse::<f64>().ok().map(|n| n.round() as i32);
    }
    if let Some(v) = s.strip_suffix('s') {
        return v.trim().parse::<f64>().ok().map(|n| (n * 1000.0).round() as i32);
    }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => parts[0].parse::<f64>().ok().map(|n| (n * 1000.0).round() as i32),
        2 => Some(((parts[0].parse::<f64>().ok()? * 60.0 + parts[1].parse::<f64>().ok()?) * 1000.0).round() as i32),
        3 => Some(((parts[0].parse::<f64>().ok()? * 3600.0 + parts[1].parse::<f64>().ok()? * 60.0 + parts[2].parse::<f64>().ok()?) * 1000.0).round() as i32),
        _ => None,
    }
}

fn parse_ttml_basic(xml: &str, song_ms: i32) -> Vec<Line> {
    let Ok(doc) = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "p") {
        let Some(begin) = p.attribute("begin").and_then(parse_ttml_time) else { continue };
        let end = p.attribute("end").and_then(parse_ttml_time);
        let mut parts = Vec::new();
        for span in p.children().filter(|n| n.is_element() && n.tag_name().name() == "span") {
            let role = span.attribute("role").unwrap_or("");
            if role == "x-translation" || role == "x-roman" {
                continue;
            }
            let Some(sb) = span.attribute("begin").and_then(parse_ttml_time) else { continue };
            let se = span.attribute("end").and_then(parse_ttml_time).unwrap_or(sb);
            let txt: String = span.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
            if !txt.is_empty() {
                parts.push(Part { start: sb, dur: (se - sb).max(0), text: txt, bg: role == "x-bg" });
            }
        }
        let text: String = if !parts.is_empty() {
            parts.iter().map(|pt| pt.text.as_str()).collect()
        } else {
            p.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        out.push(Line {
            start: begin,
            dur: end.map(|e| (e - begin).max(0)).unwrap_or(0),
            text: text.to_owned(),
            parts,
            ..Default::default()
        });
    }
    for i in 0..out.len() {
        if out[i].dur == 0 {
            let next_start = out.get(i + 1).map(|l| l.start).unwrap_or_else(|| {
                if song_ms > out[i].start { song_ms } else { out[i].start + 4000 }
            });
            out[i].dur = (next_start - out[i].start).max(0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_sync_classifies_plain_line_and_word() {
        let plain = vec![Line { text: "Hello".into(), ..Default::default() }];
        assert_eq!(Lyrics::from_raw(Provider::YouTubeMusic, plain, None, vec![]).unwrap().sync, Sync::None);

        let synced = vec![
            Line { start: 1000, dur: 2000, text: "Hello".into(), ..Default::default() },
            Line { start: 3000, dur: 2000, text: "World".into(), ..Default::default() },
        ];
        assert_eq!(Lyrics::from_raw(Provider::YouTubeMusic, synced, None, vec![]).unwrap().sync, Sync::Line);

        let word = vec![Line {
            start: 1000,
            dur: 1000,
            text: "Hello".into(),
            parts: vec![Part { start: 1000, dur: 500, text: "Hel".into(), bg: false }],
            ..Default::default()
        }];
        assert_eq!(Lyrics::from_raw(Provider::Spotify, word, None, vec![]).unwrap().sync, Sync::Word);
    }

    #[test]
    fn lrc_and_lrclib_search_handle_sinhala_and_remastered_english_tracks() {
        let lrc = "[00:12.50]ආදරය පාන මා සඳේ\n[00:18.00]නුඹට හිත තාම ආදරේ";
        let lines = parse_lrc(lrc, 30_000);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].start, 12_500);
        assert_eq!(lines[0].dur, 5_500);
        assert_eq!(lines[0].text, "ආදරය පාන මා සඳේ");

        let hits = vec![
            serde_json::json!({
                "trackName": "Sanda Numba Awidin",
                "artistName": "Uvindu Ayshcharya",
                "duration": 236.0,
                "plainLyrics": "මගේ හදවතේ"
            }),
            serde_json::json!({
                "trackName": "Caribbean Blue",
                "artistName": "Enya",
                "duration": 239.0,
                "syncedLyrics": "[00:10.00]Eurus"
            }),
        ];
        let si = best_lrclib_hit(&hits, "Sanda Numba Awidin", "Uvindu Ayshcharya - Topic", Some(236)).unwrap();
        assert_eq!(si["trackName"], "Sanda Numba Awidin");

        let en = best_lrclib_hit(&hits, "Caribbean Blue - 2009 Remaster", "Enya", Some(240)).unwrap();
        assert_eq!(en["trackName"], "Caribbean Blue");
    }

    #[test]
    fn recent_cache_roundtrips_and_evicts_oldest() {
        let c = Client::new(reqwest::Client::new());
        for i in 0..(RECENT + 2) {
            c.insert_recent(format!("id-{i}"), LyricsSet::default());
        }
        assert!(c.get_recent("id-0").is_none());
        assert!(c.get_recent("id-1").is_none());
        assert!(c.get_recent(&format!("id-{}", RECENT + 1)).is_some());
    }
}
