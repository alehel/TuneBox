//! The song queued to follow the one playing, so the player can go straight on into it.
//!
//! Opening a file (probing its format, reading its headers) can take a while on a slow disk or a
//! network share, so it happens on a helper thread as soon as the song is queued, long before
//! it is needed. The player thread never waits for it: it keeps feeding the output meanwhile.

use crate::output::{OutputFactory, OutputFormat};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;
use tunebox_audio::Decoder;

/// The next song, opening or ready.
pub(crate) enum Upcoming {
    Opening { path: PathBuf, opened: Receiver<Result<Decoder, String>> },
    Ready(Prepared),
}

/// An open song with the output format it needs.
pub(crate) struct Prepared {
    pub path: PathBuf,
    pub decoder: Decoder,
    /// The file's own format.
    pub wanted: OutputFormat,
    /// The format the output would run at for it.
    pub format: OutputFormat,
}

/// What became of an upcoming song since it was last looked at.
pub(crate) enum Progress {
    Waiting(Upcoming),
    Failed { path: PathBuf, message: String },
}

impl Upcoming {
    /// Starts opening `path` on a helper thread.
    pub fn open(path: PathBuf) -> Self {
        let (sender, opened) = mpsc::channel();
        let target = path.clone();
        let spawned = std::thread::Builder::new().name("tunebox-preload".into()).spawn(move || {
            // The receiver may be gone (the song was replaced); nothing then wants the decoder.
            let _ = sender.send(Decoder::open(&target).map_err(|e| e.to_string()));
        });
        if let Err(e) = spawned {
            // The sender went with the closure, so the receiver reports the failure.
            log::warn!("could not start opening the next song: {e}");
        }
        Upcoming::Opening { path, opened }
    }

    /// Moves an opening song on once its file is open, asking `factory` which output format it
    /// needs.
    pub fn progress(self, factory: &mut dyn OutputFactory) -> Progress {
        let Upcoming::Opening { path, opened } = self else { return Progress::Waiting(self) };
        let decoder = match opened.try_recv() {
            Err(TryRecvError::Empty) => return Progress::Waiting(Upcoming::Opening { path, opened }),
            // The helper thread ended without an answer: it panicked inside the decoder.
            Err(TryRecvError::Disconnected) => Err("The file could not be read.".to_owned()),
            Ok(result) => result,
        };
        let prepared = decoder.and_then(|decoder| {
            let wanted = OutputFormat { sample_rate: decoder.sample_rate(), channels: decoder.channels() };
            let format = factory.negotiate(wanted)?;
            Ok(Prepared { path: path.clone(), decoder, wanted, format })
        });
        match prepared {
            Ok(prepared) => Progress::Waiting(Upcoming::Ready(prepared)),
            Err(message) => Progress::Failed { path, message },
        }
    }
}

impl Prepared {
    /// A song whose start was thrown away (a seek flushed it from the output) goes back to
    /// being next. Rewinding keeps the slow open from happening again; a file that cannot land
    /// exactly on its start is opened afresh instead.
    pub fn rewound(mut self) -> Upcoming {
        match self.decoder.seek(Duration::ZERO) {
            Ok(landed) if landed == Duration::ZERO => Upcoming::Ready(self),
            _ => Upcoming::open(self.path),
        }
    }
}
