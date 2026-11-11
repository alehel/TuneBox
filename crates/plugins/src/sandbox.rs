//! The sandbox a plug-in runs in: a WebAssembly interpreter with nothing to import, a fuel
//! budget for every call and a cap on memory.
//!
//! A plug-in is code from a stranger. It gets no imports at all (no files, no network, no
//! clock: it can only compute and return a drawing), so whatever it does stays inside its own
//! memory. Fuel bounds how much work one call may do, so an endless loop ends after a few
//! milliseconds; the memory cap bounds how much it may allocate. The interpreter (wasmi)
//! validates the whole module before running it, and every call runs behind `catch_unwind` so
//! even a bug in the runtime turns the plug-in off rather than taking TuneBox down.

use crate::error::{Fault, LoadError};
use std::panic::{AssertUnwindSafe, catch_unwind};
use wasmi::{
    CompilationMode, Config, Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TrapCode,
};

/// Largest plug-in file read.
pub const MAX_FILE_BYTES: u64 = 8 << 20;
/// Most memory a plug-in may use (its linear memory).
pub const MEMORY_LIMIT: usize = 32 << 20;
/// Fuel for one frame, roughly one unit per WebAssembly instruction. Spent in full it took
/// about 4 milliseconds in a release build on the computer it was measured on (see
/// docs/visualizer-plugins.md); a frame at 60 per second has 16.
pub const FUEL_PER_FRAME: u64 = 2_000_000;
/// Fuel for starting up (the module's start function and data) and each small call.
pub const FUEL_TO_START: u64 = 20_000_000;

/// A running module and its store.
pub(crate) struct Sandbox {
    pub store: Store<StoreLimits>,
    pub instance: Instance,
}

/// An interpreter set up for plug-ins: fuel on, every function compiled up front (so the
/// first frame does not pay for it on the interface thread).
fn engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true).compilation_mode(CompilationMode::Eager);
    Engine::new(&config)
}

/// Validates, compiles and starts a module. Runs on a worker thread: compiling is the slow part.
pub(crate) fn start(bytes: &[u8]) -> Result<Sandbox, LoadError> {
    if !bytes.starts_with(b"\0asm") {
        return Err(LoadError::NotWebAssembly("it does not start like one".into()));
    }
    let engine = engine();
    let module = Module::new(&engine, bytes).map_err(|e| LoadError::NotWebAssembly(brief(&e.to_string())))?;
    let imports: Vec<String> = module.imports().map(|i| format!("{}.{}", i.module(), i.name())).collect();
    if !imports.is_empty() {
        return Err(LoadError::HasImports(imports.join(", ")));
    }
    let limits = StoreLimitsBuilder::new()
        .memory_size(MEMORY_LIMIT)
        .memories(1)
        .tables(1)
        .table_elements(100_000)
        .instances(1)
        .build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|limits| limits);
    let linker = Linker::new(&engine);
    let instance = call(&mut store, FUEL_TO_START, |store| linker.instantiate_and_start(store, &module))
        .map_err(LoadError::Failed)?;
    Ok(Sandbox { store, instance })
}

/// Runs `work` against the store with `fuel` to spend, turning traps, running out of fuel and
/// panics inside the runtime into a [`Fault`].
pub(crate) fn call<R>(
    store: &mut Store<StoreLimits>,
    fuel: u64,
    work: impl FnOnce(&mut Store<StoreLimits>) -> Result<R, wasmi::Error>,
) -> Result<R, Fault> {
    store.set_fuel(fuel).map_err(|e| Fault::Crashed(e.to_string()))?;
    match catch_unwind(AssertUnwindSafe(|| work(store))) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) if error.as_trap_code() == Some(TrapCode::OutOfFuel) => Err(Fault::OutOfTime),
        Ok(Err(error)) => Err(Fault::Trapped(brief(&error.to_string()))),
        Err(panic) => Err(Fault::Crashed(brief(&panic_text(panic.as_ref())))),
    }
}

/// A runtime message fit for a notice: on one line, and not too long (validation messages can
/// list raw bytes).
fn brief(message: &str) -> String {
    const MOST: usize = 160;
    let line = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MOST { line } else { line.chars().take(MOST).collect::<String>() + "…" }
}

fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown problem".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_kept_to_one_short_line() {
        assert_eq!(brief("bad magic\n  [0x0,\n 0x61]"), "bad magic [0x0, 0x61]");
        let long = brief(&"x ".repeat(500));
        assert_eq!(long.chars().count(), 161);
        assert!(long.ends_with('…'));
    }
}
