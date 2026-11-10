//! The play queue: the tracks being played through, in list or shuffled order, and what comes
//! after the current one.

use tunebox_model::TrackId;

/// The repeat button's three states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    /// The next state when the repeat button is clicked.
    pub fn cycled(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
}

/// Tracks queued for playing, and where playback is among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayQueue {
    tracks: Vec<TrackId>,
    /// Play order: indexes into `tracks`. The identity in list order, a permutation when shuffled.
    order: Vec<usize>,
    /// Position in `order` of the current track.
    cursor: usize,
    shuffle: bool,
    repeat: RepeatMode,
}

impl PlayQueue {
    /// A queue playing `tracks`, starting at `tracks[start]`. When shuffled, the starting track
    /// plays first and the rest follow in random order. Returns `None` for an empty list.
    pub fn new(tracks: Vec<TrackId>, start: usize, shuffle: bool, repeat: RepeatMode, seed: u64) -> Option<Self> {
        if tracks.is_empty() {
            return None;
        }
        let start = start.min(tracks.len() - 1);
        let mut queue = Self { order: (0..tracks.len()).collect(), tracks, cursor: start, shuffle: false, repeat };
        if shuffle {
            queue.set_shuffle(true, seed);
        }
        Some(queue)
    }

    /// The track playing now.
    pub fn current(&self) -> Option<TrackId> {
        self.order.get(self.cursor).and_then(|&i| self.tracks.get(i)).copied()
    }

    pub fn tracks(&self) -> &[TrackId] {
        &self.tracks
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    /// Turns shuffle on (a new random order after the current track) or off (list order, from
    /// the current track).
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        let current = self.order.get(self.cursor).copied().unwrap_or(0);
        if on {
            let mut rest: Vec<usize> = (0..self.tracks.len()).filter(|&i| i != current).collect();
            shuffle_in_place(&mut rest, seed);
            self.order = std::iter::once(current).chain(rest).collect();
            self.cursor = 0;
        } else {
            self.order = (0..self.tracks.len()).collect();
            self.cursor = current;
        }
        self.shuffle = on;
    }

    /// The track to play when the current one finishes by itself. Repeat One plays it again;
    /// Repeat All wraps around at the end; otherwise the queue ends (`None`).
    pub fn advance_after_finish(&mut self) -> Option<TrackId> {
        if self.repeat == RepeatMode::One {
            return self.current();
        }
        self.step_forward()
    }

    /// The Next button: like a finish, but Repeat One does not hold the listener on one track.
    pub fn skip_forward(&mut self) -> Option<TrackId> {
        self.step_forward()
    }

    /// The Previous button: the track before, wrapping round under Repeat All. At the start of
    /// the queue it stays on the first track.
    pub fn skip_back(&mut self) -> Option<TrackId> {
        if self.cursor > 0 {
            self.cursor -= 1;
        } else if self.repeat == RepeatMode::All {
            self.cursor = self.order.len().saturating_sub(1);
        }
        self.current()
    }

    fn step_forward(&mut self) -> Option<TrackId> {
        if self.cursor + 1 < self.order.len() {
            self.cursor += 1;
            self.current()
        } else if self.repeat == RepeatMode::All {
            self.cursor = 0;
            self.current()
        } else {
            None
        }
    }
}

/// Fisher–Yates with SplitMix64: deterministic for a seed, which keeps tests exact.
fn shuffle_in_place(items: &mut [usize], seed: u64) {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: u64) -> Vec<TrackId> {
        (1..=n).map(TrackId).collect()
    }

    #[test]
    fn plays_through_in_order_and_ends() {
        let mut q = PlayQueue::new(ids(3), 1, false, RepeatMode::Off, 0).unwrap();
        assert_eq!(q.current(), Some(TrackId(2)));
        assert_eq!(q.advance_after_finish(), Some(TrackId(3)));
        assert_eq!(q.advance_after_finish(), None);
    }

    #[test]
    fn repeat_all_wraps_and_repeat_one_holds() {
        let mut q = PlayQueue::new(ids(2), 1, false, RepeatMode::All, 0).unwrap();
        assert_eq!(q.advance_after_finish(), Some(TrackId(1)));
        q.set_repeat(RepeatMode::One);
        assert_eq!(q.advance_after_finish(), Some(TrackId(1)));
        assert_eq!(q.skip_forward(), Some(TrackId(2)), "Next leaves a repeating track");
    }

    #[test]
    fn skip_back_stops_at_the_start() {
        let mut q = PlayQueue::new(ids(3), 0, false, RepeatMode::Off, 0).unwrap();
        assert_eq!(q.skip_back(), Some(TrackId(1)));
        q.set_repeat(RepeatMode::All);
        assert_eq!(q.skip_back(), Some(TrackId(3)));
    }

    #[test]
    fn shuffle_starts_with_the_chosen_track_and_visits_every_track_once() {
        let mut q = PlayQueue::new(ids(10), 4, true, RepeatMode::Off, 42).unwrap();
        let mut played = vec![q.current().unwrap()];
        while let Some(next) = q.advance_after_finish() {
            played.push(next);
        }
        assert_eq!(played[0], TrackId(5));
        let mut sorted = played.clone();
        sorted.sort();
        assert_eq!(sorted, ids(10));
        assert_ne!(played, ids(10).into_iter().cycle().skip(4).take(10).collect::<Vec<_>>(), "order is shuffled");
    }

    #[test]
    fn turning_shuffle_off_resumes_list_order_from_the_current_track() {
        let mut q = PlayQueue::new(ids(5), 0, true, RepeatMode::Off, 7).unwrap();
        q.skip_forward();
        let current = q.current().unwrap();
        q.set_shuffle(false, 0);
        assert_eq!(q.current(), Some(current));
        let expected_next = if current.0 < 5 { Some(TrackId(current.0 + 1)) } else { None };
        assert_eq!(q.skip_forward(), expected_next);
    }

    #[test]
    fn empty_queue_is_none() {
        assert!(PlayQueue::new(Vec::new(), 0, false, RepeatMode::Off, 0).is_none());
    }
}
