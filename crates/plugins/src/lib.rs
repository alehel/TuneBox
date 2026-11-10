//! tunebox-plugins — visualizer looks loaded at run time, as WebAssembly plug-ins.
//!
//! A plug-in is a `.wasm` file in TuneBox's `Visualizers` folder. [`load_folder`] finds and
//! loads them; each loaded [`Plugin`] is then asked, once a frame, to turn a [`FrameData`] into a
//! list of [`DrawCommand`]s, which the visualizer crate paints. The contract (exports, memory
//! layout, command encoding, limits) is `tunebox_visualizer_sdk::abi`; plug-in authors use that
//! SDK too.
//!
//! A plug-in is code from a stranger, so it runs in a [`sandbox`]: no imports (no files, no
//! network), a fuel budget per call, a memory cap, and every result checked ([`drawing`]). Any
//! [`Fault`] means the plug-in must be turned off; [`Fault::notice`] says so to the listener.
//!
//! This crate knows nothing of egui: shapes are plain data, so the engine and the command line
//! can load and run plug-ins too (`tunebox-cli visualizer-plugins`).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod drawing;
mod error;
mod folder;
mod frame;
mod info;
mod pace;
mod plugin;
pub mod sandbox;

pub use drawing::{DrawCommand, InvalidDrawing, MeshVertex, Point, Rect, Rgba};
pub use error::{Fault, LoadError};
pub use folder::{FolderReport, MAX_PLUGINS, load_folder};
pub use frame::{FrameData, encode as encode_frame};
pub use info::LookInfo;
pub use pace::SLOW_FRAME;
pub use plugin::Plugin;
