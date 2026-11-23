//! Response types deserialized from the Spotify Web API.
//!
//! Only the fields `bop-merge` needs are modeled; serde ignores the rest.

use serde::Deserialize;

/// A Spotify user profile, as returned by `GET /me`.
#[derive(Debug, Deserialize)]
pub struct User {
    /// The Spotify user ID.
    pub id: String,
    /// The user's display name, if one is set.
    pub display_name: Option<String>,
}

/// The owner of a [`Playlist`].
#[derive(Debug, Deserialize)]
pub struct PlaylistOwner {
    /// The owner's Spotify user ID.
    pub id: String,
}

/// A playlist summary, as returned by `GET /me/playlists`.
#[derive(Debug, Deserialize)]
pub struct Playlist {
    /// The Spotify playlist ID.
    pub id: String,
    /// The playlist's name.
    pub name: String,
    /// The user who owns the playlist.
    pub owner: PlaylistOwner,
}

/// The track or episode referenced by a [`PlaylistItem`].
#[derive(Debug, Deserialize)]
pub struct ItemRef {
    /// The object type, such as `track` or `episode` (the JSON `type` field).
    #[serde(rename = "type")]
    pub kind: String,
    /// The Spotify URI, such as `spotify:track:<id>`.
    pub uri: String,
}

/// One entry in a playlist.
#[derive(Debug, Deserialize)]
pub struct PlaylistItem {
    /// Whether the entry is a local file rather than a Spotify catalog item. Defaults to `false`
    /// when the field is absent.
    #[serde(default)]
    pub is_local: bool,
    /// The referenced track or episode, or `None` when Spotify returns no object, for example
    /// content that is no longer available.
    pub item: Option<ItemRef>,
}

/// One page of a paginated Web API response.
#[derive(Deserialize)]
pub(super) struct Page<T> {
    /// The items on this page.
    pub(super) items: Vec<T>,
    /// The URL of the next page, or `None` on the last page.
    pub(super) next: Option<String>,
}
