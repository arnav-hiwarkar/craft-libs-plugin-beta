use std::num::NonZeroU32;
use std::time::Duration;

use super::{
    AA_VERDICTS, Arm, Calibration, Controls, Decision, Doubt, Eligibility, Gate, Hist, LOOKS, Measurement, PairError, Quantile, Reading, RunRecord,
    SplitMix64, Verdict, calibration_verdict, decide,
};

const IMAGE: &str = "macos-15-arm64";
const SEED: u64 = 0x5EED;
const NIGHTLY: Load = Load { block_noise: 0.25, ..STEADY };

fn hz(rate: u32) -> NonZeroU32 {
    NonZeroU32::new(rate).unwrap_or(NonZeroU32::MIN)
}

fn uniform(rng: &mut SplitMix64) -> f64 {
    ((rng.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
}

fn gaussian(rng: &mut SplitMix64) -> f64 {
    let (u, v) = (uniform(rng), uniform(rng));
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

struct Load {
    median_ns: f64,
    spread: f64,
    unit_drift: f64,
    block_noise: f64,
    frames: usize,
}

const STEADY: Load = Load { median_ns: 2_440_000.0, spread: 0.3, unit_drift: 0.05, block_noise: 0.02, frames: 500 };

fn record(night: &str, block: u32, position: usize, arm: Arm, samples: Hist) -> RunRecord {
    RunRecord {
        test_id: "CONF-aa-sequential".to_owned(),
        run_id: format!("{night}-{block}-{position}"),
        app_pin: "pc-359051359".to_owned(),
        fw_rev: "cl-9b33fe822".to_owned(),
        os: "macos-15".to_owned(),
        runner_image: IMAGE.to_owned(),
        night: night.to_owned(),
        block,
        arm,
        gate: Gate::UiEqCpu,
        refresh_hz: hz(120),
        samples,
    }
}

fn night(rng: &mut SplitMix64, load: &Load, name: &str, units: u32, shift: Duration) -> Vec<RunRecord> {
    let mut records = Vec::new();
    for block in 0..units {
        let drift = (load.unit_drift * gaussian(rng)).exp();
        for (position, arm) in [Arm::A, Arm::B, Arm::B, Arm::A].into_iter().enumerate() {
            let noise = (load.block_noise * gaussian(rng)).exp();
            let added = if arm == Arm::B { shift.as_nanos() as f64 } else { 0.0 };
            let mut samples = Hist::new();
            for _ in 0..load.frames {
                let nanos = load.median_ns * drift * noise * (load.spread * gaussian(rng)).exp() + added;
                samples.record(Duration::from_nanos(nanos as u64));
            }
            records.push(record(name, block, position, arm, samples));
        }
    }
    records
}

fn calibration(aa: Vec<Verdict>) -> Calibration {
    Calibration {
        gate: Gate::UiEqCpu,
        runner_image: IMAGE.to_owned(),
        aa,
        controls: Controls { injected_delay: Verdict::Fail, gpu_heavy: Verdict::Fail, null: Verdict::Pass },
    }
}

fn eligible() -> Vec<Calibration> {
    vec![calibration(vec![Verdict::Pass; AA_VERDICTS])]
}

#[test]
fn hist_quantiles_stay_within_one_percent_from_one_microsecond_to_sixty_seconds() {
    let mut rng = SplitMix64(7);
    let (low, high) = (1_000f64.ln(), 60_000_000_000f64.ln());
    let mut worst = 0f64;
    for _ in 0..40 {
        let samples: Vec<u64> = (0..1_000).map(|_| (low + uniform(&mut rng) * (high - low)).exp() as u64).collect();
        let (mut even, mut odd) = (Hist::new(), Hist::new());
        for (index, nanos) in samples.iter().enumerate() {
            let half = if index % 2 == 0 { &mut even } else { &mut odd };
            half.record(Duration::from_nanos(*nanos));
        }
        even.merge(&odd);
        let mut sorted = samples.clone();
        sorted.sort_unstable();
        for permyriad in [1, 10, 100, 2_500, 5_000, 9_000, 9_500, 9_900, 9_999, 10_000, 1 + (rng.next() % 10_000) as u16] {
            let rank = (sorted.len() * usize::from(permyriad)).div_ceil(10_000);
            let exact = sorted[rank - 1] as f64;
            let Some(Some(Reading::At(estimate))) = Quantile::from_permyriad(permyriad).map(|quantile| even.quantile(quantile)) else {
                panic!("quantile {permyriad} of in-range samples is not a value");
            };
            worst = worst.max((estimate.as_nanos() as f64 - exact).abs() / exact);
        }
    }
    assert!(worst <= 0.01, "worst relative quantile error {worst}");
    assert!(worst > 0.001, "the property never exercised bucket error: {worst}");
}

#[test]
fn hist_counts_samples_outside_its_range_and_keeps_its_edges() {
    let mut hist = Hist::new();
    for nanos in [0, 999, 1_000, 60_000_000_000, 60_000_000_001, u64::MAX] {
        hist.record(Duration::from_nanos(nanos));
    }
    hist.record(Duration::MAX);
    assert_eq!((hist.count(), hist.underflow(), hist.overflow()), (7, 2, 3));
    assert_eq!(hist.quantile(Quantile::P50), Some(Reading::At(Duration::from_nanos(59_861_106_688))));
    assert_eq!(hist.quantile(Quantile::from_permyriad(2_857).unwrap()), Some(Reading::Below));
    assert_eq!(hist.quantile(Quantile::from_permyriad(4_285).unwrap()), Some(Reading::At(Duration::from_nanos(1_004))));
    assert_eq!(hist.quantile(Quantile::P95), Some(Reading::Above));
    assert_eq!(Hist::new().quantile(Quantile::P50), None);
    assert_eq!((Quantile::from_permyriad(0), Quantile::from_permyriad(10_001)), (None, None));
}

#[test]
fn allow_is_the_floor_or_the_share_of_the_native_quantile_per_gate() {
    let allow = |gate: Gate, base_ms: u64, rate: u32| gate.spec().allowance.allow(Duration::from_millis(base_ms), hz(rate));
    let cases = [
        (Gate::UiEqCpu, 4, 120, Duration::from_millis(1)),
        (Gate::UiEqCpu, 30, 120, Duration::from_millis(3)),
        (Gate::UiEqLatency, 20, 60, Duration::from_nanos(16_666_666)),
        (Gate::UiEqExtractionLatency, 4, 120, Duration::from_millis(2)),
        (Gate::UiLoadLatency, 20, 120, Duration::from_nanos(8_333_333)),
        (Gate::UiLoadStrokeLatency, 4, 120, Duration::from_millis(2)),
        (Gate::OverheadCpu, 10, 120, Duration::from_micros(1_500)),
        (Gate::OverheadInterval, 8, 120, Duration::from_micros(1_200)),
        (Gate::PlaybackSeek, 100, 120, Duration::from_millis(50)),
        (Gate::CommandWallMedian, 40, 120, Duration::from_millis(50)),
        (Gate::CommandWallTail, 80, 120, Duration::from_millis(20)),
        (Gate::CommandWallTail, 600, 120, Duration::from_millis(60)),
        (Gate::ExportWall, 1_000, 120, Duration::from_millis(100)),
        (Gate::ExportGpu, 0, 120, Duration::ZERO),
    ];
    for (gate, base_ms, rate, expected) in cases {
        assert_eq!(allow(gate, base_ms, rate), expected, "{gate:?} at {base_ms} ms");
    }
}

#[test]
fn identical_seeds_give_an_identical_decision_and_interval() {
    let records = night(&mut SplitMix64(11), &STEADY, "2026-11-21", 6, Duration::from_micros(300));
    let first = decide(&records, &eligible(), SEED);
    assert_eq!(first, decide(&records, &eligible(), SEED));
    let Ok(Decision::Measured(measured)) = first else { panic!("an eligible pool was not measured: {first:?}") };
    assert_eq!(measured, Measurement { allow: Duration::from_millis(1), delta_ns: 344_064, low_ns: 110_154, high_ns: 577_974 });
}

#[test]
fn conf_power_twice_the_allowance_fails_and_a_null_shift_passes() {
    let shifted = night(&mut SplitMix64(21), &STEADY, "2026-11-21", 4, Duration::from_millis(2));
    let null = night(&mut SplitMix64(22), &STEADY, "2026-11-21", 4, Duration::ZERO);
    assert_eq!(calibration_verdict(&shifted, SEED), Ok(Verdict::Fail));
    assert_eq!(calibration_verdict(&null, SEED), Ok(Verdict::Pass));
}

#[test]
fn a_shift_equal_to_the_allowance_is_noisy_and_noise_never_widens_the_allowance() {
    let noisy = Load { block_noise: 0.2, ..STEADY };
    let records = night(&mut SplitMix64(31), &noisy, "2026-11-21", 4, Duration::from_millis(1));
    let decision = decide(&records, &eligible(), SEED);
    assert_eq!(decision.as_ref().map(Decision::verdict), Ok(Verdict::Inconclusive(Doubt::Noisy)));
    let Ok(Decision::Measured(Measurement { allow, low_ns, high_ns, .. })) = decision else { panic!("not measured: {decision:?}") };
    assert_eq!(allow, Duration::from_millis(1));
    assert!(low_ns <= 1_000_000 && high_ns > 1_000_000, "interval {low_ns}..{high_ns}");
}

#[test]
fn conf_calibrate_eligibility_needs_59_clean_a_a_verdicts_and_behaving_controls() {
    let records = night(&mut SplitMix64(41), &STEADY, "2026-11-21", 2, Duration::ZERO);
    let verdict_with = |calibrations: Vec<Calibration>| decide(&records, &calibrations, SEED).map(|decision| decision.verdict());
    let mut one_fail = vec![Verdict::Pass; AA_VERDICTS];
    one_fail[30] = Verdict::Fail;
    let mut misbehaved = calibration(vec![Verdict::Pass; AA_VERDICTS]);
    misbehaved.controls.injected_delay = Verdict::Inconclusive(Doubt::Noisy);
    let mut other_image = calibration(vec![Verdict::Pass; AA_VERDICTS]);
    other_image.runner_image = "macos-14-arm64".to_owned();
    let mut other_gate = calibration(vec![Verdict::Pass; AA_VERDICTS]);
    other_gate.gate = Gate::UiEqLatency;

    assert_eq!(calibration(vec![Verdict::Pass; 58]).eligibility(), Eligibility::TooFewAa(58));
    assert_eq!(calibration(one_fail.clone()).eligibility(), Eligibility::AaFailed(1));
    assert_eq!(misbehaved.eligibility(), Eligibility::ControlsMisbehaved);
    let no_runner = Ok(Verdict::Inconclusive(Doubt::NoRunner));
    assert_eq!(verdict_with(vec![calibration(vec![Verdict::Pass; 58])]), no_runner);
    assert_eq!(verdict_with(vec![calibration(one_fail)]), no_runner);
    assert_eq!(verdict_with(vec![misbehaved]), no_runner);
    assert_eq!(verdict_with(vec![other_image, other_gate]), no_runner);
    let mut inconclusive_aa = vec![Verdict::Pass; AA_VERDICTS];
    inconclusive_aa[0] = Verdict::Inconclusive(Doubt::Noisy);
    assert!(matches!(decide(&records, &[calibration(inconclusive_aa)], SEED), Ok(Decision::Measured(_))));
}

#[test]
fn a_sixth_night_is_over_budget() {
    let mut rng = SplitMix64(51);
    let nights: Vec<String> = (1..=LOOKS + 1).map(|day| format!("2026-11-{day:02}")).collect();
    let mut records: Vec<RunRecord> = Vec::new();
    for name in &nights[..LOOKS] {
        records.extend(night(&mut rng, &STEADY, name, 2, Duration::ZERO));
    }
    assert_eq!(decide(&records, &eligible(), SEED).map(|decision| decision.verdict()), Ok(Verdict::Pass));
    records.extend(night(&mut rng, &STEADY, &nights[LOOKS], 2, Duration::ZERO));
    assert_eq!(decide(&records, &eligible(), SEED), Ok(Decision::OverBudget));
}

#[test]
fn pools_that_cannot_pair_are_errors_and_a_lone_unit_is_noisy() {
    let records = night(&mut SplitMix64(61), &Load { frames: 20, ..STEADY }, "2026-11-21", 2, Duration::ZERO);
    let without_last_b: Vec<RunRecord> = records.iter().filter(|record| !(record.block == 1 && record.arm == Arm::B)).cloned().collect();
    let mut with_aa = records.clone();
    with_aa[0].arm = Arm::AA;
    let mut other_pin = records.clone();
    other_pin[5].app_pin = "pc-other".to_owned();

    assert_eq!(decide(&[], &eligible(), SEED), Err(PairError::NoRecords));
    assert_eq!(
        decide(&without_last_b, &eligible(), SEED),
        Err(PairError::Unpaired { night: "2026-11-21".to_owned(), runner_image: IMAGE.to_owned(), block: 1 })
    );
    assert_eq!(decide(&with_aa, &eligible(), SEED), Err(PairError::Arms(vec![Arm::A, Arm::B, Arm::AA])));
    assert_eq!(decide(&other_pin, &eligible(), SEED), Err(PairError::MixedDigest { run_id: "2026-11-21-1-1".to_owned() }));
    assert_eq!(decide(&records[..4], &eligible(), SEED), Ok(Decision::Unmeasurable));
}

fn first_decisive_verdict(trial: u64, shift: Duration) -> Verdict {
    let mut rng = SplitMix64(1_000 + trial);
    let mut pooled = Vec::new();
    let mut verdict = Verdict::Inconclusive(Doubt::Noisy);
    for look in 1..=LOOKS {
        pooled.extend(night(&mut rng, &NIGHTLY, &format!("night-{look}"), 3, shift));
        verdict = decide(&pooled, &eligible(), trial).unwrap().verdict();
        if matches!(verdict, Verdict::Pass | Verdict::Fail) {
            break;
        }
    }
    verdict
}

fn sequential(trials: u64, shift: Duration) -> Vec<Verdict> {
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                scope.spawn(move || (worker as u64..trials).step_by(workers).map(|trial| first_decisive_verdict(trial, shift)).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect()
    })
}

#[test]
#[ignore = "CONF-aa-sequential: 2,000 verdicts take longer than a unit test; run it with cargo test -p craft-testkit -- --ignored conf_aa_sequential"]
fn conf_aa_sequential_false_verdicts_stay_under_five_percent_within_five_looks() {
    let trials = 200;
    let count = |verdicts: &[Verdict], wanted: Verdict| verdicts.iter().filter(|verdict| **verdict == wanted).count();
    let null = sequential(trials, Duration::ZERO);
    let regressed = sequential(trials, Duration::from_micros(1_200));
    assert_eq!((null.len(), regressed.len()), (200, 200));
    let tally = [count(&null, Verdict::Pass), count(&null, Verdict::Fail), count(&regressed, Verdict::Pass), count(&regressed, Verdict::Fail)];
    let [null_passes, false_fails, false_passes, true_fails] = tally;
    assert!(false_fails <= 10 && false_passes <= 10, "false Fail {false_fails} and false Pass {false_passes} in {trials} trials each");
    assert!(null_passes >= 80 && true_fails >= 5, "the simulation decided too little: {tally:?}");
}
