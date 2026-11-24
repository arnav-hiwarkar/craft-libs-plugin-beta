use craft_plugin::{BlendMode, BlurShape, Dab, Encoding, Filter, HostKernel, Measure, Negatives, Pixel, Plan, PlanImage, PlanPass, Taps};

use super::{Image, Lowered, Ref, Step, StepImage, blur_taps, lower, run, run_fused, stamp, steps};

const SIZE: [u32; 2] = [37, 23];

fn picture(size: [u32; 2]) -> Vec<Pixel> {
    let mut state = 0x2545_f491_u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state % 1000) as f32 / 1000.0
    };
    (0..size[0] * size[1])
        .map(|i| {
            let alpha = if i % 7 == 0 { 0.0 } else { next() };
            [next() * alpha * 3.0, next() * alpha, next() * alpha, alpha]
        })
        .collect()
}

fn at(pixels: &[Pixel], size: [u32; 2], x: isize, y: isize) -> Pixel {
    let [w, h] = size.map(|side| side as isize);
    if x < 0 || y < 0 || x >= w || y >= h { [0.0; 4] } else { pixels[(y * w + x) as usize] }
}

fn host(kernel: HostKernel, src: &[Pixel], src_size: [u32; 2], aux: Option<&[Pixel]>, size: [u32; 2]) -> Vec<Pixel> {
    let mut out = vec![[f32::NAN; 4]; (size[0] * size[1]) as usize];
    let aux = aux.map(|pixels| Image { size, pixels });
    run(&kernel, Image { size: src_size, pixels: src }, aux, size, &mut out);
    out
}

fn assert_close(got: &[Pixel], expected: &[Pixel], tolerance: f32, case: &str) {
    assert_eq!(got.len(), expected.len(), "{case}");
    let worst = got.iter().flatten().zip(expected.iter().flatten()).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(worst <= tolerance, "{case}: worst difference {worst}");
}

fn reference_convolve(src: &[Pixel], size: [u32; 2], x_taps: &[f32], y_taps: &[f32]) -> Vec<Pixel> {
    let (rx, ry) = ((x_taps.len() / 2) as isize, (y_taps.len() / 2) as isize);
    let mut out = Vec::new();
    for y in 0..size[1] as isize {
        for x in 0..size[0] as isize {
            let mut sum = [0.0; 4];
            for (j, wy) in y_taps.iter().enumerate() {
                for (i, wx) in x_taps.iter().enumerate() {
                    let p = at(src, size, x + i as isize - rx, y + j as isize - ry);
                    (0..4).for_each(|c| sum[c] += p[c] * wx * wy);
                }
            }
            out.push(sum);
        }
    }
    out
}

#[test]
fn blur_taps_have_their_textbook_shapes() {
    assert_eq!(blur_taps(BlurShape::Box, 1.5), vec![0.125, 0.25, 0.25, 0.25, 0.125]);
    assert_eq!(blur_taps(BlurShape::Box, 0.0), vec![1.0]);
    assert_eq!(blur_taps(BlurShape::Gaussian, 0.0), vec![1.0]);
    let gaussian = blur_taps(BlurShape::Gaussian, 3.0);
    let raw: Vec<f32> = (-3..=3).map(|i: i32| (-(i * i) as f32 / 2.0).exp()).collect();
    let total: f32 = raw.iter().sum();
    assert_eq!(gaussian.len(), 7);
    assert!(gaussian.iter().zip(&raw).all(|(got, raw)| (got - raw / total).abs() < 1e-7), "{gaussian:?}");
}

#[test]
fn blur_and_separable_convolve_match_a_direct_sum() {
    let src = picture(SIZE);
    let gaussian = host(HostKernel::Blur { shape: BlurShape::Gaussian, radius: [4.5, 2.0] }, &src, SIZE, None, SIZE);
    assert_close(
        &gaussian,
        &reference_convolve(&src, SIZE, &blur_taps(BlurShape::Gaussian, 4.5), &blur_taps(BlurShape::Gaussian, 2.0)),
        1e-5,
        "gaussian",
    );
    let boxed = host(HostKernel::Blur { shape: BlurShape::Box, radius: [0.0, 1.5] }, &src, SIZE, None, SIZE);
    assert_close(&boxed, &reference_convolve(&src, SIZE, &[1.0], &[0.125, 0.25, 0.25, 0.25, 0.125]), 1e-5, "box");
    let (x, y) = (vec![-1.0, 3.0, -1.0], vec![0.25, 0.5, 0.25, 0.0, 0.0]);
    let sharpened = host(HostKernel::Convolve { taps: Taps::Separable { x: x.clone(), y: y.clone() } }, &src, SIZE, None, SIZE);
    assert_close(&sharpened, &reference_convolve(&src, SIZE, &x, &y), 1e-5, "separable");
}

#[test]
fn full_convolve_matches_a_direct_sum() {
    let src = picture(SIZE);
    let weights: Vec<f32> = (0..15).map(|i| (i as f32 - 6.0) * 0.1).collect();
    let got = host(HostKernel::Convolve { taps: Taps::Full { width: 5, weights: weights.clone() } }, &src, SIZE, None, SIZE);
    let mut expected = Vec::new();
    for y in 0..SIZE[1] as isize {
        for x in 0..SIZE[0] as isize {
            let mut sum = [0.0; 4];
            for (k, w) in weights.iter().enumerate() {
                let p = at(&src, SIZE, x + (k % 5) as isize - 2, y + (k / 5) as isize - 1);
                (0..4).for_each(|c| sum[c] += p[c] * w);
            }
            expected.push(sum);
        }
    }
    assert_close(&got, &expected, 1e-5, "full");
}

fn reference_resample(src: &[Pixel], src_size: [u32; 2], filter: Filter, scale: [f32; 2], size: [u32; 2]) -> Vec<Pixel> {
    let weight = |d: f32| {
        let d = d.abs();
        match filter {
            Filter::Bilinear => (1.0 - d).max(0.0),
            Filter::Bicubic if d < 1.0 => 1.5 * d * d * d - 2.5 * d * d + 1.0,
            Filter::Bicubic if d < 2.0 => -0.5 * d * d * d + 2.5 * d * d - 4.0 * d + 2.0,
            Filter::Bicubic => 0.0,
        }
    };
    let axis = |o: u32, s: f32| {
        let (centre, widen) = ((o as f32 + 0.5) * s - 0.5, s.max(1.0));
        let taps: Vec<(isize, f32)> = (-300..300).map(|i| (i, weight((i as f32 - centre) / widen))).filter(|(_, w)| *w != 0.0).collect();
        let total: f32 = taps.iter().map(|(_, w)| w).sum();
        taps.into_iter().map(move |(i, w)| (i, w / total))
    };
    let mut out = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let mut sum = [0.0f32; 4];
            for (j, wy) in axis(y, scale[1]) {
                for (i, wx) in axis(x, scale[0]) {
                    let p = at(src, src_size, i, j);
                    (0..4).for_each(|c| sum[c] += p[c] * wx * wy);
                }
            }
            out.push(match filter {
                Filter::Bicubic if sum[3] <= 0.0 => [0.0; 4],
                Filter::Bicubic => sum.map(|v| v.max(0.0)),
                Filter::Bilinear => sum,
            });
        }
    }
    out
}

#[test]
fn resample_matches_a_direct_sum_both_ways_with_either_filter() {
    let src = picture(SIZE);
    for (filter, size, scale) in [
        (Filter::Bilinear, [19, 12], None),
        (Filter::Bilinear, [19, 12], Some([2.0, 2.0])),
        (Filter::Bilinear, [74, 23], None),
        (Filter::Bicubic, [80, 50], None),
        (Filter::Bicubic, [9, 5], None),
    ] {
        let got = host(HostKernel::Resample { filter, scale }, &src, SIZE, None, size);
        let used = scale.unwrap_or([0, 1].map(|a| SIZE[a] as f32 / size[a] as f32));
        assert_close(&got, &reference_resample(&src, SIZE, filter, used, size), 1e-5, &format!("{filter:?} to {size:?} by {scale:?}"));
    }
}

#[test]
fn halving_by_two_uses_the_eighths_filter_and_doubling_the_quarters() {
    let mut src = vec![[0.0; 4]; 8 * 8];
    src[4 * 8 + 3] = [8.0; 4];
    let half = host(HostKernel::Resample { filter: Filter::Bilinear, scale: None }, &src, [8, 8], None, [4, 4]);
    let column: Vec<f32> = (0..4).map(|y| half[y * 4 + 1][0]).collect();
    assert_eq!(column, [0.0, 8.0 * 0.375 * 0.125, 8.0 * 0.375 * 0.375, 0.0]);
    let double = host(HostKernel::Resample { filter: Filter::Bilinear, scale: None }, &half, [4, 4], None, [8, 8]);
    let h = |x: usize, y: usize| half[y * 4 + x][0];
    let expected = 0.75 * (0.75 * h(1, 2) + 0.25 * h(0, 2)) + 0.25 * (0.75 * h(1, 1) + 0.25 * h(0, 1));
    assert!((double[4 * 8 + 2][0] - expected).abs() < 1e-7, "{} against {expected}", double[4 * 8 + 2][0]);
}

#[test]
fn every_blend_mode_gives_its_formula() {
    let (base, top) = ([0.2, 0.6, 0.9, 0.8], [0.5, 0.3, 0.1, 0.5]);
    let expected = [
        (BlendMode::Over, [0.6, 0.6, 0.55, 0.9]),
        (BlendMode::Add, [0.7, 0.9, 1.0, 1.3]),
        (BlendMode::Subtract, [-0.3, 0.3, 0.8, 0.3]),
        (BlendMode::Multiply, [0.1, 0.18, 0.09, 0.4]),
        (BlendMode::Screen, [0.6, 0.72, 0.91, 0.9]),
        (BlendMode::Overlay, [0.2, 0.44, 0.82, 0.8]),
        (BlendMode::Darken, [0.2, 0.3, 0.1, 0.5]),
        (BlendMode::Lighten, [0.5, 0.6, 0.9, 0.8]),
        (BlendMode::Difference, [0.3, 0.3, 0.8, 0.3]),
        (BlendMode::ScreenHdr, [0.6, 0.72, 0.91, 0.9]),
    ];
    for (mode, expected) in expected {
        let got = host(HostKernel::Blend { mode, amount: 1.0 }, &[base], [1, 1], Some(&[top]), [1, 1]);
        assert_close(&got, &[expected], 1e-6, &format!("{mode:?}"));
    }
    let outside = host(HostKernel::Blend { mode: BlendMode::ScreenHdr, amount: 1.0 }, &[[-0.25, 1.5, 0.5, 1.0]], [1, 1], Some(&[[0.5; 4]]), [1, 1]);
    assert_close(&outside, &[[0.25, 1.5, 0.75, 1.0]], 1e-6, "screen past 0 and 1 adds below black and keeps above white");
    let half = host(HostKernel::Blend { mode: BlendMode::Add, amount: 0.5 }, &[base], [1, 1], Some(&[top]), [1, 1]);
    assert_close(&half, &[[0.45, 0.75, 0.95, 1.05]], 1e-6, "half add");
}

#[test]
fn curves_work_on_straight_colour_and_clamp_their_input() {
    let table = [[0.0, 0.0, 1.0, 0.0], [0.8, 0.5, 0.5, 0.5], [1.0, 1.0, 0.0, 1.0]];
    let pixels = [[0.25, 0.1, 0.0, 0.5], [0.0; 4], [3.0, 0.5, 0.25, 1.0]];
    let got = host(HostKernel::Curves { table: table.to_vec() }, &pixels, [3, 1], None, [3, 1]);
    assert_close(&got, &[[0.4, 0.1, 0.5, 0.5], [0.0; 4], [1.0, 0.5, 0.75, 1.0]], 1e-6, "curves");
}

#[test]
fn warp_moves_each_pixel_by_its_displacement() {
    let src = picture(SIZE);
    let area = (SIZE[0] * SIZE[1]) as usize;
    let whole = host(HostKernel::Warp { amount: [2.0, 4.0] }, &src, SIZE, Some(&vec![[0.5, -0.25, 0.0, 0.0]; area]), SIZE);
    let half = host(HostKernel::Warp { amount: [1.0, 0.0] }, &src, SIZE, Some(&vec![[0.5, 0.0, 0.0, 0.0]; area]), SIZE);
    let (mut moved, mut averaged) = (Vec::new(), Vec::new());
    for y in 0..SIZE[1] as isize {
        for x in 0..SIZE[0] as isize {
            moved.push(at(&src, SIZE, x + 1, y - 1));
            let (a, b) = (at(&src, SIZE, x, y), at(&src, SIZE, x + 1, y));
            averaged.push(std::array::from_fn(|c| 0.5 * (a[c] + b[c])));
        }
    }
    assert_close(&whole, &moved, 1e-6, "whole pixels");
    assert_close(&half, &averaged, 1e-6, "half pixels");
}

#[test]
fn matrix_mixes_channels_per_pixel() {
    let src = picture(SIZE);
    let rows = [[0.0, 0.0, 1.0, 0.0], [0.5, 0.5, 0.0, 0.0], [1.0, 0.0, 0.0, 0.2], [0.0, 0.0, 0.0, 1.0]];
    let got = host(HostKernel::Matrix { rows }, &src, SIZE, None, SIZE);
    let expected: Vec<Pixel> = src.iter().map(|p| [p[2], 0.5 * p[0] + 0.5 * p[1], p[0] + 0.2 * p[3], p[3]]).collect();
    assert_close(&got, &expected, 1e-6, "matrix");
}

fn exact_to_linear(x: f64) -> f64 {
    let a = x.abs();
    (if a <= 0.04045 { a / 12.92 } else { ((a + 0.055) / 1.055).powf(2.4) }).copysign(x)
}

fn exact_to_srgb(x: f64) -> f64 {
    let a = x.abs();
    (if a <= 0.003_130_8 { a * 12.92 } else { 1.055 * a.powf(1.0 / 2.4) - 0.055 }).copysign(x)
}

#[test]
fn transfers_with_linear_negatives_continue_the_straight_segment_below_zero() {
    let pixels = [[-0.5, -0.01, 0.5, 0.7]];
    let decoded = host(HostKernel::Transfer { to: Encoding::Linear, negatives: Negatives::Linear }, &pixels, [1, 1], None, [1, 1]);
    let encoded = host(HostKernel::Transfer { to: Encoding::Srgb, negatives: Negatives::Linear }, &pixels, [1, 1], None, [1, 1]);
    assert_close(&decoded, &[[-0.5 / 12.92, -0.01 / 12.92, 0.214_041_14, 0.7]], 1e-6, "decode");
    assert_close(&encoded, &[[-6.46, -0.1292, 0.735_356_6, 0.7]], 1e-5, "encode");
    let mirrored = host(HostKernel::Transfer { to: Encoding::Linear, negatives: Negatives::Mirror }, &pixels, [1, 1], None, [1, 1]);
    assert_eq!(mirrored, host(HostKernel::ToLinear, &pixels, [1, 1], None, [1, 1]));
}

#[test]
fn srgb_transfers_follow_the_standard_curve_and_keep_alpha() {
    let values: Vec<f32> = (-400..=4000).map(|i| i as f32 / 200.0).chain([0.0, 1e-6, 1.0 / 16384.0, 15.999, 16.0, 300.0]).collect();
    let pixels: Vec<Pixel> = values.iter().map(|v| [*v, *v * 0.5, *v * 0.25, 0.7]).collect();
    let size = [pixels.len() as u32, 1];
    for (kernel, exact) in [(HostKernel::ToLinear, exact_to_linear as fn(f64) -> f64), (HostKernel::ToSrgb, exact_to_srgb)] {
        let got = host(kernel.clone(), &pixels, size, None, size);
        for (p, g) in pixels.iter().zip(&got) {
            for c in 0..3 {
                let want = exact(f64::from(p[c]));
                assert!((f64::from(g[c]) - want).abs() <= 1e-5 * want.abs().max(0.01), "{kernel:?} of {}: {} against {want}", p[c], g[c]);
            }
            assert_eq!(g[3], 0.7, "{kernel:?} keeps alpha");
        }
    }
}

#[test]
fn brightness_curve_scales_colour_by_the_curve_of_its_measure() {
    let curve = vec![0.0, 0.0, 0.5, 2.0];
    let range = 3.0;
    let lookup = |m: f32| {
        let t = m / range * 3.0;
        let i = (t as usize).min(2);
        curve[i] + (curve[i + 1] - curve[i]) * (t - i as f32)
    };
    let src = picture(SIZE);
    for measure in [Measure::MaxChannel, Measure::Luma] {
        let got = host(HostKernel::BrightnessCurve { measure, curve: curve.clone(), range }, &src, SIZE, None, SIZE);
        let expected: Vec<Pixel> = src
            .iter()
            .map(|p| {
                let m = match measure {
                    Measure::MaxChannel => p[0].max(p[1]).max(p[2]),
                    Measure::Luma => 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2],
                };
                let gain = if m > 0.0 { lookup(m) / m } else { 0.0 };
                [p[0] * gain, p[1] * gain, p[2] * gain, p[3]]
            })
            .collect();
        assert_close(&got, &expected, 1e-5, &format!("{measure:?}"));
    }
    let far = host(
        HostKernel::BrightnessCurve { measure: Measure::MaxChannel, curve, range },
        &[[4.0, 2.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.4]],
        [2, 1],
        None,
        [2, 1],
    );
    assert_close(&far, &[[3.5, 1.75, 0.0, 1.0], [0.0, 0.0, 0.0, 0.4]], 1e-6, "past the range the last segment continues; black stays black");
}

#[test]
fn dither_adds_at_most_one_level_of_seeded_noise_and_leaves_black_and_white() {
    let src: Vec<Pixel> = (0..64 * 64).map(|i| [0.5, if i % 2 == 0 { 0.0 } else { 1.0 }, 0.25, 1.0]).collect();
    let size = [64, 64];
    let got = host(HostKernel::Dither { seed: 7, origin: [3, -5] }, &src, size, None, size);
    let again = host(HostKernel::Dither { seed: 7, origin: [3, -5] }, &src, size, None, size);
    let other = host(HostKernel::Dither { seed: 8, origin: [3, -5] }, &src, size, None, size);
    assert_eq!(got, again, "the same seed gives the same noise");
    assert_ne!(got, other, "another seed gives other noise");
    let moved: Vec<f32> = got.iter().zip(&src).map(|(g, s)| g[0] - s[0]).collect();
    assert!(moved.iter().all(|d| d.abs() < 1.0 / 255.0), "within one level");
    let mean = moved.iter().sum::<f32>() / moved.len() as f32;
    assert!(mean.abs() < 0.02 / 255.0 && moved.iter().filter(|d| **d != 0.0).count() > 4000, "centred noise on every pixel: mean {mean}");
    assert!(got.iter().zip(&src).all(|(g, s)| g[1] == s[1] && g[3] == s[3]), "black, white and alpha are left exact");
    let shifted = host(HostKernel::Dither { seed: 7, origin: [4, -5] }, &src, size, None, size);
    assert_eq!(shifted[1][0], got[0][0], "the pattern is anchored at the origin");
}

#[test]
fn light_alpha_raises_alpha_to_the_brightest_channel() {
    let got = host(
        HostKernel::LightAlpha,
        &[[0.3, 0.6, 0.1, 0.2], [2.0, 0.0, 0.0, 0.5], [0.1, 0.1, 0.1, 0.9], [-1.0, -2.0, -3.0, 0.0]],
        [4, 1],
        None,
        [4, 1],
    );
    assert_eq!(got, [[0.3, 0.6, 0.1, 0.6], [2.0, 0.0, 0.0, 1.0], [0.1, 0.1, 0.1, 0.9], [-1.0, -2.0, -3.0, 0.0]]);
}

fn sequential(plan: &Plan, source: &[Pixel]) -> Vec<Pixel> {
    let mut images: Vec<Vec<Pixel>> = vec![Vec::new(); plan.images.len() + 1];
    let index = |image: PlanImage| match image {
        PlanImage::Output => 0,
        PlanImage::Scratch(i) => i as usize + 1,
        _ => usize::MAX,
    };
    for pass in &plan.passes {
        let read = |image: PlanImage| Image { size: SIZE, pixels: if image == PlanImage::Source { source } else { &images[index(image)] } };
        let mut out = vec![[0.0; 4]; source.len()];
        run(pass.host.as_ref().expect("a host kernel"), read(pass.src), pass.aux.map(read), SIZE, &mut out);
        images[index(pass.out)] = out;
    }
    std::mem::take(&mut images[0])
}

fn fused(plan: &Plan, source: &[Pixel]) -> Vec<Pixel> {
    let mut images: Vec<Vec<Pixel>> = vec![vec![[0.0; 4]; source.len()]; plan.images.len() + 1];
    let index = |image: PlanImage| match image {
        PlanImage::Output => 0,
        PlanImage::Scratch(i) => i as usize + 1,
        _ => usize::MAX,
    };
    for step in steps(plan, SIZE) {
        let Step::Fused(group) = step else { panic!("only pointwise kernels in this plan") };
        let inputs: Vec<Image> = group
            .inputs
            .iter()
            .map(|image| Image { size: SIZE, pixels: if *image == PlanImage::Source { source } else { &images[index(*image)] } })
            .collect();
        let mut out = vec![[0.0; 4]; source.len()];
        run_fused(&group, &inputs, SIZE[0] as usize, &mut out);
        images[index(group.out)] = out;
    }
    std::mem::take(&mut images[0])
}

#[test]
fn fused_pointwise_passes_give_exactly_what_they_give_one_by_one() {
    let source = picture(SIZE);
    let mut plan = Plan::new();
    let [a, b, c] = [plan.scratch(SIZE), plan.scratch(SIZE), plan.scratch(SIZE)];
    let rows = [[0.5, 0.2, 0.0, 0.0], [0.0, 1.0, 0.3, 0.0], [0.1, 0.0, 0.7, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let plan = plan
        .pass(PlanPass::to_linear(PlanImage::Source, a))
        .pass(PlanPass::matrix(a, b, rows))
        .pass(PlanPass::brightness_curve(a, a, Measure::Luma, vec![0.0, 0.2, 1.5], 1.0))
        .pass(PlanPass::blend_images(b, a, c, BlendMode::Screen, 0.8))
        .pass(PlanPass::blend_images(c, PlanImage::Source, c, BlendMode::Lighten, 1.0))
        .pass(PlanPass::to_srgb(c, c))
        .pass(PlanPass::dither(c, c, 9, [2, 2]))
        .pass(PlanPass::light_alpha(c, b))
        .pass(PlanPass::curves(b, PlanImage::Output, &[[0.0; 4], [1.0, 0.5, 1.0, 1.0]]));
    assert_eq!(plan.validate(SIZE), Ok(()), "pointwise host kernels may write the image they read");
    let groups: Vec<usize> = steps(&plan, SIZE).iter().map(|step| if let Step::Fused(group) = step { group.ops.len() } else { 0 }).collect();
    assert_eq!(groups, [9], "the whole chain is one loop");
    assert_eq!(fused(&plan, &source), sequential(&plan, &source));
}

#[test]
fn a_fused_loop_reads_an_overwritten_image_and_one_read_twice_as_written_at_that_point() {
    let source = vec![[0.25, 0.5, 0.75, 1.0]; (SIZE[0] * SIZE[1]) as usize];
    let mut plan = Plan::new();
    let [a, b] = [plan.scratch(SIZE), plan.scratch(SIZE)];
    let double = [[2.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let plan = plan
        .pass(PlanPass::matrix(PlanImage::Source, a, double))
        .pass(PlanPass::blend_images(a, PlanImage::Source, b, BlendMode::Add, 1.0))
        .pass(PlanPass::matrix(a, a, [[0.5, 0.0, 0.0, 0.0], [0.0, 0.5, 0.0, 0.0], [0.0, 0.0, 0.5, 0.0], [0.0, 0.0, 0.0, 0.5]]))
        .pass(PlanPass::blend_images(b, a, PlanImage::Output, BlendMode::Subtract, 1.0));
    assert_eq!(steps(&plan, SIZE).len(), 1, "one loop");
    let out = fused(&plan, &source);
    assert!(out.iter().all(|p| *p == [0.5, 1.0, 1.5, 1.5]), "{:?}", out[0]);
}

#[test]
fn a_fused_loop_ends_where_a_value_is_read_later_or_a_third_image_is_needed() {
    let mut plan = Plan::new();
    let [a, b, c, d] = [plan.scratch(SIZE), plan.scratch(SIZE), plan.scratch(SIZE), plan.scratch(SIZE)];
    let half = plan.scratch([19, 12]);
    let plan = plan
        .pass(PlanPass::to_linear(PlanImage::Source, a))
        .pass(PlanPass::to_srgb(a, b))
        .pass(PlanPass::resample(a, half, Filter::Bilinear))
        .pass(PlanPass::blend_images(b, PlanImage::Source, c, BlendMode::Add, 1.0))
        .pass(PlanPass::blend_images(c, a, d, BlendMode::Add, 1.0))
        .pass(PlanPass::resample(half, b, Filter::Bilinear))
        .pass(PlanPass::blend_images(d, b, PlanImage::Output, BlendMode::Add, 1.0));
    let shape: Vec<(Vec<usize>, Vec<Ref>)> = steps(&plan, SIZE)
        .into_iter()
        .map(|step| match step {
            Step::Single(index) => (vec![index], Vec::new()),
            Step::Fused(group) => (group.passes.collect(), group.ops.iter().map(|op| op.src).collect()),
        })
        .collect();
    assert_eq!(
        shape,
        [
            (vec![0], vec![Ref::Input(0)]),
            (vec![1], vec![Ref::Input(0)]),
            (vec![2], vec![]),
            (vec![3], vec![Ref::Input(0)]),
            (vec![4], vec![Ref::Input(0)]),
            (vec![5], vec![]),
            (vec![6], vec![Ref::Input(0)]),
        ]
    );
}

#[test]
fn on_the_gpu_a_resample_read_only_by_the_next_fused_loop_is_sampled_inside_it() {
    let mut plan = Plan::new();
    let (half, up) = (plan.scratch([19, 12]), plan.scratch(SIZE));
    let plan = plan
        .pass(PlanPass::resample(PlanImage::Source, half, Filter::Bilinear))
        .pass(PlanPass::resample(half, up, Filter::Bicubic))
        .pass(PlanPass::blend_images(PlanImage::Source, up, PlanImage::Output, BlendMode::Screen, 1.0));
    let (lowered, temps) = lower(&plan, SIZE);
    type Summary<'a> = (&'a str, StepImage, Option<StepImage>, Vec<u32>, bool);
    let steps: Vec<Summary> = lowered
        .iter()
        .map(|step| match step {
            Lowered::Host(step) => (
                step.entry,
                step.src,
                step.aux,
                step.u.clone(),
                step.wgsl.contains("craft_resampled(aux, p, vec2<f32>(pass_f(0u), pass_f(1u)), true)"),
            ),
            Lowered::Plugin(pass) => panic!("no plugin pass in this plan: {pass:?}"),
        })
        .collect();
    assert_eq!(
        steps,
        [
            ("craft_resample", StepImage::Plan(PlanImage::Source), None, vec![0], false),
            ("craft_pointwise", StepImage::Plan(PlanImage::Source), Some(StepImage::Plan(half)), vec![], true),
        ]
    );
    assert!(temps.is_empty());
}

#[test]
fn a_gaussian_too_small_to_matter_keeps_the_image() {
    assert_eq!(blur_taps(BlurShape::Gaussian, 1e-30), vec![0.0, 1.0, 0.0]);
    let src = [[0.5, 0.25, 0.125, 1.0]];
    let got = host(HostKernel::Blur { shape: BlurShape::Gaussian, radius: [1e-30, 0.0] }, &src, [1, 1], None, [1, 1]);
    assert_eq!(got, src);
}

#[test]
fn a_warp_that_lands_outside_the_image_or_overflows_reads_clear() {
    let src = [[0.5, 0.25, 0.125, 1.0]];
    let warp = |displacement: Pixel| host(HostKernel::Warp { amount: [2.0, 0.0] }, &src, [1, 1], Some(&[displacement]), [1, 1])[0];
    assert_eq!(warp([f32::MAX, 0.0, 0.0, 0.0]), [0.0; 4]);
    assert_eq!(warp([-f32::MAX, 0.0, 0.0, 0.0]), [0.0; 4]);
    assert_eq!(warp([f32::NAN, 0.0, 0.0, 0.0]), [0.0; 4]);
    assert_eq!(warp([-0.5, 0.0, 0.0, 0.0]), [0.0; 4]);
    assert_eq!(warp([0.25, 0.0, 0.0, 0.0]), [0.25, 0.125, 0.0625, 0.5]);
}

#[test]
fn bicubic_resampling_clears_a_pixel_whose_alpha_filters_to_nothing() {
    let src = [[0.0; 4], [1.0 / 16.0, 0.0, 0.0, 1.0 / 16.0], [0.0, 0.0, 0.0, 1.0]];
    let got = host(HostKernel::Resample { filter: Filter::Bicubic, scale: Some([0.5, 1.0]) }, &src, [3, 1], None, [6, 1]);
    assert_eq!(got[1], [0.0; 4]);
    assert!(got.iter().all(|p| p[3] > 0.0 || *p == [0.0; 4]), "{got:?}");
}

#[test]
fn a_blend_at_full_amount_is_the_blend_and_at_none_the_base() {
    let base = [16_777_216.0, 0.5, 0.25, 1.0];
    let blend = |mode, amount, top: Pixel| host(HostKernel::Blend { mode, amount }, &[base], [1, 1], Some(&[top]), [1, 1])[0];
    assert_eq!(blend(BlendMode::Over, 1.0, [0.25, 0.0, 0.0, 1.0]), [0.25, 0.0, 0.0, 1.0]);
    assert_eq!(blend(BlendMode::Over, 0.0, [f32::INFINITY, 0.0, 0.0, 1.0]), base);
    assert_eq!(blend(BlendMode::Add, 0.0, [f32::INFINITY, 0.0, 0.0, 1.0]), base);
}

#[test]
fn on_the_gpu_blurs_of_one_size_share_one_temporary() {
    let mut plan = Plan::new();
    let (soft, small, smaller) = (plan.scratch(SIZE), plan.scratch([5, 4]), plan.scratch([5, 4]));
    let mut plan = plan.pass(PlanPass::blur(PlanImage::Source, soft, [2.0; 2]));
    for k in 0..62 {
        let (src, out) = if k % 2 == 0 { (soft, PlanImage::Output) } else { (PlanImage::Output, soft) };
        plan = plan.pass(PlanPass::blur(src, out, [2.0; 2]));
    }
    let plan = plan.pass(PlanPass::resample(soft, small, Filter::Bilinear)).pass(PlanPass::blur(small, smaller, [1.0; 2]));
    assert_eq!(plan.validate(SIZE), Ok(()));
    let (lowered, temps) = lower(&plan, SIZE);
    assert_eq!(lowered.len(), 2 * 64 + 1);
    assert_eq!(temps, vec![SIZE, [5, 4]]);
}

#[test]
fn on_the_gpu_a_fused_loop_is_straight_line_code_whose_numbers_stay_in_its_data() {
    let plan = |amount: f32, mode: BlendMode, curve: Vec<f32>| {
        let mut plan = Plan::new();
        let soft = plan.scratch(SIZE);
        plan.pass(PlanPass::brightness_curve(PlanImage::Source, soft, Measure::Luma, curve, 2.0))
            .pass(PlanPass::blend_images(soft, PlanImage::Source, soft, mode, amount))
            .pass(PlanPass::light_alpha(soft, PlanImage::Output))
    };
    let step = |plan: &Plan| match lower(plan, SIZE).0.as_slice() {
        [Lowered::Host(step)] => step.clone(),
        other => panic!("one fused step: {other:?}"),
    };
    let screen = step(&plan(0.5, BlendMode::Screen, vec![0.0, 0.25, 2.0]));
    assert_eq!(
        screen.wgsl,
        "@compute @workgroup_size(16, 16)
fn craft_pointwise(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!inside_out(id.xy)) { return; }
    let p = vec2<i32>(id.xy);
    let i0 = texel(src, p);
    let v0 = craft_brightness_curve(0u, i0);
    let v1 = craft_amount(v0, v0 + i0 - v0 * i0, craft_data(6u));
    let v2 = craft_light_alpha(v1);
    store(id.xy, v2);
}
"
    );
    assert_eq!(screen.data, [1.0, 3.0, 2.0, 0.0, 0.25, 2.0, 0.5]);
    let other_numbers = step(&plan(0.75, BlendMode::Screen, vec![0.0, 0.5, 1.0]));
    assert_eq!((other_numbers.wgsl == screen.wgsl, other_numbers.data[6]), (true, 0.75));
    assert!(step(&plan(0.5, BlendMode::Multiply, vec![0.0, 0.25, 2.0])).wgsl.contains("craft_amount(v0, v0 * i0, craft_data(6u))"));
}

fn grey(values: &[f32]) -> Vec<Pixel> {
    values.iter().map(|v| [*v, *v, *v, 1.0]).collect()
}

#[test]
fn dilate_and_erode_take_the_largest_and_smallest_value_in_a_window_inside_the_image() {
    let row = grey(&[0.0, 0.0, 0.9, 0.0, 0.0]);
    assert_eq!(host(HostKernel::Dilate { radius: [1, 0] }, &row, [5, 1], None, [5, 1]), grey(&[0.0, 0.9, 0.9, 0.9, 0.0]));
    let held = grey(&[1.0, 1.0, 1.0, 0.0, 1.0]);
    assert_eq!(
        host(HostKernel::Erode { radius: [1, 0] }, &held, [5, 1], None, [5, 1]),
        grey(&[1.0, 1.0, 0.0, 0.0, 0.0]),
        "the frame edge does not erode"
    );
    let column = grey(&[0.2, 0.7, 0.4]);
    assert_eq!(host(HostKernel::Dilate { radius: [0, 1] }, &column, [1, 3], None, [1, 3]), grey(&[0.7, 0.7, 0.7]));
    assert_eq!(host(HostKernel::Erode { radius: [5, 5] }, &column, [1, 3], None, [1, 3]), grey(&[0.2, 0.2, 0.2]));
    let square = grey(&[0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(host(HostKernel::Dilate { radius: [1, 1] }, &square, [3, 3], None, [3, 3]), grey(&[0.5; 9]), "a square window");
}

#[test]
fn a_directional_blur_averages_samples_along_its_angle() {
    let row = grey(&[0.0, 3.0, 0.0, 0.0]);
    let along = host(HostKernel::DirectionalBlur { angle: 0.0, length: 2.0 }, &row, [4, 1], None, [4, 1]);
    let edge = 2.0 / 3.0;
    assert_close(&along, &[[1.0, 1.0, 1.0, edge], [1.0; 4], [1.0; 4], [0.0, 0.0, 0.0, edge]], 1e-6, "three samples, clear outside");
    let column = grey(&[0.0, 3.0, 0.0, 0.0]);
    let down = host(HostKernel::DirectionalBlur { angle: 90.0, length: 2.0 }, &column, [1, 4], None, [1, 4]);
    assert_close(&down, &along, 1e-6, "at 90 degrees it runs down the image");
    let source = picture(SIZE);
    assert_eq!(host(HostKernel::DirectionalBlur { angle: 30.0, length: 0.0 }, &source, SIZE, None, SIZE), source, "no length keeps the image");
}

#[test]
fn a_bilateral_filter_keeps_edges_beyond_its_tolerance_and_smooths_within_it() {
    let edge = grey(&[0.0, 0.0, 1.0, 1.0]);
    let kept = host(HostKernel::Bilateral { radius: 1, tolerance: 0.01 }, &edge, [4, 1], None, [4, 1]);
    assert_eq!(kept, edge, "a step far beyond the tolerance stays sharp");
    let smoothed = host(HostKernel::Bilateral { radius: 1, tolerance: 1000.0 }, &edge, [4, 1], None, [4, 1]);
    let w = (-2.0f32).exp();
    let near = w / (1.0 + 2.0 * w);
    assert_close(
        &smoothed,
        &[[0.0, 0.0, 0.0, 1.0], [near, near, near, 1.0], [1.0 - near, 1.0 - near, 1.0 - near, 1.0], [1.0; 4]],
        1e-6,
        "a spatial Gaussian",
    );
}

#[test]
fn noise_is_seeded_value_noise_anchored_at_its_origin() {
    let size = [64, 64];
    let source = picture(size);
    let noise = |seed, origin| host(HostKernel::Noise { scale: 1.0, octaves: 1, seed, origin }, &source, size, None, size);
    let got = noise(3, [0, 0]);
    assert_eq!(got[..2], [[0.5556614, 0.5556614, 0.5556614, 1.0], [0.30317625, 0.30317625, 0.30317625, 1.0]]);
    assert_eq!(noise(3, [0, 0]), got, "the same seed gives the same noise");
    assert_ne!(noise(4, [0, 0]), got, "another seed gives other noise");
    assert_eq!(noise(3, [1, 0])[1], got[0], "the pattern is anchored at the origin");
    assert!(got.iter().all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0 && (0.0..=1.0).contains(&p[0])), "opaque grey in 0..1");
    let mean = got.iter().map(|p| p[0]).sum::<f32>() / got.len() as f32;
    assert!((mean - 0.5).abs() < 0.03, "mean {mean}");
    let smooth = host(HostKernel::Noise { scale: 16.0, octaves: 4, seed: 3, origin: [0, 0] }, &source, size, None, size);
    let step = smooth.chunks(64).flat_map(|row| row.windows(2)).map(|pair| (pair[1][0] - pair[0][0]).abs()).fold(0.0, f32::max);
    assert!(step < 0.25, "neighbours stay close at a scale of 16 with four octaves: {step}");
}

#[test]
fn noise_fuses_with_the_pointwise_kernels_around_it() {
    let source = picture(SIZE);
    let mut plan = Plan::new();
    let grain = plan.scratch(SIZE);
    let plan = plan.pass(PlanPass::noise(grain, 8.0, 3, 1, [5, -7])).pass(PlanPass::blend_images(
        PlanImage::Source,
        grain,
        PlanImage::Output,
        BlendMode::Multiply,
        0.5,
    ));
    assert_eq!(plan.validate(SIZE), Ok(()));
    assert!(matches!(&steps(&plan, SIZE)[..], [Step::Fused(group)] if group.ops.len() == 2), "noise and the blend run as one loop");
    assert_eq!(fused(&plan, &source), sequential(&plan, &source));
}

#[test]
fn the_new_kernels_give_the_same_pixels_in_bands_on_every_core_as_on_one_thread() {
    let size = [64, 600];
    let source = picture(size);
    let kernels = [
        HostKernel::Dilate { radius: [3, 7] },
        HostKernel::Erode { radius: [7, 3] },
        HostKernel::DirectionalBlur { angle: 37.0, length: 9.5 },
        HostKernel::Bilateral { radius: 3, tolerance: 0.2 },
        HostKernel::Noise { scale: 6.0, octaves: 5, seed: 11, origin: [-3, 40] },
        HostKernel::HslRange { hue: [300.0, 40.0], saturation: [0.2, 1.0], lightness: [0.1, 0.9], softness: 0.1 },
        HostKernel::Splat {
            dabs: (0..300).map(|i| Dab::new([(i * 7 % 70) as f32, (i * 13 % 610) as f32], 3.0 + (i % 9) as f32, [0.2, 0.4, 0.1])).collect(),
        },
    ];
    let one_thread = rayon::ThreadPoolBuilder::new().num_threads(1).build().expect("a one-thread pool");
    for kernel in kernels {
        let banded = host(kernel.clone(), &source, size, None, size);
        let whole = one_thread.install(|| host(kernel.clone(), &source, size, None, size));
        assert!(banded == whole, "{}: bands and one pass differ", kernel.name());
        assert!(banded.iter().flatten().all(|v| v.is_finite()), "{}: every pixel is written", kernel.name());
    }
}

#[test]
fn hsl_range_gives_a_soft_grey_mask_of_straight_colours_and_wraps_hue() {
    let pixels = [[1.0, 0.0, 0.0, 1.0], [0.5, 0.0, 0.0, 0.5], [0.0, 1.0, 0.0, 1.0], [1.0, 0.5, 0.0, 1.0], [0.0; 4], [1.0, 0.0, 0.5, 1.0]];
    let select = |softness| HostKernel::HslRange { hue: [340.0, 20.0], saturation: [0.5, 1.0], lightness: [0.25, 0.75], softness };
    let mask = |kernel| host(kernel, &pixels, [6, 1], None, [6, 1]).iter().map(|p| p[0]).collect::<Vec<_>>();
    assert_eq!(mask(select(0.0)), [1.0, 0.5, 0.0, 0.0, 0.0, 0.0], "red is inside, half-covered red half, green and orange outside");
    let soft = mask(select(0.05));
    let ten_out = 1.0 - 10.0 / 18.0;
    assert!((soft[3] - ten_out).abs() < 1e-6 && (soft[5] - ten_out).abs() < 1e-6, "orange and pink are 10 degrees out each side: {soft:?}");
    assert!(host(select(0.05), &pixels, [6, 1], None, [6, 1]).iter().all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0), "opaque grey");
    let whole = HostKernel::HslRange { hue: [0.0, 360.0], saturation: [0.5, 1.0], lightness: [0.25, 0.75], softness: 0.0 };
    assert_eq!(mask(whole), [1.0, 0.5, 1.0, 1.0, 0.0, 1.0], "the full circle keeps every hue, green included");
    let bad = HostKernel::HslRange { hue: [0.0, 400.0], saturation: [0.0, 1.0], lightness: [0.0, 1.0], softness: 0.0 };
    assert!(bad.check([5, 1], None, [5, 1]).unwrap_err().contains("hue in degrees 0 to 360"));
}

#[test]
fn hsl_range_fuses_with_the_blend_that_uses_its_mask() {
    let source = picture(SIZE);
    let mut plan = Plan::new();
    let mask = plan.scratch(SIZE);
    let plan = plan.pass(PlanPass::hsl_range(PlanImage::Source, mask, [200.0, 30.0], [0.1, 1.0], [0.0, 0.8], 0.2)).pass(PlanPass::blend_images(
        PlanImage::Source,
        mask,
        PlanImage::Output,
        BlendMode::Multiply,
        1.0,
    ));
    assert_eq!(plan.validate(SIZE), Ok(()));
    assert!(matches!(&steps(&plan, SIZE)[..], [Step::Fused(group)] if group.ops.len() == 2), "the mask and the blend run as one loop");
    assert_eq!(fused(&plan, &source), sequential(&plan, &source));
}

#[test]
fn splat_adds_soft_dabs_as_light() {
    let clear = [[0.0; 4]; 5];
    let dab = Dab { colour: [1.0, 0.5, 0.0], ..Dab::new([2.5, 0.5], 2.0, [0.0; 3]) };
    let soft = host(HostKernel::Splat { dabs: vec![dab] }, &clear, [5, 1], None, [5, 1]);
    assert_eq!(soft, [[0.0; 4], [0.5, 0.25, 0.0, 0.5], [1.0, 0.5, 0.0, 1.0], [0.5, 0.25, 0.0, 0.5], [0.0; 4]]);
    let hard = host(HostKernel::Splat { dabs: vec![Dab { hardness: 0.5, opacity: 0.5, ..dab }] }, &clear, [5, 1], None, [5, 1]);
    assert_eq!(hard[1], [0.5, 0.25, 0.0, 0.5], "inside its hardness a dab has its full opacity");
    let twice = host(HostKernel::Splat { dabs: vec![dab, dab] }, &clear, [5, 1], None, [5, 1]);
    assert_eq!(twice[2], [2.0, 1.0, 0.0, 1.0], "light adds up and alpha stops at 1");
}

#[test]
fn splat_tiles_give_what_every_dab_on_every_pixel_gives() {
    let size = [53, 41];
    let source = picture(size);
    let dabs: Vec<Dab> = (0..200)
        .map(|i| Dab {
            opacity: 0.7,
            hardness: 0.3,
            ..Dab::new([(i * 37 % 90) as f32 - 20.0, (i * 23 % 70) as f32 - 15.0], 0.5 + (i % 23) as f32, [0.3, 0.1, 0.6])
        })
        .collect();
    let mut expected = source.clone();
    for (k, p) in expected.iter_mut().enumerate() {
        *p = dabs.iter().fold(*p, |p, dab| stamp(p, dab, k % 53, k / 53));
    }
    assert!(host(HostKernel::Splat { dabs: dabs.clone() }, &source, size, None, size) == expected, "tiles skip only dabs that miss the pixel");
    assert_ne!(expected, source, "the dabs reach the picture");
}

#[test]
fn splat_refuses_bad_dabs_and_too_much_area() {
    let check = |dabs: Vec<Dab>| HostKernel::Splat { dabs }.check([64, 64], None, [64, 64]);
    assert_eq!(check(vec![Dab::new([3.0, 4.0], 2.0, [1.0; 3])]), Ok(()));
    assert!(check(vec![Dab { hardness: 2.0, ..Dab::new([0.0; 2], 1.0, [1.0; 3]) }]).unwrap_err().starts_with("dab 0 is"));
    assert!(check(vec![Dab::new([0.0; 2], 1.0, [-1.0, 0.0, 0.0])]).is_err(), "negative light");
    assert!(check(vec![Dab::new([0.0; 2], 1.0, [0.0; 3]); craft_plugin::MAX_DABS + 1]).unwrap_err().contains("at most 65536"));
    let wide = HostKernel::Splat { dabs: vec![Dab::new([2048.0; 2], 256.0, [1.0; 3]); 9000] };
    assert!(wide.check([4096, 4096], None, [4096, 4096]).unwrap_err().contains("tiles"), "9000 dabs of 33x33 tiles");
}

#[test]
fn a_histogram_counts_straight_values_per_channel() {
    let counts = craft_plugin::histogram(&[[0.5, 0.25, 0.0, 0.5], [1.0, 1.0, 1.0, 1.0], [0.0; 4], [2.0, -1.0, 0.5, 1.0]]);
    assert_eq!([counts[0][255], counts[0][0], counts[1][128], counts[1][0], counts[2][128], counts[3][128]], [3, 1, 1, 2, 1, 1]);
    assert_eq!(counts.map(|channel| channel.iter().sum::<u32>()), [4; 4]);
}

fn pattern(x: f32, y: f32) -> f32 {
    (0..6)
        .map(|octave| {
            let (frequency, angle) = (0.4 / (1 << octave) as f32, 0.9 + 2.1 * octave as f32);
            let along = x * angle.cos() + y * angle.sin();
            let across = -x * angle.sin() + y * angle.cos();
            0.08 * ((along * frequency).sin() + (across * frequency * 0.7 + octave as f32).sin())
        })
        .sum::<f32>()
        + 0.5
}

fn moved(size: [u32; 2], to_source: impl Fn(f32, f32) -> (f32, f32)) -> Vec<Pixel> {
    (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| (x as f32, y as f32)))
        .map(|(x, y)| {
            let (sx, sy) = to_source(x, y);
            let v = pattern(sx, sy);
            [v, v, v, 1.0]
        })
        .collect()
}

fn endpoint_error(flow: &[Pixel], size: [u32; 2], truth: impl Fn(f32, f32) -> [f32; 2], margin: u32) -> (f32, f32) {
    let (mut sum, mut worst, mut n) = (0.0, 0.0f32, 0);
    for y in margin..size[1] - margin {
        for x in margin..size[0] - margin {
            let p = flow[(y * size[0] + x) as usize];
            let t = truth(x as f32, y as f32);
            let e = ((p[0] - t[0]).powi(2) + (p[1] - t[1]).powi(2)).sqrt();
            sum += e;
            worst = worst.max(e);
            n += 1;
        }
    }
    (sum / n as f32, worst)
}

#[test]
fn flow_follows_a_translated_and_a_rotated_pattern_within_a_tenth_of_a_pixel_on_average() {
    let size = [256, 192];
    let centre = [128.0, 96.0];
    let (shift, angle) = ([6.3, -4.6], 3.0f32.to_radians());
    let still = moved(size, |x, y| (x, y));
    let translated = moved(size, |x, y| (x - shift[0], y - shift[1]));
    let rotated = moved(size, |x, y| {
        let (dx, dy) = (x - centre[0], y - centre[1]);
        (centre[0] + angle.cos() * dx + angle.sin() * dy, centre[1] - angle.sin() * dx + angle.cos() * dy)
    });
    let flow = |to: &[Pixel]| host(HostKernel::Flow { levels: 6, radius: 3, iterations: 2 }, &still, size, Some(to), size);
    let (mean, worst) = endpoint_error(&flow(&translated), size, |_, _| shift, 32);
    assert!(mean < 0.05 && worst < 0.3, "translation: mean {mean} worst {worst}");
    let turned = |x: f32, y: f32| {
        let (dx, dy) = (x - centre[0], y - centre[1]);
        [centre[0] + angle.cos() * dx - angle.sin() * dy - x, centre[1] + angle.sin() * dx + angle.cos() * dy - y]
    };
    let (mean, worst) = endpoint_error(&flow(&rotated), size, turned, 32);
    assert!(mean < 0.2 && worst < 0.7, "rotation: mean {mean} worst {worst}");
}

#[test]
fn flow_refuses_settings_out_of_range_and_a_missing_second_frame() {
    let check = |levels, radius, iterations, aux| HostKernel::Flow { levels, radius, iterations }.check([64, 64], aux, [64, 64]);
    assert_eq!(check(6, 3, 2, Some([64, 64])), Ok(()));
    for (levels, radius, iterations) in [(0, 3, 2), (9, 3, 2), (6, 0, 2), (6, 9, 2), (6, 3, 0), (6, 3, 9)] {
        assert!(check(levels, radius, iterations, Some([64, 64])).unwrap_err().contains("give 1 to 8 levels"), "{levels} {radius} {iterations}");
    }
    assert!(check(6, 3, 2, None).unwrap_err().contains("frame to track to"));
    assert!(check(6, 3, 2, Some([32, 64])).unwrap_err().contains("must be the same size"));
}

#[test]
fn flow_on_the_gpu_runs_the_same_steps_and_two_flows_share_their_working_images() {
    let mut plan = Plan::new();
    let (forward, backward) = (plan.scratch([64, 48]), plan.scratch([64, 48]));
    let later = plan.source_at(1.0);
    let plan = plan
        .pass(PlanPass::flow(PlanImage::Source, later, forward))
        .pass(PlanPass::flow(later, PlanImage::Source, backward))
        .pass(PlanPass::warp(PlanImage::Source, forward, PlanImage::Output, [1.0, 1.0]));
    assert_eq!(plan.validate([64, 48]), Ok(()));
    let (lowered, temps) = lower(&plan, [64, 48]);
    let entries: Vec<&str> = lowered.iter().filter_map(|step| if let Lowered::Host(step) = step { Some(step.entry) } else { None }).collect();
    let levels = craft_plugin::flow_levels([64, 48], 6).len();
    assert_eq!(levels, 3);
    let one = 2 + 2 * (levels - 1) + 2 * levels;
    assert_eq!(entries.len(), 2 * one + 1);
    assert_eq!(&entries[..4], ["craft_flow_pack", "craft_flow_gradient", "craft_resample", "craft_flow_gradient"]);
    assert_eq!(temps.len(), 3 * levels, "the second flow reuses the first one's images");
}
