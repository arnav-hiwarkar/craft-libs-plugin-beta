use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::ops::Range;

use super::hist::{HIGHEST_NS, OVERFLOW, first_slot, half_width, reading};
use super::{Arm, Count, Decision, Doubt, Gate, Hist, Ident, Kind, Measurement, PairError, Quantile, RESAMPLES, Reading, RunRecord, Samples};

const T_995: [f64; 30] = [
    63.657, 9.925, 5.841, 4.604, 4.032, 3.707, 3.499, 3.355, 3.250, 3.169, 3.106, 3.055, 3.012, 2.977, 2.947, 2.921, 2.898, 2.878, 2.861, 2.845,
    2.831, 2.819, 2.807, 2.797, 2.787, 2.779, 2.771, 2.763, 2.756, 2.750,
];
const T_995_BEYOND: [(usize, f64); 3] = [(40, 2.750), (60, 2.704), (120, 2.660)];
const T_995_LIMIT: f64 = 2.617;
const MIN_UNITS_PER_STRATUM: usize = 2;
const CONTRASTS: [(Arm, Arm); 3] = [(Arm::A, Arm::B), (Arm::A0, Arm::A), (Arm::A, Arm::AA)];
const PARTS_PER_BILLION: f64 = 1e9;
const Z_995: f64 = 2.576;

enum Units {
    Latency { quantile: Quantile, cumulative: Vec<[Vec<u64>; 2]> },
    Rate(Vec<[Count; 2]>),
}

impl Units {
    fn len(&self) -> usize {
        match self {
            Units::Latency { cumulative, .. } => cumulative.len(),
            Units::Rate(counts) => counts.len(),
        }
    }
}

pub(super) struct Pool<'a> {
    gate: Gate,
    refresh_hz: NonZeroU32,
    images: BTreeSet<&'a Ident>,
    nights: BTreeSet<&'a Ident>,
    units: Units,
    strata: Vec<Range<usize>>,
    excluded_strata: usize,
    seed: u64,
}

struct Estimate {
    base: f64,
    test: f64,
    censored: bool,
    widening: f64,
    sampling: f64,
}

type Digest<'a> = (&'a Ident, &'a Ident, &'a Ident, &'a Ident, &'a super::Workload, Gate, NonZeroU32);

fn digest(record: &RunRecord) -> Digest<'_> {
    (&record.test_id, &record.app_pin, &record.fw_rev, &record.os, &record.workload, record.gate, record.refresh_hz)
}

type Blocks<'a> = BTreeMap<(&'a Ident, &'a Ident), BTreeMap<u32, [Vec<&'a RunRecord>; 2]>>;

impl<'a> Pool<'a> {
    pub(super) fn new(records: &'a [RunRecord]) -> Result<Self, PairError> {
        let first = records.first().ok_or(PairError::NoRecords)?;
        if let Some(stray) = records.iter().find(|record| digest(record) != digest(first)) {
            return Err(PairError::MixedDigest { run_id: stray.run_id.clone() });
        }
        let mut run_ids = BTreeSet::new();
        if let Some(repeat) = records.iter().find(|record| !run_ids.insert(&record.run_id)) {
            return Err(PairError::DuplicateRun { run_id: repeat.run_id.clone() });
        }
        if let Some(stray) = records.iter().find(|record| !record.samples.fits(record.gate)) {
            return Err(PairError::SamplesKind { run_id: stray.run_id.clone() });
        }
        let arms: BTreeSet<Arm> = records.iter().map(|record| record.arm).collect();
        let test = CONTRASTS
            .into_iter()
            .find(|(base, test)| arms == BTreeSet::from([*base, *test]))
            .map(|(_, test)| test)
            .ok_or_else(|| PairError::Arms(arms.iter().copied().collect()))?;
        let mut blocks: Blocks = BTreeMap::new();
        for record in records {
            let unit = blocks.entry((&record.night, &record.runner_image)).or_default().entry(record.block).or_default();
            if let Some(side) = unit.get_mut(usize::from(record.arm == test)) {
                side.push(record);
            }
        }
        let mut pool = Pool {
            gate: first.gate,
            refresh_hz: first.refresh_hz,
            images: records.iter().map(|record| &record.runner_image).collect(),
            nights: records.iter().map(|record| &record.night).collect(),
            units: match first.gate.kind() {
                Kind::Latency { quantile, .. } => Units::Latency { quantile, cumulative: Vec::new() },
                Kind::Rate { .. } => Units::Rate(Vec::new()),
            },
            strata: Vec::new(),
            excluded_strata: 0,
            seed: seed(&blocks),
        };
        for ((night, image), stratum) in blocks {
            pool.add_stratum(night, image, stratum)?;
        }
        Ok(pool)
    }

    fn add_stratum(&mut self, night: &Ident, image: &Ident, stratum: BTreeMap<u32, [Vec<&RunRecord>; 2]>) -> Result<(), PairError> {
        let unpaired = |block| PairError::Unpaired { night: night.clone(), runner_image: image.clone(), block };
        let start = self.units.len();
        let size = stratum.len();
        let keep = size >= MIN_UNITS_PER_STRATUM;
        for (block, sides) in stratum {
            match &mut self.units {
                Units::Latency { cumulative, .. } => {
                    let [base, test] = sides.map(|side| merged(&side));
                    if base.count() == 0 || test.count() == 0 {
                        return Err(unpaired(block));
                    }
                    if keep {
                        cumulative.push([base.cumulative(), test.cumulative()]);
                    }
                }
                Units::Rate(counts) => {
                    let [base, test] = sides.map(|side| summed(&side));
                    if base.frames() == 0 || test.frames() == 0 {
                        return Err(unpaired(block));
                    }
                    if keep {
                        counts.push([base, test]);
                    }
                }
            }
        }
        if keep {
            self.strata.push(start..start + size);
        } else {
            self.excluded_strata += 1;
        }
        Ok(())
    }

    pub(super) fn gate(&self) -> Gate {
        self.gate
    }

    pub(super) fn images(&self) -> impl Iterator<Item = &Ident> {
        self.images.iter().copied()
    }

    pub(super) fn looks(&self) -> usize {
        self.nights.len()
    }

    pub(super) fn measure(&self) -> Decision {
        let Some(point) = self.estimate(&vec![1.0; self.units.len()]) else {
            return Decision::Withheld(Doubt::Noisy);
        };
        let allow = self.gate.kind().allow(point.base.round() as u64, self.refresh_hz);
        let mut rng = SplitMix64(self.seed);
        let mut weights = vec![0.0; self.units.len()];
        let mut deltas = Vec::with_capacity(RESAMPLES);
        let mut censored = point.censored;
        for _ in 0..RESAMPLES {
            weights.fill(0.0);
            for stratum in &self.strata {
                let size = stratum.len();
                let rescale = size as f64 / (size - 1) as f64;
                for _ in 1..size {
                    if let Some(weight) = weights.get_mut(stratum.start + rng.below(size)) {
                        *weight += rescale;
                    }
                }
            }
            let Some(resample) = self.estimate(&weights) else {
                return Decision::Withheld(Doubt::Noisy);
            };
            censored |= resample.censored;
            deltas.push(resample.test - resample.base);
        }
        let delta = point.test - point.base;
        let bootstrap = critical_t(self.units.len() - self.strata.len()) * standard_deviation(&deltas);
        let spread = (bootstrap * bootstrap + point.sampling * point.sampling).sqrt() + point.widening;
        Decision::Measured(Measurement {
            unit: self.gate.kind().unit(),
            allow,
            delta: delta.round() as i64,
            low: (delta - spread).round() as i64,
            high: if censored { i64::MAX } else { (delta + spread).round() as i64 },
            excluded_strata: self.excluded_strata,
        })
    }

    fn estimate(&self, weights: &[f64]) -> Option<Estimate> {
        let drawn = || weights.iter().enumerate().filter(|(_, weight)| **weight > 0.0);
        match &self.units {
            Units::Latency { quantile, cumulative } => {
                let draw: Vec<(f64, &[Vec<u64>; 2])> = drawn().filter_map(|(index, weight)| Some((*weight, cumulative.get(index)?))).collect();
                let side = |arm: usize| {
                    let at = |slot: usize| {
                        draw.iter()
                            .map(|(weight, unit)| weight * unit.get(arm).and_then(|counts| counts.get(slot)).copied().unwrap_or(0) as f64)
                            .sum::<f64>()
                    };
                    let target = quantile.share_of(at(OVERFLOW));
                    let slot = (target > 0.0).then(|| first_slot(|slot| at(slot) >= target))?;
                    Some((reading(slot), half_width(slot) as f64))
                };
                let (base, base_width) = match side(0)? {
                    (Reading::Below, width) => (0.0, width),
                    (Reading::At(value), width) => (value.as_nanos() as f64, width),
                    (Reading::Above, _) => return None,
                };
                let (test, test_width, censored) = match side(1)? {
                    (Reading::Below, width) => (0.0, width, false),
                    (Reading::At(value), width) => (value.as_nanos() as f64, width, false),
                    (Reading::Above, _) => (HIGHEST_NS as f64, 0.0, true),
                };
                Some(Estimate { base, test, censored, widening: base_width + test_width, sampling: 0.0 })
            }
            Units::Rate(counts) => {
                let totals = |arm: usize| {
                    let (events, frames) = drawn()
                        .filter_map(|(index, weight)| Some((*weight, counts.get(index)?.get(arm)?)))
                        .fold((0.0, 0.0), |(events, frames), (weight, count)| {
                            (events + weight * count.events() as f64, frames + weight * count.frames() as f64)
                        });
                    (frames > 0.0).then_some((events, frames))
                };
                let (base, test) = (totals(0)?, totals(1)?);
                let sampling = Z_995 * PARTS_PER_BILLION * (wilson_variance(base) + wilson_variance(test)).sqrt();
                Some(Estimate {
                    base: PARTS_PER_BILLION * base.0 / base.1,
                    test: PARTS_PER_BILLION * test.0 / test.1,
                    censored: false,
                    widening: 0.0,
                    sampling,
                })
            }
        }
    }
}

fn wilson_variance((events, frames): (f64, f64)) -> f64 {
    let padded = frames + Z_995 * Z_995;
    let rate = (events + Z_995 * Z_995 / 2.0) / padded;
    rate * (1.0 - rate) / padded
}

fn seed(blocks: &Blocks) -> u64 {
    let mut lines: Vec<String> = Vec::new();
    for (stratum, units) in blocks.values().enumerate() {
        for (unit, sides) in units.values().enumerate() {
            for record in sides.iter().flatten() {
                let workload = &record.workload;
                lines.push(format!(
                    "{stratum} {unit} {} {} {} {} {} {} {} {} {}",
                    record.arm.name(),
                    record.gate.name(),
                    workload.fixture_hash,
                    workload.mode,
                    workload.backend,
                    workload.load,
                    workload.scale,
                    record.refresh_hz,
                    record.samples,
                ));
            }
        }
    }
    lines.sort_unstable();
    lines
        .iter()
        .flat_map(|line| line.bytes().chain(*b"\n"))
        .fold(0xCBF2_9CE4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3))
}

fn merged(records: &[&RunRecord]) -> Hist {
    let mut hist = Hist::new();
    for record in records {
        if let Samples::Latency(more) = &record.samples {
            hist.merge(more);
        }
    }
    hist
}

fn summed(records: &[&RunRecord]) -> Count {
    let mut total = Count::default();
    for record in records {
        if let Samples::Count(more) = &record.samples {
            total.add(more);
        }
    }
    total
}

fn critical_t(degrees: usize) -> f64 {
    match T_995.get(degrees.saturating_sub(1)) {
        Some(&t) => t,
        None => T_995_BEYOND.iter().find(|(most, _)| degrees <= *most).map_or(T_995_LIMIT, |&(_, t)| t),
    }
}

fn standard_deviation(values: &[f64]) -> f64 {
    let count = values.len().max(2) as f64;
    let mean = values.iter().sum::<f64>() / count;
    (values.iter().map(|value| (value - mean) * (value - mean)).sum::<f64>() / (count - 1.0)).sqrt()
}

pub(super) struct SplitMix64(pub(super) u64);

impl SplitMix64 {
    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        ((u128::from(self.next()) * bound as u128) >> 64) as usize
    }
}
