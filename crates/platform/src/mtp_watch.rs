//! Finding MTP players without stalling the interface. Opening a device takes a moment (and a
//! device that stopped answering takes longer), so a thread of its own lists the USB devices
//! every few seconds and opens only the ones that are new, or that could not be used last time
//! (busy in another program, a locked phone): a player that works is opened once per
//! connection, and never while TuneBox itself is syncing it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tunebox_engine::{Candidate, MtpLink, mtp_links, probe_mtp_link};

/// How often the USB devices are looked at again.
const EVERY: Duration = Duration::from_secs(3);

/// The MTP players found by the last look, kept up to date on a thread of its own.
pub struct MtpPlayers {
    latest: Arc<Mutex<Option<Vec<Candidate>>>>,
    /// What [`MtpPlayers::changed`] last handed out.
    reported: Option<Vec<Candidate>>,
}

/// One look: devices already known and usable keep what was found; new ones and ones with a
/// problem are opened again; gone ones are forgotten.
fn look(known: &mut HashMap<String, Vec<Candidate>>, links: &[MtpLink], open: impl Fn(&MtpLink) -> Vec<Candidate>) {
    known.retain(|key, _| links.iter().any(|link| link.key() == *key));
    for link in links {
        let usable = known.get(&link.key()).is_some_and(|found| found.iter().all(|c| c.problem.is_none()));
        if !usable {
            known.insert(link.key(), open(link));
        }
    }
}

impl MtpPlayers {
    pub fn start() -> Self {
        let latest = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&latest);
        let spawned = std::thread::Builder::new().name("tunebox-mtp".into()).spawn(move || {
            let mut known: HashMap<String, Vec<Candidate>> = HashMap::new();
            // Ends when the last handle is dropped.
            while Arc::strong_count(&shared) > 1 {
                let links = mtp_links();
                look(&mut known, &links, probe_mtp_link);
                let found: Vec<Candidate> =
                    links.iter().filter_map(|link| known.get(&link.key())).flatten().cloned().collect();
                *shared.lock().unwrap_or_else(PoisonError::into_inner) = Some(found);
                std::thread::sleep(EVERY);
            }
        });
        if let Err(e) = spawned {
            log::warn!("cannot look for MTP players: {e}");
        }
        MtpPlayers { latest, reported: None }
    }

    /// The MTP players, when they differ from what was last handed out.
    pub fn changed(&mut self) -> Option<Vec<Candidate>> {
        let latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner).clone()?;
        if self.reported.as_ref() == Some(&latest) {
            return None;
        }
        self.reported = Some(latest.clone());
        Some(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tunebox_mtp::simulator::SimulatedDevice;

    #[test]
    fn usable_players_are_opened_once_and_busy_ones_again() {
        let player = SimulatedDevice::new("Player", 1_000);
        let links = [MtpLink::Simulated(player.clone())];
        let opened = Cell::new(0);
        let open = |link: &MtpLink| {
            opened.set(opened.get() + 1);
            probe_mtp_link(link)
        };
        let mut known = HashMap::new();

        player.hold(true);
        look(&mut known, &links, open);
        look(&mut known, &links, open);
        assert_eq!(opened.get(), 2, "a busy player is tried again");
        assert!(known["simulated"][0].problem.is_some());

        player.hold(false);
        look(&mut known, &links, open);
        look(&mut known, &links, open);
        assert_eq!(opened.get(), 3, "a usable one is opened once");
        assert!(known["simulated"][0].problem.is_none());

        look(&mut known, &[], open);
        assert!(known.is_empty(), "an unplugged one is forgotten");
    }
}
