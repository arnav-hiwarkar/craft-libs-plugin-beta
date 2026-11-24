use std::collections::BTreeMap;
use std::ops::Deref;

use crate::Result;
use crate::abi::Buffer;
use crate::gradient::Gradient;
use crate::host;
use crate::ui::Ui;
use crate::viewer::Overlay;
use crate::wire::{
    AuxDesc, Channel, EffectSpec, HostCall, ImageDesc, LayerPlacement, Layout, MaskPath, ParamsAt, Pixel, Rect, RenderCx, Rendered, Scene, Value,
    applied_padding,
};

pub trait Effect: Send + Sync + 'static {
    fn spec(&self) -> EffectSpec;

    fn layout(&self, _cx: &Cx) -> Layout {
        Layout::whole_frame()
    }

    fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
        frame.map(|pixel| pixel);
        Ok(())
    }

    fn input(&self, _cx: &Cx, out: Rect) -> Rect {
        out
    }

    fn controls(&self, _instance: u64, _param: &str, _value: &mut String, _ui: &mut Ui) {}

    fn pass(&self, _cx: &Cx, pass: &Pass, _out: &mut Frame) -> Result<()> {
        Err(format!(
            "the plan names the kernel `{}` but this effect has no CPU code for it; handle it in Effect::pass or use a host kernel such as PlanPass::blur",
            pass.kernel
        )
        .into())
    }

    fn overlay(&self, _cx: &Cx, _overlay: &mut Overlay) {}

    fn drag(&self, _cx: &Cx, _key: &str, _to: [f64; 2], _value: &mut String) {}

    fn migrate(&self, _from: u32, _values: &mut BTreeMap<String, Value>) {}

    fn button(&self, press: &Press) -> Result<()> {
        Err(format!("this effect has no `Effect::button`, so its button `{}` does nothing; implement it", press.param).into())
    }
}

pub struct Press {
    pub param: String,
    pub instance: u64,
    pub layer: u64,
}

pub struct Pass<'a> {
    pub kernel: &'a str,
    pub f: &'a [f32],
    pub u: &'a [u32],
    pub src: Rows<'a>,
    pub aux: Option<Rows<'a>>,
    pub out_size: [usize; 2],
}

#[derive(Clone, Copy)]
pub struct Rows<'a> {
    pub width: usize,
    pub height: usize,
    pub first_row: usize,
    pub pixels: &'a [Pixel],
}

impl<'a> Rows<'a> {
    pub fn row(&self, y: isize) -> Option<&'a [Pixel]> {
        let held = usize::try_from(y).ok()?.checked_sub(self.first_row)?;
        self.pixels.chunks_exact(self.width.max(1)).nth(held)
    }

    pub fn at(&self, x: isize, y: isize) -> Pixel {
        let (x, row) = (x as usize, y.wrapping_sub(self.first_row as isize) as usize);
        let at = if x < self.width { row.checked_mul(self.width).and_then(|start| start.checked_add(x)) } else { None };
        at.and_then(|at| self.pixels.get(at)).copied().unwrap_or([0.0; 4])
    }
}

#[derive(Clone, Copy, Default)]
struct Band {
    rows: Option<[u32; 2]>,
    size: Option<[u32; 2]>,
}

pub struct Cx(RenderCx, Band);

impl Deref for Cx {
    type Target = RenderCx;

    fn deref(&self) -> &RenderCx {
        &self.0
    }
}

impl Cx {
    pub fn new(cx: RenderCx) -> Self {
        Cx(cx, Band::default())
    }

    pub(crate) fn band(self, first_row: usize, rows: usize) -> Self {
        let rows = Some([first_row, rows].map(|n| u32::try_from(n).unwrap_or(u32::MAX)));
        Cx(self.0, Band { rows, ..self.1 })
    }

    pub(crate) fn on_grid(self, size: [u32; 2]) -> Self {
        Cx(self.0, Band { size: Some(size), ..self.1 })
    }

    pub fn f(&self, id: &str) -> f64 {
        match self.params.get(id) {
            Some(Value::Number(n)) => *n,
            Some(Value::Bool(b)) => f64::from(u8::from(*b)),
            _ => 0.0,
        }
    }

    pub fn padded(&self, padding: u32) -> Cx {
        self.padded_sides(padding, padding, padding, padding)
    }

    pub fn padded_sides(&self, left: u32, top: u32, right: u32, bottom: u32) -> Cx {
        let [left, top, right, bottom] = applied_padding([left, top, right, bottom], self.size);
        let shift = [f64::from(left), f64::from(top)];
        let mut padded = self.0.clone();
        padded.size = [padded.size[0].saturating_add(left).saturating_add(right), padded.size[1].saturating_add(top).saturating_add(bottom)];
        padded.offset = [padded.offset[0] + shift[0], padded.offset[1] + shift[1]];
        for value in padded.params.values_mut() {
            if let Value::List(point) = value
                && point.len() < 4
            {
                point.iter_mut().zip(shift).for_each(|(c, by)| *c += by);
            }
        }
        Cx(padded, self.1)
    }

    pub fn px(&self, id: &str) -> f64 {
        self.f(id) * self.scale
    }

    pub fn bool(&self, id: &str) -> bool {
        match self.params.get(id) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => *n != 0.0,
            _ => false,
        }
    }

    pub fn index(&self, id: &str) -> usize {
        self.f(id).max(0.0) as usize
    }

    pub fn list<const N: usize>(&self, id: &str) -> [f64; N] {
        let mut out = [0.0; N];
        if let Some(Value::List(values)) = self.params.get(id) {
            out.iter_mut().zip(values).for_each(|(slot, v)| *slot = *v);
        }
        out
    }

    pub fn point(&self, id: &str) -> [f64; 2] {
        self.list(id)
    }

    pub fn point3(&self, id: &str) -> [f64; 3] {
        self.list(id)
    }

    pub fn color(&self, id: &str) -> [f32; 4] {
        self.list::<4>(id).map(|c| c as f32)
    }

    pub fn gradient(&self, id: &str) -> &Gradient {
        static NONE: Gradient = Gradient { colors: Vec::new(), opacities: Vec::new() };
        match self.params.get(id) {
            Some(Value::Gradient(gradient)) => gradient,
            _ => &NONE,
        }
    }

    pub fn text(&self, id: &str) -> &str {
        match self.params.get(id) {
            Some(Value::Text(text)) => text,
            _ => "",
        }
    }

    pub fn curve(&self, id: &str) -> crate::Curve {
        self.curves(id).swap_remove(0)
    }

    pub fn curves(&self, id: &str) -> Vec<crate::Curve> {
        let text = self.text(id);
        let channels = text.split('|').count();
        match crate::curve::parse_curves(text, channels) {
            Ok(curves) => curves.iter().map(|points| crate::Curve::new(points)).collect(),
            Err(_) => vec![crate::Curve::identity(); channels],
        }
    }

    pub fn layer(&self, param: &str) -> Option<Image> {
        image(&HostCall::Layer { param: param.into(), time: None })
    }

    pub fn layer_at(&self, param: &str, comp_time: f64) -> Option<Image> {
        image(&HostCall::Layer { param: param.into(), time: Some(comp_time) })
    }

    pub fn source_at(&self, layer_time: f64) -> Option<Image> {
        image(&HostCall::Source { time: layer_time })
    }

    pub fn masks(&self) -> Vec<MaskPath> {
        host::call(&HostCall::Masks, &[]).map(|(masks, _)| masks).unwrap_or_default()
    }

    pub fn aux(&self, channels: &[Channel]) -> Result<Option<Aux>> {
        let (desc, bulk) = host::call::<Option<AuxDesc>>(&HostCall::Aux { channels: channels.to_vec(), rows: self.1.rows, size: self.1.size }, &[])?;
        Ok(desc.map(|desc| Aux::of(desc, bulk.bytes())))
    }

    pub fn recall(&self, name: &str) -> Option<Vec<u8>> {
        host::cache_get(&format!("{}/{name}", self.instance))
    }

    pub fn remember(&self, name: &str, bytes: &[u8]) -> Result<()> {
        host::cache_put(&format!("{}/{name}", self.instance), bytes)
    }

    pub fn at(&self, layer_time: f64) -> Option<Cx> {
        let answer = params_at(layer_time)?;
        let mut cx = self.0.clone();
        cx.comp_time += layer_time - cx.time;
        cx.time = layer_time;
        cx.params = answer.params;
        Some(Cx(cx, self.1))
    }

    pub fn param_history(&self) -> Option<u64> {
        params_at(self.time)?.history
    }

    pub fn scene(&self) -> Option<Scene> {
        host::call(&HostCall::Scene, &[]).ok().map(|(scene, _)| scene)
    }

    pub fn scene_at(&self, comp_time: f64) -> crate::Result<Scene> {
        host::call(&HostCall::SceneAt { time: comp_time }, &[]).map(|(scene, _)| scene)
    }

    pub fn layer_data(&self, key: &str) -> crate::Result<Option<Vec<u8>>> {
        host::data(crate::Place::OwnLayer, key)
    }

    pub fn placement(&self, param: &str) -> crate::Result<LayerPlacement> {
        placement(param, None)
    }

    pub fn placement_at(&self, param: &str, comp_time: f64) -> crate::Result<LayerPlacement> {
        placement(param, Some(comp_time))
    }
}

fn placement(param: &str, time: Option<f64>) -> crate::Result<LayerPlacement> {
    host::call(&HostCall::Placement { param: param.into(), time }, &[]).map(|(placement, _)| placement)
}

fn params_at(time: f64) -> Option<ParamsAt> {
    host::call(&HostCall::ParamsAt { time }, &[]).ok().map(|(answer, _)| answer)
}

fn image(call: &HostCall) -> Option<Image> {
    let (desc, buffer) = host::call::<ImageDesc>(call, &[]).ok()?;
    image_of(desc, buffer)
}

pub(crate) fn image_of(desc: ImageDesc, buffer: crate::abi::Buffer) -> Option<Image> {
    let (width, height) = (desc.width as usize, desc.height as usize);
    width.checked_mul(height).is_some_and(|area| area > 0 && buffer.pixels().len() == area).then_some(Image { width, height, buffer })
}

pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    pub origin: [usize; 2],
    pub pixels: &'a mut [Pixel],
    pub input: Rows<'a>,
    drawn: Option<Rect>,
}

impl<'a> Frame<'a> {
    pub fn whole(width: usize, height: usize, input: &'a [Pixel], pixels: &'a mut [Pixel]) -> Frame<'a> {
        Frame::new(width, height, [0, 0], pixels, Rows { width, height, first_row: 0, pixels: input })
    }

    pub(crate) fn new(width: usize, height: usize, origin: [usize; 2], pixels: &'a mut [Pixel], input: Rows<'a>) -> Frame<'a> {
        Frame { width, height, origin, pixels, input, drawn: None }
    }

    pub fn drawn(&mut self, rect: Rect) {
        self.drawn = Some(rect);
    }

    pub(crate) fn reply(&self) -> Option<Rendered> {
        self.drawn.map(|drawn| Rendered { drawn: Some(drawn) })
    }

    pub fn rows_mut(&mut self) -> impl Iterator<Item = (usize, &mut [Pixel])> {
        let first = self.origin[1];
        self.pixels.chunks_exact_mut(self.width.max(1)).enumerate().map(move |(y, row)| (first + y, row))
    }

    pub fn rows_in_place(&mut self) -> impl Iterator<Item = (usize, &mut [Pixel])> {
        let ([x0, _], width, input) = (self.origin, self.width, self.input);
        self.rows_mut().map(move |(y, row)| {
            if let Some(read) = input.row(y as isize).and_then(|read| read.get(x0..x0 + width)) {
                row.copy_from_slice(read);
            }
            (y, row)
        })
    }

    pub fn rows(&mut self) -> impl Iterator<Item = (usize, &'a [Pixel], &mut [Pixel])> {
        if self.input.pixels.is_empty() {
            (self.input.pixels, self.pixels) = crate::abi::snapshot(std::mem::take(&mut self.pixels));
        }
        let ([x0, y0], width, input) = (self.origin, self.width, self.input);
        self.pixels.chunks_exact_mut(width.max(1)).enumerate().map(move |(i, out)| {
            let y = y0 + i;
            let read = input.row(y as isize).and_then(|row| row.get(x0..x0 + width)).unwrap_or(&[]);
            (y, read, out)
        })
    }

    pub fn map(&mut self, f: impl Fn(Pixel) -> Pixel) {
        if self.input.pixels.is_empty() {
            self.pixels.iter_mut().for_each(|pixel| *pixel = f(*pixel));
            return;
        }
        for (_, read, out) in self.rows() {
            out.iter_mut().zip(read).for_each(|(out, read)| *out = f(*read));
        }
    }

    pub fn at(&self, x: isize, y: isize) -> Pixel {
        if self.input.pixels.is_empty() {
            let own = Rows { width: self.width, height: self.input.height, first_row: self.origin[1], pixels: self.pixels };
            return own.at(x - self.origin[0] as isize, y);
        }
        self.input.at(x, y)
    }
}

pub struct Image {
    pub width: usize,
    pub height: usize,
    buffer: Buffer,
}

impl Image {
    pub fn pixels(&self) -> &[Pixel] {
        self.buffer.pixels()
    }

    pub fn at(&self, x: isize, y: isize) -> Pixel {
        at(self.pixels(), self.width, self.height, x, y)
    }

    pub fn sample(&self, x: f64, y: f64) -> Pixel {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let mix = |a: Pixel, b: Pixel, t: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        let top: Pixel = mix(self.at(x0, y0), self.at(x0 + 1, y0), fx);
        let bottom: Pixel = mix(self.at(x0, y0 + 1), self.at(x0 + 1, y0 + 1), fx);
        mix(top, bottom, fy)
    }
}

pub struct Aux {
    pub width: usize,
    pub height: usize,
    pub first_row: usize,
    planes: Vec<(Channel, Vec<f32>)>,
}

impl Aux {
    fn of(desc: AuxDesc, bytes: &[u8]) -> Aux {
        let (width, height, first_row) = (desc.width as usize, desc.height as usize, desc.first_row as usize);
        let mut floats = bytes.as_chunks::<4>().0.iter().map(|chunk| f32::from_le_bytes(*chunk));
        let planes = desc.channels.into_iter().map(|channel| {
            let plane = floats.by_ref().take(width * height * channel.components()).collect();
            (channel, plane)
        });
        Aux { width, height, first_row, planes: planes.collect() }
    }

    pub fn plane(&self, channel: &Channel) -> Option<&[f32]> {
        self.planes.iter().find(|(found, _)| found == channel).map(|(_, plane)| plane.as_slice())
    }

    pub fn at(&self, channel: &Channel, x: usize, y: usize) -> Option<&[f32]> {
        let y = y.checked_sub(self.first_row)?;
        if x >= self.width || y >= self.height {
            return None;
        }
        let components = channel.components();
        let start = (y * self.width + x) * components;
        self.plane(channel)?.get(start..start + components)
    }
}

fn at(pixels: &[Pixel], width: usize, height: usize, x: isize, y: isize) -> Pixel {
    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
        return [0.0; 4];
    }
    pixels[y as usize * width + x as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_at_reads_held_rows_by_frame_coordinates_and_is_transparent_outside_them() {
        let pixels: Vec<Pixel> = (1..=6).map(|i| [i as f32; 4]).collect();
        let rows = Rows { width: 3, height: 10, first_row: 4, pixels: &pixels };
        let probes = [(0, 4), (2, 5), (1, 3), (3, 4), (-1, 4), (0, 6), (isize::MIN, 4), (0, isize::MIN), (0, isize::MAX)];
        assert_eq!(probes.map(|(x, y)| rows.at(x, y)[0]), [1.0, 6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn rows_in_place_hold_the_input_whether_it_is_separate_or_already_in_the_rows() {
        let input: Vec<Pixel> = (0..6).map(|i| [i as f32; 4]).collect();
        let first = |frame: &mut Frame| frame.rows_in_place().map(|(y, row)| (y, row.iter().map(|p| p[0]).collect::<Vec<_>>())).collect::<Vec<_>>();
        let mut written = vec![[9.0; 4]; 2];
        let separate = Rows { width: 3, height: 2, first_row: 0, pixels: &input };
        assert_eq!(first(&mut Frame::new(2, 1, [1, 1], &mut written, separate)), [(1, vec![4.0, 5.0])]);
        let mut own = vec![[7.0; 4]; 2];
        let in_place = Rows { width: 2, height: 4, first_row: 3, pixels: &[] };
        assert_eq!(first(&mut Frame::new(2, 1, [0, 3], &mut own, in_place)), [(3, vec![7.0, 7.0])]);
    }
}
