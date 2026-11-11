//! One loaded plug-in: checked once when loaded, then asked for a drawing every frame.

use crate::drawing::{self, DrawCommand, InvalidDrawing};
use crate::error::{Fault, LoadError};
use crate::frame::{FrameData, encode};
use crate::info::{self, LookInfo};
use crate::pace::Pace;
use crate::sandbox::{self, FUEL_PER_FRAME, FUEL_TO_START, MAX_FILE_BYTES, Sandbox};
use std::time::{Duration, Instant};
use tunebox_visualizer_sdk::abi::{
    EXPORT_DRAW, EXPORT_FRAME_BUFFER, EXPORT_INFO, EXPORT_MEMORY, EXPORT_VERSION, FRAME_BYTES, MAX_DRAWING_BYTES,
    MAX_ID_BYTES, MAX_NAME_CHARS, VERSION,
};
use wasmi::{Memory, TypedFunc};

/// A visualizer plug-in, ready to draw.
pub struct Plugin {
    info: LookInfo,
    file: String,
    sandbox: Sandbox,
    memory: Memory,
    draw: TypedFunc<(f32, f32), i32>,
    /// Where the frame goes in the plug-in's memory.
    frame_at: usize,
    pace: Pace,
    /// Fuel the last frame used.
    fuel_used: u64,
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plugin").field("info", &self.info).field("file", &self.file).finish_non_exhaustive()
    }
}

impl Plugin {
    /// Loads a plug-in from the bytes of its `.wasm` file (`file` names it in messages):
    /// validates and compiles it, starts it, and checks its version, id, name and frame buffer.
    /// Slow for a large module: call it off the interface thread.
    pub fn load(bytes: &[u8], file: impl Into<String>) -> Result<Plugin, LoadError> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(LoadError::TooBig(bytes.len() as u64));
        }
        let mut sandbox = sandbox::start(bytes)?;
        let instance = sandbox.instance;
        let store = &mut sandbox.store;
        let memory = instance.get_memory(&*store, EXPORT_MEMORY).ok_or(LoadError::MissingExport(EXPORT_MEMORY))?;
        let version = instance
            .get_typed_func::<(), i32>(&*store, EXPORT_VERSION)
            .map_err(|_| LoadError::MissingExport(EXPORT_VERSION))?;
        let info_fn = instance
            .get_typed_func::<(), i32>(&*store, EXPORT_INFO)
            .map_err(|_| LoadError::MissingExport(EXPORT_INFO))?;
        let frame_fn = instance
            .get_typed_func::<(), i32>(&*store, EXPORT_FRAME_BUFFER)
            .map_err(|_| LoadError::MissingExport(EXPORT_FRAME_BUFFER))?;
        let draw = instance
            .get_typed_func::<(f32, f32), i32>(&*store, EXPORT_DRAW)
            .map_err(|_| LoadError::MissingExport(EXPORT_DRAW))?;

        let found = sandbox::call(store, FUEL_TO_START, |s| version.call(s, ())).map_err(LoadError::Failed)? as u32;
        if found != VERSION {
            return Err(LoadError::WrongVersion { found, expected: VERSION });
        }
        let info_at = sandbox::call(store, FUEL_TO_START, |s| info_fn.call(s, ())).map_err(LoadError::Failed)?;
        let info = read_info(memory.data(&*store), address(info_at)).map_err(LoadError::BadInfo)?;
        let frame_at =
            address(sandbox::call(store, FUEL_TO_START, |s| frame_fn.call(s, ())).map_err(LoadError::Failed)?);
        if memory.data(&*store).get(frame_at..frame_at.saturating_add(FRAME_BYTES)).is_none() {
            return Err(LoadError::BadInfo("its frame buffer lies outside its memory".into()));
        }
        Ok(Plugin { info, file: file.into(), sandbox, memory, draw, frame_at, pace: Pace::default(), fuel_used: 0 })
    }

    /// The look's id and name.
    pub fn info(&self) -> &LookInfo {
        &self.info
    }

    /// The file it came from, for messages.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The average time a frame has taken so far.
    pub fn average_draw_time(&self) -> Duration {
        self.pace.average()
    }

    /// The share of its fuel budget (0–1) the last frame used.
    pub fn last_fuel_share(&self) -> f32 {
        self.fuel_used as f32 / FUEL_PER_FRAME as f32
    }

    /// Asks for one frame's drawing at `width` × `height` points. Any fault means the plug-in
    /// must not be called again.
    pub fn draw(&mut self, frame: &FrameData, width: f32, height: f32) -> Result<Vec<DrawCommand>, Fault> {
        let started = Instant::now();
        let store = &mut self.sandbox.store;
        self.memory
            .write(&mut *store, self.frame_at, &encode(frame))
            .map_err(|e| Fault::Trapped(format!("its frame buffer could not be written: {e}")))?;
        let draw = self.draw;
        let drawing_at = address(sandbox::call(store, FUEL_PER_FRAME, |s| draw.call(s, (width, height)))?);
        self.fuel_used = FUEL_PER_FRAME.saturating_sub(store.get_fuel().unwrap_or(0));
        let commands = drawing::decode(drawing_bytes(self.memory.data(&*store), drawing_at)?)?;
        if !self.pace.record(started.elapsed()) {
            return Err(Fault::TooSlow);
        }
        Ok(commands)
    }
}

/// A module address as an index into its memory. WebAssembly addresses are unsigned 32-bit.
fn address(value: i32) -> usize {
    value as u32 as usize
}

fn word_at(memory: &[u8], at: usize) -> Option<u32> {
    memory.get(at..at.checked_add(4)?)?.try_into().ok().map(u32::from_le_bytes)
}

/// The bytes of a drawing: a length word at `at`, then that many bytes, all inside memory.
fn drawing_bytes(memory: &[u8], at: usize) -> Result<&[u8], InvalidDrawing> {
    let length = word_at(memory, at).ok_or(InvalidDrawing::OutsideMemory)? as usize;
    if length > MAX_DRAWING_BYTES {
        return Err(InvalidDrawing::TooLarge(length));
    }
    let start = at.saturating_add(4);
    memory.get(start..start.saturating_add(length)).ok_or(InvalidDrawing::OutsideMemory)
}

/// The id and name from the four words at `at`.
fn read_info(memory: &[u8], at: usize) -> Result<LookInfo, String> {
    let words: Option<Vec<usize>> =
        (0..4).map(|i| word_at(memory, at.saturating_add(i * 4)).map(|w| w as usize)).collect();
    let [id_at, id_len, name_at, name_len] = words.as_deref().unwrap_or_default() else {
        return Err("its id and name lie outside its memory".into());
    };
    // Bounded before reading: a name is at most this many characters of up to four bytes.
    if *id_len > MAX_ID_BYTES || *name_len > MAX_NAME_CHARS * 4 {
        return Err("its id or name is too long".into());
    }
    let id = memory.get(*id_at..id_at.saturating_add(*id_len));
    let name = memory.get(*name_at..name_at.saturating_add(*name_len));
    match (id, name) {
        (Some(id), Some(name)) => info::check(id, name),
        _ => Err("its id and name lie outside its memory".into()),
    }
}
