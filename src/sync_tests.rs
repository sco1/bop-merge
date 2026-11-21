use super::*;
use crate::spotify::PlaylistOwner;

const ME: &str = "me";

fn playlist(id: &str, name: &str, owner: &str) -> Playlist {
    Playlist {
        id: id.to_owned(),
        name: name.to_owned(),
        owner: PlaylistOwner {
            id: owner.to_owned(),
        },
    }
}

/// Reduce `select_sources` output to comparable `(year, id)` pairs.
fn year_ids(sources: &[(u16, Playlist)]) -> Vec<(u16, &str)> {
    sources.iter().map(|(y, p)| (*y, p.id.as_str())).collect()
}

#[test]
fn bop_year_matches_exact_name() {
    assert_eq!(bop_year("2024 Bops"), Some(2024));
}

#[test]
fn bop_year_ignores_ascii_case() {
    assert_eq!(bop_year("2024 bops"), Some(2024));
    assert_eq!(bop_year("2024 BOPS"), Some(2024));
    assert_eq!(bop_year("2024 bOpS"), Some(2024));
}

#[test]
fn bop_year_requires_single_space() {
    assert_eq!(bop_year("2024Bops"), None);
    assert_eq!(bop_year("2024  Bops"), None);
    assert_eq!(bop_year("2024\tBops"), None);
}

#[test]
fn bop_year_is_anchored() {
    assert_eq!(bop_year("My 2024 Bops"), None);
    assert_eq!(bop_year("2024 Bops "), None);
    assert_eq!(bop_year(" 2024 Bops"), None);
    assert_eq!(bop_year("2024 Bops 2"), None);
}

#[test]
fn bop_year_requires_exactly_four_digits() {
    assert_eq!(bop_year("24 Bops"), None);
    assert_eq!(bop_year("20245 Bops"), None);
    assert_eq!(bop_year("ABCD Bops"), None);
}

#[test]
fn bop_year_rejects_missing_parts() {
    assert_eq!(bop_year(""), None);
    assert_eq!(bop_year("Bops"), None);
    assert_eq!(bop_year("2024"), None);
}

#[test]
fn bop_year_rejects_non_ascii_digits() {
    // Arabic-Indic digits. Not observable as a `\d` regression: `parse::<u16>()`
    // rejects these too, so the result is `None` either way.
    assert_eq!(bop_year("\u{0662}\u{0660}\u{0662}\u{0664} Bops"), None);
}

#[test]
fn bop_year_rejects_unicode_case_folding() {
    // U+017F LATIN SMALL LETTER LONG S case-folds to `s` under Unicode rules;
    // ASCII-only lowercasing must not treat it as `s`.
    assert_eq!(bop_year("2024 Bop\u{017F}"), None);
}

#[test]
fn select_keeps_owned_bops() {
    let sources = select_sources(vec![playlist("a", "2024 Bops", ME)], ME);
    assert_eq!(year_ids(&sources), [(2024, "a")]);
}

#[test]
fn select_drops_other_owners() {
    let sources = select_sources(vec![playlist("a", "2024 Bops", "someone-else")], ME);
    assert!(sources.is_empty());
}

#[test]
fn select_drops_other_names() {
    let sources = select_sources(vec![playlist("a", "Road Trip", ME)], ME);
    assert!(sources.is_empty());
}

#[test]
fn select_sorts_by_year() {
    let sources = select_sources(
        vec![
            playlist("c", "2026 Bops", ME),
            playlist("a", "2023 Bops", ME),
            playlist("b", "2025 Bops", ME),
        ],
        ME,
    );
    assert_eq!(year_ids(&sources), [(2023, "a"), (2025, "b"), (2026, "c")]);
}

#[test]
fn select_keeps_duplicate_years_in_input_order() {
    let sources = select_sources(
        vec![
            playlist("a", "2024 Bops", ME),
            playlist("b", "2024 bops", ME),
        ],
        ME,
    );
    assert_eq!(year_ids(&sources), [(2024, "a"), (2024, "b")]);
}

#[test]
fn select_filters_a_mixed_library() {
    let sources = select_sources(
        vec![
            playlist("x", "Road Trip", ME),
            playlist("b", "2024 Bops", ME),
            playlist("y", "2023 Bops", "someone-else"),
            playlist("a", "2023 BOPS", ME),
            playlist("z", "My 2025 Bops", ME),
        ],
        ME,
    );
    assert_eq!(year_ids(&sources), [(2023, "a"), (2024, "b")]);
}

#[test]
fn select_empty_input() {
    assert!(select_sources(Vec::new(), ME).is_empty());
}

fn item(kind: &str, uri: &str, is_local: bool) -> PlaylistItem {
    PlaylistItem {
        is_local,
        item: Some(ItemRef {
            kind: kind.to_owned(),
            uri: uri.to_owned(),
        }),
    }
}

fn track(id: &str) -> PlaylistItem {
    item("track", &format!("{TRACK_URI_PREFIX}{id}"), false)
}

fn expected(uris: &[&str], n_skipped: usize) -> TrackUris {
    TrackUris {
        uris: uris.iter().map(|u| (*u).to_owned()).collect(),
        n_skipped,
    }
}

#[test]
fn track_uris_empty_input() {
    assert_eq!(track_uris(Vec::new()), TrackUris::default());
}

#[test]
fn track_uris_keeps_track() {
    assert_eq!(
        track_uris(vec![track("a")]),
        expected(&["spotify:track:a"], 0)
    );
}

#[test]
fn track_uris_skips_local_file() {
    let local = item("track", "spotify:local:Artist:Album:Song:180", true);
    assert_eq!(track_uris(vec![local]), expected(&[], 1));
}

#[test]
fn track_uris_skips_local_flag_even_with_track_uri() {
    // `is_local` alone is enough to skip, whatever the URI looks like.
    let local = item("track", "spotify:track:a", true);
    assert_eq!(track_uris(vec![local]), expected(&[], 1));
}

#[test]
fn track_uris_skips_missing_item() {
    let missing = PlaylistItem {
        is_local: false,
        item: None,
    };
    assert_eq!(track_uris(vec![missing]), expected(&[], 1));
}

#[test]
fn track_uris_skips_episode() {
    let episode = item("episode", "spotify:episode:e", false);
    assert_eq!(track_uris(vec![episode]), expected(&[], 1));
}

#[test]
fn track_uris_skips_kind_uri_mismatch() {
    let items = vec![
        item("track", "spotify:episode:e", false),
        item("episode", "spotify:track:a", false),
    ];
    assert_eq!(track_uris(items), expected(&[], 2));
}

#[test]
fn track_uris_keeps_duplicates() {
    // Dedupe is plan()'s job, not this function's.
    assert_eq!(
        track_uris(vec![track("a"), track("a")]),
        expected(&["spotify:track:a", "spotify:track:a"], 0)
    );
}

#[test]
fn track_uris_filters_a_mixed_playlist_in_order() {
    let items = vec![
        track("a"),
        item("track", "spotify:local:Artist:Album:Song:180", true),
        track("b"),
        item("episode", "spotify:episode:e", false),
        PlaylistItem {
            is_local: false,
            item: None,
        },
        track("c"),
    ];
    let n_items = items.len();

    let result = track_uris(items);

    assert_eq!(
        result,
        expected(
            &["spotify:track:a", "spotify:track:b", "spotify:track:c"],
            3
        )
    );
    assert_eq!(result.uris.len() + result.n_skipped, n_items);
}

// --- take_target_playlist ---

/// Reduce playlists to their ids, keeping order.
fn ids(playlists: &[Playlist]) -> Vec<&str> {
    playlists.iter().map(|p| p.id.as_str()).collect()
}

#[test]
fn take_target_removes_owned_target_and_keeps_order() {
    let playlists = vec![
        playlist("a", "2023 Bops", ME),
        playlist("t", "All Bops", ME),
        playlist("b", "2024 Bops", ME),
        playlist("c", "Road Trip", ME),
    ];

    let (target, rest) = take_target_playlist(playlists, "t", ME).unwrap();

    assert_eq!(target.id, "t");
    // `remove`, not `swap_remove`: the rest stays in API order.
    assert_eq!(ids(&rest), ["a", "b", "c"]);
}

#[test]
fn take_target_with_bops_name_is_not_a_source() {
    let playlists = vec![
        playlist("a", "2023 Bops", ME),
        playlist("t", "2027 Bops", ME),
    ];

    let (target, rest) = take_target_playlist(playlists, "t", ME).unwrap();

    assert_eq!(target.id, "t");
    assert_eq!(year_ids(&select_sources(rest, ME)), [(2023, "a")]);
}

#[test]
fn take_target_missing_id() {
    let playlists = vec![playlist("a", "2023 Bops", ME)];

    let err = take_target_playlist(playlists, "nope", ME).unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("nope"), "{msg}");
}

#[test]
fn take_target_not_owned() {
    let playlists = vec![playlist("t", "Their Bops", "someone-else")];

    let err = take_target_playlist(playlists, "t", ME).unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("owned by"), "{msg}");
    assert!(msg.contains("someone-else"), "{msg}");
}

// --- plan ---

fn uri(id: &str) -> String {
    format!("{TRACK_URI_PREFIX}{id}")
}

fn tracks(ids: &[&str]) -> TrackUris {
    TrackUris {
        uris: ids.iter().map(|id| uri(id)).collect(),
        n_skipped: 0,
    }
}

fn uri_list(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| uri(id)).collect()
}

/// Two sources with a duplicate inside one and across both.
fn two_sources() -> [TrackUris; 2] {
    [tracks(&["a", "b", "a"]), tracks(&["c", "b", "d"])]
}

#[test]
fn plan_first_run_adds_each_source_track_once_in_order() {
    assert_eq!(
        plan(&two_sources(), &tracks(&[])),
        SyncPlan {
            add: uri_list(&["a", "b", "c", "d"]),
            remove: Vec::new(),
            n_sources: 2,
            n_source_tracks: 6,
            n_target_tracks: 0,
        }
    );
}

#[test]
fn plan_in_sync_target_changes_nothing() {
    // Idempotency: a target that already mirrors the sources, in any order.
    let result = plan(&two_sources(), &tracks(&["d", "c", "b", "a"]));

    assert!(result.add.is_empty(), "{result:?}");
    assert!(result.remove.is_empty(), "{result:?}");
}

#[test]
fn plan_removes_target_extras_once_in_target_order() {
    assert_eq!(
        plan(&two_sources(), &tracks(&["a", "x", "b", "x", "y"])),
        SyncPlan {
            add: uri_list(&["c", "d"]),
            remove: uri_list(&["x", "y"]),
            n_sources: 2,
            n_source_tracks: 6,
            n_target_tracks: 5,
        }
    );
}

#[test]
fn plan_without_sources_removes_whole_target() {
    // check_plan must refuse this; plan itself just reports it.
    assert_eq!(
        plan(&[], &tracks(&["a"])),
        SyncPlan {
            add: Vec::new(),
            remove: uri_list(&["a"]),
            n_sources: 0,
            n_source_tracks: 0,
            n_target_tracks: 1,
        }
    );
}

// --- check_plan ---

fn sync_plan(
    n_sources: usize,
    n_source_tracks: usize,
    n_remove: usize,
    n_target_tracks: usize,
) -> SyncPlan {
    SyncPlan {
        remove: (0..n_remove).map(|i| uri(&i.to_string())).collect(),
        n_sources,
        n_source_tracks,
        n_target_tracks,
        ..Default::default()
    }
}

fn check_err(plan: &SyncPlan) -> String {
    format!("{:#}", check_plan(plan, false).unwrap_err())
}

#[test]
fn check_allows_healthy_plan() {
    assert!(check_plan(&sync_plan(4, 340, 0, 331), false).is_ok());
}

#[test]
fn check_allows_first_run_with_empty_target() {
    assert!(check_plan(&sync_plan(4, 340, 0, 0), false).is_ok());
}

#[test]
fn check_allows_exactly_a_quarter_removed() {
    assert!(check_plan(&sync_plan(4, 340, 1, 4), false).is_ok());
}

#[test]
fn check_refuses_no_sources() {
    let msg = check_err(&sync_plan(0, 0, 331, 331));
    assert!(msg.contains("no source"), "{msg}");
}

#[test]
fn check_refuses_sources_without_tracks() {
    let msg = check_err(&sync_plan(4, 0, 331, 331));
    assert!(msg.contains("no tracks"), "{msg}");
}

#[test]
fn check_refuses_more_than_a_quarter_removed() {
    let msg = check_err(&sync_plan(4, 340, 2, 7));
    assert!(msg.contains("2 of 7"), "{msg}");
}

#[test]
fn check_force_overrides_every_guard() {
    for failing in [
        sync_plan(0, 0, 331, 331),
        sync_plan(4, 0, 331, 331),
        sync_plan(4, 340, 2, 7),
    ] {
        // Fails without force (check_err panics on Ok), passes with it.
        check_err(&failing);
        assert!(check_plan(&failing, true).is_ok(), "{failing:?}");
    }
}
