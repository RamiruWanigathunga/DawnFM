//! The listener's lyrics settings: translation language, timing offsets, and text scale.
//! Also the smoothed clock the lyrics follow.

use std::collections::HashMap;

use crate::lyrics::{Line, Lyrics, LyricsSet, Provider};

/// Offsets are seconds taken off the lyrics clock: positive shows lyrics later.
pub struct Prefs {
    pub target: String,
    pub global_offset: f32,
    pub word_trim: f32,
    pub line_trim: f32,
    /// "videoId|source" → offset.
    pub song_offsets: HashMap<String, f32>,
    /// Lyrics text size, as a multiple of the theme's own size.
    pub text_scale: f32,
}

/// The range and step of the lyrics text size setting.
pub const TEXT_SCALE: (f32, f32, f32) = (0.6, 1.8, 0.1);

/// Languages offered for translation.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("pl", "Polish"),
    ("tr", "Turkish"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("id", "Indonesian"),
    ("vi", "Vietnamese"),
    ("th", "Thai"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese (Simplified)"),
    ("zh-TW", "Chinese (Traditional)"),
];

impl Prefs {
    pub fn load(get: &dyn Fn(&str) -> Option<String>) -> Self {
        let num = |k: &str| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        Self {
            target: get("lyric_lang").filter(|l| !l.is_empty()).unwrap_or_else(|| "en".into()),
            global_offset: num("lyric_offset"),
            word_trim: num("lyric_trim_word"),
            line_trim: num("lyric_trim_line"),
            text_scale: get("lyric_scale").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0).clamp(TEXT_SCALE.0, TEXT_SCALE.1),
            song_offsets: get("lyric_song_offsets").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
        }
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string("lyric_lang", self.target.clone());
        storage.set_string("lyric_offset", format!("{:.1}", self.global_offset));
        storage.set_string("lyric_trim_word", format!("{:.1}", self.word_trim));
        storage.set_string("lyric_trim_line", format!("{:.1}", self.line_trim));
        storage.set_string("lyric_scale", format!("{:.2}", self.text_scale));
        storage.set_string("lyric_song_offsets", serde_json::to_string(&self.song_offsets).unwrap_or_default());
    }

    /// The official source for this track when it has usable lyrics.
    pub fn choose(&self, _video_id: &str, set: &LyricsSet) -> Option<Provider> {
        set.sources.iter().find(|l| usable(l)).map(|l| l.provider)
    }

    pub fn song_offset(&self, video_id: &str, source: Provider) -> f32 {
        self.song_offsets.get(&format!("{video_id}|{}", source.key())).copied().unwrap_or(0.0)
    }

    pub fn set_song_offset(&mut self, video_id: &str, source: Provider, v: f32) {
        let k = format!("{video_id}|{}", source.key());
        let v = (v * 10.0).round() / 10.0;
        if v == 0.0 {
            self.song_offsets.remove(&k);
        } else {
            self.song_offsets.insert(k, v);
        }
    }

    /// Seconds to take off the clock for this song and source
    /// (`globalLyricOffset + lyricOffset` and the per-sync-type trim).
    pub fn offset(&self, video_id: &str, source: Provider, richsync: bool) -> f64 {
        (self.global_offset + self.song_offset(video_id, source) + if richsync { self.word_trim } else { self.line_trim }) as f64
    }
}

/// Lyrics worth showing: some line is sung.
pub fn usable(l: &Lyrics) -> bool {
    l.lines.iter().any(|x| !x.instrumental && !x.text.trim().is_empty())
}

/// `findBestLanguageMatch`: a translation shipped with the lyrics in the listener's language.
pub fn shipped_translation<'a>(l: &'a Line, target: &str) -> Option<&'a str> {
    let base = |s: &str| s.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
    l.translations
        .iter()
        .find(|(lang, _)| lang.eq_ignore_ascii_case(target))
        .or_else(|| l.translations.iter().find(|(lang, _)| base(lang) == base(target)))
        .map(|(_, t)| t.as_str())
}

/// The lyrics clock: the audio position smoothed so it never steps backwards while
/// playing (the output device takes audio in ~40 ms blocks, so the raw position jumps).
#[derive(Default)]
pub struct Clock {
    shown: f64,
    wall: f64,
    playing: bool,
}

impl Clock {
    pub fn tick(&mut self, pos: f64, playing: bool, now: f64) -> f64 {
        if !playing || !self.playing || self.wall == 0.0 {
            self.shown = pos;
        } else {
            let dt = (now - self.wall).max(0.0);
            let predicted = self.shown + dt;
            let err = pos - predicted;
            if err.abs() > 0.25 {
                self.shown = pos;
            } else {
                // Ease toward the audio, never backwards.
                self.shown = (predicted + err * (dt * 6.0).min(1.0)).max(self.shown);
            }
        }
        self.wall = now;
        self.playing = playing;
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(of: &[Provider]) -> LyricsSet {
        use crate::lyrics::Sync;
        let line = Line { start: 1000, dur: 1000, text: "la".into(), ..Default::default() };
        LyricsSet { sources: of.iter().map(|p| Lyrics { lines: vec![line.clone()], sync: Sync::Line, provider: *p, language: None, songwriters: vec![] }).collect() }
    }

    #[test]
    fn chooses_usable_official_source() {
        let p = Prefs::load(&|_| None);
        let s = set(&[Provider::YouTubeMusic]);
        assert_eq!(p.choose("v", &s), Some(Provider::YouTubeMusic));

        let mut empty_sung = set(&[Provider::Spotify]);
        empty_sung.sources[0].lines[0].instrumental = true;
        assert_eq!(p.choose("spotify:track:1", &empty_sung), None);
        assert_eq!(p.choose("v", &LyricsSet::default()), None);
    }

    #[test]
    fn tracks_per_song_timing_offsets() {
        let mut p = Prefs::load(&|_| None);
        p.set_song_offset("v1", Provider::YouTubeMusic, 0.3);
        assert_eq!(p.song_offset("v1", Provider::YouTubeMusic), 0.3);
        p.set_song_offset("v1", Provider::YouTubeMusic, 0.0);
        assert_eq!(p.song_offset("v1", Provider::YouTubeMusic), 0.0);
    }
}
