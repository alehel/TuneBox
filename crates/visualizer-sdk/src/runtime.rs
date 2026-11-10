//! The plumbing behind [`export!`](crate::export): the buffers TuneBox reads and writes, and
//! the calls into the look. Not for direct use.
//!
//! The frame buffer is an array of atomics, not a `static mut`: TuneBox writes into it between
//! calls, and atomics let safe Rust share a global the host can address. On WebAssembly without
//! threads a relaxed atomic load is a plain load.

use crate::abi::{FRAME_BYTES, MAX_DRAWING_BYTES};
use crate::{Canvas, Frame, Look};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

const FRAME_WORDS: usize = FRAME_BYTES / 4;

static FRAME: [AtomicU32; FRAME_WORDS] = [const { AtomicU32::new(0) }; FRAME_WORDS];
static INFO: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
/// The last drawing: a byte length, then the commands.
static DRAWING: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// A module address (WebAssembly addresses are 32-bit).
fn address<T>(pointer: *const T) -> u32 {
    pointer as usize as u32
}

/// The id and name, as four words: id address, id length, name address, name length.
pub fn info<L: Look>() -> u32 {
    let words = [address(L::ID.as_ptr()), L::ID.len() as u32, address(L::NAME.as_ptr()), L::NAME.len() as u32];
    for (slot, word) in INFO.iter().zip(words) {
        slot.store(word, Ordering::Relaxed);
    }
    address(INFO.as_ptr())
}

/// Where TuneBox writes each frame.
pub fn frame_buffer() -> u32 {
    address(FRAME.as_ptr())
}

/// Draws one frame with the look in `slot` (made on first use) and returns the drawing's
/// address.
pub fn draw<L: Look>(slot: &Mutex<Option<L>>, width: f32, height: f32) -> u32 {
    let bytes: Vec<u8> = FRAME.iter().flat_map(|w| w.load(Ordering::Relaxed).to_le_bytes()).collect();
    let frame = Frame::from_bytes(&bytes);
    let mut canvas = Canvas::new(width, height);
    if let Ok(mut look) = slot.lock() {
        look.get_or_insert_with(L::default).draw(&frame, &mut canvas);
    }
    let Ok(mut drawing) = DRAWING.lock() else { return 0 };
    drawing.clear();
    let length = (canvas.words().len() * 4).min(MAX_DRAWING_BYTES);
    drawing.push(length as u32);
    drawing.extend_from_slice(canvas.words());
    address(drawing.as_ptr())
}
