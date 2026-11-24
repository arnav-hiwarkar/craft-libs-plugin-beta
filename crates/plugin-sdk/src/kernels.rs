use serde::{Deserialize, Serialize};

use crate::wire::Pixel;

pub const MAX_BLUR_RADIUS: f32 = 512.0;
pub const MAX_TAPS: usize = 1025;
pub const MAX_FULL_SIDE: usize = 31;
pub const MAX_CURVE_POINTS: usize = 4096;
pub const MAX_RESAMPLE_SCALE: f32 = 64.0;
pub const MAX_WARP: f32 = 4096.0;
pub const MAX_MORPHOLOGY_RADIUS: u32 = 512;
pub const MAX_DIRECTIONAL_LENGTH: f32 = 1024.0;
pub const MAX_BILATERAL_RADIUS: u32 = 32;
pub const MAX_NOISE_OCTAVES: u32 = 12;
pub const MAX_DABS: usize = 65536;
pub const MAX_DAB_RADIUS: f32 = 256.0;
pub const DAB_TILE: u32 = 16;
pub const MAX_DAB_TILES: u64 = 1 << 21;
pub const MAX_FLOW_LEVELS: u32 = 8;
pub const MAX_FLOW_RADIUS: u32 = 8;
pub const MAX_FLOW_ITERATIONS: u32 = 8;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HostKernel {
    Blur {
        shape: BlurShape,
        radius: [f32; 2],
    },
    Resample {
        filter: Filter,
        #[serde(default)]
        scale: Option<[f32; 2]>,
    },
    Blend {
        mode: BlendMode,
        amount: f32,
    },
    Curves {
        table: Vec<Pixel>,
    },
    Convolve {
        taps: Taps,
    },
    Warp {
        amount: [f32; 2],
    },
    Matrix {
        rows: [[f32; 4]; 4],
    },
    ToLinear,
    ToSrgb,
    Transfer {
        to: Encoding,
        negatives: Negatives,
    },
    BrightnessCurve {
        measure: Measure,
        curve: Vec<f32>,
        range: f32,
    },
    Dither {
        seed: u32,
        origin: [i64; 2],
    },
    LightAlpha,
    Dilate {
        radius: [u32; 2],
    },
    Erode {
        radius: [u32; 2],
    },
    DirectionalBlur {
        angle: f32,
        length: f32,
    },
    Bilateral {
        radius: u32,
        tolerance: f32,
    },
    Noise {
        scale: f32,
        octaves: u32,
        seed: u32,
        origin: [i64; 2],
    },
    HslRange {
        hue: [f32; 2],
        saturation: [f32; 2],
        lightness: [f32; 2],
        softness: f32,
    },
    Splat {
        dabs: Vec<Dab>,
    },
    Flow {
        levels: u32,
        radius: u32,
        iterations: u32,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub at: [f32; 2],
    pub radius: f32,
    pub colour: [f32; 3],
    pub opacity: f32,
    pub hardness: f32,
}

impl Dab {
    pub fn new(at: [f32; 2], radius: f32, colour: [f32; 3]) -> Self {
        Dab { at, radius, colour, opacity: 1.0, hardness: 0.0 }
    }

    pub fn tiles(&self, out: [u32; 2]) -> [std::ops::Range<u32>; 2] {
        [0, 1].map(|axis| {
            let tiles = out[axis].div_ceil(DAB_TILE);
            let tile = |v: f32| (v / DAB_TILE as f32).floor().clamp(0.0, tiles as f32) as u32;
            let (first, last) = (self.at[axis] - self.radius, self.at[axis] + self.radius);
            if self.radius > 0.0 && last >= 0.0 { tile(first)..(tile(last) + 1).min(tiles) } else { 0..0 }
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    MaxChannel,
    Luma,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Linear,
    Srgb,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Negatives {
    Mirror,
    Linear,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlurShape {
    Gaussian,
    Box,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    Bilinear,
    Bicubic,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    Over,
    Add,
    Subtract,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    ScreenHdr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Taps {
    Separable { x: Vec<f32>, y: Vec<f32> },
    Full { width: u32, weights: Vec<f32> },
}

impl BlendMode {
    pub const ALL: [BlendMode; 10] = [
        BlendMode::Over,
        BlendMode::Add,
        BlendMode::Subtract,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Difference,
        BlendMode::ScreenHdr,
    ];
}

impl HostKernel {
    pub fn name(&self) -> &'static str {
        match self {
            HostKernel::Blur { .. } => "blur",
            HostKernel::Resample { .. } => "resample",
            HostKernel::Blend { .. } => "blend",
            HostKernel::Curves { .. } => "curves",
            HostKernel::Convolve { .. } => "convolve",
            HostKernel::Warp { .. } => "warp",
            HostKernel::Matrix { .. } => "matrix",
            HostKernel::ToLinear => "to_linear",
            HostKernel::ToSrgb => "to_srgb",
            HostKernel::Transfer { .. } => "transfer",
            HostKernel::BrightnessCurve { .. } => "brightness_curve",
            HostKernel::Dither { .. } => "dither",
            HostKernel::LightAlpha => "light_alpha",
            HostKernel::Dilate { .. } => "dilate",
            HostKernel::Erode { .. } => "erode",
            HostKernel::DirectionalBlur { .. } => "directional_blur",
            HostKernel::Bilateral { .. } => "bilateral",
            HostKernel::Noise { .. } => "noise",
            HostKernel::HslRange { .. } => "hsl_range",
            HostKernel::Splat { .. } => "splat",
            HostKernel::Flow { .. } => "flow",
        }
    }

    pub fn pointwise(&self) -> bool {
        !matches!(
            self,
            HostKernel::Blur { .. }
                | HostKernel::Resample { .. }
                | HostKernel::Convolve { .. }
                | HostKernel::Warp { .. }
                | HostKernel::Dilate { .. }
                | HostKernel::Erode { .. }
                | HostKernel::DirectionalBlur { .. }
                | HostKernel::Bilateral { .. }
                | HostKernel::Splat { .. }
                | HostKernel::Flow { .. }
        )
    }

    pub(crate) fn needs_temporary(&self) -> bool {
        matches!(
            self,
            HostKernel::Blur { .. } | HostKernel::Convolve { taps: Taps::Separable { .. } } | HostKernel::Dilate { .. } | HostKernel::Erode { .. }
        )
    }

    pub fn check(&self, src: [u32; 2], aux: Option<[u32; 2]>, out: [u32; 2]) -> Result<(), String> {
        let same = |field: &str, size: [u32; 2]| {
            if size == out {
                Ok(())
            } else {
                Err(format!("`{field}` is {}x{} but `out` is {}x{}; they must be the same size", size[0], size[1], out[0], out[1]))
            }
        };
        let needs_aux = |what: &str| aux.ok_or_else(|| format!("`aux` is missing; give the {what} with `.aux(image)`"));
        match self {
            HostKernel::Blur { radius, .. } => {
                if !radius.iter().all(|r| (0.0..=MAX_BLUR_RADIUS).contains(r)) {
                    return Err(format!("`radius` is {radius:?}; each axis must be 0 to {MAX_BLUR_RADIUS}"));
                }
                same("src", src)
            }
            HostKernel::Resample { scale, .. } => match scale {
                Some(scale) if !scale.iter().all(|s| *s > 0.0 && *s <= MAX_RESAMPLE_SCALE) => {
                    Err(format!("`scale` is {scale:?}; each axis must be above 0 and at most {MAX_RESAMPLE_SCALE} source pixels per output pixel"))
                }
                None if src.iter().zip(out).any(|(s, o)| *s > o.saturating_mul(MAX_RESAMPLE_SCALE as u32)) => Err(format!(
                    "`src` is {}x{} and `out` {}x{}; one pass shrinks at most {MAX_RESAMPLE_SCALE} times, so resample in steps",
                    src[0], src[1], out[0], out[1]
                )),
                _ => Ok(()),
            },
            HostKernel::Blend { amount, .. } => {
                if !amount.is_finite() {
                    return Err(format!("`amount` is {amount}; it must be a finite number"));
                }
                same("src", src)?;
                same("aux", needs_aux("top image")?)
            }
            HostKernel::Curves { table } => {
                if !(2..=MAX_CURVE_POINTS).contains(&table.len()) {
                    return Err(format!("`table` has {} entries; it needs 2 to {MAX_CURVE_POINTS}", table.len()));
                }
                if table.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("`table` holds a value that is not a finite number".into());
                }
                same("src", src)
            }
            HostKernel::Convolve { taps } => {
                taps.check()?;
                same("src", src)
            }
            HostKernel::Warp { amount } => {
                if !amount.iter().all(|a| a.abs() <= MAX_WARP) {
                    return Err(format!("`amount` is {amount:?}; each axis must be -{MAX_WARP} to {MAX_WARP} pixels"));
                }
                same("src", src)?;
                same("aux", needs_aux("displacement image")?)
            }
            HostKernel::Matrix { rows } => {
                if rows.iter().flatten().any(|v| !v.is_finite()) {
                    return Err("`rows` holds a value that is not a finite number".into());
                }
                same("src", src)
            }
            HostKernel::BrightnessCurve { curve, range, .. } => {
                if !(2..=MAX_CURVE_POINTS).contains(&curve.len()) {
                    return Err(format!("`curve` has {} entries; it needs 2 to {MAX_CURVE_POINTS}", curve.len()));
                }
                if curve.iter().any(|v| !v.is_finite()) {
                    return Err("`curve` holds a value that is not a finite number".into());
                }
                if !(range.is_finite() && *range > 0.0) {
                    return Err(format!("`range` is {range}; it must be a finite number above 0"));
                }
                same("src", src)
            }
            HostKernel::Dilate { radius } | HostKernel::Erode { radius } => {
                if radius.iter().any(|r| *r > MAX_MORPHOLOGY_RADIUS) {
                    return Err(format!("`radius` is {radius:?}; each axis must be 0 to {MAX_MORPHOLOGY_RADIUS}"));
                }
                same("src", src)
            }
            HostKernel::DirectionalBlur { angle, length } => {
                if !angle.is_finite() || !(0.0..=MAX_DIRECTIONAL_LENGTH).contains(length) {
                    return Err(format!(
                        "`angle` is {angle} and `length` {length}; give a finite angle in degrees and a length of 0 to {MAX_DIRECTIONAL_LENGTH} pixels"
                    ));
                }
                same("src", src)
            }
            HostKernel::Bilateral { radius, tolerance } => {
                if *radius > MAX_BILATERAL_RADIUS || !(tolerance.is_finite() && *tolerance > 0.0) {
                    return Err(format!(
                        "`radius` is {radius} and `tolerance` {tolerance}; give a radius of 0 to {MAX_BILATERAL_RADIUS} and a finite tolerance above 0"
                    ));
                }
                same("src", src)
            }
            HostKernel::Noise { scale, octaves, .. } => {
                if !(scale.is_finite() && *scale > 0.0) || !(1..=MAX_NOISE_OCTAVES).contains(octaves) {
                    return Err(format!(
                        "`scale` is {scale} and `octaves` {octaves}; give a finite scale above 0 pixels and 1 to {MAX_NOISE_OCTAVES} octaves"
                    ));
                }
                same("src", src)
            }
            HostKernel::HslRange { hue, saturation, lightness, softness } => {
                let within = |range: &[f32; 2], max: f32| range.iter().all(|v| (0.0..=max).contains(v));
                if !within(hue, 360.0) || !within(saturation, 1.0) || !within(lightness, 1.0) || !(0.0..=1.0).contains(softness) {
                    return Err(format!(
                        "`hue` is {hue:?}, `saturation` {saturation:?}, `lightness` {lightness:?} and `softness` {softness}; give hue in degrees 0 to 360 and the others 0 to 1"
                    ));
                }
                same("src", src)
            }
            HostKernel::Splat { dabs } => {
                if dabs.len() > MAX_DABS {
                    return Err(format!("`dabs` has {} entries; give at most {MAX_DABS}, or split them over several passes", dabs.len()));
                }
                let fits = |d: &Dab| {
                    d.at.iter().chain(&d.colour).all(|v| v.is_finite())
                        && d.colour.iter().all(|c| *c >= 0.0)
                        && (0.0..=MAX_DAB_RADIUS).contains(&d.radius)
                        && (0.0..=1.0).contains(&d.opacity)
                        && (0.0..=1.0).contains(&d.hardness)
                };
                if let Some((i, d)) = dabs.iter().enumerate().find(|(_, d)| !fits(d)) {
                    return Err(format!(
                        "dab {i} is {d:?}; give a finite position, a radius of 0 to {MAX_DAB_RADIUS}, a colour of 0 or more and an opacity and hardness of 0 to 1"
                    ));
                }
                let tiles = dabs.iter().map(|d| d.tiles(out).map(|r| u64::from(r.end - r.start))).map(|[x, y]| x * y).sum::<u64>();
                if tiles > MAX_DAB_TILES {
                    return Err(format!(
                        "the dabs cover {tiles} tiles of {DAB_TILE}x{DAB_TILE} pixels; the limit is {MAX_DAB_TILES}, so use fewer or smaller dabs"
                    ));
                }
                same("src", src)
            }
            HostKernel::Flow { levels, radius, iterations } => {
                if !(1..=MAX_FLOW_LEVELS).contains(levels)
                    || !(1..=MAX_FLOW_RADIUS).contains(radius)
                    || !(1..=MAX_FLOW_ITERATIONS).contains(iterations)
                {
                    return Err(format!(
                        "`levels` is {levels}, `radius` {radius} and `iterations` {iterations}; give 1 to {MAX_FLOW_LEVELS} levels, a radius of 1 to {MAX_FLOW_RADIUS} and 1 to {MAX_FLOW_ITERATIONS} iterations"
                    ));
                }
                same("src", src)?;
                same("aux", needs_aux("frame to track to")?)
            }
            HostKernel::ToLinear | HostKernel::ToSrgb | HostKernel::Transfer { .. } | HostKernel::Dither { .. } | HostKernel::LightAlpha => {
                same("src", src)
            }
        }
    }
}

impl Taps {
    fn check(&self) -> Result<(), String> {
        let finite = |weights: &[f32], field: &str| {
            if weights.iter().all(|w| w.is_finite()) { Ok(()) } else { Err(format!("`{field}` holds a weight that is not a finite number")) }
        };
        match self {
            Taps::Separable { x, y } => {
                for (field, taps) in [("x", x), ("y", y)] {
                    if taps.len() % 2 == 0 || taps.len() > MAX_TAPS {
                        return Err(format!("`{field}` has {} taps; give an odd number from 1 to {MAX_TAPS}, centred on the pixel", taps.len()));
                    }
                    finite(taps, field)?;
                }
                Ok(())
            }
            Taps::Full { width, weights } => {
                let width = *width as usize;
                let height = weights.len().checked_div(width).unwrap_or(0);
                let odd_side = |side: usize| side % 2 == 1 && side <= MAX_FULL_SIDE;
                if !odd_side(width) || !odd_side(height) || width * height != weights.len() {
                    return Err(format!(
                        "`weights` has {} values for `width` {width}; give an odd width and an odd number of rows, each 1 to {MAX_FULL_SIDE}",
                        weights.len()
                    ));
                }
                finite(weights, "weights")
            }
        }
    }
}

pub fn histogram<'a>(pixels: impl IntoIterator<Item = &'a Pixel>) -> [[u32; 256]; 4] {
    let mut counts = [[0; 256]; 4];
    for p in pixels {
        let straight = if p[3] > 0.0 { [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]] } else { [0.0; 4] };
        for (channel, v) in counts.iter_mut().zip(straight) {
            channel[(v.clamp(0.0, 1.0) * 255.0).round() as usize] += 1;
        }
    }
    counts
}

pub const MIN_FLOW_LEVEL_SIDE: u32 = 8;

pub fn flow_levels(size: [u32; 2], levels: u32) -> Vec<[u32; 2]> {
    let mut sizes = vec![size];
    while let Some(&last) = sizes.last()
        && sizes.len() < levels as usize
        && last.iter().all(|side| side.div_ceil(2) >= MIN_FLOW_LEVEL_SIDE)
    {
        sizes.push(last.map(|side| side.div_ceil(2)));
    }
    sizes
}
