use regex::regex;

use crate::spotify::Playlist;

pub fn bop_year(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    regex!(r"^([0-9]{4}) bops$")
        .captures(&lower)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
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

#[cfg(test)]
mod tests {
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
}
