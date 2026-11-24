use std::path::Path;
use std::time::Instant;

use craft_plugin::{Call, Layout, Pixel, RenderCx, Value, native_target};
use craft_plugin_host::{Bundle, Host, Limits, Plugin};
use serde_json::json;

struct NoHost;
impl Host for NoHost {}

const EFFECT: &str = "host-calls";
const SIZE: [u32; 2] = [8, 8];

fn plugin(wasm: bool) -> Plugin {
    let dir = std::env::temp_dir().join("craft-host-calls").join(if wasm { "wasm" } else { "native" });
    std::fs::create_dir_all(&dir).unwrap();
    let built = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/probe/target");
    let (from, file) = match wasm {
        true => (built.join("wasm32-unknown-unknown/release"), "probe.wasm".to_string()),
        false => (built.join("release"), libloading::library_filename("probe").into_string().unwrap()),
    };
    let _ = std::fs::remove_file(dir.join(&file));
    std::fs::copy(from.join(&file), dir.join(&file)).unwrap();
    let entry = if wasm { json!({"wasm": file}) } else { json!({"native": {native_target(): file}}) };
    let manifest = json!({"id": "bench.host-calls", "name": "Host Calls", "version": "1", "entry": entry});
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
    Plugin::load(&Bundle::open(&dir).unwrap(), Default::default(), dir.join("data"), Limits::default()).unwrap()
}

fn render_ms(plugin: &Plugin, calls: u64) -> f64 {
    let mut cx = RenderCx { size: SIZE, scale: 1.0, frame_rate: 24.0, layer_size: [8.0, 8.0], ..Default::default() };
    cx.params.insert("calls".into(), Value::Number(calls as f64));
    let layout: Layout = plugin.call(&Call::Layout { effect: EFFECT.into(), cx: cx.clone() }, None, &mut NoHost).unwrap();
    let mut pixels: Vec<Pixel> = vec![[0.5; 4]; 64];
    let start = Instant::now();
    plugin.render(EFFECT, &cx, &layout, None, &mut pixels, || NoHost).unwrap();
    start.elapsed().as_secs_f64() * 1000.0
}

fn minimum(plugin: &Plugin, calls: u64, reps: usize) -> f64 {
    (0..reps).map(|_| render_ms(plugin, calls)).fold(f64::MAX, f64::min)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let reps: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(30);
    if let [_, mode, calls] = &args[..] {
        let plugin = plugin(mode == "wasm");
        let calls: u64 = calls.parse().unwrap();
        (0..reps).for_each(|_| drop(render_ms(&plugin, calls)));
        return;
    }
    println!("one render of the probe's host-calls effect (8x8); `calls` cache_has host calls inside it; minimum of {reps}");
    println!("{:<7} {:>7} {:>11} {:>12} {:>12}", "mode", "calls", "render ms", "ns per call", "net ns/call");
    for (name, wasm) in [("wasm", true), ("native", false)] {
        let plugin = plugin(wasm);
        let base = minimum(&plugin, 0, reps);
        println!("{name:<7} {:>7} {base:>11.4}", 0);
        for calls in [1u64, 100, 10_000] {
            let ms = minimum(&plugin, calls, reps);
            println!("{name:<7} {calls:>7} {ms:>11.4} {:>12.0} {:>12.0}", ms * 1e6 / calls as f64, (ms - base) * 1e6 / calls as f64);
        }
    }
}
