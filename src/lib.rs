//! The `bop-merge` binary is a thin CLI over this library:
//!
//! - [`auth`]: OAuth 2.0 PKCE login, token refresh, and refresh-token storage in the OS
//!   credential store.
//! - [`spotify`]: a minimal blocking client for the Spotify Web API.
//! - [`sync`]: Logic for selecting source playlists and planning which tracks to add or remove from
//!   the target playlist.
//! - [`app`]: orchestration of the `login` and `sync` commands.

pub mod app;
pub mod auth;
pub mod spotify;
pub mod sync;

#[cfg(test)]
mod test_support;
