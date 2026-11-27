//! What the backend answers, applied to the app.

use crate::sync::MutexExt;
use std::collections::HashSet;
use std::time::Duration;

use souvlaki::MediaControlEvent;

use crate::audio::{AudioCmd, AudioEvent};
use crate::backend::{Req, Resp};
use crate::model::{ShelfKind, Track};
use crate::ytm::HomePage;

use super::sleep::Sleep;
use super::{Action, App, Dialog, HomeState, LibTab, MixFrom, Page, PlayingAs, Repeat, dev_var, shuffle};

impl App {
    pub(super) fn pump(&mut self, now: f64) {
        let cover = self.media_cover.lock_unpoisoned().take();
        // The listed track's art, with what plays's title, artists and length.
        if let Some((id, file)) = cover
            && self.current().is_some_and(|t| t.id == id)
            && self.mix_hold().is_none()
            && let Some(track) = self.playing_track().cloned()
            && let Some(m) = &mut self.media
        {
            m.set_track(&track.title, &track.artist_line(), track.album.as_ref().map(|a| a.name.as_str()), Some(&file), track.duration);
        }
        while let Ok(r) = self.ch.resp_rx.try_recv() {
            self.on_resp(r, now);
        }
        while let Ok(ev) = self.ch.audio_rx.try_recv() {
            match ev {
                AudioEvent::Finished { generation } if generation == self.generation && self.jam.guest() => {}
                AudioEvent::Finished { generation } if generation == self.generation => {
                    if self.sleep == Sleep::EndOfSong {
                        self.sleep_at_song_end();
                    } else if self.queue.repeat == Repeat::One {
                        self.play_current();
                    } else {
                        self.next();
                    }
                }
                AudioEvent::Finished { .. } => {}
                // Crossfade into the next song; repeat-one, the end of the queue and a sleep
                // timer set for this song's end wait for the end.
                AudioEvent::Ending { generation }
                    if generation == self.generation && !self.jam.guest() && self.queue.repeat != Repeat::One && self.sleep != Sleep::EndOfSong =>
                {
                    // Automix: the next song eases in under this one, which keeps the
                    // player until the blend has given the next song the louder half
                    // (see `MixFrom`).
                    if let Some(from) = self.playing_track().cloned() {
                        let total = self
                            .audio
                            .status
                            .duration()
                            .map(|d| d.as_secs_f32())
                            .or_else(|| from.duration.map(|d| d as f32))
                            .unwrap_or(0.0);
                        let mix = self.audio.status.mix_secs(Some(std::time::Duration::from_secs_f32(total)));
                        if total > 0.0 && mix > 0.5 {
                            self.pending_mix = Some(MixFrom { track: from, total, mix, started: false, released: false });
                        }
                    }
                    let before = self.playing_id().map(str::to_owned);
                    self.handoff = true;
                    self.next();
                    self.handoff = false;
                    if self.playing_id().map(str::to_owned) != before {
                        self.mix_from = self.pending_mix.take();
                    } else {
                        self.pending_mix = None;
                    }
                }
                AudioEvent::Ending { .. } => {}
                AudioEvent::Interrupted { generation, at, reason } if generation == self.generation => self.recover(at, &reason),
                AudioEvent::Interrupted { .. } => {}
                AudioEvent::Error(e) => self.toast(e),
            }
        }
        // Once the next song is the louder of the two, it takes the player over; the
        // caption stays while the handoff's tail runs out.
        let released = self.mix_from.as_ref().is_some_and(|h| h.released);
        let switched = self.watch_mix();
        let over = self.mix_over();
        if (switched || over) && !released {
            self.release_mix();
        }
        if over {
            self.mix_from = None;
        }
        while let Ok(ev) = self.ch.media_rx.try_recv() {
            match ev {
                MediaControlEvent::Play | MediaControlEvent::Pause if self.jam.guest() => self.toggle(),
                MediaControlEvent::Play if self.resume_at.is_some() => self.toggle(),
                MediaControlEvent::Play => self.audio.send(AudioCmd::Resume),
                MediaControlEvent::Pause | MediaControlEvent::Stop => self.audio.send(AudioCmd::Pause),
                MediaControlEvent::Toggle => self.toggle(),
                MediaControlEvent::Next => self.next(),
                MediaControlEvent::Previous => self.prev(),
                _ => {}
            }
        }
        if self.buffering && self.playing() {
            self.buffering = false;
            self.skip_streak = 0;
            self.prefetch_next();
            // Dev hook: YTM_SEEK=<secs> jumps into the first track once it starts.
            if self.generation == 1 {
                if let Some(s) = std::env::var("YTM_SEEK").ok().and_then(|v| v.parse::<f32>().ok()) {
                    self.audio.send(AudioCmd::Seek(Duration::from_secs_f32(s)));
                }
                // YTM_SEEK_LATER=<secs>@<delay> seeks again that many seconds later.
                if let Some((s, d)) = dev_var("YTM_SEEK_LATER").and_then(|v| v.split_once('@')).and_then(|(s, d)| Some((s.parse::<f32>().ok()?, d.parse::<f32>().ok()?))) {
                    let audio = self.audio.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs_f32(d));
                        audio.send(AudioCmd::Seek(Duration::from_secs_f32(s)));
                    });
                }
            }
        }
        if self.suggest_due.is_some_and(|d| now >= d) {
            self.suggest_due = None;
            let q = self.search_text.trim().to_owned();
            // An empty box signed in shows the account's recent searches.
            if self.forgets_pending == 0 && (!q.is_empty() || self.logged_in) {
                self.backend.request(Req::Suggest { query: q, round: self.suggest_round });
            }
        }
    }

    /// A search's results are on screen: the first result clicked becomes what
    /// "Recently Searched" remembers, not the words that were typed.
    pub(super) fn expect_search_click(&mut self, query: &str) {
        if matches!(&self.page, Page::Search(q) if q == query) || self.search_drop.as_deref() == Some(query) {
            self.search_pending = true;
        }
    }

    /// The entry a click makes: the item itself, with the artwork it was shown with.
    fn search_click_entry(&self, a: &Action) -> Option<super::RecentSearch> {
        let r = self.search.as_ref()?;
        let best = |thumbs: &[crate::model::Thumb]| thumbs.iter().max_by_key(|t| t.width).map(|t| t.url.clone());
        let song = |t: &Track| {
            Some(super::RecentSearch {
                query: r.query.clone(),
                target: Some(crate::model::Target::Song(t.clone())),
                title: t.title.clone(),
                subtitle: format!("Song • {}", t.artist_line()),
                thumb: best(&t.thumbs),
                round: false,
            })
        };
        let card = |target: &crate::model::Target| {
            let c = r.artists.iter().chain(&r.albums).chain(&r.playlists).find(|c| c.target == *target)?;
            Some(super::RecentSearch {
                query: r.query.clone(),
                target: Some(target.clone()),
                title: c.title.clone(),
                subtitle: c.subtitle.clone(),
                thumb: best(&c.thumbs),
                round: c.round,
            })
        };
        match a {
            Action::Open(crate::model::Target::Song(t)) | Action::Radio(t) => song(t),
            Action::Open(target) | Action::PlayCollection(target, _) => card(target),
            Action::Play(tracks, i) => tracks.get(*i).and_then(song),
            _ => None,
        }
    }

    /// Record the thing the search's first click opened, once.
    pub(super) fn remember_search_click(&mut self, a: &Action) {
        if !self.search_pending || !(matches!(self.page, Page::Search(_)) || self.search_drop.is_some()) {
            return;
        }
        let Some(entry) = self.search_click_entry(a) else { return };
        self.search_pending = false;
        // One entry per thing; a bare query left by an older version goes with it.
        self.recent_searches.retain(|s| {
            s.target.as_ref() != entry.target.as_ref() && !(s.target.is_none() && s.query.eq_ignore_ascii_case(&entry.query))
        });
        self.recent_searches.insert(0, entry);
        self.recent_searches.truncate(8);
    }

    fn on_resp(&mut self, r: Resp, now: f64) {
        match r {
            Resp::Home(page) => self.on_home(page),
            Resp::HomeMore(page) => {
                if let Some(h) = &mut self.home {
                    h.shelves.extend(page.shelves);
                    h.continuation = page.continuation;
                    h.loading_more = None;
                }
            }
            Resp::Explore(e) => {
                let mut prefetch_urls = Vec::new();
                for card in e.new_albums.iter().chain(&e.charts).take(16) {
                    if let Some(t) = crate::model::pick_thumb(&card.thumbs, 360) {
                        prefetch_urls.push(crate::model::sized(&t.url, 544));
                    }
                }
                for track in e.top.iter().take(16) {
                    if let Some(t) = crate::model::pick_thumb(&track.thumbs, 120) {
                        prefetch_urls.push(crate::model::sized(&t.url, 120));
                    }
                }
                if !prefetch_urls.is_empty() {
                    crate::images::prefetch_thumbs(&self.backend, prefetch_urls);
                }
                self.explore = Some(e);
                self.loading = false;
            }
            Resp::Search(r) => {
                if self.smoke.as_ref() == Some(&r.query) {
                    self.smoke = None;
                    if !r.tracks.is_empty() {
                        log::info!("smoke: playing {}", r.tracks[0].title);
                        self.play_list(r.tracks.clone(), 0);
                    }
                }
                self.expect_search_click(&r.query);
                self.search = Some(r);
                self.loading = false;
            }
            Resp::SearchForgotten => {
                // Replies dropped meanwhile left the list behind what's typed: ask again.
                self.forgets_pending = self.forgets_pending.saturating_sub(1);
                if self.forgets_pending == 0 {
                    self.suggest_due = Some(now);
                }
            }
            Resp::Suggest { query, round, terms } => {
                if round == self.suggest_round && self.forgets_pending == 0 {
                    // Keep late answers for backspacing/revisiting, but never display
                    // an answer for a query other than the one currently in the box.
                    self.suggest_cache.retain(|(q, _)| q != &query);
                    self.suggest_cache.push_back((query.clone(), terms.clone()));
                    if self.suggest_cache.len() > 32 {
                        self.suggest_cache.pop_front();
                    }
                    if self.search_text.trim() == query {
                        self.suggestions = (query, terms);
                        self.suggest_sel = None;
                    }
                }
            }
            Resp::Collection { load, c, more } => self.collection_in(load, c, more),
            Resp::CollectionMore { id, load, tracks, set_ids, more } => self.collection_more(id, load, tracks, set_ids, more),
            Resp::ArtistPhoto { id, thumbs } => {
                self.artist_photos.insert(id, thumbs);
            }
            Resp::Artist(mut a) => {
                // A slow answer for an artist left meanwhile doesn't replace the one on screen.
                if matches!(&self.page, Page::Artist(id) if *id != a.id) {
                    return;
                }
                if self.page == Page::Artist(a.id.clone()) {
                    self.loading = false;
                }
                let thumbs = if !a.banner.is_empty() {
                    &a.banner
                } else if !a.photo.is_empty() {
                    &a.photo
                } else {
                    a.albums.first().map(|c| &c.thumbs[..]).unwrap_or(&[])
                };
                if !thumbs.is_empty() {
                    self.request_artist_art(&a.id, thumbs);
                }
                let apple_key = format!("artist:{}", a.id);
                let answered = self.apple_artists.get(&a.id).is_some_and(Option::is_some);
                let cooling = self.apple_missed.get(&apple_key).is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(120));
                if !answered && !cooling {
                    let id = a.id.clone();
                    let name = a.name.clone();
                    if self.apple_waiting.insert(apple_key) {
                        self.backend.request(Req::AppleArtist { id, name });
                    }
                }
                let mut prefetch_urls = Vec::new();
                if let Some(t) = crate::model::pick_thumb(&a.banner, 1920).or_else(|| a.banner.iter().max_by_key(|t| t.width)) {
                    prefetch_urls.push(t.url.clone());
                }
                if let Some(t) = crate::model::pick_thumb(&a.photo, 544) {
                    prefetch_urls.push(crate::model::sized(&t.url, 544));
                }
                for card in a.albums.iter().chain(&a.playlists).chain(&a.similar).take(16) {
                    if let Some(t) = crate::model::pick_thumb(&card.thumbs, 360) {
                        prefetch_urls.push(crate::model::sized(&t.url, 544));
                    }
                }
                for track in a.top.iter().take(15) {
                    if let Some(t) = crate::model::pick_thumb(&track.thumbs, 120) {
                        prefetch_urls.push(crate::model::sized(&t.url, 120));
                    }
                }
                if !prefetch_urls.is_empty() {
                    crate::images::prefetch_thumbs(&self.backend, prefetch_urls);
                }
                // A page answered while a subscribe is in flight can carry the state from
                // before it: keep the flipped button until YouTube confirms the change.
                if let Some((channel, was)) = &self.optimistic_sub
                    && let Some(sub) = a.following.as_mut().filter(|s| &s.channel == channel)
                {
                    sub.subscribed = self.artist.as_ref().filter(|p| p.id == a.id).and_then(|p| p.following.as_ref()).map_or(!*was, |s| s.subscribed);
                }
                self.artists_cache.insert(a.id.clone(), (a.clone(), std::time::Instant::now()));
                if self.artists_cache.len() > 12 {
                    if let Some(oldest) = self.artists_cache.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| k.clone()) {
                        self.artists_cache.remove(&oldest);
                    }
                }
                self.artist = Some(a);
            }
            Resp::Library(mut l) => {
                // A half that didn't load keeps what was shown (or last launch's sidebar),
                // rather than emptying it.
                if l.playlists_failed {
                    l.playlists = self.library.as_ref().map(|o| o.playlists.clone()).or_else(|| self.sidebar_cache.clone()).unwrap_or_default();
                }
                if l.albums_failed {
                    l.albums = self.library.as_ref().map(|o| o.albums.clone()).unwrap_or_default();
                }
                self.library = Some(l);
                self.loading = false;
            }
            Resp::History(h) => {
                if matches!(self.page, Page::Library(LibTab::Artists)) {
                    for a in super::pages::played_artists(&h).into_iter().take(super::pages::HISTORY_ARTISTS_LIMIT) {
                        if let Some(id) = a.id {
                            if !self.artist_photos.contains_key(&id) {
                                self.artist_photos.insert(id.clone(), Vec::new());
                                self.backend.request(Req::ArtistPhoto(id));
                            }
                        }
                    }
                }
                self.history = Some(h);
                self.loading = false;
            }
            Resp::Artists(a) => {
                self.library_artists = Some(a);
                self.loading = false;
            }
            Resp::Downloaded { title, path } => {
                log::info!("downloaded to {}", path.display());
                self.toast(format!("Downloaded \u{201c}{title}\u{201d}"));
            }
            Resp::Lyrics { video_id, lyrics } => {
                if self.shown().is_some_and(|t| t.id == video_id) {
                    let shown = self.lyric_prefs.choose(&video_id, &lyrics);
                    log::info!("lyrics: {}", shown.map_or("none", |p| p.key()));
                    self.lyrics = Some((video_id, lyrics));
                }
            }
            Resp::Segments { video_id, segments } => {
                if self.playing_id() == Some(video_id.as_str()) {
                    log::info!("sponsorblock: {} segment(s) for {video_id}", segments.len());
                    self.segments = Some((video_id, segments));
                }
            }
            Resp::LyricsExtras { video_id, source, target, extras } => {
                if self.shown().is_some_and(|t| t.id == video_id) {
                    self.lyrics_extras = Some((video_id, source, target, extras));
                }
            }
            Resp::Art { key, art } => {
                self.art_waiting.remove(&key);
                self.art_colors.insert(key.clone(), art.dominant);
                self.art_accents.insert(key.clone(), art.accent);
                self.art_blurs.insert(key.clone(), art.rgba.clone());
                self.art_blur_tex.remove(&key);
                self.art_loaded_at.insert(key.clone(), now);
                if self.mix_hold().is_none() && self.current_id() == Some(key.as_str()) {
                    self.accent = art.accent;
                    self.dominant = art.dominant;
                    if let Some(bg) = &self.bg {
                        bg.set_art(art);
                    }
                    self.art_key = Some(key);
                    self.art_changed = now;
                }
            }
            Resp::AppleArtist { id, info } => {
                // A miss is cooled down rather than cached for the session: Apple's
                // search sometimes fails one time and answers the next.
                let key = format!("artist:{id}");
                self.apple_waiting.remove(&key);
                match info {
                    Some(artwork) => {
                        self.apple_missed.remove(&key);
                        if let Some(logo) = &artwork.logo {
                            let prefetch_url = logo.at_height(144);
                            crate::images::prefetch_thumbs(&self.backend, vec![prefetch_url]);
                        }
                        self.apple_artists.insert(id, Some(artwork));
                    }
                    None => {
                        self.apple_artists.remove(&id);
                        self.apple_missed.insert(key, std::time::Instant::now());
                    }
                }
            }
            Resp::ArtistSignature { key, signature } => {
                self.signature_waiting.remove(&key);
                match signature {
                    Some(s) => {
                        self.signature_missed.remove(&key);
                        self.signature_images.insert(key, (s.width, s.height, s.rgba));
                    }
                    None => {
                        self.signature_missed.insert(key, std::time::Instant::now());
                    }
                }
            }
            Resp::AppleCover { key, motion } => {
                self.apple_waiting.remove(&key);
                match motion.and_then(|m| m.square) {
                    Some(url) => {
                        self.apple_missed.remove(&key);
                        self.apple_covers.insert(key, Some(url));
                    }
                    None => {
                        self.apple_missed.insert(key, std::time::Instant::now());
                    }
                }
            }
            Resp::Radio { seed, tracks } => {
                self.radio_pending = false;
                let was_at_end = self.queue.add_radio(&seed, tracks);
                if was_at_end && self.queue.index.is_some() && !self.buffering && !self.playing() {
                    // The track ended while radio was loading: continue now.
                    self.next();
                } else if !self.buffering {
                    self.prefetch_next();
                }
            }
            // Unless something else was asked to play meanwhile; shuffled as last asked.
            Resp::PlayQueue { target, mut tracks } => {
                if let Some((p, shuffled)) = &self.play_pending
                    && *p == target
                {
                    if *shuffled {
                        shuffle(&mut tracks);
                    }
                    self.queue_source = Some(target);
                    self.play_list(tracks, 0);
                }
            }
            Resp::Liked { video_id, like } => {
                self.lists.edits.confirm_likes(std::slice::from_ref(&video_id), &[]);
                if like {
                    self.liked.insert(video_id);
                } else if crate::spotify::is_spotify_id(&video_id) {
                    self.liked.remove(&video_id);
                }
            }
            Resp::QuickPicksMore { seed, tracks } => {
                if let Some(h) = &mut self.home {
                    h.picks_loading = false;
                    if let Some(shelf) = h.shelves.iter_mut().find(|s| s.title == "Quick picks")
                        && let ShelfKind::Songs { tracks: picks, .. } = &mut shelf.kind
                        && picks.iter().any(|t| t.id == seed)
                    {
                        h.picks_expanded = true;
                        let mut seen: HashSet<String> = picks.iter().map(|t| t.id.clone()).collect();
                        picks.extend(tracks.into_iter().filter(|t| seen.insert(t.id.clone())).take(24));
                    }
                }
            }
            Resp::LikeProgress { taken, done, total, like } => {
                if !like {
                    let tracks: Vec<Track> = taken.iter().filter_map(|id| self.unliking.remove(id)).collect();
                    self.set_liked(&tracks, false);
                }
                if done == 0 {
                    // A Spotify transfer is adding to Liked Music (maybe paused): this waits for it.
                    self.toast("Waiting for the transfer to finish adding to Liked Music…");
                } else if total > 1 && done < total {
                    let note = if self.backend.api().limited() { " (YouTube Music is limiting requests; waiting)" } else { "" };
                    self.toast(format!("{} {done} of {total} songs…{note}", if like { "Liking" } else { "Removing likes:" }));
                }
            }
            Resp::LikedMany { video_ids, failed, like } => self.on_liked_many(video_ids, failed, like),
            Resp::PlaylistCreated { id, songs } => {
                self.request_library();
                self.toast(match songs {
                    0 => "Playlist created".to_owned(),
                    1 => "Song added to new playlist".to_owned(),
                    n => format!("{n} songs added to new playlist"),
                });
                self.go(Page::Playlist(id));
            }
            Resp::PlaylistChanged { id, message } => self.on_playlist_changed(id, message),
            Resp::Subscribed { channel, subscribed } => {
                self.optimistic_sub = None;
                if let Some(sub) = self.artist.as_mut().and_then(|a| a.following.as_mut()).filter(|s| s.channel == channel) {
                    sub.subscribed = subscribed;
                }
                for (cached, _) in self.artists_cache.values_mut() {
                    if let Some(sub) = cached.following.as_mut().filter(|s| s.channel == channel) {
                        sub.subscribed = subscribed;
                    }
                }
            }
            Resp::Collaboration(c) => {
                if let Some(Dialog::Collaborate { id, info, busy, .. }) = &mut self.dialog
                    && *id == c.playlist_id
                {
                    *info = Some(c);
                    *busy = false;
                }
            }
            Resp::PlayingAs { generation, listed, song } => {
                // For the track it was looked up for: joining a jam swaps the queue without a new play.
                if generation == self.generation
                    && self.current_id() == Some(listed.as_str())
                    && let Some(track) = self.current().cloned()
                {
                    let shown = match song {
                        // The song keeps the listed track's album and art when it has none.
                        Some(song) => {
                            let song = Track { album: song.album.or(track.album.clone()), thumbs: if song.thumbs.is_empty() { track.thumbs.clone() } else { song.thumbs }, ..song };
                            self.playing_as = Some(PlayingAs { generation, listed: track.id.clone(), song: song.clone() });
                            song
                        }
                        None => {
                            self.playing_as = None;
                            track
                        }
                    };
                    if self.mix_hold().is_none() {
                        self.show_in_media(&shown);
                        self.refresh_lyrics(shown);
                    }
                    // A jam began while it was being looked up: everyone plays the host's upload.
                    if self.jam.active() {
                        self.play_listed_upload();
                    }
                }
            }
            Resp::PlayError { generation, msg } => self.on_play_error(generation, msg),
            Resp::AudioInfo { generation, info } => {
                // The stream that answered may already have been skipped past.
                if generation == self.generation {
                    self.audio_info = Some((generation, info));
                }
            }
            Resp::ServerQueue(q) => self.adopt_server_queue(q),
            // The profile picture isn't shown anywhere, so it isn't kept.
            Resp::Account { name } => {
                self.account_name = name.or(self.account_name.take());
            }
            Resp::Channels(c) => self.channels = Some(c),
            Resp::SwitchedChannel { page_id, epoch, error } => {
                let name = self.switching_to.take().unwrap_or_else(|| "another channel".to_owned());
                match error {
                    Some(e) => self.toast(e),
                    None => {
                        // Everything reloads as after signing in, but it's the same sign-in
                        // with the same channels: only which one is used changed.
                        let channels = self.channels.take().map(|c| crate::backend::Channels { current: page_id, epoch, ..c });
                        let asked = self.channels_asked;
                        self.on_resp(Resp::LoggedIn(true), now);
                        (self.channels, self.channels_asked) = (channels, asked);
                        self.toast(format!("Switched to {name}"));
                    }
                }
            }
            Resp::LoggedIn(v) => self.on_logged_in(v, now),
            Resp::SpotifyAuth { username, challenge, message } => {
                self.spotify_auth_busy = false;
                let was_signed_in = self.spotify_user.is_some();
                self.spotify_user = username.clone();
                self.spotify_challenge = challenge;
                if let Some(_u) = username {
                    self.spotify_code.clear();
                    self.spotify_password.clear();
                    self.logged_in = false;
                    self.account_name = None;
                    self.channels = None;
                    self.library = None;
                    self.library_artists = None;
                    self.history = None;
                    self.sidebar_cache = None;
                    self.liked.retain(|tid| crate::spotify::is_spotify_id(tid));
                    self.set_spotify_mode(true);
                    if !was_signed_in || self.spotify_library.is_none() {
                        self.backend.request(Req::SpotifyLibrary);
                        self.load_list(crate::spotify::SPOTIFY_LIKED_ID, false, false);
                    }
                    if self.home.is_none() && !self.home_loading {
                        self.home_loading = true;
                        self.backend.request(Req::SpotifyHome);
                    }
                } else if was_signed_in {
                    self.spotify_library = None;
                    self.spotify_artists = None;
                    self.liked.retain(|tid| !crate::spotify::is_spotify_id(tid));
                    self.set_spotify_mode(false);
                }
                if let Some(msg) = message {
                    self.toast(msg);
                }
            }
            Resp::SpotifyLibrary { library, artists, liked } => {
                self.spotify_library = Some(library);
                self.spotify_artists = Some(artists);
                self.liked.retain(|tid| !crate::spotify::is_spotify_id(tid));
                self.liked.extend(liked);
                self.loading = false;
            }
            Resp::Error(e) => self.on_error(e),
        }
    }

    /// Re-rank shelves on Home to elevate personalized mixes and maintain a balanced flow.
    fn reorder_home_shelves(mut shelves: Vec<crate::model::Shelf>, is_mood: bool) -> Vec<crate::model::Shelf> {
        if is_mood || shelves.len() <= 2 {
            return shelves;
        }
        if let Some(pos) = shelves.iter().position(|s| s.title.eq_ignore_ascii_case("Mixed for you")) {
            if pos > 1 {
                let mixed = shelves.remove(pos);
                shelves.insert(1, mixed);
            }
        }
        shelves
    }

    /// The home feed, replacing the one shown.
    fn on_home(&mut self, page: HomePage) {
        let mut prefetch_urls = Vec::new();
        for shelf in page.shelves.iter().take(5) {
            match &shelf.kind {
                crate::model::ShelfKind::Cards(cards) => {
                    for card in cards.iter().take(8) {
                        if let Some(t) = crate::model::pick_thumb(&card.thumbs, 360) {
                            prefetch_urls.push(crate::model::sized(&t.url, 544));
                        }
                    }
                }
                crate::model::ShelfKind::Songs { tracks, .. } => {
                    for track in tracks.iter().take(12) {
                        if let Some(t) = crate::model::pick_thumb(&track.thumbs, 120) {
                            prefetch_urls.push(crate::model::sized(&t.url, 120));
                        }
                    }
                }
            }
        }
        if !prefetch_urls.is_empty() {
            crate::images::prefetch_thumbs(&self.backend, prefetch_urls);
        }

        let mut chips = page.chips;
        // The feed marks the active mood itself; keep the bar when it doesn't send chips.
        if chips.is_empty() {
            chips = self.home.as_ref().map(|h| h.chips.clone()).unwrap_or_default();
        } else if !chips.iter().any(|c| c.selected) {
            if let Some(prev) = &self.home {
                for c in &mut chips {
                    if let Some(p) = prev.chips.iter().find(|pc| pc.title == c.title) {
                        c.selected = p.selected;
                    }
                }
            }
        }
        let is_mood = chips.iter().any(|c| c.selected);
        self.home = Some(HomeState {
            chips,
            shelves: Self::reorder_home_shelves(page.shelves, is_mood),
            continuation: page.continuation,
            loading_more: None,
            picks_loading: false,
            picks_expanded: false,
        });
        self.home_backdrop_key = None;
        self.home_loading = false;
        if self.page == Page::Home {
            self.loading = false;
        }
    }

    /// Liking or unliking many songs at once has finished: `failed` are the ones YouTube
    /// Music refused.
    fn on_liked_many(&mut self, video_ids: Vec<String>, failed: Vec<String>, like: bool) {
        // Any left (a retry took them) leave the list now.
        if !like {
            let rest: Vec<Track> = video_ids.iter().filter_map(|id| self.unliking.remove(id)).collect();
            self.set_liked(&rest, false);
            for id in &failed {
                self.unliking.remove(id);
            }
        }
        log::debug!("like={like} answered: {} taken, {} refused {:?}", video_ids.len(), failed.len(), failed.iter().take(5).collect::<Vec<_>>());
        self.lists.edits.confirm_likes(&video_ids, &failed);
        // The refused ones come back as they were with a reload.
        if !failed.is_empty() {
            self.request_liked(true);
        }
        let count = video_ids.len();
        let mut msg = format!("{} {count} {}", if like { "Liked" } else { "Removed like from" }, if count == 1 { "song" } else { "songs" });
        if count == 0 {
            msg = format!("YouTube Music refused all {} songs; try again in a while", failed.len());
        } else if !failed.is_empty() {
            msg += &format!("; YouTube Music refused {}", failed.len());
        }
        self.toast(msg);
    }

    /// A playlist was edited, added to, removed from, saved or deleted; `message` says which.
    fn on_playlist_changed(&mut self, id: String, message: &'static str) {
        self.optimistic_save = None;
        if crate::spotify::is_spotify_id(&id) {
            if self.spotify_user.is_some() {
                self.backend.request(Req::SpotifyLibrary);
            }
        } else {
            self.request_library();
        }
        if message == "Playlist deleted" {
            // Its load is dropped too, and the queue stops waiting for it.
            self.lists.forget(&id);
            if self.queue.fill.as_ref().is_some_and(|f| f.id == id) {
                self.queue.fill = None;
            }
            let gone = Page::Playlist(id);
            if self.page == gone {
                self.go(Page::Library(LibTab::Playlists));
            }
            // Back and Forward skip it.
            self.back.retain(|p| *p != gone);
            self.back.dedup();
            self.forward.retain(|p| *p != gone);
            self.forward.dedup();
        } else {
            if message == "Removed from playlist" {
                self.lists.edits.confirm_removal(&id);
            }
            // Kept for going back (or playing): reload it. What YouTube Music still
            // lists after a removal is dropped as it arrives.
            if self.lists.known(&id) {
                self.load_list(&id, false, true);
            }
        }
        self.toast(message);
    }

    /// A song couldn't play: skip it, or stop when skipping can't help.
    fn on_play_error(&mut self, generation: u64, msg: String) {
        if generation == self.generation {
            self.buffering = false;
            if self.mix_from.take().is_some() {
                self.pending_mix = None;
                self.audio.send(AudioCmd::Stop);
                self.release_mix();
            }
            let title = self.current().map(|t| t.title.clone()).unwrap_or_default();
            let reason = if msg.contains("age-restricted") || msg.contains("confirm your age") {
                "it's age-restricted".to_owned()
            } else {
                msg
            };
            let has_next = !self.queue.at_end() && self.queue.index.is_some();
            // Skipping can't help when the account is the problem: the rest of
            // an album like this needs it too. Stop on the song instead.
            if let Some(why) = account_block(&reason, self.logged_in) {
                self.skip_streak = 0;
                self.toast(why.replace("{title}", &title));
            } else if has_next && self.skip_streak < MAX_SKIPS {
                // Don't stall the queue on one unplayable song.
                self.skip_streak += 1;
                self.toast(format!("Skipped \"{title}\": {reason}"));
                self.next();
            } else {
                self.skip_streak = 0;
                self.toast(format!("Can't play \"{title}\": {reason}"));
            }
        }
    }

    /// Signed in or out: what belonged to the last account goes, and the new one's home,
    /// library and likes load.
    fn on_logged_in(&mut self, v: bool, now: f64) {
        self.logged_in = v;
        if v {
            self.spotify_user = None;
            self.spotify_challenge = None;
            self.spotify_library = None;
            self.spotify_artists = None;
            self.spotify_mode = false;
        }
        self.channels = None;
        self.channels_asked = None;
        // Recent searches belong to the account.
        self.suggestions = Default::default();
        self.suggest_sel = None;
        self.forgetting = None;
        self.suggest_round += 1;
        self.suggest_cache.clear();
        self.forgets_pending = 0;
        self.suggest_due = Some(now);
        self.account_name = None;
        self.sidebar_cache = None;
        self.library = None;
        self.history = None;
        // Your own playlists show differently (and private ones not at all) to another
        // account, and its likes aren't this one's.
        self.lists.clear();
        self.lists.edits = Default::default();
        self.liked.retain(|tid| crate::spotify::is_spotify_id(tid));
        self.unliking.clear();
        self.queue.fill = None;
        self.play_pending = None;
        if self.page_list().is_some() {
            self.refresh();
        }
        self.home = None;
        self.home_loading = true;
        self.backend.request(Req::Home(None));
        if v {
            self.request_library();
            self.request_liked(false);
            self.backend.request(Req::Account);
        }
        self.toast(if v { "Signed in" } else { "Signed out" });
    }

    /// A request failed: pages waiting for it stop waiting, and the error shows.
    fn on_error(&mut self, e: String) {
        self.spotify_auth_busy = false;
        if let Some(Dialog::Collaborate { info, busy, .. }) = &mut self.dialog {
            *busy = false;
            // Nothing to show if it never loaded.
            if info.is_none() {
                self.dialog = None;
            }
        }
        // A page still waiting for its first data shows a retry instead of loading forever.
        self.load_failed |= self.loading || (self.page == Page::Home && self.home_loading);
        self.loading = false;
        self.home_loading = false;
        self.radio_pending = false;
        // A save that failed didn't happen: take the optimistic entry back out.
        if e.starts_with("Save to library failed")
            && let Some((card, save)) = self.optimistic_save.take()
        {
            self.set_saved(&card, !save);
        }
        // Same for a subscription YouTube Music refused: the button goes back.
        if (e.starts_with("Subscribe failed") || e.starts_with("Unsubscribe failed"))
            && let Some((channel, was)) = self.optimistic_sub.take()
        {
            if let Some(sub) = self.artist.as_mut().and_then(|a| a.following.as_mut()).filter(|s| s.channel == channel) {
                sub.subscribed = was;
            }
            for (cached, _) in self.artists_cache.values_mut() {
                if let Some(sub) = cached.following.as_mut().filter(|s| s.channel == channel) {
                    sub.subscribed = was;
                }
            }
        }
        if let Some(h) = &mut self.home {
            h.loading_more = None;
            h.picks_loading = false;
        }
        self.toast(e);
    }
}

/// Unplayable songs skipped in a row before playback stops.
const MAX_SKIPS: u32 = 3;

/// When a song failed because of the account (Premium-only, or needs signing in),
/// the message to show instead of skipping: the rest of an album like that fails the
/// same way.
fn account_block(msg: &str, logged_in: bool) -> Option<&'static str> {
    let m = msg.to_ascii_lowercase();
    if m.contains("premium") {
        Some("\"{title}\" needs YouTube Music Premium")
    } else if !logged_in && (m.contains("sign in") || m.contains("age-restricted")) {
        Some("Sign in to play \"{title}\"")
    } else {
        None
    }
}

#[cfg(test)]
mod account_block_tests {
    use super::account_block;

    #[test]
    fn account_errors_stop_instead_of_skipping() {
        assert!(account_block("sign in to play this song", false).is_some());
        assert!(account_block("sign in to play this song", true).is_none());
        assert!(account_block("This video is only available to Music Premium members", true).is_some());
        assert!(account_block("decode: end of stream", false).is_none());
    }
}
