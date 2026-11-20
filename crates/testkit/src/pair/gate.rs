use std::num::NonZeroU32;
use std::time::Duration;

use super::Quantile;

named_variants! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Gate, GATES {
        UiEqCpu,
        UiEqGpu,
        UiEqLatency,
        UiEqExtractionLatency,
        UiEqMissed,
        UiEqLongStalls,
        UiLoadMissed,
        UiLoadLatency,
        UiLoadStrokeLatency,
        OverheadCpu,
        OverheadInterval,
        PlaybackLate,
        PlaybackSeek,
        CommandWallMedian,
        CommandWallTail,
        ExportWall,
        ExportGpu,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floor {
    Fixed(Duration),
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Latency { quantile: Quantile, floor: Floor, per_mille: u32, ceiling: Option<Duration> },
    Rate { floor_ppb: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Nanoseconds,
    PartsPerBillion,
}

const fn latency(quantile: Quantile, floor: Floor, per_mille: u32) -> Kind {
    Kind::Latency { quantile, floor, per_mille, ceiling: None }
}

const fn ms(millis: u64) -> Floor {
    Floor::Fixed(Duration::from_millis(millis))
}

const fn pp_hundredths(hundredths: u64) -> Kind {
    Kind::Rate { floor_ppb: hundredths * 100_000 }
}

impl Gate {
    pub const fn kind(self) -> Kind {
        match self {
            Gate::UiEqCpu | Gate::UiEqGpu => latency(Quantile::P95, ms(1), 100),
            Gate::UiEqLatency | Gate::UiLoadLatency => latency(Quantile::P95, Floor::Refresh, 100),
            Gate::UiEqExtractionLatency | Gate::UiLoadStrokeLatency => latency(Quantile::P95, ms(2), 100),
            Gate::UiEqMissed => pp_hundredths(50),
            Gate::UiEqLongStalls => pp_hundredths(1),
            Gate::UiLoadMissed | Gate::PlaybackLate => pp_hundredths(100),
            Gate::OverheadCpu | Gate::OverheadInterval => latency(Quantile::P50, ms(0), 150),
            Gate::PlaybackSeek => latency(Quantile::P95, ms(50), 100),
            Gate::CommandWallMedian => latency(Quantile::P50, ms(50), 100),
            Gate::CommandWallTail => {
                Kind::Latency { quantile: Quantile::P95, floor: ms(50), per_mille: 100, ceiling: Some(Duration::from_millis(100)) }
            }
            Gate::ExportWall | Gate::ExportGpu => latency(Quantile::P50, ms(0), 100),
        }
    }
}

impl Kind {
    pub fn unit(&self) -> Unit {
        match self {
            Kind::Latency { .. } => Unit::Nanoseconds,
            Kind::Rate { .. } => Unit::PartsPerBillion,
        }
    }

    pub fn allow(&self, base: u64, refresh_hz: NonZeroU32) -> u64 {
        match *self {
            Kind::Rate { floor_ppb } => floor_ppb,
            Kind::Latency { floor, per_mille, ceiling, .. } => {
                let floor = match floor {
                    Floor::Fixed(floor) => u64::try_from(floor.as_nanos()).unwrap_or(u64::MAX),
                    Floor::Refresh => 1_000_000_000 / u64::from(refresh_hz.get()),
                };
                let share = u64::try_from(u128::from(base) * u128::from(per_mille) / 1_000).unwrap_or(u64::MAX);
                let widest = floor.max(share);
                match ceiling.map(|ceiling| u64::try_from(ceiling.as_nanos()).unwrap_or(u64::MAX)) {
                    Some(ceiling) if base <= ceiling => widest.min(ceiling - base),
                    _ => widest,
                }
            }
        }
    }
}
