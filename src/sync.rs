#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;

use anyhow::{Context, Result, bail};
use regex::regex;

use crate::spotify::{ItemRef, Playlist, PlaylistItem};

const TRACK_URI_PREFIX: &str = "spotify:track:";

pub fn bop_year(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    regex!(r"^([0-9]{4}) bops$")
        .captures(&lower)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

pub fn take_target_playlist(
    mut playlists: Vec<Playlist>,
    target_id: &str,
    user_id: &str,
) -> Result<(Playlist, Vec<Playlist>)> {
    let target_idx = playlists
        .iter()
        .position(|p| p.id == target_id)
        .with_context(|| format!("target playlist {target_id} not found in user playlists"))?;
    let target_playlist = playlists.remove(target_idx);

    if target_playlist.owner.id != user_id {
        bail!(
            "target playlist {} ({}) owned by other user {}",
            target_playlist.name,
            target_playlist.id,
            target_playlist.owner.id
        );
    }

    Ok((target_playlist, playlists))
}

pub fn select_sources(playlists: Vec<Playlist>, user_id: &str) -> Vec<(u16, Playlist)> {
    let mut sources: Vec<_> = playlists
        .into_iter()
        .filter(|p| p.owner.id == user_id)
        .filter_map(|p| bop_year(&p.name).map(|y| (y, p)))
        .collect();

    sources.sort_by_key(|(y, _)| *y);
    sources
}

#[derive(Debug, Default, PartialEq)]
pub struct TrackUris {
    pub uris: Vec<String>,
    pub n_skipped: usize,
}

pub fn track_uris(items: Vec<PlaylistItem>) -> TrackUris {
    let mut tracks = TrackUris::default();

    for PlaylistItem { is_local, item } in items {
        match item {
            Some(ItemRef { kind, uri })
                if !is_local && kind == "track" && uri.starts_with(TRACK_URI_PREFIX) =>
            {
                tracks.uris.push(uri);
            }
            _ => {
                tracks.n_skipped += 1;
            }
        }
    }

    tracks
}
