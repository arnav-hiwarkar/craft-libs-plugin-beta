mod bootstrap;
mod calibration;
mod gate;
mod hist;
mod line;
#[cfg(test)]
mod tests;

use std::fmt;
use std::num::NonZeroU32;

pub use calibration::{AA_VERDICTS, Calibration, Controls, Eligibility};
pub use gate::{Floor, Gate, Kind, Unit};
pub use hist::{Hist, Quantile, Reading};
pub use line::{LineError, Problem, parse_records, print_records};

pub const RESAMPLES: usize = 10_000;
pub const LOOKS: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ident(String);

impl Ident {
    pub fn new(text: &str) -> Option<Self> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || "._:/+@-".contains(c);
        ((1..=128).contains(&text.len()) && text.chars().all(allowed)).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Workload {
    pub fixture_hash: Ident,
    pub mode: Ident,
    pub backend: Ident,
    pub load: Ident,
    pub scale: Ident,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arm {
    A0,
    A,
    B,
    AA,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    events: u64,
    frames: u64,
}

impl Count {
    pub fn new(events: u64, frames: u64) -> Option<Self> {
        (events <= frames).then_some(Self { events, frames })
    }

    pub fn events(&self) -> u64 {
        self.events
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn add(&mut self, other: &Count) {
        self.events = self.events.saturating_add(other.events);
        self.frames = self.frames.saturating_add(other.frames);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Samples {
    Latency(Hist),
    Count(Count),
}

impl Samples {
    fn fits(&self, gate: Gate) -> bool {
        matches!((self, gate.kind()), (Samples::Latency(_), Kind::Latency { .. }) | (Samples::Count(_), Kind::Rate { .. }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub test_id: Ident,
    pub run_id: Ident,
    pub app_pin: Ident,
    pub fw_rev: Ident,
    pub os: Ident,
    pub runner_image: Ident,
    pub night: Ident,
    pub workload: Workload,
    pub block: u32,
    pub arm: Arm,
    pub gate: Gate,
    pub refresh_hz: NonZeroU32,
    pub samples: Samples,
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
    pub unit: Unit,
    pub allow: u64,
    pub delta: i64,
    pub low: i64,
    pub high: i64,
    pub excluded_strata: usize,
}

impl Measurement {
    pub fn verdict(&self) -> Verdict {
        let allow = i64::try_from(self.allow).unwrap_or(i64::MAX);
        if self.high <= allow {
            Verdict::Pass
        } else if self.low > allow {
            Verdict::Fail
        } else {
            Verdict::Inconclusive(Doubt::Noisy)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Measured(Measurement),
    Withheld(Doubt),
}

impl Decision {
    pub fn verdict(&self) -> Verdict {
        match self {
            Decision::Measured(measurement) => measurement.verdict(),
            Decision::Withheld(doubt) => Verdict::Inconclusive(*doubt),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairError {
    NoRecords,
    MixedDigest { run_id: Ident },
    SamplesKind { run_id: Ident },
    Arms(Vec<Arm>),
    Unpaired { night: Ident, runner_image: Ident, block: u32 },
}

pub fn decide(records: &[RunRecord], calibrations: &[Calibration]) -> Result<Decision, PairError> {
    let pool = bootstrap::Pool::new(records)?;
    if !pool.images().all(|image| calibration::eligible(calibrations, pool.gate(), image)) {
        return Ok(Decision::Withheld(Doubt::NoRunner));
    }
    if pool.looks() > LOOKS {
        return Ok(Decision::Withheld(Doubt::Budget));
    }
    Ok(pool.measure(seed(records)))
}

pub fn calibration_verdict(records: &[RunRecord]) -> Result<Verdict, PairError> {
    Ok(bootstrap::Pool::new(records)?.measure(seed(records)).verdict())
}

fn seed(records: &[RunRecord]) -> u64 {
    let mut lines: Vec<String> = records.iter().map(RunRecord::to_string).collect();
    lines.sort_unstable();
    lines
        .iter()
        .flat_map(|line| line.bytes().chain(*b"\n"))
        .fold(0xCBF2_9CE4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3))
}
