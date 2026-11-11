//! Background jobs: slow work on worker threads, reporting progress and handing results back to
//! the session.
//!
//! A job is a closure that receives a [`JobContext`] (progress reporting and a cancel flag) and
//! returns an outcome of the table's type `T` (the session's `Outcome` enum). A panic inside a job
//! is caught and becomes an error, so a bad file can never take the app down with it.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

/// Identifies a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(pub u64);

/// What kind of work a job does (one of each may run at a time where that matters).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobKind {
    Import,
    ReadDisc,
    LookupNames,
    Rip,
    Sync,
    Refresh,
    Locate,
    Artwork,
    Rename,
    /// Writing an MTP player's manifest after its settings changed.
    SaveDevice,
    /// Fetching a cover from the Cover Art Archive (for the CD, or a chosen release).
    FetchCover,
    /// Searching MusicBrainz for an album's releases.
    FindArtwork,
    /// Fetching the picker's thumbnails, one at a time.
    Thumbnail,
    VisualizerPlugins,
    /// Ripping a disc of the audiobook in progress into its staging folder.
    RipBookDisc,
    /// Building the audiobook into the library.
    BuildBook,
}

impl JobKind {
    pub fn noun(self) -> &'static str {
        match self {
            JobKind::Import => "An import",
            JobKind::ReadDisc => "Reading the disc",
            JobKind::LookupNames => "Looking up track names",
            JobKind::Rip => "Importing the CD",
            JobKind::Sync => "A sync",
            JobKind::Refresh => "Refreshing from files",
            JobKind::Locate => "Looking for missing songs",
            JobKind::Artwork => "Changing covers",
            JobKind::SaveDevice => "Saving a player's settings",
            JobKind::Rename => "Renaming files",
            JobKind::FetchCover => "Getting a cover",
            JobKind::FindArtwork => "Looking for album artwork",
            JobKind::Thumbnail => "Getting cover thumbnails",
            JobKind::VisualizerPlugins => "Loading visualizer plug-ins",
            JobKind::RipBookDisc => "Ripping an audiobook disc",
            JobKind::BuildBook => "Building the audiobook",
        }
    }
}

/// A running job, as the interface shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct JobStatus {
    pub id: JobId,
    pub kind: JobKind,
    /// "Importing “Coastline”", "Syncing “Pocket Player”".
    pub title: String,
    /// The current step, e.g. the file being copied.
    pub message: String,
    /// 0–1, when known.
    pub progress: Option<f32>,
}

/// Given to a job's closure.
pub struct JobContext {
    id: JobId,
    updates: Sender<Progress>,
    cancel: Arc<AtomicBool>,
}

impl JobContext {
    /// Reports progress (0–1) and the current step.
    pub fn progress(&self, fraction: f32, message: impl Into<String>) {
        let _ =
            self.updates.send(Progress { id: self.id, fraction: fraction.clamp(0.0, 1.0), message: message.into() });
    }

    /// The flag the job should check between steps.
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }
}

struct Progress {
    id: JobId,
    fraction: f32,
    message: String,
}

struct Done<T> {
    id: JobId,
    outcome: Result<T, String>,
}

struct Running {
    status: JobStatus,
    cancel: Arc<AtomicBool>,
}

/// A job table whose jobs all produce a `T`.
pub(crate) struct Jobs<T> {
    next_id: u64,
    running: Vec<Running>,
    progress_tx: Sender<Progress>,
    progress_rx: Receiver<Progress>,
    done_tx: Sender<Done<T>>,
    done_rx: Receiver<Done<T>>,
}

/// A finished job's result. `Err` holds the message of a job that panicked.
pub(crate) struct Finished<T> {
    pub kind: JobKind,
    pub outcome: Result<T, String>,
}

impl<T: Send + 'static> Jobs<T> {
    pub fn new() -> Self {
        let (progress_tx, progress_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        Self { next_id: 1, running: Vec::new(), progress_tx, progress_rx, done_tx, done_rx }
    }

    pub fn statuses(&self) -> Vec<JobStatus> {
        self.running.iter().map(|r| r.status.clone()).collect()
    }

    pub fn is_running(&self, kind: JobKind) -> bool {
        self.running.iter().any(|r| r.status.kind == kind)
    }

    pub fn cancel(&self, id: JobId) {
        if let Some(job) = self.running.iter().find(|r| r.status.id == id) {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel_all(&self) {
        self.running.iter().for_each(|r| r.cancel.store(true, Ordering::Relaxed));
    }

    /// Starts `work` on a worker thread.
    pub fn spawn<F>(&mut self, kind: JobKind, title: String, work: F) -> JobId
    where
        F: FnOnce(&JobContext) -> T + Send + 'static,
    {
        let id = JobId(self.next_id);
        self.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let context = JobContext { id, updates: self.progress_tx.clone(), cancel: Arc::clone(&cancel) };
        let done = self.done_tx.clone();
        let spawned = std::thread::Builder::new().name(format!("tunebox-job-{}", id.0)).spawn(move || {
            let outcome =
                catch_unwind(AssertUnwindSafe(|| work(&context))).map_err(|panic| panic_message(panic.as_ref()));
            let _ = done.send(Done { id, outcome });
        });
        if let Err(e) = spawned {
            let _ = self.done_tx.send(Done { id, outcome: Err(format!("could not start a worker thread: {e}")) });
        }
        self.running
            .push(Running { status: JobStatus { id, kind, title, message: String::new(), progress: None }, cancel });
        id
    }

    /// Applies progress updates and returns the jobs that finished since the last call.
    pub fn poll(&mut self) -> Vec<Finished<T>> {
        while let Ok(Progress { id, fraction, message }) = self.progress_rx.try_recv() {
            if let Some(job) = self.running.iter_mut().find(|r| r.status.id == id) {
                job.status.progress = Some(fraction);
                job.status.message = message;
            }
        }
        let mut finished = Vec::new();
        while let Ok(Done { id, outcome }) = self.done_rx.try_recv() {
            if let Some(index) = self.running.iter().position(|r| r.status.id == id) {
                let job = self.running.remove(index);
                finished.push(Finished { kind: job.status.kind, outcome });
            }
        }
        finished
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let detail = panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown problem".into());
    format!("The task stopped unexpectedly ({detail}).")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait<T: Send + 'static>(jobs: &mut Jobs<T>) -> Vec<Finished<T>> {
        let start = Instant::now();
        loop {
            let done = jobs.poll();
            if !done.is_empty() || start.elapsed() > Duration::from_secs(5) {
                return done;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn returns_the_outcome() {
        let mut jobs = Jobs::new();
        let id = jobs.spawn(JobKind::Import, "t".into(), |ctx| {
            ctx.progress(0.5, "half");
            21 * 2
        });
        assert!(jobs.is_running(JobKind::Import));
        let done = wait(&mut jobs);
        assert_eq!(jobs.statuses().iter().filter(|s| s.id == id).count(), 0);
        assert_eq!(done.into_iter().next().unwrap().outcome, Ok(42));
        assert!(!jobs.is_running(JobKind::Import));
    }

    #[test]
    fn a_panicking_job_becomes_an_error() {
        let mut jobs: Jobs<()> = Jobs::new();
        jobs.spawn(JobKind::Sync, "t".into(), |_| -> () { panic!("boom") });
        let done = wait(&mut jobs);
        let message = done.into_iter().next().unwrap().outcome.err().unwrap();
        assert!(message.contains("boom"), "{message}");
    }

    #[test]
    fn cancel_sets_the_flag() {
        let mut jobs = Jobs::new();
        let id = jobs.spawn(JobKind::Rip, "t".into(), |ctx| {
            let start = Instant::now();
            while !ctx.cancel_flag().load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(1));
            }
            ctx.cancel_flag().load(Ordering::Relaxed)
        });
        jobs.cancel(id);
        let done = wait(&mut jobs);
        assert_eq!(done.into_iter().next().unwrap().outcome, Ok(true));
    }
}
