//! A virtual drive whose disc can be taken out and another put in, as a listener does with the
//! discs of an audiobook. It makes the multi-disc loop testable without hardware.

use crate::drive::{CdDrive, DriveError, DriveIdentity};
use crate::toc::DiscToc;
use crate::virtual_drive::VirtualDrive;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

/// A drive that holds one [`VirtualDrive`] disc at a time, or none.
#[derive(Debug)]
pub struct VirtualChanger {
    name: String,
    disc: Mutex<Option<VirtualDrive>>,
    ejections: AtomicUsize,
}

impl VirtualChanger {
    /// An empty drive.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), disc: Mutex::new(None), ejections: AtomicUsize::new(0) }
    }

    /// Puts `disc` in, replacing any disc there.
    pub fn insert(&self, disc: VirtualDrive) {
        *self.slot() = Some(disc);
    }

    /// Whether a disc is in.
    pub fn has_disc(&self) -> bool {
        self.slot().is_some()
    }

    /// How many times the tray was opened.
    pub fn ejections(&self) -> usize {
        self.ejections.load(Ordering::Relaxed)
    }

    fn current(&self) -> Result<VirtualDrive, DriveError> {
        self.slot().clone().ok_or(DriveError::NoDisc)
    }

    /// The slot, even if a reader panicked while holding it: the disc in it is still whole.
    fn slot(&self) -> MutexGuard<'_, Option<VirtualDrive>> {
        self.disc.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CdDrive for VirtualChanger {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn identity(&self) -> DriveIdentity {
        DriveIdentity::Model(self.name.clone())
    }

    fn read_toc(&self) -> Result<DiscToc, DriveError> {
        self.current()?.read_toc()
    }

    fn read_sectors(
        &self,
        toc: &DiscToc,
        first: u32,
        count: u32,
        sink: &mut dyn FnMut(&[u8], f32) -> bool,
    ) -> Result<(), DriveError> {
        // A copy of the disc, so the slot is not held while reading: ejecting stays possible.
        self.current()?.read_sectors(toc, first, count, sink)
    }

    fn eject(&self) -> Result<(), DriveError> {
        *self.slot() = None;
        self.ejections.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discs_go_in_and_come_out() {
        let drive = VirtualChanger::new("Changer");
        assert_eq!(drive.read_toc(), Err(DriveError::NoDisc));
        drive.insert(VirtualDrive::new("one", vec![1, 2]));
        let first = drive.read_toc().unwrap();
        drive.eject().unwrap();
        assert!(!drive.has_disc());
        assert_eq!(drive.ejections(), 1);
        drive.insert(VirtualDrive::new("two", vec![3]));
        assert_ne!(drive.read_toc().unwrap(), first);
        // Reading with the old disc's contents is refused, as a real drive would.
        let mut sink = |_: &[u8], _: f32| true;
        assert_eq!(drive.read_sectors(&first, 150, 1, &mut sink), Err(DriveError::DiscChanged));
    }
}
