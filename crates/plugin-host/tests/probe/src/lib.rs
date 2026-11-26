use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use craft_plugin::math;
use craft_plugin::prelude::*;
use serde_json::json;

struct Invert;

impl Effect for Invert {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("invert", "Invert", "Probe").param(Param::slider("amount", "Amount", 1.0, 0.0..=1.0))
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let amount = cx.f("amount") as f32;
        frame.map(|mut pixel| {
            pixel[..3].iter_mut().for_each(|channel| *channel += (1.0 - 2.0 * *channel) * amount);
            pixel
        });
        Ok(())
    }
}

struct Renamed;

impl Effect for Renamed {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("renamed", "Renamed Parameter", "Probe").version(1).param(Param::slider("strength", "Strength", 0.5, 0.0..=1.0))
    }

    fn migrate(&self, from: u32, values: &mut BTreeMap<String, Value>) {
        if from < 1
            && let Some(amount) = values.remove("amt")
        {
            values.insert("strength".into(), amount);
        }
    }
}

struct Tinted;

impl Effect for Tinted {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("tinted", "Tinted By Layer Data", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let red = cx.layer_data("tint")?.and_then(|bytes| bytes.first().copied()).map_or(0.0, |byte| f32::from(byte) / 255.0);
        frame.map(|pixel| [red, pixel[1], pixel[2], pixel[3]]);
        Ok(())
    }
}

fn write_data() -> Result<()> {
    let request: serde_json::Value = serde_json::from_slice(&host::read_file(Root::Bundle, "data.json")?)?;
    let place: Place = serde_json::from_value(request["place"].clone())?;
    let key = request["key"].as_str().ok_or("data.json names no key")?;
    if request["null"].as_bool() == Some(true) {
        host::command("layer.newNull", json!({"name": "Marker"}))?;
    }
    match (&request["value"], request["size"].as_u64()) {
        (serde_json::Value::String(text), _) => host::set_data(place, key, text.as_bytes())?,
        (_, Some(size)) => host::set_data(place, key, &vec![b'x'; size as usize])?,
        _ => host::remove_data(place, key)?,
    }
    let read = host::data(place, key)?.map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    host::write_file(Root::Data, "data-read.json", json!(read).to_string().as_bytes())
}

struct HostCalls;

impl Effect for HostCalls {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("host-calls", "Host Calls", "Probe").param(Param::slider("calls", "Calls", 0.0, 0.0..=1_000_000.0))
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, _frame: &mut Frame) -> Result<()> {
        (0..cx.f("calls") as u64).for_each(|_| {
            let _ = host::cache_has("k");
        });
        Ok(())
    }
}

struct Drift {
    declared: bool,
}

impl Effect for Drift {
    fn spec(&self) -> EffectSpec {
        let spec = EffectSpec::new(if self.declared { "drift" } else { "silent-drift" }, "Drift", "Probe");
        if self.declared { spec.time_dependent() } else { spec }
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let lift = cx.time as f32 * 0.5;
        frame.map(|mut pixel| {
            pixel[1] += lift;
            pixel
        });
        Ok(())
    }
}

struct FastMaths;

impl Effect for FastMaths {
    fn spec(&self) -> EffectSpec {
        let spec = EffectSpec::new("fast-maths", "Fast Maths", "Probe").param(Param::checkbox("cheaper", "Cheaper", false));
        spec.param(Param::checkbox("exp2", "Exp2", false))
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        if cx.bool("exp2") {
            frame.map(|pixel| [math::fast::exp2(pixel[0]), 0.0, 0.0, 1.0]);
            return Ok(());
        }
        if cx.bool("cheaper") {
            frame.map(|pixel| {
                let x = pixel[0];
                [math::fast::exp(x), math::fast::sin(x), math::fast::cos(x), math::fast::ln(x.abs())]
            });
            return Ok(());
        }
        frame.map(|pixel| {
            let x = pixel[0];
            [math::exp(x), math::sin(x), math::atan2(x, 2.0 - x), math::pow(x.abs(), 2.4)]
        });
        Ok(())
    }
}

struct Scale;

impl Effect for Scale {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("scale", "Scale", "Probe").param(Param::custom("level", "Level", "1"))
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let level: f32 = cx.text("level").parse().map_err(|_| Error::from("the level is not a number"))?;
        frame.map(|mut pixel| {
            pixel[..3].iter_mut().for_each(|channel| *channel *= level);
            pixel
        });
        Ok(())
    }

    fn controls(&self, instance: u64, _param: &str, value: &mut String, ui: &mut Ui) {
        wait_while_held();
        ui.label(&format!("Level {value}"));
        ui.label(&format!("Instance {instance}"));
        if ui.button("Halve") {
            let level: f64 = value.parse().unwrap_or(1.0);
            *value = (level / 2.0).to_string();
        }
    }
}

const GPU_KERNELS: &str = r#"
@compute @workgroup_size(16, 16)
fn probe_invert(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!inside_out(id.xy)) { return; }
    let c = texel(src, vec2<i32>(id.xy));
    store(id.xy, vec4<f32>(c.rgb + (vec3<f32>(c.a) - 2.0 * c.rgb) * pass_f(0u), c.a));
}

@compute @workgroup_size(16, 16)
fn probe_down(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!inside_out(id.xy)) { return; }
    let p = vec2<i32>(id.xy) * 2;
    let sum = texel(src, p) + texel(src, p + vec2<i32>(1, 0)) + texel(src, p + vec2<i32>(0, 1)) + texel(src, p + vec2<i32>(1, 1));
    store(id.xy, sum * 0.25);
}

@compute @workgroup_size(16, 16)
fn probe_up(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!inside_out(id.xy)) { return; }
    let c = texel(src, vec2<i32>(id.xy));
    let soft = sample_uv(aux, out_uv(id.xy));
    store(id.xy, c + (soft - c) * pass_f(0u));
}
"#;

struct GpuInvert;

impl Effect for GpuInvert {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("gpu-invert", "GPU Invert", "Probe").param(Param::slider("amount", "Amount", 1.0, 0.0..=1.0)).shader(GPU_KERNELS)
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let pass = PlanPass::new("probe_invert", PlanImage::Source, PlanImage::Output).floats(&[cx.f("amount") as f32]);
        Layout::pointwise().gpu(Plan::new().pass(pass))
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let amount = cx.f("amount") as f32;
        frame.map(|mut pixel| {
            let alpha = pixel[3];
            pixel[..3].iter_mut().for_each(|channel| *channel += (alpha - 2.0 * *channel) * amount);
            pixel
        });
        Ok(())
    }
}

const FILL: [f32; 3] = [0.25, 0.5, 0.75];

const FILL_SHADER: &str = r#"
@vertex
fn vs_probe_fill(@location(0) corner: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(corner, 0.5, 1.0);
}

@fragment
fn fs_probe_fill() -> @location(0) vec4<f32> {
    return vec4<f32>(pass_f(0u), pass_f(1u), pass_f(2u), 1.0);
}
"#;

struct GpuCachedFill;

impl Effect for GpuCachedFill {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("gpu-cached-fill", "GPU Cached Fill", "Probe").param(Param::slider("reach", "Reach", 3.0, 1.0..=3.0)).shader(FILL_SHADER)
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let reach = cx.f("reach") as f32;
        let corners: Vec<u8> = [-1.0, -1.0, reach, -1.0, -1.0, reach].iter().flat_map(|v: &f32| v.to_le_bytes()).collect();
        if host::cache_put("corners", &corners).is_err() {
            return Layout::pointwise();
        }
        let mut plan = Plan::new();
        let corners = plan.buffer(PlanData::Cache("corners".into()));
        let triangle = plan.buffer(PlanData::Indices(vec![0, 1, 2]));
        Layout::pointwise()
            .gpu(plan.pass(PlanPass::draw("probe_fill", PlanImage::Output).vertices(corners, &[2]).indices(triangle).floats(&FILL).clear()))
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.pixels.fill([FILL[0], FILL[1], FILL[2], 1.0]);
        Ok(())
    }
}

struct GpuFill;

impl Effect for GpuFill {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("gpu-fill", "GPU Fill", "Probe")
            .param(Param::slider("last_index", "Last Index", 2.0, 0.0..=9.0))
            .param(Param::checkbox("onto_scratch", "Onto Scratch", false))
            .shader(FILL_SHADER)
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let mut plan = Plan::new();
        let corners = plan.buffer(PlanData::Floats(vec![-1.0, -1.0, 3.0, -1.0, -1.0, 3.0]));
        let triangle = plan.buffer(PlanData::Indices(vec![0, 1, cx.f("last_index") as u32]));
        let fill = PlanPass::draw("probe_fill", PlanImage::Output).vertices(corners, &[2]).indices(triangle).floats(&FILL);
        if !cx.bool("onto_scratch") {
            return Layout::pointwise().gpu(plan.pass(fill.clear()));
        }
        let frame = cx.size;
        let (a, b) = (plan.scratch(frame), plan.scratch(frame));
        let from_alpha = [[0.0, 0.0, 0.0, 0.5], [0.0, 0.0, 0.0, 0.25], [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]];
        let identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let plan = plan
            .pass(PlanPass::matrix(PlanImage::Source, a, from_alpha))
            .pass(PlanPass::matrix(a, b, identity))
            .pass(PlanPass { out: a, ..fill.count(0) })
            .pass(PlanPass::matrix(a, PlanImage::Output, identity));
        Layout::pointwise().gpu(plan)
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.pixels.fill([FILL[0], FILL[1], FILL[2], 1.0]);
        Ok(())
    }
}

const MATTE_SHADER: &str = r#"
@compute @workgroup_size(16, 16)
fn probe_matte(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!inside_out(id.xy)) { return; }
    store(id.xy, texel(aux, vec2<i32>(id.xy)));
}
"#;

struct GpuMatte;

impl Effect for GpuMatte {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("gpu-matte", "GPU Matte", "Probe").param(Param::layer("matte", "Matte")).shader(MATTE_SHADER)
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        let mut plan = Plan::new();
        let matte = plan.layer("matte");
        Layout::pointwise().gpu(plan.pass(PlanPass::new("probe_matte", PlanImage::Source, PlanImage::Output).aux(matte)))
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        match cx.layer("matte") {
            Some(matte) => frame.pixels.copy_from_slice(matte.pixels()),
            None => frame.pixels.fill([0.0; 4]),
        }
        Ok(())
    }
}

type Taps = fn(&dyn Fn(isize, isize) -> Pixel, isize, isize) -> Vec<(Pixel, f32)>;

struct Passes;

impl Effect for Passes {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("passes", "Passes", "Probe")
    }

    fn pass(&self, cx: &Cx, pass: &Pass, out: &mut Frame) -> Result<()> {
        let src = pass.src;
        if pass.kernel == "probe_aux_depth" {
            let aux = cx.aux(&[Channel::Depth, Channel::Depth])?.ok_or("probe_aux_depth reads depth")?;
            for (y, row) in out.rows_mut() {
                for (x, pixel) in row.iter_mut().enumerate() {
                    *pixel = [aux.at(&Channel::Depth, x, y).map_or(-1.0, |depth| depth[0]), aux.width as f32, aux.height as f32, 1.0];
                }
            }
            return Ok(());
        }
        let sum = |pixels: &[(Pixel, f32)]| -> Pixel { std::array::from_fn(|c| pixels.iter().map(|(pixel, weight)| pixel[c] * weight).sum()) };
        if pass.kernel == "probe_tall_aux" {
            let aux = pass.aux.ok_or("probe_tall_aux reads aux")?;
            for (y, row) in out.rows_mut() {
                for (x, pixel) in row.iter_mut().enumerate() {
                    let (x, y) = (x as isize, y as isize);
                    let tall = sum(&(-3..=3).map(|dy| (aux.at(x, y + dy), (dy + 4) as f32)).collect::<Vec<_>>());
                    *pixel = std::array::from_fn(|c| src.at(x, y)[c] + tall[c]);
                }
            }
            return Ok(());
        }
        let kernel: Taps = match pass.kernel {
            "probe_red" => {
                out.pixels.fill([1.0, 0.0, 0.0, 1.0]);
                return Ok(());
            }
            "probe_nothing" => return Ok(()),
            "probe_down" => |at, x, y| [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| (at(2 * x + dx, 2 * y + dy), 0.25)).to_vec(),
            "probe_up" => |at, x, y| [0, 1].map(|dy| (at(x / 2, y / 2 + dy), 0.5)).to_vec(),
            "probe_tall" => |at, x, y| (-3..=3).map(|dy| (at(x, y + dy), (dy + 4) as f32)).collect(),
            other => return Err(format!("no probe kernel `{other}`").into()),
        };
        for (y, row) in out.rows_mut() {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = sum(&kernel(&|x, y| src.at(x, y), x as isize, y as isize));
            }
        }
        Ok(())
    }
}

const SOFTEN_PADDING: u32 = 4;

struct GpuSoften;

impl GpuSoften {
    fn half(frame: [usize; 2]) -> [usize; 2] {
        [frame[0].div_ceil(2), frame[1].div_ceil(2)]
    }
}

impl Effect for GpuSoften {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("gpu-soften", "GPU Soften", "Probe").param(Param::slider("strength", "Strength", 0.75, 0.0..=1.0)).shader(GPU_KERNELS)
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let pad = 2 * SOFTEN_PADDING;
        let [w, h] = GpuSoften::half([(cx.size[0] + pad) as usize, (cx.size[1] + pad) as usize]);
        let mut plan = Plan::new();
        let half = plan.scratch([w as u32, h as u32]);
        let plan = plan
            .pass(PlanPass::new("probe_down", PlanImage::Source, half))
            .pass(PlanPass::new("probe_up", PlanImage::Source, PlanImage::Output).aux(half).floats(&[cx.f("strength") as f32]));
        Layout::whole_frame().padded(SOFTEN_PADDING).gpu(plan)
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let (width, height, input) = (frame.input.width, frame.input.height, frame.input);
        let at = |x: i64, y: i64| input.at(x as isize, y as isize);
        let [hw, hh] = GpuSoften::half([width, height]);
        let mut half = vec![[0.0f32; 4]; hw * hh];
        for y in 0..hh {
            for x in 0..hw {
                let (sx, sy) = (2 * x as i64, 2 * y as i64);
                let block = [at(sx, sy), at(sx + 1, sy), at(sx, sy + 1), at(sx + 1, sy + 1)];
                half[y * hw + x] = std::array::from_fn(|c| (block[0][c] + block[1][c] + block[2][c] + block[3][c]) * 0.25);
            }
        }
        let clamped = |x: i64, y: i64| half[y.clamp(0, hh as i64 - 1) as usize * hw + x.clamp(0, hw as i64 - 1) as usize];
        let strength = cx.f("strength") as f32;
        let x0 = frame.origin[0];
        for (y, read, row) in frame.rows() {
            for (x, (pixel, source)) in (x0..).zip(row.iter_mut().zip(read)) {
                let fx = (x as f32 + 0.5) / width as f32 * hw as f32 - 0.5;
                let fy = (y as f32 + 0.5) / height as f32 * hh as f32 - 0.5;
                let (x0, y0) = (fx.floor(), fy.floor());
                let (tx, ty) = (fx - x0, fy - y0);
                let (xi, yi) = (x0 as i64, y0 as i64);
                let (a, b, c, d) = (clamped(xi, yi), clamped(xi + 1, yi), clamped(xi, yi + 1), clamped(xi + 1, yi + 1));
                for k in 0..4 {
                    let top = a[k] + (b[k] - a[k]) * tx;
                    let bottom = c[k] + (d[k] - c[k]) * tx;
                    let soft = top + (bottom - top) * ty;
                    pixel[k] = source[k] + (soft - source[k]) * strength;
                }
            }
        }
        Ok(())
    }
}

const KERNEL_PADDING: u32 = 6;

struct HostKernels;

impl Effect for HostKernels {
    fn spec(&self) -> EffectSpec {
        let kernels = [
            "blur",
            "box blur",
            "resample",
            "blend",
            "curves",
            "convolve",
            "full convolve",
            "warp",
            "matrix",
            "glow",
            "to linear",
            "to sRGB",
            "brightness curve",
            "dither",
            "light alpha",
            "graded glow",
            "tiny blur",
            "far warp",
            "bicubic zoom",
            "full over",
            "no over",
            "tiny curve range",
            "huge resample",
            "huge to linear",
            "to linear, negatives linear",
            "to sRGB, negatives linear",
            "dilate",
            "erode",
            "directional blur",
            "bilateral",
            "noise grain",
            "hsl range mask",
            "splat",
            "flow",
        ];
        let modes = ["over", "add", "subtract", "multiply", "screen", "overlay", "darken", "lighten", "difference", "screen hdr"];
        EffectSpec::new("host-kernels", "Host Kernels", "Probe")
            .param(Param::popup("kernel", "Kernel", &kernels, 0))
            .param(Param::popup("mode", "Mode", &modes, 0))
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let frame = cx.padded(KERNEL_PADDING).size;
        let (source, output) = (PlanImage::Source, PlanImage::Output);
        let mut plan = Plan::new();
        let (half, soft) = (plan.scratch(frame.map(|side| side.div_ceil(2))), plan.scratch(frame));
        let blurred = PlanPass::blur(source, soft, [3.0, 5.0]);
        let curve: Vec<Pixel> = (0..17).map(|i| i as f32 / 16.0).map(|t| [t * t, t.sqrt(), 1.0 - t, t]).collect();
        let plan = match cx.index("kernel") {
            0 => plan.pass(PlanPass::blur(source, output, [4.0, 2.5])),
            1 => plan.pass(PlanPass::box_blur(source, output, [2.5, 0.0])),
            2 => plan.pass(PlanPass::resample(source, half, Filter::Bilinear)).pass(PlanPass::resample(half, output, Filter::Bicubic)),
            3 => plan.pass(blurred).pass(PlanPass::blend_images(source, soft, output, BlendMode::ALL[cx.index("mode").min(9)], 0.8)),
            4 => plan.pass(PlanPass::curves(source, output, &curve)),
            5 => plan.pass(PlanPass::convolve(source, output, craft_plugin::Taps::Separable { x: vec![-0.5, 2.0, -0.5], y: vec![0.25, 0.5, 0.25] })),
            6 => plan.pass(PlanPass::convolve(
                source,
                output,
                craft_plugin::Taps::Full { width: 3, weights: vec![0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0] },
            )),
            7 => plan.pass(blurred).pass(PlanPass::warp(source, soft, output, [6.0, -4.0])),
            8 => {
                plan.pass(PlanPass::matrix(source, output, [[0.0, 0.0, 1.0, 0.0], [0.3, 0.6, 0.1, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]]))
            }
            10 => plan.pass(PlanPass::to_linear(source, output)),
            11 => plan.pass(PlanPass::to_srgb(source, output)),
            12 => plan.pass(PlanPass::brightness_curve(source, output, Measure::MaxChannel, vec![0.0, 0.0, 0.3, 1.2], 1.5)),
            13 => plan.pass(PlanPass::dither(source, output, 5, [-3, 9])),
            14 => plan
                .pass(PlanPass::matrix(source, soft, [[3.0, 0.0, 0.0, 0.0], [0.0, 3.0, 0.0, 0.0], [0.0, 0.0, 3.0, 0.0], [0.0; 4]]))
                .pass(PlanPass::light_alpha(soft, output)),
            15 => plan
                .pass(blurred)
                .pass(PlanPass::brightness_curve(soft, soft, Measure::Luma, vec![0.0, 0.1, 0.9], 1.0))
                .pass(PlanPass::blend_images(soft, source, soft, BlendMode::Screen, 0.9))
                .pass(PlanPass::to_srgb(soft, soft))
                .pass(PlanPass::dither(soft, soft, 2, [0, 0]))
                .pass(PlanPass::light_alpha(soft, output)),
            16 => plan.pass(PlanPass::blur(source, output, [1e-30, 0.0])),
            17 => {
                let far = PlanPass::matrix(source, soft, [[1e33, 0.0, 0.0, 1e33], [0.0; 4], [0.0; 4], [0.0; 4]]);
                plan.pass(far).pass(PlanPass::warp(source, soft, output, [4096.0, 0.0]))
            }
            18 => plan.pass(PlanPass::host(HostKernel::Resample { filter: Filter::Bicubic, scale: Some([0.5, 1.0]) }, source, output)),
            19 | 20 => {
                let (top, amount) = if cx.index("kernel") == 19 { ([0.0, 0.0, 1.0, 0.0], 1.0) } else { ([1e38, 0.0, 0.0, 0.0], 0.0) };
                let top_image = PlanPass::matrix(source, soft, [top, [0.0; 4], [0.0; 4], [0.0, 0.0, 0.0, 1.0]]);
                plan.pass(top_image).pass(PlanPass::blend_images(source, soft, output, BlendMode::Over, amount))
            }
            21 => {
                let ramp = (0..4096).map(|i| i as f32 / 4095.0 * 1e-36).collect();
                plan.pass(PlanPass::brightness_curve(source, output, Measure::MaxChannel, ramp, 1e-36))
            }
            22 => plan.pass(PlanPass::host(HostKernel::Resample { filter: Filter::Bilinear, scale: Some([2.0, 2.0]) }, source, output)),
            23 => plan.pass(PlanPass::to_linear(source, output)),
            24 => plan.pass(PlanPass::transfer(source, output, Encoding::Linear, Negatives::Linear)),
            25 => plan.pass(PlanPass::transfer(source, output, Encoding::Srgb, Negatives::Linear)),
            26 => plan.pass(PlanPass::dilate(source, output, [3, 1])),
            27 => plan.pass(PlanPass::erode(source, output, [1, 3])),
            28 => plan.pass(PlanPass::directional_blur(source, output, 30.0, 9.0)),
            29 => plan.pass(PlanPass::bilateral(source, output, 3, 0.15)),
            30 => plan.pass(PlanPass::noise(soft, 4.0, 3, 7, [-3, 9])).pass(PlanPass::blend_images(source, soft, output, BlendMode::Multiply, 0.6)),
            31 => plan.pass(PlanPass::hsl_range(source, soft, [300.0, 60.0], [0.1, 1.0], [0.05, 0.9], 0.15)).pass(PlanPass::blend_images(
                source,
                soft,
                output,
                BlendMode::Multiply,
                1.0,
            )),
            32 => {
                let dab = |i: u32| Dab {
                    opacity: 0.8,
                    hardness: (i % 4) as f32 * 0.25,
                    ..Dab::new(
                        [(i * 37 % (frame[0] + 20)) as f32 - 10.0, (i * 53 % (frame[1] + 20)) as f32 - 10.0],
                        1.0 + (i % 13) as f32,
                        [0.9, 0.5, 0.2],
                    )
                };
                plan.pass(PlanPass::splat(source, output, (0..400).map(dab).collect()))
            }
            33 => {
                let mut plan = plan;
                let (moved, flow) = (plan.scratch(frame), plan.scratch(frame));
                plan.pass(blurred)
                    .pass(PlanPass::warp(source, soft, moved, [3.0, -2.0]))
                    .pass(PlanPass::flow(source, moved, flow))
                    .pass(PlanPass::warp(moved, flow, output, [1.0, 1.0]))
            }
            _ => {
                let quarter = frame.map(|side| side.div_ceil(4));
                let (small, blurred) = (plan.scratch(quarter), plan.scratch(quarter));
                plan.pass(PlanPass::resample(source, small, Filter::Bilinear))
                    .pass(PlanPass::blur(small, blurred, [1.5; 2]))
                    .pass(PlanPass::resample(blurred, soft, Filter::Bilinear))
                    .pass(PlanPass::blend_images(source, soft, output, BlendMode::Screen, 0.7))
            }
        };
        Layout::whole_frame().padded(KERNEL_PADDING).passes(plan)
    }
}

struct ShiftDown;

impl Effect for ShiftDown {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("shift-down", "Shift Down", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::neighbourhood(1)
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let (input, x0, width) = (frame.input, frame.origin[0], frame.width);
        for (y, row) in frame.rows_mut() {
            match y.checked_sub(1).and_then(|above| input.row(above as isize)) {
                Some(above) => row.copy_from_slice(&above[x0..x0 + width]),
                None => row.fill([0.0; 4]),
            }
        }
        Ok(())
    }
}

const LIFT_ROWS: i32 = 8;

struct Lift;

impl Effect for Lift {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("lift", "Lift", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise().maps_input()
    }

    fn input(&self, _cx: &Cx, out: Rect) -> Rect {
        Rect { top: out.top + LIFT_ROWS, bottom: out.bottom + LIFT_ROWS, ..out }
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let (input, x0, width) = (frame.input, frame.origin[0], frame.width);
        for (y, row) in frame.rows_mut() {
            match input.row(y as isize + LIFT_ROWS as isize) {
                Some(below) => row.copy_from_slice(&below[x0..x0 + width]),
                None => row.fill([0.0; 4]),
            }
        }
        Ok(())
    }
}

struct Swap;

const SWAP: [[f32; 4]; 4] = [[0.0, 0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

impl Effect for Swap {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("swap", "Swap", "Probe").param(Param::checkbox("planned", "Planned", false))
    }

    fn layout(&self, cx: &Cx) -> Layout {
        if cx.bool("planned") {
            Layout::pointwise().passes(Plan::new().pass(PlanPass::matrix(PlanImage::Source, PlanImage::Output, SWAP)))
        } else {
            Layout::pointwise()
        }
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.map(|[r, g, b, a]| [b, g, r, a]);
        Ok(())
    }
}

struct SmallSwap;

impl Effect for SmallSwap {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("small-swap", "Small Swap", "Probe")
    }

    fn layout(&self, cx: &Cx) -> Layout {
        match cx.size[0] <= 512 {
            true => Layout::pointwise().passes(Plan::new().pass(PlanPass::matrix(PlanImage::Source, PlanImage::Output, SWAP))),
            false => Layout::pointwise(),
        }
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        Swap.render(cx, frame)
    }
}

struct Streak;

impl Effect for Streak {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("streak", "Streak", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise().padded_sides(5, 0, 2, 0)
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.map(|pixel| pixel);
        Ok(())
    }
}

struct Sprite;

const SPRITE: usize = 200;

impl Effect for Sprite {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("sprite", "Sprite", "Probe")
            .param(Param::checkbox("whole", "Whole Frame", false))
            .param(Param::checkbox("drawn", "Ignores Input, Reports Drawn", false))
            .param(Param::checkbox("fails", "Last Band Fails", false))
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let layout = if cx.bool("whole") { Layout::whole_frame() } else { Layout::pointwise() };
        if cx.bool("drawn") { layout.ignores_input() } else { layout }
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let side = SPRITE as i32;
        let (left, top) = ((cx.size[0] as i32 - side) / 2, (cx.size[1] as i32 - side) / 2);
        let (x0, width, drawn) = (frame.origin[0] as i32, frame.width as i32, cx.bool("drawn"));
        let cleared = cx.bool("whole") || drawn;
        for (y, row) in frame.rows_mut() {
            if !cleared {
                row.fill([0.0; 4]);
            }
            let y = y as i32;
            if !(top..top + side).contains(&y) {
                continue;
            }
            let v = (y - top) as f32 / SPRITE as f32;
            for x in left.max(x0)..(left + side).min(x0 + width) {
                let u = (x - left) as f32 / SPRITE as f32;
                row[(x - x0) as usize] = [u, v, 1.0 - u * v, 1.0];
            }
        }
        if drawn {
            frame.drawn(Rect { left, top, right: left + side, bottom: top + side });
        }
        if cx.bool("fails") && frame.origin[1] + frame.height >= cx.size[1] as usize {
            return Err("the last band failed".into());
        }
        Ok(())
    }
}

struct Plain;

impl Effect for Plain {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("plain", "Plain", "Probe")
    }
}

struct Switches;

impl Effect for Switches {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("switches", "Switches", "Probe")
            .params([
                Param::popup("mode", "Mode", &["Plain", "Soft"], 0),
                Param::slider("softness", "Softness", 4.0, 0.0..=10.0).visible_when("mode", Is::Equals(1.0)),
                Param::checkbox("manual", "Manual", false),
                Param::slider("amount", "Amount", 1.0, 0.0..=10.0).enabled_when("manual", Is::On),
                Param::button("reset", "Reset"),
                Param::button("ask", "Ask Then Reset").visible_when("mode", Is::Equals(0.0)),
            ])
            .preset("Calm", [("softness", craft_plugin::Value::Number(2.0)), ("amount", craft_plugin::Value::Number(5.0))])
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let softness = cx.f("softness") as f32;
        frame.map(|pixel| [pixel[0] * softness, pixel[1], pixel[2], pixel[3]]);
        Ok(())
    }

    fn button(&self, press: &Press) -> Result<()> {
        let zero = |param: &str| {
            host::command("prop.set", json!({"layer": press.layer, "path": format!("effects/@{}/{param}", press.instance), "value": 0.0}))
        };
        zero("softness")?;
        if press.param == "ask" {
            host::form(Form::new("Go on?"))?;
        }
        zero("amount").map(drop)
    }
}

struct Tuned;

impl Effect for Tuned {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("tuned", "Tuned", "Probe").group("Fine Tuning", [Param::slider("detail", "Detail", 2.0, 0.0..=10.0)])
    }
}

struct Checks;

impl Effect for Checks {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("checks", "Checks", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let x0 = frame.origin[0];
        for (y, row) in frame.rows_mut() {
            for (x, pixel) in (x0..).zip(row) {
                *pixel = if (x / 2 + y / 2) % 2 == 0 { [1.0; 4] } else { [0.0, 0.0, 0.0, 1.0] };
            }
        }
        Ok(())
    }
}

params! {
    struct RampSettings {
        ramp: Gradient = gradient("Ramp", &[(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])]),
        at: f64 = slider("At", 0.5, 0.0..=1.0),
    }
}

struct Ramp;

impl Effect for Ramp {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("ramp", "Ramp", "Probe").params(RampSettings::params())
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let settings = RampSettings::read(cx);
        let colour = settings.ramp.sample(settings.at);
        frame.map(|_| colour);
        Ok(())
    }
}

struct Halve;

impl Effect for Halve {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("halve", "Halve", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        for (_, row) in frame.rows_in_place() {
            row.iter_mut().for_each(|pixel| pixel[..3].iter_mut().for_each(|channel| *channel *= 0.5));
        }
        Ok(())
    }
}

struct Tally;

impl Effect for Tally {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("tally", "Tally", "Probe")
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.map(|pixel| pixel);
        let before = host::cache_get("tally").and_then(|bytes| <[u8; 8]>::try_from(bytes.as_slice()).ok()).map_or(0, u64::from_le_bytes);
        host::cache_put("tally", &(before + (frame.width * frame.height) as u64).to_le_bytes())
    }
}

struct Rewind {
    declared: bool,
}

impl Effect for Rewind {
    fn spec(&self) -> EffectSpec {
        let spec = EffectSpec::new(if self.declared { "rewind" } else { "silent-rewind" }, "Rewind", "Probe").time_dependent();
        if self.declared { spec.reads_other_frames() } else { spec }
    }

    fn layout(&self, cx: &Cx) -> Layout {
        let mut plan = Plan::new();
        let (earlier, flow) = (plan.source_at(cx.time - 0.5), plan.scratch(cx.size));
        let plan = plan.pass(PlanPass::flow(PlanImage::Source, earlier, flow)).pass(PlanPass::warp(earlier, flow, PlanImage::Output, [1.0, 1.0]));
        Layout::whole_frame().passes(plan)
    }
}

struct ReadInputs;

impl Effect for ReadInputs {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("read-inputs", "Read Inputs", "Probe").param(Param::layer("matte", "Matte"))
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let masks = cx.masks();
        let matte = cx.layer("matte").ok_or("no matte layer")?;
        let earlier = cx.source_at(cx.time - 1.0).ok_or("no earlier frame")?;
        frame.pixels[0] = [masks.len() as f32, masks[0].points[1][0] as f32, matte.at(1, 0)[0], earlier.at(0, 0)[3]];
        Ok(())
    }
}

struct Recurse;

impl Effect for Recurse {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("recurse", "Recurse", "Probe")
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let request = craft_plugin::FrameRequest { time: 0.0, scale: 1.0, ..Default::default() };
        let refused = host::frame(request).err().is_some_and(|e| e.0.contains("cannot render a frame while it renders"));
        frame.pixels[0][0] = if refused { 1.0 } else { 2.0 };
        Ok(())
    }
}

struct Explode;

impl Effect for Explode {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("explode", "Explode", "Probe")
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let missing: usize = frame.pixels.len() + 3;
        frame.pixels[missing] = [1.0; 4];
        Ok(())
    }
}

struct Memo;

impl Effect for Memo {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("memo", "Memo", "Probe")
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        host::cache_put("memo", &[1, 2, 3])?;
        frame.pixels[0][0] = 1.0;
        Ok(())
    }
}

struct Report;

impl Effect for Report {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("report", "Report", "Probe")
            .param(Param::slider("level", "Level", 10.0, 0.0..=100.0))
            .param(Param::point("spot", "Spot", [0.5, 0.5]))
            .param(Param::curve("tone", "Tone", &[[0.0, 0.0], [1.0, 1.0]]))
            .param(Param::curve("grade", "Grade", &[[0.0, 0.2], [1.0, 1.0]]).channels())
            .reads_param_history()
            .preset("Loud", [("level", craft_plugin::Value::Number(90.0)), ("spot", craft_plugin::Value::List(vec![0.25, 0.75]))])
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let at_zero = cx.at(0.0).ok_or("no parameters at time 0")?;
        let history = cx.param_history();
        let grade = cx.curves("grade");
        let report = json!({"instance": cx.instance.to_string(), "tone": cx.curve("tone").at(0.5), "grade": grade.iter().map(|c| c.at(0.0)).collect::<Vec<_>>(), "now": cx.f("level"), "at_zero": at_zero.f("level"), "at_zero_time": at_zero.time, "history": history.map(|h| h.to_string())});
        host::write_file(Root::Data, &format!("report-{}.json", cx.instance), report.to_string().as_bytes())?;
        frame.pixels[0] = [cx.f("level") as f32, at_zero.f("level") as f32, at_zero.time as f32, f32::from(u8::from(history.is_some()))];
        Ok(())
    }
}

struct Ranges;

impl Effect for Ranges {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("ranges", "Ranges", "Probe")
            .param(Param::slider("gain", "Gain", 1.0, 0.0..=1000.0).soft(0.01..=10.0).log())
            .param(Param::percent("mix", "Mix", 50.0).range(-100.0..=300.0))
            .param(Param::slider("plain", "Plain", 5.0, 0.0..=20.0))
    }
}

struct SceneCamera;

impl Effect for SceneCamera {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("scene-camera", "Scene Camera", "Probe").reads_scene()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let camera = cx.scene().and_then(|scene| scene.camera).ok_or("no scene camera")?;
        let [x, y, z] = camera.position.map(|c| c as f32);
        frame.pixels[0] = [x, y, z, 1.0];
        frame.pixels[1] = [camera.zoom as f32, 0.0, 0.0, 1.0];
        Ok(())
    }
}

struct Placed;

impl Effect for Placed {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("placed", "Placed", "Probe").param(Param::layer("other", "Other")).reads_scene().reads_other_frames()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let placed = cx.placement("other")?;
        let camera = cx.scene().and_then(|scene| scene.camera).ok_or("no scene camera")?;
        let later = cx.scene_at(1.0)?.camera.ok_or("no scene camera at 1 s")?;
        let focus = camera.focus.unwrap_or_default();
        let m = placed.matrix.map(|row| row.map(|v| v as f32));
        frame.pixels[..4].copy_from_slice(&[
            [m[0][0], m[0][1], m[0][2], m[0][3]],
            [m[1][0], m[1][1], m[1][2], m[1][3]],
            [m[2][0], m[2][1], m[2][2], 1.0],
            [m[2][3], m[3][0], m[3][1], 1.0],
        ]);
        frame.pixels[4] = [m[3][2], m[3][3], placed.opacity as f32, 1.0];
        frame.pixels[5] = [placed.size[0] as f32, placed.size[1] as f32, 0.0, 1.0];
        frame.pixels[6] = [focus.distance as f32, focus.aperture as f32, focus.blur as f32, 1.0];
        let [x, y, z] = later.position.map(|c| c as f32);
        frame.pixels[7] = [x, y, z, 1.0];
        Ok(())
    }
}

struct SceneLater;

impl Effect for SceneLater {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("scene-later", "Scene Later", "Probe").reads_scene()
    }

    fn render(&self, cx: &Cx, _frame: &mut Frame) -> Result<()> {
        cx.scene_at(1.0).map(|_| ())
    }
}

struct AuxDepth;

impl Effect for AuxDepth {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("aux-depth", "Aux Depth", "Probe").reads_aux([Channel::Depth, Channel::Normal])
    }

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::pointwise()
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        let aux = cx.aux(&[Channel::Depth, Channel::Normal])?;
        let x0 = frame.origin[0];
        for (y, row) in frame.rows_mut() {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = match &aux {
                    None => [0.0, 0.0, 1.0, 1.0],
                    Some(aux) => {
                        let depth = aux.at(&Channel::Depth, x0 + x, y).map_or(-1.0, |depth| depth[0]);
                        let facing = aux.at(&Channel::Normal, x0 + x, y).map_or(-1.0, |normal| -normal[2]);
                        [depth / 1000.0, facing, 0.0, 1.0]
                    }
                };
            }
        }
        Ok(())
    }
}

struct AuxUndeclared;

impl Effect for AuxUndeclared {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("aux-undeclared", "Aux Undeclared", "Probe").reads_aux([Channel::Depth])
    }

    fn render(&self, cx: &Cx, _frame: &mut Frame) -> Result<()> {
        cx.aux(&[Channel::Depth, Channel::Uv]).map(|_| ())
    }
}

struct HalfDone;

impl Effect for HalfDone {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("half-done", "Half Done", "Probe")
    }

    fn render(&self, cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.pixels.fill([9.0; 4]);
        if frame.origin[1] + frame.height < cx.size[1] as usize {
            return Ok(());
        }
        Err("gave up half way".into())
    }
}

struct MarkBand;

impl Effect for MarkBand {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("mark-band", "Mark Band", "Probe")
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let input = frame.input;
        let (top, rows) = (input.first_row as f32, (input.pixels.len() / input.width.max(1)) as f32);
        for (y, row) in frame.rows_mut() {
            row.fill([top, rows, y as f32, 1.0]);
        }
        Ok(())
    }
}

struct Spin;

impl Effect for Spin {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("spin", "Spin", "Probe")
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        let mut n = 0u64;
        loop {
            n = n.wrapping_add(1);
            frame.pixels[0][0] = n as f32;
        }
    }
}

struct Aim;

impl Effect for Aim {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("aim", "Aim", "Probe")
            .param(Param::point("centre", "Centre", [0.5, 0.5]))
            .param(Param::angle("turn", "Turn", 0.0))
            .param(Param::slider("reach", "Reach", 10.0, 0.0..=100.0))
            .group("Box", [Param::point("min", "Box Min", [0.25, 0.25]), Param::point("max", "Box Max", [0.5, 0.5])])
            .param(Param::custom("pin", "Pin", "0,0"))
            .param(Param::slider("crowd", "Crowd", 0.0, 0.0..=200000.0))
    }

    fn overlay(&self, cx: &Cx, overlay: &mut Overlay) {
        let [x, y] = cx.point("centre");
        let turn = cx.f("turn").to_radians();
        overlay.point(cx, "centre");
        overlay.angle("turn", [x, y], [x + 8.0 * turn.cos(), y + 8.0 * turn.sin()]);
        overlay.radius("reach", [x, y], cx.f("reach") * 2.0, 2.0);
        overlay.rect(cx, "Box/min", "Box/max");
        overlay.custom("pin", "pin", [1.0, 2.0]).named("Pin");
        overlay.space(Space::Comp).line(&[[0.0, 0.0], [4.0, 4.0]]);
        for n in 0..cx.f("crowd") as usize {
            overlay.circle([n as f64, 0.0], 1.0);
        }
    }

    fn drag(&self, _cx: &Cx, key: &str, to: [f64; 2], value: &mut String) {
        if key == "pin" {
            *value = format!("{},{}", to[0], to[1]);
        }
    }
}

#[derive(Default)]
struct Stamp {
    seen: Vec<String>,
    null: Option<u64>,
}

impl Tool for Stamp {
    fn event(&mut self, event: &ToolEvent, overlay: &mut Overlay) -> Result<()> {
        if event.key == "hold" {
            wait_while_held();
            return Ok(());
        }
        let [x, y] = event.comp;
        let pen = event.pen.map_or(String::new(), |pen| {
            let end = if pen.eraser { " eraser" } else { "" };
            format!(" pen {:.2} {:.2} {:.2}{end} at {:.2}", pen.pressure, pen.tilt[0], pen.tilt[1], event.pressure)
        });
        let line = format!("{:?} {x:.1} {y:.1}{pen}", event.kind);
        match event.kind {
            ToolEventKind::Down => {
                self.seen = vec![line];
                let reply = host::command("layer.newNull", json!({"name": "Stamp"}))?;
                self.null = reply["layer"].as_u64();
            }
            ToolEventKind::Move | ToolEventKind::Up => {
                self.seen.push(line);
                if let Some(layer) = self.null {
                    host::command("prop.set", json!({"layer": layer, "path": "transform/position", "value": [x, y]}))?;
                }
            }
            _ => self.seen.push(line),
        }
        for (row, line) in self.seen.iter().enumerate() {
            overlay.text([x, y + 12.0 * row as f64], line);
        }
        Ok(())
    }
}

static SHARED: AtomicU32 = AtomicU32::new(0);

fn shared() -> String {
    format!("shared {}", SHARED.load(Ordering::Relaxed))
}

struct Peek;

impl Effect for Peek {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("peek", "Peek", "Probe")
    }

    fn overlay(&self, _cx: &Cx, overlay: &mut Overlay) {
        overlay.text([0.0, 0.0], &shared());
    }
}

struct Census;

impl Tool for Census {
    fn event(&mut self, _event: &ToolEvent, overlay: &mut Overlay) -> Result<()> {
        overlay.text([0.0, 0.0], &shared());
        Ok(())
    }
}

struct Shared;

impl Panel for Shared {
    fn ui(&mut self, ui: &mut Ui) {
        ui.label(&shared());
    }
}

#[derive(Default)]
struct Counter {
    clicks: u32,
    speed: f64,
    calls: u32,
}

impl Panel for Counter {
    fn ui(&mut self, ui: &mut Ui) {
        self.calls += 1;
        if ui.button("Count") {
            self.clicks += 1;
        }
        ui.slider("Speed", &mut self.speed, 0.0..=10.0);
        ui.label(&format!("{} clicks at speed {}", self.clicks, self.speed));
        ui.label(&format!("calls {}", self.calls));
    }
}

static OPENED: std::sync::Mutex<Option<usize>> = std::sync::Mutex::new(None);

#[derive(Default)]
struct Files {
    selected: Option<usize>,
    built: usize,
}

impl Panel for Files {
    fn ui(&mut self, ui: &mut Ui) {
        let picked = ui.list("files", 100_000, &mut self.selected, |row| {
            self.built += 1;
            Item::new(&format!("file {row}"))
        });
        if let Some(row) = picked.activated {
            *OPENED.lock().unwrap_or_else(|e| e.into_inner()) = Some(row);
            if let Err(error) = host::show_panel("inspector") {
                ui.error(&error.0);
            }
        }
        ui.label(&format!("built {}", self.built));
    }
}

#[derive(Default)]
struct Inspector;

impl Panel for Inspector {
    fn ui(&mut self, ui: &mut Ui) {
        let opened = *OPENED.lock().unwrap_or_else(|e| e.into_inner());
        ui.label(&opened.map_or("nothing open".into(), |row| format!("inspecting file {row}")));
        if ui.button("Close") && host::close_panel("inspector", None).is_err() {
            ui.error("the app did not close the inspector");
        }
    }
}

#[derive(Default)]
struct Watch {
    seen: Vec<String>,
}

impl Panel for Watch {
    fn ui(&mut self, ui: &mut Ui) {
        for topic in [Topic::Selection, Topic::Time, Topic::Comp, Topic::Project, Topic::Edit] {
            if let Some(event) = ui.on(topic) {
                self.seen.push(serde_json::to_string(event).unwrap_or_default());
            }
        }
        let mut keys = Vec::new();
        ui.canvas("keys", [100.0, 40.0], |c| keys.extend(c.keys.iter().map(|key| format!("key {} shift {}", key.name, key.modifiers.shift))));
        self.seen.extend(keys);
        for line in &self.seen {
            ui.mono(line);
        }
    }
}

fn wait_while_held() {
    while host::read_file(Root::Bundle, "hold").is_ok() {
        host::notify("held");
    }
}

fn relayed() -> Result<String> {
    let request: serde_json::Value = serde_json::from_slice(&host::read_file(Root::Bundle, "relay.json")?)?;
    if request["frame"].is_object() {
        let image = host::frame(craft_plugin::FrameRequest { time: 0.0, scale: 1.0, ..Default::default() })?;
        return Ok(format!("{}x{}", image.width, image.height));
    }
    match request["task"].as_str() {
        Some(task) => host::start(task, request["args"].clone()).map(drop)?,
        None => host::command(request["command"].as_str().ok_or("relay.json names no command")?, request["params"].clone()).map(drop)?,
    };
    Ok("relayed".into())
}

#[derive(Default)]
struct Relay {
    said: String,
}

impl Panel for Relay {
    fn ui(&mut self, ui: &mut Ui) {
        if ui.button("Relay") {
            wait_while_held();
            self.said = relayed().unwrap_or_else(|error| error.to_string());
        }
        ui.label(&self.said);
    }
}

#[derive(Default)]
struct Ticker {
    ticks: u32,
    once: bool,
}

impl Panel for Ticker {
    fn ui(&mut self, ui: &mut Ui) {
        if ui.every("tick", 100) {
            self.ticks += 1;
        }
        if !self.once && ui.after("once", 250) {
            self.once = true;
        }
        ui.label(&format!("ticks {} once {}", self.ticks, self.once));
    }
}

#[derive(Default)]
struct Lookout {
    seen: std::sync::Mutex<(String, u32)>,
}

impl Effect for Lookout {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("lookout", "Lookout", "Probe").param(Param::custom("selection", "Selection", "")).param(Param::custom("ticks", "Ticks", ""))
    }

    fn controls(&self, _instance: u64, param: &str, _value: &mut String, ui: &mut Ui) {
        let mut seen = self.seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if param == "selection" {
            if let Some(AppEvent::Selection { layers, .. }) = ui.on(Topic::Selection) {
                seen.0 = format!("selected {layers:?}");
            }
            ui.label(&seen.0);
        } else {
            if ui.every("tick", 50) {
                seen.1 += 1;
            }
            ui.label(&format!("ticks {}", seen.1));
        }
    }
}

fn make() -> Plugin {
    Plugin::new()
        .effect(Invert)
        .effect(Tinted)
        .effect(Renamed)
        .effect(Drift { declared: true })
        .effect(Drift { declared: false })
        .effect(Rewind { declared: true })
        .effect(Rewind { declared: false })
        .effect(FastMaths)
        .effect(Scale)
        .effect(ShiftDown)
        .effect(Lift)
        .effect(Tally)
        .effect(Checks)
        .effect(Ramp)
        .effect(Halve)
        .effect(Streak)
        .effect(Plain)
        .effect(Sprite)
        .effect(ReadInputs)
        .effect(SceneCamera)
        .effect(Placed)
        .effect(SceneLater)
        .effect(Spin)
        .effect(HalfDone)
        .effect(MarkBand)
        .effect(Recurse)
        .effect(Memo)
        .effect(Explode)
        .effect(Report)
        .effect(Ranges)
        .effect(GpuInvert)
        .effect(GpuSoften)
        .effect(GpuCachedFill)
        .effect(HostKernels)
        .effect(Swap)
        .effect(SmallSwap)
        .effect(HostCalls)
        .effect(Passes)
        .effect(GpuFill)
        .effect(GpuMatte)
        .effect(Aim)
        .effect(Tuned)
        .effect(Switches)
        .effect(Lookout::default())
        .effect(AuxDepth)
        .effect(AuxUndeclared)
        .tool("stamp", "Stamp", Stamp::default())
        .options("stamp-options")
        .panel("counter", "Counter", Counter::default())
        .panel("watch", "Watch", Watch::default())
        .panel("ticker", "Ticker", Ticker::default())
        .panel("relay", "Relay", Relay::default())
        .panel("files", "Files", Files::default())
        .icon("list")
        .panel("inspector", "Inspector", Inspector)
        .icon("sliders")
        .place(PanelPlace::Float)
        .size(PanelSize::Large)
        .panel("tree", "Tree", Tree::default())
        .icon("list")
        .panel("status", "Status", StatusItem::default())
        .icon("bell")
        .place(PanelPlace::Status)
        .panel("stamp-options", "Stamp Options", StampOptions::default())
        .place(PanelPlace::Bar)
        .panel_each("notes", "Notes", Notes::default)
        .icon("text")
        .place(PanelPlace::Bottom)
        .panel("controls", "Controls", Controls::default())
        .icon("sliders")
        .panel("pad", "Pad", Pad::default())
        .effect(Peek)
        .tool("census", "Census", Census)
        .panel("shared", "Shared", Shared)
        .command("bump", "Bump", || {
            SHARED.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .command("edit", "Edit Document", || {
            let reply = host::command("layer.newNull", json!({"name": "From Plugin"}))?;
            host::write_file(Root::Data, "last-reply.json", reply.to_string().as_bytes())
        })
        .menu("Layer/Probe Tools")
        .context(Context::Layers)
        .toolbar()
        .icon("wand")
        .command("read", "Read Document", || host::command("comp.info", json!({})).map(|_| ()))
        .shortcut("Cmd+Shift+J")
        .command("ask", "Ask For Settings", || {
            let form = Form::new("Settings")
                .field(Param::slider("radius", "Radius", 10.0, 0.0..=100.0))
                .field(Param::checkbox("soft", "Soft", false))
                .field(Param::popup("mode", "Mode", &["Fast", "Best"], 0))
                .field(Param::text("label", "Label", "none"));
            let answer = host::form(form)?.map_or("cancelled".to_string(), |values| json!(values).to_string());
            host::write_file(Root::Data, "form.json", answer.as_bytes())
        })
        .shortcut("Alt+Shift+F11")
        .command("ask-layer", "Ask For A Layer", || host::form(Form::new("Layer").field(Param::layer("in", "Input"))).map(drop))
        .command("swap-clipboard", "Swap Clipboard", || {
            let before = host::clipboard()?;
            host::set_clipboard(&format!("probe saw {before}"))
        })
        .command("batch-edit", "Edit In A Batch", || {
            let steps = json!([{"command": "layer.newNull", "params": {"name": "One"}}, {"command": "layer.newNull", "params": {"name": "Two"}}]);
            host::command("engine.batch", json!({"label": "Two Nulls", "steps": steps})).map(|_| ())
        })
        .command("save-copy", "Save A Copy", || {
            let path = String::from_utf8(host::read_file(Root::Bundle, "user-target.txt")?).map_err(|e| e.to_string())?;
            host::command("file.saveCopy", json!({"path": path.trim()})).map(|_| ())
        })
        .command("escape-data", "Escape Data", || host::write_file(Root::Data, "../outside.txt", b"x"))
        .command("write-bundle", "Write Bundle", || host::write_file(Root::Bundle, "plugin.json", b"{}"))
        .command("write-user", "Write User", || {
            let path = String::from_utf8(host::read_file(Root::Bundle, "user-target.txt")?).map_err(|e| e.to_string())?;
            host::write_file(Root::User, path.trim(), b"written by plugin")
        })
        .command("read-user", "Read User", || {
            let path = String::from_utf8(host::read_file(Root::Bundle, "user-target.txt")?).map_err(|e| e.to_string())?;
            let bytes = host::read_file(Root::User, path.trim())?;
            host::write_file(Root::Data, "read.txt", &bytes)
        })
        .command("read-kinds", "Read Kinds", || {
            let reads = [(Root::Data, "absent.txt"), (Root::Data, "gone/deeper.txt"), (Root::Data, "."), (Root::Data, "not found: cache")];
            let kinds: Vec<String> = reads
                .iter()
                .map(|(root, path)| match host::read_file(*root, path) {
                    Ok(_) => "read".to_string(),
                    Err(error) => format!("{:?}|{}", error.kind, error.message),
                })
                .collect();
            host::write_file(Root::Data, "kinds.txt", kinds.join("\n").as_bytes())
        })
        .command("put-table", "Put A Lookup Table", || host::cache_put("table", &[4, 5, 6]))
        .command("panic", "Panic", || panic!("oops"))
        .command("approve-itself", "Approve Itself", || host::command("plugin.approve", json!({"id": "test.gate-self"})).map(|_| ()))
        .command("read-inside-edit", "Read Inside An Edit", || {
            let steps = json!([{"command": "comp.info", "params": {}}, {"command": "layer.newNull", "params": {}}]);
            host::command("engine.batch", json!({"steps": steps})).map(|_| ())
        })
        .command("open-website", "Open Website", || host::command("help.website", json!({})).map(|_| ()))
        .command("write-link", "Write Through A Link", || host::write_file(Root::Data, "link.txt", b"x"))
        .command("greet", "Greet", || {
            host::notify("3 layers renamed");
            Ok(())
        })
        .command("relay", "Relay A Command", || {
            let request: serde_json::Value = serde_json::from_slice(&host::read_file(Root::Bundle, "relay.json")?)?;
            let command = request["command"].as_str().ok_or("relay.json names no command")?;
            host::command(command, request["params"].clone()).map(|_| ())
        })
        .command("data", "Write Data", write_data)
        .command("run-script", "Run Script", || host::command("script.run", json!({"code": "1"})).map(|_| ()))
        .command("start-count", "Start Counting", || {
            let id = host::start("count", json!({"steps": 3}))?;
            host::write_file(Root::Data, "task-id.txt", id.to_string().as_bytes())
        })
        .task("count", "Count", |task: &Task| {
            let steps = task.args()["steps"].as_u64().ok_or("count needs `steps`")?;
            for step in 0..steps {
                task.progress(step as f64 / steps as f64, &format!("step {step}"))?;
            }
            Ok(json!({"counted": steps}))
        })
        .task("forever", "Forever", |_task: &Task| {
            let mut n = 0u64;
            loop {
                n = std::hint::black_box(n.wrapping_add(1));
            }
        })
        .task("patient", "Patient", |task: &Task| {
            loop {
                task.progress(0.5, "waiting")?;
                std::hint::black_box(0);
            }
        })
        .command("pick-and-read", "Pick And Read", || {
            let request = craft_plugin::PickFile { title: "Choose".into(), ..Default::default() };
            let path = host::pick_file(request)?.ok_or("nothing was picked")?;
            let bytes = host::read_file(Root::User, &path)?;
            host::write_file(Root::Data, "picked.txt", &bytes)?;
            host::write_file(Root::User, &path, b"overwritten")
        })
        .command("pick-and-save", "Pick And Save", || {
            let request = craft_plugin::PickFile { save: true, title: "Save".into(), ..Default::default() };
            let path = host::pick_file(request)?.ok_or("nothing was picked")?;
            host::write_file(Root::User, &path, b"saved")
        })
        .command("pick-save-target", "Pick A Save Target", || {
            host::pick_file(craft_plugin::PickFile { save: true, title: "Save".into(), ..Default::default() }).map(drop)
        })
        .task("pick", "Pick From A Task", |_task: &Task| host::pick_file(craft_plugin::PickFile::default()).map(|path| json!(path)))
        .task("frame", "Read A Frame", |task: &Task| {
            let args = task.args();
            let layer = args["layer"].as_str().map(String::from);
            let request = craft_plugin::FrameRequest {
                comp: args["comp"].as_str().map(String::from),
                layer,
                time: args["time"].as_f64().unwrap_or(0.0),
                scale: args["scale"].as_f64().unwrap_or(1.0),
                ids: args["ids"].as_bool().unwrap_or(false),
                source: args["source"].as_bool().unwrap_or(false),
            };
            let image = host::frame(request)?;
            let center = image.at(image.width as isize / 2, image.height as isize / 2);
            Ok(json!({"width": image.width, "height": image.height, "center": center}))
        })
        .task("audio", "Read Audio", |task: &Task| {
            let args = task.args();
            let layer = args["layer"].as_str().map(String::from);
            let request = craft_plugin::AudioRequest { layer, start: 0.0, duration: args["duration"].as_f64().unwrap_or(0.5), rate: 8000 };
            let audio = host::audio(request)?;
            let peak = audio.samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
            Ok(json!({"channels": audio.channels, "rate": audio.rate, "len": audio.samples.len(), "peak": peak}))
        })
        .task("edit", "Edit From A Task", |_task: &Task| host::command("layer.newNull", json!({"name": "From Task"})))
}

#[derive(Default)]
struct Controls {
    live: bool,
    mode: usize,
    notes: String,
    shape: Option<Vec<[f64; 2]>>,
    ramp: Option<Gradient>,
    pinned: bool,
    said: String,
    focused: bool,
}

impl Panel for Controls {
    fn ui(&mut self, ui: &mut Ui) {
        ui.text(&[("Probe ", TextStyle::Body), ("controls", TextStyle::Strong)]);
        ui.toggle("Live", &mut self.live);
        ui.context_menu(|menu| {
            if menu.item("Say hello") {
                self.said = "hello".into();
            }
        });
        ui.radio("Mode", &mut self.mode, &["Plain", "Soft", "Hard"]);
        ui.text_area("notes", &mut self.notes, "Notes", 3);
        if !std::mem::replace(&mut self.focused, true) {
            ui.focus();
        }
        ui.gap(Gap::Medium);
        if ui.icon_button("trash", "Clear notes") {
            self.notes.clear();
        }
        ui.name("Clear the notes");
        ui.menu("More", |menu| {
            if menu.item("Reset") {
                self.said = "reset".into();
            }
            menu.check("Pinned", &mut self.pinned);
            menu.separator();
            menu.submenu("Mode", |menu| {
                if menu.item("Hard") {
                    self.mode = 2;
                }
            });
        });
        let shape = self.shape.get_or_insert_with(|| vec![[0.0, 0.0], [1.0, 1.0]]);
        ui.curve("Shape", shape);
        let points = shape.len();
        let ramp = self.ramp.get_or_insert_with(|| Gradient::new(&[(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])]));
        ui.gradient("Ramp", ramp);
        let stops = ramp.colors.len();
        ui.split("summary", Axis::Horizontal, |ui| ui.label(&format!("{points} points")), |ui| ui.label(&format!("{stops} stops")));
        ui.label(&format!("live {} mode {} pinned {} notes {:?} said {}", self.live, self.mode, self.pinned, self.notes, self.said));
    }
}

craft_plugin::export!(make);

fn folders() -> Vec<(u8, String)> {
    let mut nodes = Vec::new();
    for a in 0..3 {
        nodes.push((0, format!("Folder {a}")));
        for b in 0..2 {
            nodes.push((1, format!("Shot {a}.{b}")));
            nodes.extend((0..2).map(|c| (2, format!("Take {a}.{b}.{c}"))));
        }
    }
    nodes
}

#[derive(Default)]
struct Tree {
    open: Vec<usize>,
    folder: Option<usize>,
    takes: Vec<usize>,
    said: String,
}

impl Tree {
    fn shown(&self, nodes: &[(u8, String)]) -> Vec<usize> {
        let mut shut_below = u8::MAX;
        (0..nodes.len())
            .filter(|at| {
                let depth = nodes[*at].0;
                if depth > shut_below {
                    return false;
                }
                shut_below = if depth < 2 && !self.open.contains(at) { depth } else { u8::MAX };
                true
            })
            .collect()
    }
}

impl Panel for Tree {
    fn ui(&mut self, ui: &mut Ui) {
        let nodes = folders();
        let before = self.shown(&nodes);
        if let Some((row, open)) = ui.toggled("folders") {
            let picked = self.folder.and_then(|row| before.get(row).copied());
            let node = before.get(row).copied().unwrap_or_default();
            self.open.retain(|at| *at != node);
            if open {
                self.open.push(node);
            }
            self.folder = picked.and_then(|node| self.shown(&nodes).iter().position(|at| *at == node));
        }
        let shown = self.shown(&nodes);
        let picked = ui.list("folders", shown.len(), &mut self.folder, |row| {
            let (depth, name) = &nodes[shown[row]];
            let item = Item::new(name).depth(*depth).icon(if *depth < 2 { "folder" } else { "film" });
            if *depth < 2 { item.open(self.open.contains(&shown[row])) } else { item }
        });
        if let Some((row, open)) = picked.toggled {
            self.said = format!("{} {}", if open { "opened" } else { "closed" }, nodes[shown[row]].1);
        }
        let takes = ui.table("takes", &["Take", "Length", "Rating"], 10_000, &mut self.takes, |row| {
            Item::cells(&[&format!("take {row}"), &format!("{}.{} s", row % 60, row % 10), &"*".repeat(row % 5 + 1)])
        });
        if let Some(row) = takes.activated {
            self.said = format!("activated take {row}");
        }
        ui.label(&self.said);
    }
}

#[derive(Default)]
struct StatusItem {
    pings: u32,
}

impl Panel for StatusItem {
    fn ui(&mut self, ui: &mut Ui) {
        ui.row(|ui| {
            if ui.button("Ping") {
                self.pings += 1;
            }
            ui.label(&format!("{} pings", self.pings));
        });
    }
}

#[derive(Default)]
struct StampOptions {
    soft: bool,
}

impl Panel for StampOptions {
    fn ui(&mut self, ui: &mut Ui) {
        ui.row(|ui| {
            ui.checkbox("Soft", &mut self.soft);
            ui.label(if self.soft { "soft stamp" } else { "hard stamp" });
        });
    }
}

#[derive(Default)]
struct Notes {
    text: String,
}

impl Panel for Notes {
    fn ui(&mut self, ui: &mut Ui) {
        ui.text_edit("note", &mut self.text, "Note");
        ui.label(&format!("note {}: {}", ui.instance().unwrap_or("-"), self.text));
    }
}

#[derive(Default)]
struct Pad {
    said: String,
    ink: Option<[f32; 4]>,
}

impl Panel for Pad {
    fn ui(&mut self, ui: &mut Ui) {
        ui.canvas("pad", [120.0, 80.0], |canvas| {
            if let Some(at) = canvas.context {
                self.said = format!("context {} {}", at.pos[0], at.pos[1]);
            }
            if let Some(at) = canvas.opened {
                self.said = format!("open {} {}", at.pos[0], at.pos[1]);
            }
        });
        let ink = self.ink.get_or_insert([0.0, 0.0, 0.0, 1.0]);
        ui.color("Ink", ink);
        ui.label(&format!("pad {} ink {ink:?}", self.said));
    }
}
