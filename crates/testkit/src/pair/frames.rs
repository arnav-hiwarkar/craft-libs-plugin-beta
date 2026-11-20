use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU32;
use std::time::Duration;

use super::{Arm, Count, Gate, Hist, Ident, PairError, Quantile, Reading, RunRecord, Samples, Verdict, Workload, calibration_verdict};

mod run;
#[cfg(test)]
mod tests;

pub use run::{DEFAULT_REFRESH_HZ, Opts, Run, clear, load_label};

pub const CALIBRATE: &str = "CONF-calibrate";
pub const ABBA: [Arm; 4] = [Arm::A, Arm::AA, Arm::AA, Arm::A];
pub const LONG_STALL_FLOOR: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    OnTime,
    Missed,
    LongStall,
}

pub fn cadence(interval: Duration, refresh_hz: NonZeroU32) -> Cadence {
    let interval = interval.as_nanos();
    let periods = interval * u128::from(refresh_hz.get());
    if periods > 4_000_000_000 && interval > LONG_STALL_FLOOR.as_nanos() {
        Cadence::LongStall
    } else if 2 * periods > 3_000_000_000 {
        Cadence::Missed
    } else {
        Cadence::OnTime
    }
}

pub fn millis(ms: f64) -> Option<Duration> {
    let nanos = (ms * 1e6).round();
    (nanos.is_finite() && nanos >= 0.0 && nanos < u64::MAX as f64).then(|| Duration::from_nanos(nanos as u64))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame<const N: usize> {
    pub start: Duration,
    pub stages: [Duration; N],
    pub latency: Option<Duration>,
    pub playback: Count,
    pub seek: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe<const N: usize> {
    pub stages: [Hist; N],
    pub cpu: Hist,
    pub interval: Hist,
    pub latency: Hist,
    pub seek: Hist,
    pub intervals: u64,
    pub missed: u64,
    pub stalls: u64,
    pub playback: Count,
}

impl<const N: usize> Default for Probe<N> {
    fn default() -> Self {
        Self {
            stages: std::array::from_fn(|_| Hist::new()),
            cpu: Hist::new(),
            interval: Hist::new(),
            latency: Hist::new(),
            seek: Hist::new(),
            intervals: 0,
            missed: 0,
            stalls: 0,
            playback: Count::default(),
        }
    }
}

impl<const N: usize> Probe<N> {
    pub fn of(frames: &[Frame<N>], refresh_hz: NonZeroU32) -> Self {
        let mut probe = Probe::default();
        for frame in frames {
            for (hist, took) in probe.stages.iter_mut().zip(frame.stages) {
                hist.record(took);
            }
            probe.cpu.record(frame.stages.iter().fold(Duration::ZERO, |total, took| total.saturating_add(*took)));
            if let Some(latency) = frame.latency {
                probe.latency.record(latency);
            }
            if let Some(seek) = frame.seek {
                probe.seek.record(seek);
            }
            probe.playback.add(&frame.playback);
        }
        for interval in frames.windows(2).filter_map(|pair| pair[1].start.checked_sub(pair[0].start)) {
            probe.interval.record(interval);
            probe.intervals += 1;
            match cadence(interval, refresh_hz) {
                Cadence::OnTime => {}
                Cadence::Missed => probe.missed += 1,
                Cadence::LongStall => {
                    probe.missed += 1;
                    probe.stalls += 1;
                }
            }
        }
        probe
    }

    pub fn merge(&mut self, other: &Probe<N>) {
        for (hist, more) in self.stages.iter_mut().zip(&other.stages) {
            hist.merge(more);
        }
        self.cpu.merge(&other.cpu);
        self.interval.merge(&other.interval);
        self.latency.merge(&other.latency);
        self.seek.merge(&other.seek);
        self.intervals += other.intervals;
        self.missed += other.missed;
        self.stalls += other.stalls;
        self.playback.add(&other.playback);
    }

    pub fn samples(&self, gate: Gate) -> Option<Samples> {
        let per_interval = |events| Samples::Count(Count::new(events, self.intervals).unwrap_or_default());
        match gate {
            Gate::OverheadCpu => Some(Samples::Latency(self.cpu.clone())),
            Gate::OverheadInterval => Some(Samples::Latency(self.interval.clone())),
            Gate::UiEqMissed => Some(per_interval(self.missed)),
            Gate::UiEqLongStalls => Some(per_interval(self.stalls)),
            Gate::UiEqLatency => Some(Samples::Latency(self.latency.clone())),
            Gate::PlaybackLate => Some(Samples::Count(self.playback)),
            Gate::PlaybackSeek => Some(Samples::Latency(self.seek.clone())),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeriesError {
    Ident(String),
    Unmeasured(Gate),
    Pool(Gate, PairError),
    WorkloadChanged(String),
}

impl fmt::Display for SeriesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeriesError::Ident(text) => write!(f, "`{text}` is not a record identity: 1 to 128 of A-Z a-z 0-9 . _ : / + @ -"),
            SeriesError::Unmeasured(gate) => write!(f, "{gate:?} is not measured by a frame series"),
            SeriesError::Pool(gate, error) => write!(f, "{gate:?}: {error:?}"),
            SeriesError::WorkloadChanged(name) => write!(f, "{name}: the workload changed between blocks"),
        }
    }
}

pub fn ident(text: &str) -> Result<Ident, SeriesError> {
    Ident::new(text).ok_or_else(|| SeriesError::Ident(text.to_owned()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub run: String,
    pub app_pin: Ident,
    pub fw_rev: Ident,
    pub os: Ident,
    pub runner_image: Ident,
    pub night: Ident,
    pub refresh_hz: NonZeroU32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub unit: u32,
    pub position: usize,
    pub arm: Arm,
}

pub fn blocks(units: u32) -> impl Iterator<Item = Block> {
    (1..=units).flat_map(|unit| ABBA.into_iter().enumerate().map(move |(position, arm)| Block { unit, position, arm }))
}

pub fn records<const N: usize>(
    id: &Identity,
    name: &str,
    workload: &Workload,
    block: Block,
    probe: &Probe<N>,
    gates: &[Gate],
) -> Result<Vec<RunRecord>, SeriesError> {
    gates
        .iter()
        .map(|&gate| {
            Ok(RunRecord {
                test_id: ident(CALIBRATE)?,
                run_id: ident(&format!("{}.{name}.{}.{}.{gate:?}", id.run, block.unit, block.position))?,
                app_pin: id.app_pin.clone(),
                fw_rev: id.fw_rev.clone(),
                os: id.os.clone(),
                runner_image: id.runner_image.clone(),
                night: id.night.clone(),
                workload: workload.clone(),
                block: block.unit,
                arm: block.arm,
                gate,
                refresh_hz: id.refresh_hz,
                samples: probe.samples(gate).ok_or(SeriesError::Unmeasured(gate))?,
            })
        })
        .collect()
}

pub fn verdicts(records: &[RunRecord]) -> Result<Vec<(Gate, Verdict)>, SeriesError> {
    let mut pools: BTreeMap<Gate, Vec<RunRecord>> = BTreeMap::new();
    for record in records {
        pools.entry(record.gate).or_default().push(record.clone());
    }
    pools
        .into_iter()
        .map(|(gate, pool)| calibration_verdict(&pool).map(|verdict| (gate, verdict)).map_err(|error| SeriesError::Pool(gate, error)))
        .collect()
}

pub struct Heading<'a> {
    pub id: &'a Identity,
    pub load: &'a str,
    pub units: u32,
    pub frames: u32,
    pub records: &'a str,
}

impl fmt::Display for Heading<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Heading { id, load, units, frames, records } = self;
        write!(
            f,
            "## Paired frame probe: A/AA calibration (`{CALIBRATE}`)\n\n- Night `{}`, runner image `{}`, os `{}`, app `{}`, framework `{}`, load `{load}`\n- {units} ABBA units of {frames} frames per block at {} Hz; records in `{records}`\n\n",
            id.night, id.runner_image, id.os, id.app_pin, id.fw_rev, id.refresh_hz,
        )
    }
}

pub struct Section<'a, const N: usize> {
    pub name: &'a str,
    pub workload: &'a Workload,
    pub stages: [&'a str; N],
    pub probe: &'a Probe<N>,
    pub verdicts: &'a [(Gate, Verdict)],
}

fn number(v: f64) -> String {
    if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

fn reading(hist: &Hist, quantile: Quantile) -> String {
    match hist.quantile(quantile) {
        None => "–".into(),
        Some(Reading::Below) => "< 0.001".into(),
        Some(Reading::Above) => "> 60000".into(),
        Some(Reading::At(at)) => number(at.as_secs_f64() * 1000.0),
    }
}

fn row(f: &mut fmt::Formatter<'_>, name: &str, hist: &Hist) -> fmt::Result {
    writeln!(
        f,
        "| {name} | {} | {} | {} | {} |",
        hist.count(),
        reading(hist, Quantile::P50),
        reading(hist, Quantile::P95),
        reading(hist, Quantile::P99)
    )
}

fn verdict_word(verdict: Verdict) -> String {
    match verdict {
        Verdict::Pass => "Pass".into(),
        Verdict::Fail => "**Fail**".into(),
        Verdict::Inconclusive(doubt) => format!("Inconclusive ({doubt:?})"),
    }
}

impl<const N: usize> fmt::Display for Section<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Section { name, workload, stages, probe, verdicts } = self;
        let measures = |wanted: Gate| verdicts.iter().any(|(gate, _)| *gate == wanted);
        write!(f, "### {name} ({}, {})\n\n| Metric | n | p50 ms | p95 ms | p99 ms |\n|---|---:|---:|---:|---:|\n", workload.backend, workload.scale)?;
        for (stage, hist) in stages.iter().zip(&probe.stages) {
            row(f, &format!("C {stage}"), hist)?;
        }
        row(f, "C frame", &probe.cpu)?;
        row(f, "I", &probe.interval)?;
        if measures(Gate::UiEqLatency) {
            row(f, "L", &probe.latency)?;
        }
        if measures(Gate::PlaybackSeek) {
            row(f, "P seek", &probe.seek)?;
        }
        write!(f, "\nMissed frames: {} of {}. Long stalls: {} of {}.", probe.missed, probe.intervals, probe.stalls, probe.intervals)?;
        if measures(Gate::PlaybackLate) {
            write!(f, " Late playback frames: {} of {}.", probe.playback.events(), probe.playback.frames())?;
        }
        f.write_str("\n\n| Gate | A/AA verdict |\n|---|---|\n")?;
        for (gate, verdict) in verdicts.iter() {
            writeln!(f, "| {gate:?} | {} |", verdict_word(*verdict))?;
        }
        Ok(())
    }
}
