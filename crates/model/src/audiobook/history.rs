//! Undo and redo, as a history of whole states.
//!
//! The book's edits are small (start points, names, removals, trims), so keeping whole copies
//! is simpler and safer than recording inverse operations: undo can never drift out of step.

/// Past and future states of something edited.
#[derive(Clone, Debug)]
pub struct History<T> {
    past: Vec<T>,
    future: Vec<T>,
    limit: usize,
}

impl<T: Clone + PartialEq> History<T> {
    /// Keeps at most `limit` states to undo.
    pub fn new(limit: usize) -> Self {
        Self { past: Vec::new(), future: Vec::new(), limit: limit.max(1) }
    }

    /// Records `before`, the state an edit is about to change. A new edit clears what could be
    /// redone. Recording the same state twice in a row is ignored.
    pub fn record(&mut self, before: T) {
        if self.past.last() == Some(&before) {
            return;
        }
        self.past.push(before);
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        self.future.clear();
    }

    /// The state before the last edit, given the `current` one (kept for redo).
    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.past.pop()?;
        self.future.push(current);
        Some(previous)
    }

    /// The state undone last, given the `current` one (kept for undo).
    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.future.pop()?;
        self.past.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Forgets everything, as after the edited thing is replaced.
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_walk_the_states() {
        let mut history = History::new(10);
        let mut state = 1;
        for next in [2, 3] {
            history.record(state);
            state = next;
        }
        state = history.undo(state).unwrap();
        assert_eq!(state, 2);
        state = history.undo(state).unwrap();
        assert_eq!(state, 1);
        assert!(history.undo(state).is_none());
        state = history.redo(state).unwrap();
        assert_eq!(state, 2);
        // A new edit drops what could be redone.
        history.record(state);
        assert!(!history.can_redo());
    }

    #[test]
    fn the_oldest_states_are_dropped_past_the_limit() {
        let mut history = History::new(2);
        for state in 1..=4 {
            history.record(state);
        }
        assert_eq!(history.undo(5), Some(4));
        assert_eq!(history.undo(4), Some(3));
        assert_eq!(history.undo(3), None);
    }
}
