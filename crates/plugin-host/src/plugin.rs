use std::collections::{BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use craft_plugin::{
    Call, Contributions, FrameDesc, Layout, LogLevel, Manifest, Outcome, Permission, Pixel, Plan, PlanImage, PlanPass, Rect, RenderCx, Rendered,
    TaskState, Window,
};
use rayon::prelude::*;
use serde::de::DeserializeOwned;

use crate::host::{Host, Origin};
use crate::journal::{LogLine, LogPage};
use crate::kernels::{self, Image, Step};
use crate::registry::Bundle;
use crate::runtime::{Engine, Staged};

const MIN_BAND_ROWS: usize = 64;
const STAGED_BYTES: usize = 1 << 30;

mod zeroed_pool {
    use super::Pixel;
    use std::sync::Mutex;

    const KEPT: usize = 2;
    static POOL: Mutex<Vec<Vec<Pixel>>> = Mutex::new(Vec::new());

    pub(super) fn take(len: usize) -> Vec<Pixel> {
        let Ok(mut kept) = POOL.lock() else { return vec![[0.0; 4]; len] };
        match kept.iter().position(|buffer| buffer.len() == len) {
            Some(at) => kept.swap_remove(at),
            None => {
                kept.clear();
                vec![[0.0; 4]; len]
            }
        }
    }

    pub(super) fn give(buffer: Vec<Pixel>) {
        if let Ok(mut kept) = POOL.lock()
            && kept.len() < KEPT
        {
            kept.push(buffer);
        }
    }

    pub fn held() -> usize {
        POOL.lock().map_or(0, |kept| kept.len())
    }
}

#[doc(hidden)]
pub fn pooled_generator_buffers() -> usize {
    zeroed_pool::held()
}

fn plain_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn validate(manifest: &Manifest, contributions: &Contributions) -> Result<(), String> {
    if manifest.id.starts_with("ec.") {
        return Err("plugin ids starting with `ec.` are reserved for built-in effects".into());
    }
    match contributions.effects.iter().find(|e| !plain_id(&e.id)) {
        Some(effect) => Err(format!("effect id `{}` may only use lower case letters, digits, `-` and `_`", effect.id)),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PluginError {
    #[error("the plugin could not be loaded: {0}")]
    Load(String),
    #[error("the plugin stopped: {0}")]
    Trap(String),
    #[error("{0}")]
    Failed(String),
    #[error("the plugin sent a reply this app does not understand: {0}")]
    Protocol(String),
    #[error("the render was called off")]
    Cancelled,
}

/// Cache writes of one call held apart from the plugin's cache (see [`Plugin::call_staged`]).
#[derive(Default)]
pub struct Memo(Staged);

#[cfg_attr(target_family = "wasm", expect(dead_code, reason = "no engine reads pixels in the web build"))]
pub(crate) struct Scope<'a> {
    pub cancel: Option<Arc<AtomicBool>>,
    pub memo: Option<&'a Staged>,
}

pub(crate) enum Io<'a> {
    None,
    InPlace(&'a mut [Pixel]),
    Region { input: &'a [Pixel], output: &'a mut [Pixel], skip: usize },
    Render { input: &'a [Pixel], out: &'a mut [Pixel], at: Rect },
    Draw { out: &'a mut [Pixel], at: Rect },
    Pass { src: &'a [Pixel], aux: &'a [Pixel], out: &'a mut [Pixel] },
}

impl Image<'_> {
    fn window(&self, band: Window, reach: Option<u32>) -> Window {
        let (height, out_height) = (u64::from(self.size[1]), u64::from(band.size[1]).max(1));
        let Some(reach) = reach.map(u64::from) else { return Window { size: self.size, first_row: 0, rows: self.size[1] } };
        let first = (u64::from(band.first_row) * height / out_height).saturating_sub(reach).min(height);
        let end = ((u64::from(band.first_row) + u64::from(band.rows)) * height).div_ceil(out_height).saturating_add(reach).min(height);
        Window { size: self.size, first_row: first as u32, rows: end.saturating_sub(first) as u32 }
    }

    fn held(&self, window: Window) -> &[Pixel] {
        let width = self.size[0] as usize;
        &self.pixels[window.first_row as usize * width..(window.first_row + window.rows) as usize * width]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub interactive: Duration,
    pub work: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { interactive: Duration::from_secs(2), work: Duration::from_secs(60) }
    }
}

pub struct Plugin {
    manifest: Manifest,
    engine: Arc<Engine>,
    limits: Limits,
    contributions: Contributions,
    warned_gradients: Mutex<HashMap<String, u64>>,
}

struct NoHost;
impl Host for NoHost {}

impl Plugin {
    pub fn load(bundle: &Bundle, grants: BTreeSet<Permission>, data_dir: PathBuf, limits: Limits) -> Result<Plugin, PluginError> {
        let digest = bundle.digest().map_err(|e| PluginError::Load(e.to_string()))?;
        Plugin::open(bundle, &digest, grants, data_dir, limits)
    }

    pub fn open(bundle: &Bundle, digest: &str, grants: BTreeSet<Permission>, data_dir: PathBuf, limits: Limits) -> Result<Plugin, PluginError> {
        let engine = Engine::open(bundle, digest, grants, data_dir)?;
        Plugin::described(bundle.manifest.clone(), engine, limits)
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn in_process(
        manifest: Manifest,
        plugin: craft_plugin::Plugin,
        dir: PathBuf,
        data_dir: PathBuf,
        limits: Limits,
    ) -> Result<Plugin, PluginError> {
        let engine = Engine::in_process(plugin, &manifest.id, dir, manifest.permissions.clone(), data_dir);
        Plugin::described(manifest, engine, limits)
    }

    fn described(manifest: Manifest, engine: Arc<Engine>, limits: Limits) -> Result<Plugin, PluginError> {
        let mut plugin = Plugin { manifest, engine, limits, contributions: Contributions::default(), warned_gradients: Mutex::default() };
        let kept = plugin.engine.described().and_then(|reply| parse::<Contributions>(&reply).ok().filter(|found| found.check().is_ok()));
        plugin.contributions = match kept {
            Some(contributions) => contributions,
            None => {
                let seen = plugin.engine.journal().latest();
                let reply = plugin.answer(&Call::Describe, Io::None, &mut NoHost, None);
                let contributions: Contributions = plugin.logged(&Call::Describe, seen, reply.as_deref().map_err(Clone::clone).and_then(parse))?;
                contributions.check().map_err(PluginError::Load)?;
                if let Ok(reply) = reply {
                    plugin.engine.keep_described(&reply);
                }
                contributions
            }
        };
        plugin.engine.know_tasks(plugin.contributions.tasks.iter().map(|task| task.id.clone()).collect());
        Ok(plugin)
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn module_digest(&self) -> Option<&str> {
        self.engine.module_digest()
    }

    pub fn started(&self) -> bool {
        self.engine.started()
    }

    pub fn failure(&self) -> Option<String> {
        self.engine.failure()
    }

    pub fn cached(&self, key: &str) -> Option<std::sync::Arc<[u8]>> {
        self.engine.cached(key)
    }

    pub fn image(&self, key: &str) -> Option<std::sync::Arc<crate::images::PluginImage>> {
        self.engine.image(key)
    }

    pub fn cache_generation(&self) -> u64 {
        self.engine.cache_generation()
    }

    pub fn uncache(&self, key: &str) {
        self.engine.uncache(key);
    }

    pub fn warn_dropped_stops(&self, effect: &str, param: &str, value: &craft_plugin::Value) {
        let key = format!("{effect}/{param}");
        let Ok(mut warned) = self.warned_gradients.lock() else { return };
        let craft_plugin::Value::Gradient(gradient) = value else { return };
        let dropped = gradient.dropped_stops();
        if dropped == 0 {
            warned.remove(&key);
            return;
        }
        let mut hasher = DefaultHasher::new();
        format!("{gradient:?}").hash(&mut hasher);
        if warned.insert(key, hasher.finish()) != Some(hasher.finish()) {
            let limit = craft_plugin::MAX_GRADIENT_STOPS;
            self.engine.journal().push(
                LogLevel::Warn,
                format!("effect `{effect}`: gradient parameter `{param}` has {dropped} stops beyond the limit of {limit}; the plugin received only the first {limit} colour and opacity stops"),
            );
        }
    }

    pub fn contributions(&self) -> &Contributions {
        &self.contributions
    }

    pub fn call<R: DeserializeOwned>(&self, call: &Call, pixels: Option<&mut [Pixel]>, host: &mut dyn Host) -> Result<R, PluginError> {
        if matches!(call, Call::Render { .. }) {
            return Err(PluginError::Failed("a render call stages its frame through Plugin::render; call render".into()));
        }
        let io = pixels.map_or(Io::None, Io::InPlace);
        self.exchange(call, io, host, None)
    }

    /// [`Plugin::call`] whose cache writes and removals go to `memo` instead of the plugin's
    /// cache: [`Plugin::cached_in`] reads them back, and [`Plugin::keep`] makes them visible
    /// to every later call.
    pub fn call_staged<R: DeserializeOwned>(&self, call: &Call, host: &mut dyn Host, memo: &Memo) -> Result<R, PluginError> {
        if matches!(call, Call::Render { .. }) {
            return Err(PluginError::Failed("a render call stages its frame through Plugin::render; call render".into()));
        }
        self.exchange(call, Io::None, host, Some(&memo.0))
    }

    pub fn cached_in(&self, memo: &Memo, key: &str) -> Option<std::sync::Arc<[u8]>> {
        self.engine.cached_in(&memo.0, key)
    }

    pub fn keep(&self, memo: Memo) {
        self.engine.commit(memo.0);
    }

    /// When the host's call-off flag stops this call it returns `PluginError::Cancelled`. The frame in `pixels` is then
    /// unspecified (bands may be half written, the buffer may be cropped); the caller must discard it, never show or
    /// cache it. The plugin cache and the journal's errors are untouched by a cancelled call.
    pub fn render<H: Host>(
        &self,
        effect: &str,
        cx: &RenderCx,
        layout: &Layout,
        area: Option<Rect>,
        pixels: &mut Vec<Pixel>,
        make_host: impl Fn() -> H + Sync,
    ) -> Result<Rect, PluginError> {
        let (width, height) = (cx.size[0] as usize, cx.size[1] as usize);
        if cx.size.iter().any(|&side| i32::try_from(side).is_err()) {
            return Err(PluginError::Failed(format!(
                "a {width}x{height} frame is larger than a plugin can address; each side must be under {} pixels",
                1u64 << 31
            )));
        }
        if pixels.len() != width.saturating_mul(height) {
            return Err(PluginError::Protocol(format!("{} pixels for a {width}x{height} frame", pixels.len())));
        }
        let full = Rect::of_size(cx.size);
        let separate = self.contributions.separate_input;
        let mut area = area.map_or(full, |area| area.clipped(cx.size));
        let pointwise = layout.reach == Some(0) && !(separate && layout.maps_input);
        let ignores = layout.ignores_input && separate;
        if !separate {
            (area.left, area.right) = (full.left, full.right);
        }
        if pixels.is_empty() || area.is_empty() {
            *pixels = Vec::new();
            return Ok(area);
        }
        let (rows, out_width) = (area.height() as usize, area.width() as usize);
        let bands = (rows / MIN_BAND_ROWS).clamp(1, rayon::current_num_threads());
        let even_rows = rows.div_ceil(bands);
        let memo = Staged::default();
        if pointwise && out_width == width && !ignores {
            let (top, bottom) = (area.top as usize, area.bottom as usize);
            let call = |band: usize, rows: &[Pixel]| {
                let frame = FrameDesc {
                    width: cx.size[0],
                    height: (rows.len() / width) as u32,
                    origin: [0, (top + band * even_rows) as u32],
                    ..Default::default()
                };
                Call::Render { effect: effect.into(), cx: cx.clone(), frame }
            };
            let shown = &mut pixels[top * width..bottom * width];
            let drawn = shown
                .par_chunks_mut(even_rows * width)
                .enumerate()
                .map(|(band, rows)| {
                    let mut host = make_host();
                    let reply = self.exchange(&call(band, rows), Io::InPlace(rows), &mut host, Some(&memo))?;
                    host.finish(rows);
                    let first = (top + band * even_rows) as i32;
                    Ok(drawn_in(reply, Rect { top: first, bottom: first + (rows.len() / width) as i32, ..area }))
                })
                .collect::<Result<Vec<Rect>, PluginError>>()?;
            self.commit(memo, &make_host())?;
            pixels.truncate(bottom * width);
            pixels.drain(..top * width);
            return Ok(cropped(pixels, area, &drawn));
        }
        let reach = layout.reach.map(|reach| reach as usize);
        let band_rows = match reach {
            _ if separate && layout.maps_input => even_rows,
            Some(_) if ignores => even_rows,
            Some(reach) if separate && self.engine.copies_pixels() => copied_band_rows(area, height, width, reach, bands, STAGED_BYTES),
            Some(_) if separate => even_rows,
            Some(reach) if even_rows.max(reach + 1) + 2 * reach < rows => even_rows.max(reach + 1),
            _ => rows,
        };
        let input: &[Pixel] = pixels;
        let mut output = if pointwise && !ignores {
            let mut shown = Vec::with_capacity(out_width * rows);
            for row in input.chunks_exact(width).skip(area.top as usize).take(rows) {
                shown.extend_from_slice(&row[area.left as usize..area.right as usize]);
            }
            shown
        } else if ignores {
            zeroed_pool::take(out_width * rows)
        } else {
            vec![[0.0; 4]; out_width * rows]
        };
        let drawn = output.par_chunks_mut(band_rows * out_width).enumerate().map(|(band, written)| {
            let top = area.top + (band * band_rows) as i32;
            let out = Rect { top, bottom: top + (written.len() / out_width) as i32, ..area };
            let (first, held): (usize, &[Pixel]) = if ignores {
                (top as usize, &[])
            } else {
                let needed = match (layout.maps_input && separate, layout.reach) {
                    (true, _) => {
                        self.exchange::<Rect>(&Call::Input { effect: effect.into(), cx: cx.clone(), out }, Io::None, &mut make_host(), Some(&memo))?
                    }
                    (false, Some(reach)) => out.grown(reach),
                    (false, None) => full,
                };
                let needed = needed.union(out).clipped(cx.size);
                (needed.top as usize, &input[needed.top as usize * width..needed.bottom as usize * width])
            };
            let last = first + held.len() / width;
            let mut host = make_host();
            let spread = !self.engine.copies_pixels();
            let reply = if !separate {
                let frame = FrameDesc { width: cx.size[0], height: (last - first) as u32, origin: [0, first as u32], ..Default::default() };
                let call = Call::Render { effect: effect.into(), cx: cx.clone(), frame };
                let io = Io::Region { input: held, output: written, skip: (top as usize - first) * width };
                self.exchange(&call, io, &mut host, Some(&memo))?
            } else {
                let window = Window { size: cx.size, first_row: first as u32, rows: (last - first) as u32 };
                let origin = [area.left as u32, top as u32];
                let frame = FrameDesc { width: out_width as u32, height: (written.len() / out_width) as u32, origin, input: Some(window), spread };
                let call = Call::Render { effect: effect.into(), cx: cx.clone(), frame };
                if spread {
                    let address = |pixels: &[Pixel]| [pixels.as_ptr() as u64, pixels.len() as u64];
                    let mut table: [Pixel; 3] = bytemuck::cast([address(held), address(&[]), [written.as_mut_ptr() as u64, written.len() as u64]]);
                    self.exchange(&call, Io::InPlace(&mut table), &mut host, Some(&memo))?
                } else if ignores {
                    self.exchange(&call, Io::Draw { out: written, at: out }, &mut host, Some(&memo))?
                } else {
                    self.exchange(&call, Io::Render { input: held, out: written, at: out }, &mut host, Some(&memo))?
                }
            };
            host.finish(written);
            Ok(drawn_in(reply, out))
        });
        let drawn = drawn.collect::<Result<Vec<Rect>, PluginError>>()?;
        self.commit(memo, &make_host())?;
        if !ignores {
            *pixels = output;
            return Ok(cropped(pixels, area, &drawn));
        }
        let (rect, kept) = cropped_from(&output, area, &drawn);
        let Some(kept) = kept else {
            *pixels = output;
            return Ok(rect);
        };
        for row in output.chunks_exact_mut(out_width).skip((rect.top - area.top) as usize).take(rect.height() as usize) {
            row[(rect.left - area.left) as usize..(rect.right - area.left) as usize].fill([0.0; 4]);
        }
        zeroed_pool::give(output);
        *pixels = kept;
        Ok(rect)
    }

    pub fn accept_drop(&self, path: &std::path::Path) -> Result<(), String> {
        self.engine.accept_drop(path)
    }

    pub fn start(&self, task: &str, args: serde_json::Value, host: &dyn Host) -> Result<u64, PluginError> {
        self.engine.start(task, args, host.project_closed()).map_err(PluginError::Failed)
    }

    pub fn tasks(&self) -> Vec<TaskState> {
        self.engine.tasks()
    }

    pub fn running_tasks(&self) -> usize {
        self.engine.running_tasks()
    }

    pub fn cancel(&self, id: u64) -> bool {
        self.engine.cancel(id)
    }

    pub fn cancel_all(&self) {
        self.engine.cancel_all();
    }

    pub fn pump(&self, host: &mut dyn Host) -> usize {
        self.engine.pump(host)
    }

    /// When the host's call-off flag stops this call it returns `PluginError::Cancelled`. The frame in `pixels` is then
    /// unspecified (bands may be half written, the buffer may be cropped); the caller must discard it, never show or
    /// cache it. The plugin cache and the journal's errors are untouched by a cancelled call.
    pub fn run_passes<H: Host>(
        &self,
        effect: &str,
        cx: &RenderCx,
        plan: &Plan,
        pixels: &mut Vec<Pixel>,
        make_host: impl Fn() -> H + Sync,
    ) -> Result<(), PluginError> {
        let area = |size: [u32; 2]| (size[0] as usize).saturating_mul(size[1] as usize);
        if pixels.len() != area(cx.size) {
            return Err(PluginError::Protocol(format!("{} pixels for a {}x{} frame", pixels.len(), cx.size[0], cx.size[1])));
        }
        if pixels.is_empty() {
            return Ok(());
        }
        plan.check(cx.size).map_err(PluginError::Failed)?;
        if !plan.sources.is_empty() && !make_host().effect().is_some_and(|spec| spec.reads_other_frames) {
            return Err(PluginError::Failed("the plan reads source frames at other times; declare `.reads_other_frames()` on the effect".into()));
        }
        let mut sources: Vec<Vec<Pixel>> = Vec::with_capacity(plan.sources.len());
        for time in &plan.sources {
            if called_off(&make_host()) {
                return Err(PluginError::Cancelled);
            }
            let frame = make_host().source(*time).filter(|frame| [frame.width, frame.height] == cx.size && frame.data.len() == area(cx.size));
            sources.push(frame.map_or_else(|| vec![[0.0; 4]; area(cx.size)], |frame| frame.data));
        }
        let cancel = make_host().cancel();
        let stop = || cancel.as_ref().is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed));
        let sizes: Vec<[u32; 2]> = [cx.size].into_iter().chain(plan.images.iter().copied()).collect();
        let mut written: Vec<Vec<Pixel>> = vec![Vec::new(); sizes.len()];
        let slot = |image: PlanImage| match image {
            PlanImage::Source => None,
            PlanImage::Output => Some(0),
            PlanImage::Scratch(index) => Some(index as usize + 1),
            PlanImage::Texture(_) | PlanImage::Layer(_) | PlanImage::SourceAt(_) => None,
        };
        let mut spare: Vec<Vec<Pixel>> = Vec::new();
        let memo = Staged::default();
        for step in kernels::steps(plan, cx.size) {
            if called_off(&make_host()) {
                return Err(PluginError::Cancelled);
            }
            let (out_image, inputs, end) = match &step {
                Step::Single(index) => {
                    let pass = &plan.passes[*index];
                    (pass.out, [Some(pass.src), pass.aux].into_iter().flatten().collect(), index + 1)
                }
                Step::Fused(fused) => (fused.out, fused.inputs.clone(), fused.passes.end),
            };
            let Some(target) = slot(out_image) else { continue };
            for index in inputs.iter().filter_map(|image| slot(*image)) {
                if written[index].is_empty() {
                    written[index] = vec![[0.0; 4]; area(sizes[index])];
                }
            }
            let writes_every_pixel = !matches!(&step, Step::Single(index) if plan.passes[*index].host.is_none());
            let mut out = if inputs.contains(&out_image) || written[target].is_empty() {
                let size = area(sizes[target]);
                match spare.iter().position(|buffer| writes_every_pixel && buffer.len() == size) {
                    Some(at) => spare.swap_remove(at),
                    None => vec![[0.0; 4]; size],
                }
            } else {
                std::mem::take(&mut written[target])
            };
            let read = |image: PlanImage| match (image, slot(image)) {
                (PlanImage::SourceAt(index), _) => Image { size: cx.size, pixels: &sources[index as usize] },
                (_, None) => Image { size: cx.size, pixels },
                (_, Some(index)) => Image { size: sizes[index], pixels: &written[index] },
            };
            let ran = match &step {
                Step::Fused(fused) => {
                    let inputs: Vec<Image> = fused.inputs.iter().map(|image| read(*image)).collect();
                    kernels::run_fused(fused, &inputs, sizes[target][0] as usize, &mut out);
                    Ok(())
                }
                Step::Single(index) => {
                    let pass = &plan.passes[*index];
                    match &pass.host {
                        Some(kernel) => match kernels::run(kernel, read(pass.src), pass.aux.map(read), sizes[target], &mut out, &stop) {
                            true => Ok(()),
                            false => Err(PluginError::Cancelled),
                        },
                        None => self.run_pass(effect, cx, pass, read(pass.src), pass.aux.map(read), sizes[target], &mut out, &make_host, &memo),
                    }
                }
            };
            written[target] = out;
            ran?;
            for (index, buffer) in written.iter_mut().enumerate().skip(1) {
                if !buffer.is_empty() && !kernels::live_after(&plan.passes[end..], PlanImage::Scratch(index as u32 - 1)) {
                    spare.push(std::mem::take(buffer));
                }
            }
        }
        self.commit(memo, &make_host())?;
        *pixels = match std::mem::take(&mut written[0]) {
            unwritten if unwritten.is_empty() => vec![[0.0; 4]; pixels.len()],
            output => output,
        };
        Ok(())
    }

    #[expect(clippy::too_many_arguments, reason = "one pass: its plan entry, the images it reads and the image it writes")]
    fn run_pass<H: Host>(
        &self,
        effect: &str,
        cx: &RenderCx,
        pass: &PlanPass,
        src: Image,
        aux: Option<Image>,
        size: [u32; 2],
        out: &mut [Pixel],
        make_host: &(impl Fn() -> H + Sync),
        memo: &Staged,
    ) -> Result<(), PluginError> {
        let (width, height) = (size[0] as usize, size[1] as usize);
        let spread = !self.engine.copies_pixels();
        let bands = if pass.reach.is_some() { (height / MIN_BAND_ROWS).clamp(1, rayon::current_num_threads()) } else { 1 };
        let band_rows = height.div_ceil(bands);
        out.par_chunks_mut(band_rows * width).enumerate().try_for_each(|(index, rows)| {
            let band = Window { size, first_row: (index * band_rows) as u32, rows: (rows.len() / width) as u32 };
            let src_window = src.window(band, pass.reach);
            let aux_window = aux.as_ref().map(|aux| aux.window(band, pass.aux_reach.or(pass.reach)));
            let (src_held, aux_held) = (src.held(src_window), aux.as_ref().zip(aux_window).map_or(&[][..], |(aux, window)| aux.held(window)));
            let call = Call::Pass {
                effect: effect.into(),
                cx: cx.clone(),
                kernel: pass.kernel.clone(),
                f: pass.f.clone(),
                u: pass.u.clone(),
                src: src_window,
                aux: aux_window,
                out: band,
                spread,
            };
            if spread {
                let address = |pixels: &[Pixel]| [pixels.as_ptr() as u64, pixels.len() as u64];
                let mut table: [Pixel; 3] = bytemuck::cast([address(src_held), address(aux_held), [rows.as_mut_ptr() as u64, rows.len() as u64]]);
                self.exchange::<()>(&call, Io::InPlace(&mut table), &mut make_host(), Some(memo))
            } else {
                self.exchange::<()>(&call, Io::Pass { src: src_held, aux: aux_held, out: rows }, &mut make_host(), Some(memo))
            }
        })
    }

    fn budget(&self, call: &Call) -> Duration {
        match call {
            Call::Describe
            | Call::Layout { .. }
            | Call::Input { .. }
            | Call::Panel { .. }
            | Call::Controls { .. }
            | Call::Overlay { .. }
            | Call::Drag { .. }
            | Call::Migrate { .. }
            | Call::Tool { .. } => self.limits.interactive,
            Call::Render { .. } | Call::Pass { .. } | Call::Command { .. } | Call::Button { .. } | Call::Task { .. } => self.limits.work,
        }
    }

    pub fn logs(&self, after: u64) -> Vec<LogLine> {
        self.engine.journal().page(after, None).lines
    }

    pub fn log_page(&self, after: u64, load: Option<u64>) -> LogPage {
        self.engine.journal().page(after, load)
    }

    pub fn warn(&self, message: String) {
        self.engine.journal().push(LogLevel::Warn, message);
    }

    fn commit(&self, memo: Staged, host: &dyn Host) -> Result<(), PluginError> {
        if called_off(host) {
            return Err(PluginError::Cancelled);
        }
        self.engine.commit(memo);
        Ok(())
    }

    fn exchange<R: DeserializeOwned>(&self, call: &Call, io: Io, host: &mut dyn Host, memo: Option<&Staged>) -> Result<R, PluginError> {
        let renders = matches!(call, Call::Render { .. } | Call::Pass { .. });
        if renders && called_off(host) {
            return Err(PluginError::Cancelled);
        }
        let seen = self.engine.journal().latest();
        let answered = self.answer(call, io, host, memo).and_then(|reply| parse(&reply));
        let answered = answered.map_err(|error| if renders && called_off(host) { PluginError::Cancelled } else { error });
        self.logged(call, seen, answered)
    }

    fn logged<R>(&self, call: &Call, seen: u64, answered: Result<R, PluginError>) -> Result<R, PluginError> {
        let answered = answered.map_err(|error| match error {
            PluginError::Trap(_) => self
                .engine
                .journal()
                .page(seen, None)
                .lines
                .into_iter()
                .find(|line| line.message.starts_with("the plugin panicked"))
                .map_or(error, |line| PluginError::Trap(line.message)),
            other => other,
        });
        if let Err(error) = &answered
            && !matches!(error, PluginError::Cancelled)
        {
            self.engine.journal().push(LogLevel::Error, format!("{} failed: {error}", what(call)));
        }
        answered
    }

    fn answer(&self, call: &Call, io: Io, host: &mut dyn Host, memo: Option<&Staged>) -> Result<Vec<u8>, PluginError> {
        let request = serde_json::to_vec(call).map_err(|e| PluginError::Protocol(e.to_string()))?;
        let cancel = if matches!(call, Call::Render { .. } | Call::Pass { .. }) { host.cancel() } else { None };
        self.engine.call(&request, io, host, origin(call), self.budget(call), Scope { cancel, memo })
    }
}

fn parse<R: DeserializeOwned>(reply: &[u8]) -> Result<R, PluginError> {
    let outcome: Outcome<R> = serde_json::from_slice(reply).map_err(|e| PluginError::Protocol(e.to_string()))?;
    Result::from(outcome).map_err(PluginError::Failed)
}

impl Drop for Plugin {
    fn drop(&mut self) {
        self.engine.cancel_all();
    }
}

fn called_off(host: &dyn Host) -> bool {
    host.cancel().is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed))
}

pub(crate) fn drawn_in(reply: Option<Rendered>, band: Rect) -> Rect {
    reply.and_then(|reply| reply.drawn).map_or(band, |drawn| drawn.intersection(band))
}

fn cropped(pixels: &mut Vec<Pixel>, area: Rect, bands: &[Rect]) -> Rect {
    let (rect, kept) = cropped_from(pixels, area, bands);
    if let Some(kept) = kept {
        *pixels = kept;
    }
    rect
}

fn cropped_from(pixels: &[Pixel], area: Rect, bands: &[Rect]) -> (Rect, Option<Vec<Pixel>>) {
    let size = |rect: &Rect| rect.width() as usize * rect.height() as usize;
    if bands.iter().map(size).sum::<usize>() == size(&area) {
        return (area, None);
    }
    let drawn: Vec<Rect> = bands.iter().copied().filter(|band| !band.is_empty()).collect();
    let Some(union) = drawn.iter().copied().reduce(Rect::union) else {
        return (Rect { right: area.left, bottom: area.top, ..area }, Some(Vec::new()));
    };
    let mut kept = Vec::with_capacity(size(&union));
    for (y, row) in (area.top..).zip(pixels.chunks_exact(area.width() as usize)).filter(|(y, _)| (union.top..union.bottom).contains(y)) {
        let band = drawn.iter().find(|band| (band.top..band.bottom).contains(&y)).map_or(union.left..union.left, |band| band.left..band.right);
        kept.resize(kept.len() + (band.start - union.left) as usize, [0.0; 4]);
        kept.extend_from_slice(&row[(band.start - area.left) as usize..(band.end - area.left) as usize]);
        kept.resize(kept.len() + (union.right - band.end) as usize, [0.0; 4]);
    }
    (union, Some(kept))
}

fn copied_band_rows(area: Rect, height: usize, width: usize, reach: usize, bands: usize, budget: usize) -> usize {
    let (rows, top, bottom) = (area.height() as usize, area.top as usize, area.bottom as usize);
    let staged = |band_rows: usize| {
        (0..rows).step_by(band_rows).fold((rows * area.width() as usize, true), |(pixels, covers), start| {
            let first = (top + start).saturating_sub(reach);
            let last = (top + start + band_rows).min(bottom).saturating_add(reach).min(height);
            (pixels + (last - first) * width, covers && last - first == height)
        })
    };
    (1..=bands)
        .rev()
        .map(|bands| rows.div_ceil(bands))
        .find(|&band_rows| matches!(staged(band_rows), (pixels, false) if pixels.saturating_mul(size_of::<Pixel>()) <= budget))
        .unwrap_or(rows)
}

fn what(call: &Call) -> String {
    match call {
        Call::Describe => "describing the plugin".into(),
        Call::Layout { effect, .. } => format!("layout of effect `{effect}`"),
        Call::Render { effect, .. } => format!("render of effect `{effect}`"),
        Call::Input { effect, .. } => format!("input area of effect `{effect}`"),
        Call::Pass { effect, kernel, .. } => format!("pass `{kernel}` of effect `{effect}`"),
        Call::Panel { panel, .. } => format!("panel `{panel}`"),
        Call::Command { command } => format!("command `{command}`"),
        Call::Task { task, .. } => format!("task `{task}`"),
        Call::Controls { effect, param, .. } => format!("controls of `{param}` in effect `{effect}`"),
        Call::Overlay { effect, .. } => format!("overlay of effect `{effect}`"),
        Call::Drag { effect, key, .. } => format!("drag of handle `{key}` in effect `{effect}`"),
        Call::Tool { tool, .. } => format!("tool `{tool}`"),
        Call::Button { effect, param, .. } => format!("button `{param}` of effect `{effect}`"),
        Call::Migrate { effect, from, .. } => format!("migration of effect `{effect}` from state version {from}"),
    }
}

fn origin(call: &Call) -> Origin {
    match call {
        Call::Describe
        | Call::Panel { .. }
        | Call::Controls { .. }
        | Call::Command { .. }
        | Call::Button { .. }
        | Call::Overlay { .. }
        | Call::Drag { .. }
        | Call::Migrate { .. }
        | Call::Tool { .. } => Origin::Interactive,
        Call::Layout { .. } | Call::Input { .. } | Call::Render { .. } | Call::Pass { .. } => Origin::Render,
        Call::Task { .. } => Origin::Task,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_hands_back_the_same_buffer_keeps_at_most_two_and_drops_one_of_the_wrong_size() {
        let first = zeroed_pool::take(7);
        let second = zeroed_pool::take(7);
        let (first_at, second_at) = (first.as_ptr(), second.as_ptr());
        zeroed_pool::give(first);
        zeroed_pool::give(second);
        zeroed_pool::give(vec![[0.0; 4]; 7]);
        assert_eq!(zeroed_pool::held(), 2);
        let again = zeroed_pool::take(7);
        assert!(again.as_ptr() == first_at || again.as_ptr() == second_at);
        assert_eq!(zeroed_pool::held(), 1);
        zeroed_pool::give(again);
        assert_eq!(zeroed_pool::take(9), vec![[0.0; 4]; 9]);
        assert_eq!(zeroed_pool::held(), 0);
    }

    #[test]
    fn bands_that_drew_different_rectangles_are_cropped_to_their_union_and_transparent_where_none_drew() {
        let area = Rect { left: 10, top: 20, right: 14, bottom: 23 };
        let mut pixels: Vec<Pixel> = (0..12).map(|i| [i as f32; 4]).collect();
        let bands = [Rect { left: 11, top: 20, right: 12, bottom: 21 }, Rect { left: 12, top: 22, right: 14, bottom: 23 }];
        assert_eq!(cropped(&mut pixels, area, &bands), Rect { left: 11, top: 20, right: 14, bottom: 23 });
        assert_eq!(pixels.iter().map(|pixel| pixel[0]).collect::<Vec<_>>(), [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 11.0]);
        let mut whole: Vec<Pixel> = vec![[1.0; 4]; 12];
        assert_eq!(cropped(&mut whole, area, &[Rect { bottom: 21, ..area }, Rect { top: 21, ..area }]), area);
        assert_eq!(whole.len(), 12);
        assert_eq!(cropped(&mut whole, area, &[Rect { right: 10, ..area }]), Rect { right: 10, bottom: 20, ..area });
        assert!(whole.is_empty());
    }

    #[test]
    fn a_sandboxed_render_uses_one_band_when_its_reach_covers_the_frame_and_bounds_what_its_bands_stage() {
        let frame = Rect::of_size([64, 256]);
        assert_eq!(copied_band_rows(frame, 256, 64, 1000, 4, usize::MAX), 256, "a reach over the frame");
        assert_eq!(copied_band_rows(frame, 256, 64, 8, 4, usize::MAX), 64, "a short reach");
        assert_eq!(copied_band_rows(frame, 256, 64, 100, 4, 1_064_960), 64, "four bands stage 1,064,960 bytes");
        assert_eq!(copied_band_rows(frame, 256, 64, 100, 4, 800_000), 128, "two bands stage 729,088 bytes");
        assert_eq!(copied_band_rows(frame, 256, 64, 100, 4, 1000), 256, "nothing fits: one band");
    }

    #[test]
    fn a_descriptor_from_an_sdk_without_limits_still_loads_but_keeps_the_structural_checks() {
        let effect = |ids: Vec<String>| {
            craft_plugin::EffectSpec::new("glow", "Glow", "Stylize").params(ids.iter().map(|id| craft_plugin::Param::checkbox(id, "On", true)))
        };
        let many = (0..513).map(|i| format!("p{i}")).collect::<Vec<_>>();
        let older = Contributions { effects: vec![effect(many)], bounded: false, ..Contributions::default() };
        assert_eq!(older.check(), Ok(()));
        assert_eq!(
            Contributions { bounded: true, ..older }.check(),
            Err("effect `glow` declares 513 parameters; at most 512 are allowed".to_string())
        );
        let hollow = Contributions { effects: vec![effect(vec!["a//b".into()])], bounded: false, ..Contributions::default() };
        assert_eq!(hollow.check(), Err("effect `glow`: parameter id `a//b` has an empty part; each part between `/` needs a name".to_string()));
    }
}
