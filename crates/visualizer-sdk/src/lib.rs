//! tunebox-visualizer-sdk — write a TuneBox visualizer as a WebAssembly plug-in.
//!
//! A plug-in is a Rust library built for `wasm32-unknown-unknown` as a `cdylib`. It implements
//! [`Look`] and names the type with [`export!`]; TuneBox loads the `.wasm` file from its
//! `Visualizers` folder at run time and calls the look about 60 times a second with a
//! [`Frame`] of audio measurements. The look draws on a [`Canvas`]: rectangles, circles,
//! lines, convex polygons and meshes with a colour per vertex.
//!
//! ```ignore
//! use tunebox_visualizer_sdk::{Canvas, Color, Frame, Look, export};
//!
//! #[derive(Default)]
//! struct Glow;
//!
//! impl Look for Glow {
//!     const ID: &'static str = "glow";
//!     const NAME: &'static str = "Glow";
//!     fn draw(&mut self, frame: &Frame, canvas: &mut Canvas) {
//!         let (w, h) = canvas.size();
//!         canvas.fill_rect([0.0, 0.0, w, h], 0.0, Color::rgb(0, 0, 0));
//!         canvas.fill_circle(w / 2.0, h / 2.0, 40.0 + frame.bass * 200.0, Color::rgb(255, 120, 40));
//!     }
//! }
//!
//! export!(Glow);
//! ```
//!
//! The SDK uses `std` rather than `no_std`: a look keeps state between frames, and holding it
//! in a global without `unsafe` needs `std`'s `Mutex`. `std` on `wasm32-unknown-unknown` adds
//! no imports, which TuneBox requires. [`abi`] is the contract itself; TuneBox reads the same
//! constants. See `docs/visualizer-plugins.md`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod abi;
mod canvas;
mod color;
mod frame;
#[doc(hidden)]
pub mod runtime;

pub use canvas::{Canvas, Vertex};
pub use color::Color;
pub use frame::Frame;

/// A visualizer look.
pub trait Look: Default + Send + 'static {
    /// A stable identifier, saved in preferences: lowercase letters, digits and `-`, at most
    /// [`abi::MAX_ID_BYTES`] long, never changed once released (e.g. `"ripple"`).
    const ID: &'static str;
    /// The name shown in the picker, at most [`abi::MAX_NAME_CHARS`] characters.
    const NAME: &'static str;

    /// Draws one frame on `canvas`, whose [`Canvas::size`] is the visualizer's size.
    fn draw(&mut self, frame: &Frame, canvas: &mut Canvas);
}

/// Makes `$look` (a type implementing [`Look`]) the plug-in's look, by defining the exports
/// TuneBox calls. Use it once, at the root of the plug-in crate.
///
/// The exports need `#[unsafe(no_mangle)]` (WebAssembly finds functions by name); that is the
/// only `unsafe` a plug-in needs, and it stays inside the sandbox TuneBox runs plug-ins in.
#[macro_export]
macro_rules! export {
    ($look:ty) => {
        static TUNEBOX_LOOK: ::std::sync::Mutex<::core::option::Option<$look>> = ::std::sync::Mutex::new(None);

        #[unsafe(no_mangle)]
        pub extern "C" fn tunebox_abi_version() -> u32 {
            $crate::abi::VERSION
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn tunebox_info() -> u32 {
            $crate::runtime::info::<$look>()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn tunebox_frame_buffer() -> u32 {
            $crate::runtime::frame_buffer()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn tunebox_draw(width: f32, height: f32) -> u32 {
            $crate::runtime::draw(&TUNEBOX_LOOK, width, height)
        }
    };
}
