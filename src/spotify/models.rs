use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistOwner {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner: PlaylistOwner,
}

#[derive(Debug, Deserialize)]
pub struct ItemRef {
    #[serde(rename = "type")]
    pub kind: String,
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistItem {
    #[serde(default)]
    pub is_local: bool,
    pub item: Option<ItemRef>,
}

#[derive(Deserialize)]
pub(super) struct Page<T> {
    pub(super) items: Vec<T>,
    pub(super) next: Option<String>,
}
