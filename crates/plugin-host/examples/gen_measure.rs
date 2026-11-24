use std::path::{Path, PathBuf};
use std::time::Instant;

use craft_plugin::{Call, FrameDesc, Layout, Pixel, RenderCx, Value, Window, native_target};
use craft_plugin_host::{Bundle, Host, Limits, Plugin};
use serde_json::json;
use wasmtime::{Caller, Config, Engine, Linker, Memory, Module, Store, TypedFunc};

const SIZE: [u32; 2] = [3840, 2160];
const SPRITE: usize = 200;

struct NoHost;
impl Host for NoHost {}

fn probe() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/probe/target")
}

fn plugin(wasm: bool) -> Plugin {
    let dir = std::env::temp_dir().join("craft-gen-measure").join(if wasm { "wasm" } else { "native" });
    std::fs::create_dir_all(&dir).unwrap();
    let (built, file) = match wasm {
        true => (probe().join("wasm32-unknown-unknown/release"), "probe.wasm".to_string()),
        false => (probe().join("release"), libloading::library_filename("probe").into_string().unwrap()),
    };
    let _ = std::fs::remove_file(dir.join(&file));
    std::fs::copy(built.join(&file), dir.join(&file)).unwrap();
    let entry = if wasm { json!({"wasm": file}) } else { json!({"native": {native_target(): file}}) };
    let manifest = json!({"id": "bench.gen-measure", "name": "Gen Measure", "version": "1", "entry": entry});
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
    Plugin::load(&Bundle::open(&dir).unwrap(), Default::default(), dir.join("data"), Limits::default()).unwrap()
}

fn cx(whole: bool, drawn: bool) -> RenderCx {
    let mut cx = RenderCx { size: SIZE, scale: 1.0, frame_rate: 24.0, layer_size: [f64::from(SIZE[0]), f64::from(SIZE[1])], ..Default::default() };
    cx.params.insert("whole".into(), Value::Bool(whole));
    cx.params.insert("drawn".into(), Value::Bool(drawn));
    cx
}

fn layer() -> Vec<Pixel> {
    vec![[0.25, 0.5, 0.75, 1.0]; SIZE[0] as usize * SIZE[1] as usize]
}

fn host_ms(plugin: &Plugin, whole: bool, drawn: bool) -> (f64, Vec<Pixel>) {
    let cx = cx(whole, drawn);
    let layout = if whole { Layout::whole_frame() } else { Layout::pointwise() };
    let layout = if drawn { layout.ignores_input() } else { layout };
    let mut pixels = layer();
    let start = Instant::now();
    let area = plugin.render("sprite", &cx, &layout, None, &mut pixels, || NoHost).unwrap();
    assert_eq!(pixels.len(), area.width() as usize * area.height() as usize);
    (start.elapsed().as_secs_f64() * 1000.0, pixels)
}

fn sprite_part(pixels: &[Pixel]) -> Vec<Pixel> {
    let (width, left, top) = (SIZE[0] as usize, (SIZE[0] as usize - SPRITE) / 2, (SIZE[1] as usize - SPRITE) / 2);
    (top..top + SPRITE).flat_map(|y| pixels[y * width + left..y * width + left + SPRITE].to_vec()).collect()
}

struct Guest {
    store: Store<()>,
    memory: Memory,
    call: TypedFunc<(u32, u32, u32, u32, u32, u32), ()>,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    out: u32,
    pixels: u32,
}

impl Guest {
    fn new(bytes: usize) -> Guest {
        let mut config = Config::new();
        config.epoch_interruption(true);
        config.relaxed_simd_deterministic(true);
        let engine = Engine::new(&config).unwrap();
        let module = Module::from_file(&engine, probe().join("wasm32-unknown-unknown/release/probe.wasm")).unwrap();
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap("craft", "host_call", |_: Caller<'_, ()>, _: u32, _: u32, _: u32, _: u32, _: u32, _: u32| -> wasmtime::Result<()> {
                wasmtime::bail!("the bench serves no host calls")
            })
            .unwrap();
        let mut store = Store::new(&engine, ());
        store.set_epoch_deadline(u64::MAX / 2);
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let call = instance.get_typed_func(&mut store, "craft_plugin_call").unwrap();
        let alloc: TypedFunc<u32, u32> = instance.get_typed_func(&mut store, "craft_plugin_alloc").unwrap();
        let free = instance.get_typed_func(&mut store, "craft_plugin_free").unwrap();
        let out = alloc.call(&mut store, 8).unwrap();
        let pixels = alloc.call(&mut store, u32::try_from(bytes).unwrap()).unwrap();
        assert_ne!(pixels, 0, "the guest holds the frame");
        Guest { store, memory, call, alloc, free, out, pixels }
    }

    fn place(&mut self, at: usize, len: usize) -> &mut [u8] {
        &mut self.memory.data_mut(&mut self.store)[self.pixels as usize + at..][..len]
    }

    fn call(&mut self, request: &[u8], bytes: usize) {
        let len = u32::try_from(request.len()).unwrap();
        let at = self.alloc.call(&mut self.store, len).unwrap();
        self.memory.write(&mut self.store, at as usize, request).unwrap();
        self.memory.write(&mut self.store, self.out as usize, &[0; 8]).unwrap();
        self.call.call(&mut self.store, (0, at, len, self.pixels, u32::try_from(bytes).unwrap(), self.out)).unwrap();
        self.free.call(&mut self.store, (at, len)).unwrap();
        let mut reply = [0u32; 2];
        self.memory.read(&self.store, self.out as usize, bytemuck::cast_slice_mut(&mut reply)).unwrap();
        let text = self.memory.data(&self.store)[reply[0] as usize..][..reply[1] as usize].to_vec();
        assert!(text.starts_with(b"{\"ok\""), "the render failed: {}", String::from_utf8_lossy(&text));
        self.free.call(&mut self.store, (reply[0], reply[1])).unwrap();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Way {
    Today,
    NoCopyIn,
    DrawnRows,
}

struct Split {
    copy_in: f64,
    call: f64,
    copy_out: f64,
}

struct Sandboxed {
    guest: Guest,
    whole: bool,
    request: Vec<u8>,
    input: Vec<Pixel>,
    frame_bytes: usize,
}

impl Sandboxed {
    fn new(whole: bool) -> Sandboxed {
        let (width, height) = (SIZE[0], SIZE[1]);
        let input = whole.then_some(Window { size: SIZE, first_row: 0, rows: height });
        let frame = FrameDesc { width, height, origin: [0, 0], input, spread: false };
        let request = serde_json::to_vec(&Call::Render { effect: "sprite".into(), cx: cx(whole, false), frame }).unwrap();
        let frame_bytes = width as usize * height as usize * size_of::<Pixel>();
        let staged = if whole { 2 * frame_bytes } else { frame_bytes };
        Sandboxed { guest: Guest::new(staged), whole, request, input: layer(), frame_bytes }
    }

    fn output_at(&self) -> usize {
        if self.whole { self.frame_bytes } else { 0 }
    }

    fn run(&mut self, way: Way, out: &mut [Pixel]) -> Split {
        let n = self.frame_bytes;
        let start = Instant::now();
        let input: &[u8] = bytemuck::cast_slice(&self.input);
        match (way, self.whole) {
            (Way::Today, false) => self.guest.place(0, n).copy_from_slice(input),
            (Way::Today, true) => {
                self.guest.place(0, n).copy_from_slice(input);
                self.guest.place(n, n).fill(0);
            }
            (_, true) => self.guest.place(n, n).fill(0),
            (_, false) => {}
        }
        let copied = Instant::now();
        let staged = if self.whole { 2 * n } else { n };
        self.guest.call(&self.request, staged);
        let called = Instant::now();
        let row = SIZE[0] as usize * size_of::<Pixel>();
        let rows = match way {
            Way::DrawnRows => {
                let top = (SIZE[1] as usize - SPRITE) / 2;
                top * row..(top + SPRITE) * row
            }
            _ => 0..n,
        };
        let from = self.output_at() + rows.start;
        read_back(&mut self.guest, from, &mut bytemuck::cast_slice_mut(out)[rows]);
        let done = Instant::now();
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
        Split { copy_in: ms(start, copied), call: ms(copied, called), copy_out: ms(called, done) }
    }
}

fn read_back(guest: &mut Guest, from: usize, bytes: &mut [u8]) {
    bytes.copy_from_slice(guest.place(from, bytes.len()));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [phase, whole, rounds] = &args[..] {
        return instructions(phase, whole == "whole", rounds.parse().unwrap());
    }
    let rounds: usize = args.first().map_or(7, |n| n.parse().unwrap());
    let native = plugin(false);
    let wasm = plugin(true);
    let ways = [(Way::Today, "today"), (Way::NoCopyIn, "no copy-in"), (Way::DrawnRows, "no copy-in, drawn rows out")];
    println!("3840x2160 f32 RGBA, 200x200 sprite; minimum of {rounds} alternating rounds");
    for whole in [false, true] {
        let mut sandboxed = Sandboxed::new(whole);
        let mut out = layer();
        let mut best = [[f64::MAX; 3]; 3];
        let mut host_best = [f64::MAX; 4];
        for _ in 0..rounds {
            let (ms, pixels) = host_ms(&native, whole, false);
            host_best[0] = host_best[0].min(ms);
            let reference = pixels;
            let (ms, pixels) = host_ms(&wasm, whole, false);
            host_best[1] = host_best[1].min(ms);
            assert!(pixels == reference, "the sandboxed host render matches native");
            for (slot, plugin) in [(2, &native), (3, &wasm)] {
                let (ms, pixels) = host_ms(plugin, whole, true);
                host_best[slot] = host_best[slot].min(ms);
                assert!(pixels == sprite_part(&reference), "the render that ignores its input and reports what it drew is the sprite");
            }
            for (slot, (way, _)) in ways.iter().enumerate() {
                let split = sandboxed.run(*way, &mut out);
                best[slot] = [best[slot][0].min(split.copy_in), best[slot][1].min(split.call), best[slot][2].min(split.copy_out)];
                let shown = (SIZE[1] as usize - SPRITE) / 2 * SIZE[0] as usize;
                let sprite = shown..shown + SPRITE * SIZE[0] as usize;
                assert!(out[sprite.clone()] == reference[sprite], "the sandboxed sprite rows match native");
            }
        }
        println!("layout {}:", if whole { "whole_frame (host zeroes the output)" } else { "pointwise (guest clears)" });
        let [native_best, wasm_best, native_drawn, wasm_drawn] = host_best;
        println!("  host render, {} threads: native {native_best:.2} ms (no copies), sandboxed {wasm_best:.2} ms", rayon::current_num_threads());
        println!("  host render, ignores input + drawn: native {native_drawn:.2} ms, sandboxed {wasm_drawn:.2} ms");
        for (slot, (_, name)) in ways.iter().enumerate() {
            let [copy_in, call, copy_out] = best[slot];
            let total = copy_in + call + copy_out;
            println!(
                "  split, one band, {name:<27} copy-in {copy_in:7.2}  call {call:7.2}  copy-out {copy_out:7.2}  sum {total:7.2} ms  copies {:4.1} %",
                100.0 * (copy_in + copy_out) / total
            );
        }
    }
}

fn instructions(phase: &str, whole: bool, rounds: usize) {
    let mut sandboxed = Sandboxed::new(whole);
    let mut out = layer();
    let n = sandboxed.frame_bytes;
    let input = sandboxed.input.clone();
    let from = sandboxed.output_at();
    let host = phase.strip_prefix("host-").map(|kind| plugin(kind.starts_with("wasm")));
    for _ in 0..rounds {
        match phase {
            "copy-in" => sandboxed.guest.place(0, n).copy_from_slice(bytemuck::cast_slice(&input)),
            "zero" => sandboxed.guest.place(from, n).fill(0),
            "call" => {
                let staged = if whole { 2 * n } else { n };
                let request = sandboxed.request.clone();
                sandboxed.guest.call(&request, staged);
            }
            "copy-out" => read_back(&mut sandboxed.guest, from, bytemuck::cast_slice_mut(&mut out)),
            "rows-out" => {
                let row = SIZE[0] as usize * size_of::<Pixel>();
                let top = (SIZE[1] as usize - SPRITE) / 2;
                read_back(&mut sandboxed.guest, from + top * row, &mut bytemuck::cast_slice_mut(&mut out)[top * row..(top + SPRITE) * row]);
            }
            "host-native" | "host-wasm" => drop(host_ms(host.as_ref().unwrap(), whole, false)),
            "host-native-drawn" | "host-wasm-drawn" => drop(host_ms(host.as_ref().unwrap(), whole, true)),
            "none" => {}
            other => panic!("unknown phase {other}"),
        }
    }
    std::hint::black_box(&out);
}
