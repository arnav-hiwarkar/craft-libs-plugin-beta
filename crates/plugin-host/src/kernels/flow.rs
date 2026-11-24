use craft_plugin::{Filter, Pixel, flow_levels};

use super::{CLEAR, Image, each_row, resample, resample_scale};

const STEEPEST: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlowOp {
    Pack,
    Reduce,
    Gradient,
    Track(Init),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum Init {
    Zero,
    Coarser,
    Same,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    From,
    To,
    Temp(usize),
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FlowStep {
    pub(crate) op: FlowOp,
    pub(crate) src: Slot,
    pub(crate) aux: Option<Slot>,
    pub(crate) out: Slot,
}

pub(crate) fn flow_steps(size: [u32; 2], levels: u32, iterations: u32) -> (Vec<FlowStep>, Vec<[u32; 2]>) {
    let sizes = flow_levels(size, levels);
    let temps: Vec<[u32; 2]> = sizes.iter().flat_map(|size| [*size; 3]).collect();
    let [gradient, ping, pong] = [0, 1, 2].map(|k| move |level: usize| Slot::Temp(3 * level + k));
    let step = |op, src, aux, out| FlowStep { op, src, aux, out };
    let mut steps = vec![step(FlowOp::Pack, Slot::From, Some(Slot::To), ping(0)), step(FlowOp::Gradient, ping(0), None, gradient(0))];
    for level in 1..sizes.len() {
        steps.push(step(FlowOp::Reduce, gradient(level - 1), None, ping(level)));
        steps.push(step(FlowOp::Gradient, ping(level), None, gradient(level)));
    }
    let mut last = None;
    for level in (0..sizes.len()).rev() {
        for iteration in 0..iterations.max(1) {
            let init = match (iteration, last) {
                (0, None) => Init::Zero,
                (0, Some(_)) => Init::Coarser,
                _ => Init::Same,
            };
            let out = if level == 0 && iteration + 1 == iterations.max(1) {
                Slot::Out
            } else if last == Some(ping(level)) {
                pong(level)
            } else {
                ping(level)
            };
            steps.push(step(FlowOp::Track(init), gradient(level), last, out));
            last = Some(out);
        }
    }
    (steps, temps)
}

pub(crate) fn run(levels: u32, radius: u32, iterations: u32, from: Image, to: Image, size: [u32; 2], out: &mut [Pixel]) {
    let (steps, sizes) = flow_steps(size, levels, iterations);
    let mut temps: Vec<Vec<Pixel>> = sizes.iter().map(|_| Vec::new()).collect();
    for step in steps {
        let target_size = match step.out {
            Slot::Temp(index) => sizes[index],
            _ => size,
        };
        let mut target = match step.out {
            Slot::Temp(index) => std::mem::take(&mut temps[index]),
            _ => Vec::new(),
        };
        target.resize((target_size[0] * target_size[1]) as usize, CLEAR);
        let read = |slot: Slot| match slot {
            Slot::From => from,
            Slot::To => to,
            Slot::Temp(index) => Image { size: sizes[index], pixels: &temps[index] },
            Slot::Out => Image { size, pixels: &[] },
        };
        let written: &mut [Pixel] = if step.out == Slot::Out { out } else { &mut target };
        apply(step.op, radius, read(step.src), step.aux.map(read), target_size, written);
        if let Slot::Temp(index) = step.out {
            temps[index] = target;
        }
    }
}

fn apply(op: FlowOp, radius: u32, src: Image, aux: Option<Image>, size: [u32; 2], out: &mut [Pixel]) {
    let width = size[0] as usize;
    match op {
        FlowOp::Reduce => resample(src, Filter::Bilinear, resample_scale(None, src.size, size), size, out),
        FlowOp::Pack => {
            let to = aux.unwrap_or(src);
            each_row(
                out,
                width,
                || (),
                |_, y, row| {
                    for (x, o) in row.iter_mut().enumerate() {
                        *o = [luma(src.pixels[y * width + x]), luma(to.pixels[y * width + x]), 0.0, 0.0];
                    }
                },
            );
        }
        FlowOp::Gradient => each_row(
            out,
            width,
            || (),
            |_, y, row| {
                for (x, o) in row.iter_mut().enumerate() {
                    *o = gradient(src, x as i32, y as i32);
                }
            },
        ),
        FlowOp::Track(init) => each_row(
            out,
            width,
            || (),
            |_, y, row| {
                for (x, o) in row.iter_mut().enumerate() {
                    let start = match (init, aux) {
                        (Init::Coarser, Some(coarse)) => coarser(coarse, size, x as f32, y as f32),
                        (Init::Same, Some(same)) => {
                            let p = same.pixels[y * width + x];
                            [p[0], p[1]]
                        }
                        _ => [0.0; 2],
                    };
                    *o = track(src, radius as i32, x as i32, y as i32, start);
                }
            },
        ),
    }
}

pub(crate) fn luma(p: Pixel) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

fn texel(image: Image, x: i32, y: i32) -> Pixel {
    let [w, h] = image.size.map(|side| side as i32);
    image.pixels[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
}

fn sample(image: Image, x: f32, y: f32, channel: usize) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);
    let at = |x: i32, y: i32| texel(image, x, y)[channel];
    let mix = |a: f32, b: f32, f: f32| a + (b - a) * f;
    mix(mix(at(x0, y0), at(x0 + 1, y0), fx), mix(at(x0, y0 + 1), at(x0 + 1, y0 + 1), fx), fy)
}

fn gradient(packed: Image, x: i32, y: i32) -> Pixel {
    let at = |dx: i32, dy: i32| texel(packed, x + dx, y + dy);
    let centre = at(0, 0);
    [centre[0], centre[1], (at(1, 0)[0] - at(-1, 0)[0]) * 0.5, (at(0, 1)[0] - at(0, -1)[0]) * 0.5]
}

fn coarser(coarse: Image, size: [u32; 2], x: f32, y: f32) -> [f32; 2] {
    let ratio = [0, 1].map(|axis| coarse.size[axis] as f32 / size[axis] as f32);
    let (cx, cy) = ((x + 0.5) * ratio[0] - 0.5, (y + 0.5) * ratio[1] - 0.5);
    [sample(coarse, cx, cy, 0) / ratio[0], sample(coarse, cx, cy, 1) / ratio[1]]
}

fn track(level: Image, radius: i32, x: i32, y: i32, start: [f32; 2]) -> Pixel {
    let [w, h] = level.size.map(|side| side as i32 - 1);
    let (mut xx, mut xy, mut yy, mut xt, mut yt) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (px, py) = ((x + dx).clamp(0, w), (y + dy).clamp(0, h));
            let p = texel(level, px, py);
            let t = sample(level, px as f32 + start[0], py as f32 + start[1], 1) - p[0];
            xx += p[2] * p[2];
            xy += p[2] * p[3];
            yy += p[3] * p[3];
            xt += p[2] * t;
            yt += p[3] * t;
        }
    }
    let steady = STEEPEST * ((2 * radius + 1) * (2 * radius + 1)) as f32;
    let (xx, yy) = (xx + steady, yy + steady);
    let det = xx * yy - xy * xy;
    let limit = (radius + 1) as f32;
    let du = ((xy * yt - yy * xt) / det).clamp(-limit, limit);
    let dv = ((xy * xt - xx * yt) / det).clamp(-limit, limit);
    [start[0] + du, start[1] + dv, 0.0, 1.0]
}
