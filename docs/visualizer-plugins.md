# Visualizer plug-ins

A visualizer plug-in is a look TuneBox loads at run time: a WebAssembly module in the
`Visualizers` folder. Anyone can write one without rebuilding TuneBox, and TuneBox runs it in a
sandbox, so a broken or hostile plug-in cannot crash TuneBox, hang it, slow it down, read files
or reach the network. (Looks compiled into TuneBox are described in
[visualizers.md](visualizers.md).)

## Installing a plug-in

1. In TuneBox choose **View ▸ Show Visualizer Plug-ins Folder**. It is `Visualizers` inside
   TuneBox's data folder:
   - Linux: `~/.local/share/TuneBox/Visualizers`
   - macOS: `~/Library/Application Support/TuneBox/Visualizers`
   - Windows: `%APPDATA%\TuneBox\Visualizers`
2. Put the plug-in's `.wasm` file there.
3. Choose **View ▸ Reload Visualizer Plug-ins** (or restart TuneBox).

Plug-in looks appear in the visualizer's look picker after the built-in ones, past a thin rule;
hovering one says it is a plug-in. A file that is not a usable plug-in is reported once, when
the folder is read, with the reason. Removing a plug-in whose look was chosen is harmless: the
first built-in look shows instead, and the plug-in's look returns if the file does.

`tunebox-cli visualizer-plugins` (optionally `--folder DIR`) loads every plug-in and runs each
for 300 frames of made-up music, reporting what it draws, how long it takes and whether TuneBox
would refuse it or turn it off. It is the quickest way to check a plug-in.

## Writing a plug-in

Use the SDK, `crates/visualizer-sdk` (`tunebox-visualizer-sdk`). A plug-in is a Rust library
crate built as a `cdylib` for `wasm32-unknown-unknown`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
tunebox-visualizer-sdk = { git = "https://github.com/alehel/tunebox" }
```

```rust
use tunebox_visualizer_sdk::{Canvas, Color, Frame, Look, export};

#[derive(Default)]
struct Glow {
    spin: f32,
}

impl Look for Glow {
    const ID: &'static str = "glow"; // saved in preferences: never change it
    const NAME: &'static str = "Glow"; // shown in the picker

    fn draw(&mut self, frame: &Frame, canvas: &mut Canvas) {
        self.spin += frame.dt;
        let (width, height) = canvas.size();
        canvas.fill_rect([0.0, 0.0, width, height], 0.0, Color::rgb(4, 4, 12));
        let radius = 40.0 + frame.bass * 200.0;
        canvas.fill_circle(width / 2.0, height / 2.0, radius, Color::hsva(self.spin * 0.1, 0.7, 1.0, 1.0));
    }
}

export!(Glow);
```

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
# the plug-in is target/wasm32-unknown-unknown/release/<crate name>.wasm
```

The look keeps its own state between frames (`&mut self`); it is created with `Default` before
the first frame. `Frame` carries the same measurements built-in looks get (see
[visualizers.md](visualizers.md#what-a-frame-tells-you)): `bands`, `peaks`, `level`, `bass`,
`waveforms`, `time`, `dt`, `playing`, and `band_at` and `waveform` helpers. The `Canvas` offers
`fill_rect`, `stroke_rect`, `fill_circle`, `stroke_circle`, `line` (open or closed),
`fill_polygon` (convex) and `mesh` (triangles with a colour per vertex, for gradients and
glows). Positions are in points from the visualizer's top-left corner; `canvas.size()` is its
width and height. There is no text.

The canvas leaves out any shape TuneBox would refuse (a position that is not a number, a
negative size, more shapes than a frame may have) and counts them in `canvas.left_out()`, so
a plug-in built with the SDK is never turned off for what it draws. It is still turned off if it
panics or runs out of time.

**The example**, `plugins/ripple` (rings that ripple out on the beat around a circle of
spectrum bars), is a complete plug-in. `cargo xtask plugins` builds every plug-in in `plugins/`
into `target/visualizer-plugins/`; `cargo xtask plugins --install` also copies them into your
Visualizers folder. Its look is tested on the host like any Rust code (`cargo test -p
tunebox-plugin-ripple`).

**About `unsafe`.** `export!` defines the module's exports with `#[unsafe(no_mangle)]`, which
WebAssembly needs to find functions by name. It is the only `unsafe` a plug-in needs. It is
written once, in the SDK's macro, and the code it marks runs only inside the sandbox.
The SDK uses `std` rather than `no_std`: a look keeps its state in a global, and doing that in
safe Rust needs `std`'s `Mutex`. On `wasm32-unknown-unknown`, `std` adds no imports. The
example plug-in is 37 KB.

## Limits

| What | Limit | When it is exceeded |
|---|---|---|
| File size | 8 MiB | Not loaded |
| Plug-in files read per scan | 64 | The rest are ignored, in file-name order |
| Memory | 32 MiB | Growing memory fails (Rust's allocator then aborts, which turns the plug-in off); a module declaring more is not loaded |
| Work per frame | 2,000,000 fuel (about one unit per WebAssembly instruction) | Turned off: "it took too long to draw a frame" |
| Work while starting (start function, each setup call) | 20,000,000 fuel | Not loaded |
| Time per frame | 8 ms on average, judged after 30 frames (the first 5 are not counted) | Turned off: "it kept drawing too slowly" |
| Commands per frame | 20,000 | Turned off |
| Points and mesh vertices per frame | 65,536 | Turned off |
| Mesh indices per frame | 196,608 | Turned off |
| Drawing size | 1 MiB | Turned off |
| Positions | finite, within ±100,000 points | Turned off |
| Sizes, radii, rounding | finite, 0 to 100,000 | Turned off |
| Stroke widths | finite, 0 to 1,000 | Turned off |
| Id | 1–40 lowercase ASCII letters, digits and `-` | Not loaded |
| Name | 1–40 characters of plain text | Not loaded |

Fuel bounds how much work one frame can do whatever the computer; the time limit catches a
plug-in that stays within its fuel but is still too slow on a slower computer. Spending the
whole fuel budget took about 4 ms a frame in a release build on the computer it was measured on
(a frame at 60 a second has about 16 ms). The example plug-in uses 14% of the budget and takes
about 0.3 ms a frame in a release build (`tunebox-cli visualizer-plugins`).

**Turned off** means for the rest of the session: the look disappears from the picker, the
first built-in look shows instead, and a notice says why, e.g. *The “Glow” visualizer stopped
working and was turned off: it took too long to draw a frame.* Reload Visualizer Plug-ins reads
the folder again and gives every plug-in a fresh start. Two plug-ins with the same id: the first
in file-name order loads, the other is reported.

## The ABI (version 1)

For plug-ins written without the SDK, or in another language. The constants are in
`crates/visualizer-sdk/src/abi.rs`, which TuneBox reads too.

The module **imports nothing** (no WASI, no host functions: a module with any import is not
loaded) and exports:

| Export | Type | Returns |
|---|---|---|
| `memory` | memory | The module's linear memory (32-bit) |
| `tunebox_abi_version` | `() -> i32` | `1` |
| `tunebox_info` | `() -> i32` | Address of four `u32`s: id address, id length, name address, name length (UTF-8 bytes) |
| `tunebox_frame_buffer` | `() -> i32` | Address of 6,688 bytes where TuneBox writes each frame |
| `tunebox_draw` | `(width: f32, height: f32) -> i32` | Address of the drawing: a `u32` byte length, then the commands |

TuneBox calls `tunebox_abi_version`, `tunebox_info` and `tunebox_frame_buffer` once, after the
module starts. Then, for each frame, it writes the frame into the frame buffer and calls
`tunebox_draw` with the visualizer's size in points.

Everything is little-endian. **The frame** (each field 4 bytes; `f32` unless marked):

| Offset | Field |
|---|---|
| 0 | `time`: seconds since the visualizer opened |
| 4 | `dt`: seconds since the previous frame |
| 8 | `level`: overall loudness, 0–1 |
| 12 | `bass`: loudness of the lowest bands, 0–1 |
| 16 | `playing` (`u32`): 1 while something plays |
| 20 | waveform count (`u32`, 0–6) |
| 24, 28 | reserved (zero) |
| 32 | 64 band energies, 0–1, low to high |
| 288 | 64 band peaks, 0–1 |
| 544 | 6 waveforms × 256 samples, ±1, oldest first; only the first *count* hold audio |

**The drawing** is a sequence of 4-byte words. Each command is an opcode followed by its fields;
`f32` unless marked, a colour is a `u32` holding red, green, blue and alpha bytes in that memory
order (not premultiplied), a point is `x y`:

| Opcode | Command | Fields |
|---|---|---|
| 1 | Filled rectangle | `x y width height rounding colour` |
| 2 | Rectangle outline | `x y width height rounding stroke_width colour` |
| 3 | Filled circle | `x y radius colour` |
| 4 | Circle outline | `x y radius stroke_width colour` |
| 5 | Line | `stroke_width colour closed(u32: 0/1) count(u32 ≥ 2) points…` |
| 6 | Filled convex polygon | `colour count(u32 ≥ 3) points…` |
| 7 | Mesh | `vertex_count(u32) index_count(u32) (x y colour)… indices(u32)…`: triangles, indices in threes, each naming a vertex |

TuneBox checks the whole drawing before painting any of it, and refuses (turning the plug-in
off) a drawing that breaks any rule: an unknown opcode, a command cut off, a value out of range
or not a number, a bad index, or more than the limits above. Shapes are clipped to the
visualizer.

## How the sandbox works

- **Loading** happens on a worker thread (an engine job), at start and on Reload. The module is
  validated and compiled in full there, so the interface thread never compiles plug-in code.
  Then it is started with its start-up fuel, and its version, id, name and frame buffer are
  checked.
- **Isolation.** Nothing is imported, so the only thing a plug-in can do is compute inside its
  own memory and return a drawing. It has no clock (it gets `time` and `dt`), no files, no
  network, no randomness source beyond what it computes.
- **Every call** runs with a fuel budget and behind `catch_unwind`: a trap (including a Rust
  panic, which aborts to a trap), running out of fuel, or even a panic inside the runtime
  becomes a fault that turns the plug-in off.
- **The drawing** is decoded by `tunebox_plugins::drawing`, which bounds every count before it
  allocates anything.

The code: `crates/plugins` (the host: sandbox, ABI decoding, folder scan), `crates/engine/src/visualizer_plugins.rs`
(the loading job), `crates/visualizer/src/plugin_look.rs` and `plugin_shapes.rs` (painting),
`crates/visualizer/src/registry.rs` (turning a failed plug-in off).

## Why wasmi

Two pure-Rust WebAssembly runtimes were considered.

| | wasmi 2 (interpreter) | Wasmtime 50 (compiler, Cranelift) |
|---|---|---|
| Licence | MIT or Apache-2.0: passes `deny.toml` | Apache-2.0 WITH LLVM-exception: passes `deny.toml` |
| Crates pulled in | 10 | 101 |
| Clean release build of a program that loads a module | 49 s | 223 s |
| Its size, stripped (a program without either: 0.3 MB) | 1.6 MB | 10.4 MB |
| Bounding work | Fuel: deterministic | Fuel (deterministic) or epoch interruption (timer-driven) |
| Bounding memory | `StoreLimits` | `StoreLimits` |
| Determinism | Same fuel, same result on every computer | The same with fuel |
| What it does with untrusted code | Interprets it; never generates machine code | Compiles it to machine code at run time (needs writable-then-executable memory; on macOS a hardened app needs the JIT entitlement) |
| Speed | An interpreter: several times slower than compiled code | Near native |

The build and size figures come from three tiny programs that read a file and instantiate it
as a module (one with neither runtime, one with each), built with TuneBox's release profile,
from clean, on the same Linux computer (Wasmtime with only `std`, `cranelift` and `runtime`;
wasmi as TuneBox uses it). They include fetching any crates not yet cached.

A visualizer frame is a few thousand WebAssembly instructions of arithmetic producing at most a
few thousand shapes; the example plug-in takes about 0.3 ms a frame interpreted. The speed of a
compiler is not needed, and an interpreter that never generates machine code is the smaller
attack surface and the smaller, faster build. wasmi it is.

wasmi is built with `portable-dispatch`. Its default dispatch relies on the compiler turning
calls between instruction handlers into jumps; in a debug build it did not, and a plug-in's
endless loop overflowed TuneBox's own stack before its fuel ran out (found by the test
`an_endless_loop_runs_out_of_fuel_quickly`). The portable dispatch is a plain loop, safe in
every build, at about 40% more time per instruction (a full fuel budget of 5,000,000 took
6.3 ms with the default dispatch and 10 ms with the portable one, release build, same
computer). The fuel budget was set from the portable figure.
