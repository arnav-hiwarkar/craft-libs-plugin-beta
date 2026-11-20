use std::ops::RangeInclusive;
use std::time::Duration;

const SUB_BITS: u32 = 6;
const SUBS: usize = 1 << SUB_BITS;
const LOWEST_NS: u64 = 1_000;
const HIGHEST_NS: u64 = 60_000_000_000;
const FIRST_OCTAVE: u32 = u64::BITS - 1 - LOWEST_NS.leading_zeros();
const LAST_OCTAVE: u32 = u64::BITS - 1 - HIGHEST_NS.leading_zeros();
const OCTAVES: usize = (LAST_OCTAVE - FIRST_OCTAVE + 1) as usize;
const SLOTS: usize = OCTAVES * SUBS + 2;
pub(super) const OVERFLOW: usize = SLOTS - 1;
const PERMYRIAD: u16 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quantile(u16);

impl Quantile {
    pub const P50: Self = Self(5_000);
    pub const P95: Self = Self(9_500);
    pub const P99: Self = Self(9_900);

    pub fn from_permyriad(permyriad: u16) -> Option<Self> {
        (1..=PERMYRIAD).contains(&permyriad).then_some(Self(permyriad))
    }

    fn rank(self, total: u64) -> Option<u64> {
        let rank = (u128::from(total) * u128::from(self.0)).div_ceil(u128::from(PERMYRIAD));
        (total > 0).then(|| u64::try_from(rank).unwrap_or(total))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Below,
    At(Duration),
    Above,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hist {
    slots: Vec<u64>,
}

impl Default for Hist {
    fn default() -> Self {
        Self { slots: vec![0; SLOTS] }
    }
}

impl Hist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, sample: Duration) {
        let nanos = u64::try_from(sample.as_nanos()).unwrap_or(u64::MAX);
        if let Some(count) = self.slots.get_mut(slot_of(nanos)) {
            *count = count.saturating_add(1);
        }
    }

    pub fn merge(&mut self, other: &Hist) {
        for (count, more) in self.slots.iter_mut().zip(&other.slots) {
            *count = count.saturating_add(*more);
        }
    }

    pub fn count(&self) -> u64 {
        self.slots.iter().fold(0, |total, &count| total.saturating_add(count))
    }

    pub fn underflow(&self) -> u64 {
        self.slots.first().copied().unwrap_or(0)
    }

    pub fn overflow(&self) -> u64 {
        self.slots.last().copied().unwrap_or(0)
    }

    pub fn quantile(&self, quantile: Quantile) -> Option<Reading> {
        let cumulative = self.cumulative();
        slot_at(quantile, 0..=OVERFLOW, |slot| cumulative.get(slot).copied().unwrap_or(0)).map(reading)
    }

    pub(super) fn cumulative(&self) -> Vec<u64> {
        self.slots
            .iter()
            .scan(0u64, |total, &count| {
                *total = total.saturating_add(count);
                Some(*total)
            })
            .collect()
    }
}

pub(super) fn slot_at(quantile: Quantile, within: RangeInclusive<usize>, cumulative: impl Fn(usize) -> u64) -> Option<usize> {
    let rank = quantile.rank(cumulative(OVERFLOW))?;
    let (mut low, mut high) = within.into_inner();
    while low < high {
        let middle = low + (high - low) / 2;
        if cumulative(middle) >= rank {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Some(low)
}

fn slot_of(nanos: u64) -> usize {
    if nanos < LOWEST_NS {
        return 0;
    }
    if nanos > HIGHEST_NS {
        return OVERFLOW;
    }
    let octave = u64::BITS - 1 - nanos.leading_zeros();
    let sub = (nanos >> (octave - SUB_BITS)) as usize & (SUBS - 1);
    1 + (octave - FIRST_OCTAVE) as usize * SUBS + sub
}

pub(super) fn reading(slot: usize) -> Reading {
    match slot {
        0 => Reading::Below,
        OVERFLOW.. => Reading::Above,
        _ => {
            let index = slot - 1;
            let octave = FIRST_OCTAVE + (index / SUBS) as u32;
            let width = 1u64 << (octave - SUB_BITS);
            let low = (1u64 << octave) + (index % SUBS) as u64 * width;
            Reading::At(Duration::from_nanos(low + width / 2))
        }
    }
}
