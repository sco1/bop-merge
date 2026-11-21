#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use regex::regex;

use crate::spotify::{ItemRef, Playlist, PlaylistItem};

const TRACK_URI_PREFIX: &str = "spotify:track:";
/// Removing more than 1 / `MAX_REMOVE_DIVISOR` of the target's tracks needs `--force`.
const MAX_REMOVE_DIVISOR: usize = 4;

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

#[derive(Debug, Default, PartialEq)]
pub struct SyncPlan {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub n_sources: usize,
    pub n_source_tracks: usize,
    pub n_target_tracks: usize,
}

pub fn plan(sources: &[TrackUris], target: &TrackUris) -> SyncPlan {
    let existing_uris: HashSet<&str> = target.uris.iter().map(String::as_str).collect();
    let source_uris: HashSet<&str> = sources
        .iter()
        .flat_map(|s| &s.uris)
        .map(String::as_str)
        .collect();

    let mut seen = HashSet::new();
    let add = sources
        .iter()
        .flat_map(|s| &s.uris)
        .filter(|uri| !existing_uris.contains(uri.as_str()) && seen.insert(uri.as_str()))
        .cloned()
        .collect();

    let mut seen = HashSet::new();
    let remove = target
        .uris
        .iter()
        .filter(|uri| !source_uris.contains(uri.as_str()) && seen.insert(uri.as_str()))
        .cloned()
        .collect();

    SyncPlan {
        add,
        remove,
        n_sources: sources.len(),
        n_source_tracks: sources.iter().map(|s| s.uris.len()).sum(),
        n_target_tracks: target.uris.len(),
    }
}

pub fn check_plan(plan: &SyncPlan, force: bool) -> Result<()> {
    if force {
        return Ok(());
    }

    if plan.n_sources == 0 {
        bail!("no source playlists found; refusing to empty target playlist")
    }
    if plan.n_source_tracks == 0 {
        bail!("source playlist(s) contain no tracks; refusing to empty target playlist")
    }
    if plan.remove.len() * MAX_REMOVE_DIVISOR > plan.n_target_tracks {
        bail!(
            "would remove {} of {} target tracks; exceeds threshold",
            plan.remove.len(),
            plan.n_target_tracks
        )
    }

    Ok(())
}
