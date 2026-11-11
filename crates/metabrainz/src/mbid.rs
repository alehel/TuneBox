//! MusicBrainz identifiers. They are put into URLs, so anything that is not one is refused.

/// Whether `id` is a MusicBrainz id: 36 characters, hexadecimal digits in groups of 8-4-4-4-12.
pub fn is_mbid(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, length)| group.len() == length && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ids_pass() {
        assert!(is_mbid("6a09041b-0f79-3278-88d9-0552dd28d37f"));
        assert!(!is_mbid("6a09041b-0f79-3278-88d9-0552dd28d37"));
        assert!(!is_mbid("6a09041b-0f79-3278-88d9-0552dd28d37g"));
        assert!(!is_mbid("../../etc/passwd"));
        assert!(!is_mbid(""));
    }
}
