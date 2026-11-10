//! The contract between TuneBox and a visualizer plug-in, version [`VERSION`].
//!
//! TuneBox (the host) and plug-ins (WebAssembly modules) both read these numbers, so they
//! cannot drift apart. `docs/visualizer-plugins.md` describes the same contract in prose.
//!
//! A plug-in exports its linear memory as `memory` and four functions, and imports nothing:
//!
//! | Export | Signature | Returns |
//! |---|---|---|
//! | [`EXPORT_VERSION`] | `() -> i32` | [`VERSION`] |
//! | [`EXPORT_INFO`] | `() -> i32` | Address of four `u32`s: id address, id length, name address, name length (UTF-8) |
//! | [`EXPORT_FRAME_BUFFER`] | `() -> i32` | Address of [`FRAME_BYTES`] bytes the host fills before each draw |
//! | [`EXPORT_DRAW`] | `(width: f32, height: f32) -> i32` | Address of the drawing: a `u32` byte length, then that many bytes of commands |
//!
//! Everything is little-endian. Positions are in points, from the top-left corner of the
//! visualizer (`0..width`, `0..height`). Colours are `u32`s holding red, green, blue and alpha
//! bytes in that order in memory (`0xAABBGGRR` as a number), not premultiplied.

/// The ABI version this SDK speaks. TuneBox refuses a plug-in reporting another.
pub const VERSION: u32 = 1;

/// Export names.
pub const EXPORT_MEMORY: &str = "memory";
pub const EXPORT_VERSION: &str = "tunebox_abi_version";
pub const EXPORT_INFO: &str = "tunebox_info";
pub const EXPORT_FRAME_BUFFER: &str = "tunebox_frame_buffer";
pub const EXPORT_DRAW: &str = "tunebox_draw";

// ----- the frame -----

/// Spectrum bands per frame.
pub const BANDS: usize = 64;
/// Samples per waveform.
pub const WAVE_SAMPLES: usize = 256;
/// Most waveforms a frame carries (oldest first).
pub const MAX_WAVEFORMS: usize = 6;

/// Byte offsets in the frame buffer. Every field is 4 bytes: `f32` unless noted.
pub const FRAME_TIME: usize = 0;
pub const FRAME_DT: usize = 4;
pub const FRAME_LEVEL: usize = 8;
pub const FRAME_BASS: usize = 12;
/// `u32`: 1 while something plays, else 0.
pub const FRAME_PLAYING: usize = 16;
/// `u32`: how many of the [`MAX_WAVEFORMS`] waveform slots hold audio.
pub const FRAME_WAVEFORM_COUNT: usize = 20;
/// [`BANDS`] `f32`s, 0–1.
pub const FRAME_BANDS: usize = 32;
/// [`BANDS`] `f32`s, 0–1.
pub const FRAME_PEAKS: usize = FRAME_BANDS + BANDS * 4;
/// [`MAX_WAVEFORMS`] × [`WAVE_SAMPLES`] `f32`s, ±1, oldest first.
pub const FRAME_WAVEFORMS: usize = FRAME_PEAKS + BANDS * 4;
/// The frame buffer's size.
pub const FRAME_BYTES: usize = FRAME_WAVEFORMS + MAX_WAVEFORMS * WAVE_SAMPLES * 4;

// ----- drawing commands -----
//
// A drawing is a sequence of 4-byte words. Each command is an opcode word followed by its
// fields, listed below in order. `f32` unless noted; `color` is a `u32` colour; `count`s are
// `u32`s; a point is two words, `x` then `y`.

/// `x y width height rounding color`.
pub const OP_FILL_RECT: u32 = 1;
/// `x y width height rounding stroke_width color`.
pub const OP_STROKE_RECT: u32 = 2;
/// `center_x center_y radius color`.
pub const OP_FILL_CIRCLE: u32 = 3;
/// `center_x center_y radius stroke_width color`.
pub const OP_STROKE_CIRCLE: u32 = 4;
/// `stroke_width color closed(u32: 0 or 1) count points…`: a line through `count` ≥ 2 points.
pub const OP_LINE: u32 = 5;
/// `color count points…`: a filled convex polygon of `count` ≥ 3 points.
pub const OP_FILL_POLYGON: u32 = 6;
/// `vertex_count index_count (x y color)… indices(u32)…`: triangles, three indices each,
/// colours blended across each triangle (gradients and glows).
pub const OP_MESH: u32 = 7;

// ----- limits per frame (a drawing over any of them turns the plug-in off) -----

/// Bytes of commands.
pub const MAX_DRAWING_BYTES: usize = 1 << 20;
/// Commands.
pub const MAX_COMMANDS: usize = 20_000;
/// Line and polygon points plus mesh vertices.
pub const MAX_VERTICES: usize = 65_536;
/// Mesh indices.
pub const MAX_INDICES: usize = 196_608;
/// Largest distance from the origin of any position, and the largest size, radius or rounding.
pub const MAX_COORDINATE: f32 = 100_000.0;
/// Widest stroke.
pub const MAX_STROKE_WIDTH: f32 = 1_000.0;

// ----- the look's description -----

/// Longest id, in bytes. Ids are lowercase ASCII letters, digits and `-`.
pub const MAX_ID_BYTES: usize = 40;
/// Longest name, in characters.
pub const MAX_NAME_CHARS: usize = 40;
