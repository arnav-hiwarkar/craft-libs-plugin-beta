use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use super::gate::GATES;
use super::{ARMS, Count, Hist, Ident, RunRecord, Samples, Workload};

const VERSION: &str = "craft-pair/1";
const MAX_LINE: usize = 64 * 1024;
pub const MAX_RECORDS: usize = 20_000;
pub const MAX_POOL_BYTES: usize = 64 * 1024 * 1024;
const COUNT_PREFIX: &str = "count:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Version,
    TooLong,
    TooManyRecords,
    PoolTooLarge,
    EmptyLine,
    Field(&'static str),
    Value(&'static str),
    Samples,
    SamplesKind,
    Trailing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineError {
    pub line: usize,
    pub problem: Problem,
}

impl LineError {
    pub(super) fn at(problem: Problem) -> Self {
        Self { line: 1, problem }
    }
}

pub(super) fn number<T: FromStr>(text: &str) -> Option<T> {
    let canonical = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

impl fmt::Display for Samples {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Samples::Latency(hist) => write!(f, "{hist}"),
            Samples::Count(count) => write!(f, "{COUNT_PREFIX}{}/{}", count.events(), count.frames()),
        }
    }
}

impl FromStr for Samples {
    type Err = LineError;

    fn from_str(text: &str) -> Result<Self, LineError> {
        let Some(count) = text.strip_prefix(COUNT_PREFIX) else {
            return text.parse::<Hist>().map(Samples::Latency);
        };
        let (events, frames) = count.split_once('/').ok_or(LineError::at(Problem::Samples))?;
        number(events)
            .zip(number(frames))
            .and_then(|(events, frames)| Count::new(events, frames))
            .map(Samples::Count)
            .ok_or(LineError::at(Problem::Samples))
    }
}

impl fmt::Display for RunRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Workload { fixture_hash, mode, backend, load, scale } = &self.workload;
        write!(
            f,
            "{VERSION} test_id={} run_id={} app_pin={} fw_rev={} os={} runner_image={} night={} fixture_hash={fixture_hash} mode={mode} \
             backend={backend} load={load} scale={scale} block={} arm={} gate={} refresh_hz={} samples={}",
            self.test_id,
            self.run_id,
            self.app_pin,
            self.fw_rev,
            self.os,
            self.runner_image,
            self.night,
            self.block,
            self.arm.name(),
            self.gate.name(),
            self.refresh_hz,
            self.samples,
        )
    }
}

struct Fields<'a>(std::str::Split<'a, char>);

impl<'a> Fields<'a> {
    fn value(&mut self, key: &'static str) -> Result<&'a str, LineError> {
        self.0
            .next()
            .and_then(|token| token.split_once('='))
            .filter(|(found, _)| *found == key)
            .map(|(_, value)| value)
            .ok_or(LineError::at(Problem::Field(key)))
    }

    fn ident(&mut self, key: &'static str) -> Result<Ident, LineError> {
        Ident::new(self.value(key)?).ok_or(LineError::at(Problem::Value(key)))
    }

    fn parsed<T>(&mut self, key: &'static str, parse: impl FnOnce(&str) -> Option<T>) -> Result<T, LineError> {
        parse(self.value(key)?).ok_or(LineError::at(Problem::Value(key)))
    }
}

impl FromStr for RunRecord {
    type Err = LineError;

    fn from_str(line: &str) -> Result<Self, LineError> {
        if line.len() > MAX_LINE {
            return Err(LineError::at(Problem::TooLong));
        }
        let mut tokens = line.split(' ');
        if tokens.next() != Some(VERSION) {
            return Err(LineError::at(Problem::Version));
        }
        let mut fields = Fields(tokens);
        let record = RunRecord {
            test_id: fields.ident("test_id")?,
            run_id: fields.ident("run_id")?,
            app_pin: fields.ident("app_pin")?,
            fw_rev: fields.ident("fw_rev")?,
            os: fields.ident("os")?,
            runner_image: fields.ident("runner_image")?,
            night: fields.ident("night")?,
            workload: Workload {
                fixture_hash: fields.ident("fixture_hash")?,
                mode: fields.ident("mode")?,
                backend: fields.ident("backend")?,
                load: fields.ident("load")?,
                scale: fields.ident("scale")?,
            },
            block: fields.parsed("block", number)?,
            arm: fields.parsed("arm", |text| ARMS.into_iter().find(|arm| arm.name() == text))?,
            gate: fields.parsed("gate", |text| GATES.into_iter().find(|gate| gate.name() == text))?,
            refresh_hz: fields.parsed("refresh_hz", |text| number::<u32>(text).and_then(NonZeroU32::new))?,
            samples: fields.value("samples")?.parse()?,
        };
        if fields.0.next().is_some() {
            return Err(LineError::at(Problem::Trailing));
        }
        if !record.samples.fits(record.gate) {
            return Err(LineError::at(Problem::SamplesKind));
        }
        Ok(record)
    }
}

pub fn print_records(records: &[RunRecord]) -> String {
    records.iter().map(|record| format!("{record}\n")).collect()
}

pub fn parse_records(text: &str) -> Result<Vec<RunRecord>, LineError> {
    if let Some(within) = text.as_bytes().get(..MAX_POOL_BYTES).filter(|_| text.len() > MAX_POOL_BYTES) {
        let line = within.iter().filter(|byte| **byte == b'\n').count() + 1;
        return Err(LineError { line, problem: Problem::PoolTooLarge });
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() {
        return Ok(Vec::new());
    }
    body.split('\n')
        .enumerate()
        .map(|(index, line)| {
            let problem = |problem| LineError { line: index + 1, problem };
            if index >= MAX_RECORDS {
                return Err(problem(Problem::TooManyRecords));
            }
            if line.is_empty() {
                return Err(problem(Problem::EmptyLine));
            }
            line.parse().map_err(|error: LineError| problem(error.problem))
        })
        .collect()
}
