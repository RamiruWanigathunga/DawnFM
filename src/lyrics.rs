//! Official lyrics types and in-memory recent cache.
//!
//! Lyrics come exclusively from the active playback platform's official API
//! (`ytm::lyrics` for YouTube Music, `SpotifyClient::lyrics` for Spotify).

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::sync::MutexExt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Provider {
    YouTubeMusic,
    Spotify,
}

impl Provider {
    pub const ALL: [Self; 2] = [Self::YouTubeMusic, Self::Spotify];

    pub const fn key(self) -> &'static str {
        match self {
            Self::YouTubeMusic => "youtube-music",
            Self::Spotify => "spotify",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|p| p.key() == k)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::YouTubeMusic => "YouTube Music",
            Self::Spotify => "Spotify",
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
        if lines.is_empty() {
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

/// Official lyrics for a song.
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

#[derive(Default)]
pub struct Client {
    /// The last few songs' lyrics, for replays and going back.
    recent: Mutex<VecDeque<(String, LyricsSet)>>,
}

impl Client {
    pub fn new() -> Self {
        Self::default()
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
    fn recent_cache_roundtrips_and_evicts_oldest() {
        let c = Client::new();
        for i in 0..(RECENT + 2) {
            c.insert_recent(format!("id-{i}"), LyricsSet::default());
        }
        assert!(c.get_recent("id-0").is_none());
        assert!(c.get_recent("id-1").is_none());
        assert!(c.get_recent(&format!("id-{}", RECENT + 1)).is_some());
    }
}
