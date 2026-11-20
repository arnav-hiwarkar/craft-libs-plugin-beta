use super::{Gate, Ident, Verdict};

pub const AA_VERDICTS: usize = 59;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    pub injected_delay: Verdict,
    pub gpu_heavy: Verdict,
    pub null: Verdict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Calibration {
    pub gate: Gate,
    pub runner_image: Ident,
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

pub(super) fn eligible(calibrations: &[Calibration], gate: Gate, runner_image: &Ident) -> bool {
    calibrations.iter().any(|calibration| {
        calibration.gate == gate && calibration.runner_image == *runner_image && calibration.eligibility() == Eligibility::Eligible
    })
}
