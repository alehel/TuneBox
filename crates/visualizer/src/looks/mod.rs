//! The built-in looks, one per file. To add one, write a file next to these that implements
//! [`Visualizer`](crate::Visualizer) and list it in [`builtin`].

mod aurora;
mod hyperspace;
mod pulse;
mod scope;
mod spectrum;

pub use aurora::Aurora;
pub use hyperspace::Hyperspace;
pub use pulse::Pulse;
pub use scope::Scope;
pub use spectrum::Spectrum;

use crate::Visualizer;

/// Every built-in look, in picker order.
pub fn builtin() -> Vec<Box<dyn Visualizer>> {
    vec![
        Box::new(Spectrum),
        Box::new(Aurora),
        Box::new(Pulse::default()),
        Box::new(Scope),
        Box::new(Hyperspace::default()),
    ]
}
