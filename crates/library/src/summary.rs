//! The status bar line: "1,204 songs, 3.4 days, 8.2 GB".

use std::time::Duration;
use tunebox_model::{Track, format_bytes, format_total_time};

/// Totals for a set of tracks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub count: usize,
    pub duration: Duration,
    pub bytes: u64,
}

impl Summary {
    pub fn of<'a>(tracks: impl IntoIterator<Item = &'a Track>) -> Self {
        tracks.into_iter().fold(Summary::default(), |acc, t| Summary {
            count: acc.count + 1,
            duration: acc.duration.saturating_add(t.file.duration),
            bytes: acc.bytes.saturating_add(t.file.size_bytes),
        })
    }

    /// The status bar text, e.g. "12 songs, 42 minutes, 310.2 MB". Empty for no tracks.
    pub fn label(&self) -> String {
        if self.count == 0 {
            return String::new();
        }
        let noun = if self.count == 1 { "song" } else { "songs" };
        format!("{} {noun}, {}, {}", group_thousands(self.count), format_total_time(self.duration), format_bytes(self.bytes))
    }
}

fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(7), "7");
        assert_eq!(group_thousands(1204), "1,204");
        assert_eq!(group_thousands(1_000_000), "1,000,000");
    }

    #[test]
    fn empty_summary_has_no_label() {
        assert_eq!(Summary::default().label(), "");
    }
}
