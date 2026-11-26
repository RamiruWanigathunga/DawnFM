//! First-class Spotify catalog, library, radio, lyrics, and Login5 integration.

pub mod auth;
pub mod client;
pub mod proto;

pub use auth::{EmailCodeChallenge, EmailLoginStep};
pub use client::{SPOTIFY_LIKED_ID, SpotifyClient};
pub use proto::{canonical_uri, uri_id};

pub fn is_spotify_id(id: &str) -> bool {
    id.starts_with("spotify:")
}
