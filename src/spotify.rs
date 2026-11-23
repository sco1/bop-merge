//! A minimal blocking client for the parts of the Spotify Web API that `bop-merge` uses, plus
//! the response models it deserializes.

mod client;
mod models;

pub use crate::spotify::{
    client::SpotifyClient,
    models::{ItemRef, Playlist, PlaylistItem, PlaylistOwner, User},
};
