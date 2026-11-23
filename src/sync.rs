//! Sync logic: selecting source playlists, extracting track URIs, and planning the changes that
//! make the target playlist mirror its sources.

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use regex::regex;

use crate::spotify::{ItemRef, Playlist, PlaylistItem};

/// URI prefix of Spotify catalog tracks.
const TRACK_URI_PREFIX: &str = "spotify:track:";
/// Removing more than 1 / `MAX_REMOVE_DIVISOR` of the target's tracks needs `--force`.
const MAX_REMOVE_DIVISOR: usize = 4;

/// Return the year from a playlist name of the form `YYYY Bops`, or `None` if the name does not
/// match.
///
/// Matching ignores case but is otherwise exact: the whole name must be four digits, one space,
/// and `bops`, with no surrounding whitespace.
pub fn bop_year(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    regex!(r"^([0-9]{4}) bops$")
        .captures(&lower)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

/// Separate the configured target playlist from `playlists` and returns it along with the remaining
/// playlists.
///
/// # Errors
///
/// Returns an error if no playlist has the ID `target_id`, or if the target is not owned by
/// `user_id`.
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

/// Select the source playlist(s): those owned by `user_id` whose names match [`bop_year`].
///
/// Returns `(year, playlist)` pairs sorted by year, oldest first. Playlists with the same year keep
/// their original relative order.
///
/// NOTE: It is assumed that the configured target playlist is not provided as a source.
pub fn select_sources(playlists: Vec<Playlist>, user_id: &str) -> Vec<(u16, Playlist)> {
    let mut sources: Vec<_> = playlists
        .into_iter()
        .filter(|p| p.owner.id == user_id)
        .filter_map(|p| bop_year(&p.name).map(|y| (y, p)))
        .collect();

    sources.sort_by_key(|(y, _)| *y);
    sources
}

/// The track URIs extracted from one playlist by [`track_uris`].
#[derive(Debug, Default, PartialEq)]
pub struct TrackUris {
    /// URIs of the syncable tracks, in playlist order, including any duplicates.
    pub uris: Vec<String>,
    /// Number of items skipped: local files, podcast episodes, and unavailable items.
    pub n_skipped: usize,
}

/// Extract the URIs of Spotify catalog tracks from a playlist's `items`.
///
/// An item is kept only if it is not a local file, has type `track`, and has a `spotify:track:`
/// URI. Every other item is counted in [`TrackUris::n_skipped`].
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

/// The changes that make the target playlist mirror its sources, as computed by [`plan`].
#[derive(Debug, Default, PartialEq)]
pub struct SyncPlan {
    /// URIs to append to the target, in source order, without duplicates.
    pub add: Vec<String>,
    /// URIs to remove from the target (those found in no source), without duplicates.
    pub remove: Vec<String>,
    /// Number of source playlists.
    pub n_sources: usize,
    /// Total number of tracks across all sources, counting duplicates.
    pub n_source_tracks: usize,
    /// Number of tracks currently in the target, counting duplicates.
    pub n_target_tracks: usize,
}

/// Compute the changes needed for `target` to contain exactly the tracks in `sources`.
///
/// The target is compared as a set. Tracks already in it are left where they are, duplicates
/// included. Missing tracks are added in source order, and tracks found in no source are
/// removed.
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

/// Check `plan` against safety guards that stop a sync from emptying or gutting the target playlist
/// by accident. Always succeeds when `force` is `true`.
///
/// # Errors
///
/// Unless `force` is `true`, returns an error if:
///
/// - there are no source playlists;
/// - the source playlists contain no tracks; or
/// - the plan removes more than a quarter of the target's tracks.
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
