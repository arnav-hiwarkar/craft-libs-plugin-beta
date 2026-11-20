mod hist;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::ops::{Range, RangeInclusive};
use std::time::Duration;

pub use hist::{Hist, Quantile, Reading};
use hist::{OVERFLOW, reading, slot_at};

pub const RESAMPLES: usize = 10_000;
pub const LOOKS: usize = 5;
pub const AA_VERDICTS: usize = 59;
const T_995: [f64; 30] = [
    63.657, 9.925, 5.841, 4.604, 4.032, 3.707, 3.499, 3.355, 3.250, 3.169, 3.106, 3.055, 3.012, 2.977, 2.947, 2.921, 2.898, 2.878, 2.861, 2.845,
    2.831, 2.819, 2.807, 2.797, 2.787, 2.779, 2.771, 2.763, 2.756, 2.750,
];
const T_995_BEYOND: [(usize, f64); 3] = [(40, 2.750), (60, 2.704), (120, 2.660)];
const T_995_LIMIT: f64 = 2.617;
const MIN_UNITS_PER_STRATUM: usize = 2;
const CONTRASTS: [(Arm, Arm); 3] = [(Arm::A, Arm::B), (Arm::A0, Arm::A), (Arm::A, Arm::AA)];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arm {
    A0,
    A,
    B,
    AA,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gate {
    UiEqCpu,
    UiEqLatency,
    UiEqExtractionLatency,
    UiLoadLatency,
    UiLoadStrokeLatency,
    OverheadCpu,
    OverheadInterval,
    PlaybackSeek,
    CommandWallMedian,
    CommandWallTail,
    ExportWall,
    ExportGpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floor {
    Fixed(Duration),
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowance {
    pub floor: Floor,
    pub per_mille: u32,
    pub ceiling: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSpec {
    pub quantile: Quantile,
    pub allowance: Allowance,
}

impl Gate {
    pub const fn spec(self) -> GateSpec {
        let none = Floor::Fixed(Duration::ZERO);
        let (quantile, floor, per_mille, ceiling) = match self {
            Gate::UiEqCpu => (Quantile::P95, Floor::Fixed(Duration::from_millis(1)), 100, None),
            Gate::UiEqLatency | Gate::UiLoadLatency => (Quantile::P95, Floor::Refresh, 100, None),
            Gate::UiEqExtractionLatency | Gate::UiLoadStrokeLatency => (Quantile::P95, Floor::Fixed(Duration::from_millis(2)), 100, None),
            Gate::OverheadCpu | Gate::OverheadInterval => (Quantile::P50, none, 150, None),
            Gate::PlaybackSeek => (Quantile::P95, Floor::Fixed(Duration::from_millis(50)), 100, None),
            Gate::CommandWallMedian => (Quantile::P50, Floor::Fixed(Duration::from_millis(50)), 100, None),
            Gate::CommandWallTail => (Quantile::P95, Floor::Fixed(Duration::from_millis(50)), 100, Some(Duration::from_millis(100))),
            Gate::ExportWall | Gate::ExportGpu => (Quantile::P50, none, 100, None),
        };
        GateSpec { quantile, allowance: Allowance { floor, per_mille, ceiling } }
    }
}

impl Allowance {
    pub fn allow(&self, base: Duration, refresh_hz: NonZeroU32) -> Duration {
        let floor = match self.floor {
            Floor::Fixed(floor) => floor,
            Floor::Refresh => Duration::from_nanos(1_000_000_000 / u64::from(refresh_hz.get())),
        };
        let share = base.as_nanos() * u128::from(self.per_mille) / 1_000;
        let widest = floor.max(Duration::from_nanos(u64::try_from(share).unwrap_or(u64::MAX)));
        match self.ceiling {
            Some(ceiling) if base <= ceiling => widest.min(ceiling.saturating_sub(base)),
            _ => widest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub test_id: String,
    pub run_id: String,
    pub app_pin: String,
    pub fw_rev: String,
    pub os: String,
    pub runner_image: String,
    pub night: String,
    pub block: u32,
    pub arm: Arm,
    pub gate: Gate,
    pub refresh_hz: NonZeroU32,
    pub samples: Hist,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Inconclusive(Doubt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doubt {
    Noisy,
    Budget,
    NoRunner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub allow: Duration,
    pub delta_ns: i64,
    pub low_ns: i64,
    pub high_ns: i64,
}

impl Measurement {
    pub fn verdict(&self) -> Verdict {
        let allow = i64::try_from(self.allow.as_nanos()).unwrap_or(i64::MAX);
        if self.high_ns <= allow {
            Verdict::Pass
        } else if self.low_ns > allow {
            Verdict::Fail
        } else {
            Verdict::Inconclusive(Doubt::Noisy)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Measured(Measurement),
    Unmeasurable,
    OverBudget,
    NoRunner,
}

impl Decision {
    pub fn verdict(&self) -> Verdict {
        match self {
            Decision::Measured(measurement) => measurement.verdict(),
            Decision::Unmeasurable => Verdict::Inconclusive(Doubt::Noisy),
            Decision::OverBudget => Verdict::Inconclusive(Doubt::Budget),
            Decision::NoRunner => Verdict::Inconclusive(Doubt::NoRunner),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    pub injected_delay: Verdict,
    pub gpu_heavy: Verdict,
    pub null: Verdict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Calibration {
    pub gate: Gate,
    pub runner_image: String,
    pub aa: Vec<Verdict>,
    pub controls: Controls,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eligibility {
    Eligible,
    TooFewAa(usize),
    AaFailed(usize),
    ControlsMisbehaved,
}

impl Calibration {
    pub fn eligibility(&self) -> Eligibility {
        let fails = self.aa.iter().filter(|verdict| **verdict == Verdict::Fail).count();
        let behaved = Controls { injected_delay: Verdict::Fail, gpu_heavy: Verdict::Fail, null: Verdict::Pass };
        if fails > 0 {
            Eligibility::AaFailed(fails)
        } else if self.aa.len() < AA_VERDICTS {
            Eligibility::TooFewAa(self.aa.len())
        } else if self.controls != behaved {
            Eligibility::ControlsMisbehaved
        } else {
            Eligibility::Eligible
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairError {
    NoRecords,
    MixedDigest { run_id: String },
    Arms(Vec<Arm>),
    Unpaired { night: String, runner_image: String, block: u32 },
}

pub fn decide(records: &[RunRecord], calibrations: &[Calibration], seed: u64) -> Result<Decision, PairError> {
    let pool = Pool::new(records)?;
    let eligible = |image: &str| {
        calibrations.iter().any(|calibration| {
            calibration.gate == pool.gate && calibration.runner_image == image && calibration.eligibility() == Eligibility::Eligible
        })
    };
    if !pool.strata.iter().all(|stratum| eligible(stratum.image)) {
        return Ok(Decision::NoRunner);
    }
    let looks = pool.strata.iter().map(|stratum| stratum.night).collect::<BTreeSet<_>>().len();
    if looks > LOOKS {
        return Ok(Decision::OverBudget);
    }
    Ok(pool.measure(seed))
}

pub fn calibration_verdict(records: &[RunRecord], seed: u64) -> Result<Verdict, PairError> {
    Ok(Pool::new(records)?.measure(seed).verdict())
}

type Unit = [Vec<u64>; 2];
type Draw<'a> = [(u64, &'a Unit)];

struct Stratum<'a> {
    night: &'a str,
    image: &'a str,
    units: Range<usize>,
}

struct Pool<'a> {
    gate: Gate,
    refresh_hz: NonZeroU32,
    units: Vec<Unit>,
    strata: Vec<Stratum<'a>>,
}

type Digest<'a> = (&'a str, &'a str, &'a str, &'a str, Gate, NonZeroU32);

fn digest(record: &RunRecord) -> Digest<'_> {
    (&record.test_id, &record.app_pin, &record.fw_rev, &record.os, record.gate, record.refresh_hz)
}

impl<'a> Pool<'a> {
    fn new(records: &'a [RunRecord]) -> Result<Self, PairError> {
        let first = records.first().ok_or(PairError::NoRecords)?;
        if let Some(stray) = records.iter().find(|record| digest(record) != digest(first)) {
            return Err(PairError::MixedDigest { run_id: stray.run_id.clone() });
        }
        let arms: BTreeSet<Arm> = records.iter().map(|record| record.arm).collect();
        let test = CONTRASTS
            .into_iter()
            .find(|(base, test)| arms == BTreeSet::from([*base, *test]))
            .map(|(_, test)| test)
            .ok_or_else(|| PairError::Arms(arms.iter().copied().collect()))?;
        let mut blocks: BTreeMap<(&str, &str), BTreeMap<u32, [Hist; 2]>> = BTreeMap::new();
        for record in records {
            let unit = blocks.entry((&record.night, &record.runner_image)).or_default().entry(record.block).or_default();
            if let Some(side) = unit.get_mut(usize::from(record.arm == test)) {
                side.merge(&record.samples);
            }
        }
        let mut pool = Pool { gate: first.gate, refresh_hz: first.refresh_hz, units: Vec::new(), strata: Vec::new() };
        for ((night, image), stratum) in blocks {
            let start = pool.units.len();
            for (block, [base, test]) in stratum {
                if base.count() == 0 || test.count() == 0 {
                    return Err(PairError::Unpaired { night: night.to_owned(), runner_image: image.to_owned(), block });
                }
                pool.units.push([base.cumulative(), test.cumulative()]);
            }
            pool.strata.push(Stratum { night, image, units: start..pool.units.len() });
        }
        Ok(pool)
    }

    fn measure(&self, seed: u64) -> Decision {
        if self.strata.iter().any(|stratum| stratum.units.len() < MIN_UNITS_PER_STRATUM) {
            return Decision::Unmeasurable;
        }
        let quantile = self.gate.spec().quantile;
        let everything: Vec<(u64, &Unit)> = self.units.iter().map(|unit| (1, unit)).collect();
        let windows = [0, 1].map(|side| self.window(quantile, side));
        let Some((base, test)) = quantiles(&everything, quantile, &windows) else {
            return Decision::Unmeasurable;
        };
        let allow = self.gate.spec().allowance.allow(Duration::from_nanos(base), self.refresh_hz);
        let mut rng = SplitMix64(seed);
        let mut weights = vec![0u32; self.units.len()];
        let mut draw = Vec::with_capacity(self.units.len());
        let mut deltas = Vec::with_capacity(RESAMPLES);
        for _ in 0..RESAMPLES {
            weights.fill(0);
            for stratum in &self.strata {
                for _ in 1..stratum.units.len() {
                    if let Some(weight) = weights.get_mut(stratum.units.start + rng.below(stratum.units.len())) {
                        *weight += 1;
                    }
                }
            }
            draw.clear();
            draw.extend(self.units.iter().zip(&weights).filter(|(_, weight)| **weight > 0).map(|(unit, weight)| (u64::from(*weight), unit)));
            match quantiles(&draw, quantile, &windows) {
                Some((base, test)) => deltas.push(signed(test) - signed(base)),
                None => return Decision::Unmeasurable,
            }
        }
        let delta_ns = signed(test) - signed(base);
        let spread = critical_t(self.units.len() - self.strata.len()) * standard_deviation(&deltas);
        let reach = |offset: f64| (delta_ns as f64 + offset).round() as i64;
        Decision::Measured(Measurement { allow, delta_ns, low_ns: reach(-spread), high_ns: reach(spread) })
    }

    fn window(&self, quantile: Quantile, side: usize) -> RangeInclusive<usize> {
        let slots = self.units.iter().filter_map(|unit| {
            let cumulative = unit.get(side)?;
            slot_at(quantile, 0..=OVERFLOW, |slot| cumulative.get(slot).copied().unwrap_or(0))
        });
        let (low, high) = slots.fold((OVERFLOW, 0), |(low, high), slot| (low.min(slot), high.max(slot)));
        low.min(high)..=high
    }
}

fn quantiles(draw: &Draw, quantile: Quantile, windows: &[RangeInclusive<usize>; 2]) -> Option<(u64, u64)> {
    let [base, test] = windows.clone();
    Some((nanos(draw, quantile, 0, base)?, nanos(draw, quantile, 1, test)?))
}

fn nanos(draw: &Draw, quantile: Quantile, side: usize, window: RangeInclusive<usize>) -> Option<u64> {
    let cumulative = |slot: usize| {
        draw.iter()
            .map(|(weight, unit)| weight.saturating_mul(unit.get(side).and_then(|counts| counts.get(slot)).copied().unwrap_or(0)))
            .fold(0u64, u64::saturating_add)
    };
    match reading(slot_at(quantile, window, cumulative)?) {
        Reading::Below => Some(0),
        Reading::At(value) => u64::try_from(value.as_nanos()).ok(),
        Reading::Above => None,
    }
}

fn critical_t(degrees: usize) -> f64 {
    match T_995.get(degrees.saturating_sub(1)) {
        Some(&t) => t,
        None => T_995_BEYOND.iter().find(|(most, _)| degrees <= *most).map_or(T_995_LIMIT, |&(_, t)| t),
    }
}

fn standard_deviation(values: &[i64]) -> f64 {
    let count = values.len().max(2) as f64;
    let mean = values.iter().map(|&value| value as f64).sum::<f64>() / count;
    (values.iter().map(|&value| (value as f64 - mean).powi(2)).sum::<f64>() / (count - 1.0)).sqrt()
}

fn signed(nanos: u64) -> i64 {
    i64::try_from(nanos).unwrap_or(i64::MAX)
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
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
