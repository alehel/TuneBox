//! Plug-ins built from WebAssembly text (no wasm32 toolchain needed): one that works, and one
//! for each way a stranger's module can go wrong. None may crash, hang or outlast its budget.

use std::time::{Duration, Instant};
use tunebox_plugins::{DrawCommand, Fault, FrameData, InvalidDrawing, LoadError, Plugin, Point, Rgba, load_folder};

/// Where the test modules keep their frame buffer (their drawing is at 8192, past its end).
const FRAME_AT: u32 = 1024;

/// A plug-in module whose `tunebox_draw` body is `draw` (it must leave an address on the
/// stack). `extra` adds module fields; `version` is what it reports.
fn module_with(draw: &str, extra: &str, version: u32, memory_pages: u32) -> Vec<u8> {
    let text = format!(
        r#"(module
            (memory (export "memory") {memory_pages})
            (data (i32.const 100) "\c8\00\00\00\06\00\00\00\2c\01\00\00\06\00\00\00")
            (data (i32.const 200) "circle")
            (data (i32.const 300) "Circle")
            (func (export "tunebox_abi_version") (result i32) i32.const {version})
            (func (export "tunebox_info") (result i32) i32.const 100)
            (func (export "tunebox_frame_buffer") (result i32) i32.const {FRAME_AT})
            (func (export "tunebox_draw") (param $w f32) (param $h f32) (result i32) {draw})
            {extra})"#
    );
    wat::parse_str(text).unwrap()
}

fn module(draw: &str) -> Vec<u8> {
    module_with(draw, "", 1, 1)
}

/// Draws one red circle in the middle, its radius 100 × the bass.
const CIRCLE: &str = r#"
    (i32.store (i32.const 8192) (i32.const 20))
    (i32.store (i32.const 8196) (i32.const 3))
    (f32.store (i32.const 8200) (f32.div (local.get $w) (f32.const 2)))
    (f32.store (i32.const 8204) (f32.div (local.get $h) (f32.const 2)))
    (f32.store (i32.const 8208) (f32.mul (f32.load (i32.const 1036)) (f32.const 100)))
    (i32.store (i32.const 8212) (i32.const 0xff0000ff))
    (i32.const 8192)"#;

fn frame() -> (Vec<f32>, Vec<Vec<f32>>) {
    (vec![0.5; 64], vec![vec![0.1; 256]; 6])
}

fn draw_once(bytes: &[u8]) -> Result<Vec<DrawCommand>, Fault> {
    let mut plugin = Plugin::load(bytes, "test.wasm").unwrap();
    let (bands, waves) = frame();
    let data =
        FrameData { bands: &bands, peaks: &bands, bass: 0.7, waveforms: &waves, playing: true, ..Default::default() };
    plugin.draw(&data, 800.0, 600.0)
}

/// Runs `work`, failing the test if it takes longer than `limit`.
fn within<R>(limit: Duration, work: impl FnOnce() -> R) -> R {
    let started = Instant::now();
    let result = work();
    assert!(started.elapsed() < limit, "took {:?}", started.elapsed());
    result
}

#[test]
fn a_working_plugin_draws_what_it_says() {
    let mut plugin = Plugin::load(&module(CIRCLE), "circle.wasm").unwrap();
    assert_eq!((plugin.info().id.as_str(), plugin.info().name.as_str()), ("circle", "Circle"));
    assert_eq!(plugin.file(), "circle.wasm");
    let (bands, waves) = frame();
    for bass in [0.7, 0.2] {
        let data = FrameData { bands: &bands, bass, waveforms: &waves, ..Default::default() };
        let commands = plugin.draw(&data, 800.0, 600.0).unwrap();
        assert_eq!(
            commands,
            vec![DrawCommand::FillCircle {
                center: Point { x: 400.0, y: 300.0 },
                radius: bass * 100.0,
                color: Rgba { r: 255, g: 0, b: 0, a: 255 },
            }]
        );
    }
}

#[test]
fn an_endless_loop_runs_out_of_fuel_quickly() {
    let fault = within(Duration::from_secs(1), || draw_once(&module("(loop $forever (br $forever)) (i32.const 0)")));
    assert_eq!(fault, Err(Fault::OutOfTime));
    assert_eq!(
        Fault::OutOfTime.notice("Circle"),
        "The “Circle” visualizer stopped working and was turned off: it took too long to draw a frame."
    );
}

#[test]
fn an_endless_start_function_stops_loading() {
    let bytes = module_with(CIRCLE, "(func $spin (loop $l (br $l))) (start $spin)", 1, 1);
    let loaded = within(Duration::from_secs(2), || Plugin::load(&bytes, "spin.wasm"));
    assert_eq!(loaded.err(), Some(LoadError::Failed(Fault::OutOfTime)));
}

#[test]
fn a_trap_is_a_fault() {
    assert!(matches!(draw_once(&module("unreachable")), Err(Fault::Trapped(_))));
    // Reading far outside memory traps too.
    assert!(matches!(draw_once(&module("(i32.load (i32.const -4))")), Err(Fault::Trapped(_))));
}

#[test]
fn endless_recursion_is_stopped() {
    let bytes = module_with("(call $deeper) (i32.const 0)", "(func $deeper (call $deeper))", 1, 1);
    let fault = within(Duration::from_secs(2), || draw_once(&bytes));
    assert!(matches!(fault, Err(Fault::Trapped(_) | Fault::OutOfTime)), "{fault:?}");
}

#[test]
fn memory_is_capped() {
    // Declaring more memory than the cap: refused at load.
    let greedy = module_with(CIRCLE, "", 1, 1000);
    assert!(matches!(Plugin::load(&greedy, "greedy.wasm"), Err(LoadError::Failed(_))));
    // Growing past the cap at run time fails (and this module traps when it does).
    let grower =
        module("(if (i32.eq (memory.grow (i32.const 1000)) (i32.const -1)) (then unreachable)) (i32.const 8192)");
    assert!(matches!(draw_once(&grower), Err(Fault::Trapped(_))));
}

#[test]
fn a_bad_drawing_is_a_fault() {
    let nan = CIRCLE.replace("(f32.div (local.get $w) (f32.const 2))", "(f32.const nan)");
    assert_eq!(draw_once(&module(&nan)), Err(Fault::InvalidDrawing(InvalidDrawing::NotANumber { command: 1 })));
    let outside = draw_once(&module("(i32.const -8)"));
    assert_eq!(outside, Err(Fault::InvalidDrawing(InvalidDrawing::OutsideMemory)));
    let huge = draw_once(&module("(i32.store (i32.const 8192) (i32.const -1)) (i32.const 8192)"));
    assert!(matches!(huge, Err(Fault::InvalidDrawing(InvalidDrawing::TooLarge(_)))), "{huge:?}");
    assert!(Fault::InvalidDrawing(InvalidDrawing::OutsideMemory).notice("X").contains("cannot show"));
}

#[test]
fn modules_that_are_not_plugins_are_refused() {
    assert!(matches!(Plugin::load(b"definitely not wasm", "junk.wasm"), Err(LoadError::NotWebAssembly(_))));
    assert!(matches!(Plugin::load(&[], "empty.wasm"), Err(LoadError::NotWebAssembly(_))));
    let mut truncated = module(CIRCLE);
    truncated.truncate(truncated.len() / 2);
    assert!(matches!(Plugin::load(&truncated, "cut.wasm"), Err(LoadError::NotWebAssembly(_))));

    let wrong_version = module_with(CIRCLE, "", 2, 1);
    assert_eq!(Plugin::load(&wrong_version, "v2.wasm").err(), Some(LoadError::WrongVersion { found: 2, expected: 1 }));
    let no_draw = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "tunebox_abi_version") (result i32) i32.const 1))"#,
    )
    .unwrap();
    assert_eq!(Plugin::load(&no_draw, "half.wasm").err(), Some(LoadError::MissingExport("tunebox_info")));
    let no_memory = wat::parse_str("(module)").unwrap();
    assert_eq!(Plugin::load(&no_memory, "none.wasm").err(), Some(LoadError::MissingExport("memory")));

    let wants_files = wat::parse_str(r#"(module (import "wasi_snapshot_preview1" "fd_write" (func)))"#).unwrap();
    assert_eq!(
        Plugin::load(&wants_files, "wasi.wasm").err(),
        Some(LoadError::HasImports("wasi_snapshot_preview1.fd_write".into()))
    );

    let bad_id = module_with(CIRCLE, r#"(data (i32.const 200) "Bad Id")"#, 1, 1);
    assert!(matches!(Plugin::load(&bad_id, "id.wasm"), Err(LoadError::BadInfo(_))));
}

#[test]
fn a_folder_scan_loads_plugins_and_reports_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b-circle.wasm"), module(CIRCLE)).unwrap();
    std::fs::write(dir.path().join("a-junk.wasm"), b"junk").unwrap();
    std::fs::write(dir.path().join("c-same-id.WASM"), module(CIRCLE)).unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"not a plug-in").unwrap();
    std::fs::create_dir(dir.path().join("folder.wasm")).unwrap();
    let report = load_folder(dir.path());
    let loaded: Vec<&str> = report.plugins.iter().map(|p| p.file()).collect();
    assert_eq!(loaded, ["b-circle.wasm"]);
    let problems: Vec<&str> = report.problems.iter().map(|(file, _)| file.as_str()).collect();
    assert_eq!(problems, ["a-junk.wasm", "c-same-id.WASM"]);
    assert_eq!(report.problems[1].1, LoadError::DuplicateId("b-circle.wasm".into()));

    let missing = load_folder(&dir.path().join("no such folder"));
    assert!(missing.plugins.is_empty() && missing.problems.is_empty());
}

/// How long spending a frame's whole fuel budget takes on this computer. Run with
/// `cargo test -p tunebox-plugins --release -- --ignored --nocapture measure`.
#[test]
#[ignore]
fn measure_a_full_fuel_budget() {
    let bytes = module("(loop $forever (br $forever)) (i32.const 0)");
    let mut plugin = Plugin::load(&bytes, "loop.wasm").unwrap();
    let (bands, waves) = frame();
    let data = FrameData { bands: &bands, waveforms: &waves, ..Default::default() };
    let runs = 50;
    let started = Instant::now();
    for _ in 0..runs {
        assert_eq!(plugin.draw(&data, 800.0, 600.0), Err(Fault::OutOfTime));
    }
    println!("a full fuel budget: {:?} per frame", started.elapsed() / runs);
}
